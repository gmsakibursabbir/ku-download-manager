//! yt-dlp integration: media analysis and download processes.

use crate::aria2::hide_window;
use anyhow::{anyhow, bail, Context, Result};
use ku_proto::{AudioQuality, BrowserCookie, MediaInfo, MediaOptions, PlaylistEntry, VideoQuality};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;

#[derive(Clone, Debug)]
pub struct YtEnv {
    pub ytdlp: PathBuf,
    pub ffmpeg: Option<PathBuf>,
    pub proxy: String,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    /// Temporary file with cookies sent by the browser extension (deleted after use).
    pub cookies_file: Option<PathBuf>,
    /// The user's own cookies.txt (Settings › Media); never deleted.
    pub user_cookies_file: Option<PathBuf>,
    /// Let yt-dlp read a browser's cookie store ("firefox", "chrome", …).
    pub cookies_from_browser: Option<String>,
}

impl YtEnv {
    fn base_command(&self) -> Command {
        let mut cmd = Command::new(&self.ytdlp);
        cmd.env("PYTHONIOENCODING", "utf-8").env("PYTHONUTF8", "1");
        // On-demand Linux ffmpeg (shared build) finds its libraries here.
        #[cfg(target_os = "linux")]
        if let Some(lib) = crate::tools::ffmpeg_lib_dir() {
            let prev = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
            cmd.env("LD_LIBRARY_PATH", if prev.is_empty() { lib.display().to_string() } else { format!("{}:{prev}", lib.display()) });
        }
        cmd.args(["--no-colors", "--socket-timeout", "20", "--ignore-config"]);
        if !self.proxy.trim().is_empty() {
            cmd.args(["--proxy", self.proxy.trim()]);
        }
        if let Some(ua) = &self.user_agent {
            cmd.args(["--user-agent", ua]);
        }
        if let Some(r) = &self.referer {
            cmd.args(["--referer", r]);
        }
        // Cookies from the extension win; then the user's file; then a browser's store.
        if let Some(c) = self.cookies_file.as_ref().or(self.user_cookies_file.as_ref()) {
            cmd.arg("--cookies").arg(c);
        } else if let Some(b) = &self.cookies_from_browser {
            cmd.args(["--cookies-from-browser", b]);
        }
        if let Some(f) = &self.ffmpeg {
            cmd.arg("--ffmpeg-location").arg(f);
        }
        cmd.stdin(Stdio::null()).kill_on_drop(true);
        hide_window(&mut cmd);
        #[cfg(unix)]
        cmd.process_group(0);
        cmd
    }
}

/// Write browser cookies in Netscape format for `--cookies`. Caller deletes it.
pub fn write_cookie_file(dir: &Path, cookies: &[BrowserCookie]) -> Result<Option<PathBuf>> {
    if cookies.is_empty() {
        return Ok(None);
    }
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("cookies-{}.txt", uuid::Uuid::new_v4().simple()));
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    for c in cookies {
        if c.name.contains(['\t', '\n']) || c.value.contains(['\t', '\n']) {
            continue;
        }
        let domain = if c.host_only || c.domain.starts_with('.') {
            c.domain.clone()
        } else {
            format!(".{}", c.domain)
        };
        let include_sub = if domain.starts_with('.') { "TRUE" } else { "FALSE" };
        let path_ = if c.path.is_empty() { "/" } else { &c.path };
        let exp = c.expiration_date.map(|e| e as i64).unwrap_or(0);
        let name = if c.http_only { format!("#HttpOnly_{domain}") } else { domain.clone() };
        out.push_str(&format!(
            "{name}\t{include_sub}\t{path_}\t{}\t{exp}\t{}\t{}\n",
            if c.secure { "TRUE" } else { "FALSE" },
            c.name,
            c.value
        ));
    }
    std::fs::write(&path, out)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).ok();
    }
    Ok(Some(path))
}

pub async fn version(bin: &Path) -> Option<String> {
    let mut cmd = Command::new(bin);
    cmd.arg("--version").stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    hide_window(&mut cmd);
    let out = tokio::time::timeout(Duration::from_secs(15), cmd.output()).await.ok()?.ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

fn last_error(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .rev()
        .find(|l| l.starts_with("ERROR:"))
        .map(|l| l.trim_start_matches("ERROR:").trim().to_string())
}

/// Turn yt-dlp errors into actionable messages.
pub fn friendly_error(raw: &str) -> String {
    let l = raw.to_ascii_lowercase();
    if l.contains("drm") {
        "This media is DRM-protected. KuDownloader does not bypass DRM.".into()
    } else if l.contains("unsupported url") {
        "No downloadable media was found at this address.".into()
    } else if l.contains("sign in to confirm") || l.contains("login required") || l.contains("private video") {
        format!("The site requires you to be signed in to access this media. Use the browser extension so your session cookies are sent. ({raw})")
    } else if l.contains("video unavailable") || l.contains("has been removed") {
        "This media is unavailable or has been removed.".into()
    } else if l.contains("http error 429") {
        "The site is rate limiting requests (HTTP 429). Try again later.".into()
    } else if l.contains("ffmpeg") && l.contains("not") {
        "FFmpeg is required for this format. Install FFmpeg or set its path in Settings › Media.".into()
    } else if l.contains("unable to download webpage") || l.contains("getaddrinfo") || l.contains("timed out") {
        format!("Could not reach the site. Check your connection. ({raw})")
    } else {
        raw.to_string()
    }
}

pub async fn analyze(env: &YtEnv, url: &str, playlist: bool) -> Result<MediaInfo> {
    let mut cmd = env.base_command();
    cmd.args(["-J", "--no-warnings", "--no-progress"]);
    if playlist {
        cmd.args(["--yes-playlist", "--flat-playlist"]);
    } else {
        cmd.arg("--no-playlist");
    }
    cmd.arg("--").arg(url);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd.spawn().context("starting yt-dlp")?;
    let out = tokio::time::timeout(Duration::from_secs(120), child.wait_with_output())
        .await
        .map_err(|_| anyhow!("The site took too long to respond while reading media information."))??;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        let e = last_error(&stderr).unwrap_or_else(|| stderr.trim().to_string());
        bail!("{}", friendly_error(&e));
    }
    let v: Value = serde_json::from_slice(&out.stdout).context("yt-dlp returned invalid JSON")?;
    parse_info(&v, url, env.ffmpeg.is_some())
}

fn f64_of(v: &Value) -> Option<f64> {
    v.as_f64()
}

pub fn parse_info(v: &Value, url: &str, ffmpeg: bool) -> Result<MediaInfo> {
    let s = |k: &str| v[k].as_str().map(str::to_string);
    let mut info = MediaInfo {
        url: url.to_string(),
        webpage_url: s("webpage_url").unwrap_or_else(|| url.to_string()),
        extractor: s("extractor_key").or_else(|| s("extractor")).unwrap_or_default(),
        title: s("title").unwrap_or_else(|| "Untitled".into()),
        uploader: s("uploader").or_else(|| s("channel")),
        duration: f64_of(&v["duration"]),
        view_count: v["view_count"].as_i64(),
        upload_date: s("upload_date"),
        thumbnail: s("thumbnail").or_else(|| {
            v["thumbnails"].as_array().and_then(|t| t.last()).and_then(|t| t["url"].as_str()).map(str::to_string)
        }),
        is_live: v["is_live"].as_bool().unwrap_or(false),
        ffmpeg_available: ffmpeg,
        ..Default::default()
    };
    if v["_type"] == "playlist" {
        info.is_playlist = true;
        let entries = v["entries"].as_array().cloned().unwrap_or_default();
        info.playlist_count = v["playlist_count"].as_u64().map(|c| c as u32).or(Some(entries.len() as u32));
        info.entries = entries
            .iter()
            .enumerate()
            .map(|(i, e)| PlaylistEntry {
                index: (i + 1) as u32,
                id: e["id"].as_str().unwrap_or("").to_string(),
                title: e["title"].as_str().unwrap_or("Untitled").to_string(),
                url: e["url"].as_str().or(e["webpage_url"].as_str()).unwrap_or("").to_string(),
                duration: f64_of(&e["duration"]),
            })
            .collect();
        if info.thumbnail.is_none() {
            info.thumbnail = entries.iter().find_map(|e| {
                e["thumbnails"].as_array().and_then(|t| t.last()).and_then(|t| t["url"].as_str()).map(str::to_string)
            });
        }
        info.video = [2160, 1440, 1080, 720, 480, 360]
            .iter()
            .map(|h| VideoQuality { height: *h, label: height_label(*h), ext: "mp4".into(), ..Default::default() })
            .collect();
        info.audio = audio_ladder(None, ffmpeg, None);
        return Ok(info);
    }

    let formats = v["formats"].as_array().cloned().unwrap_or_default();
    if !formats.is_empty() && formats.iter().all(|f| f["has_drm"].as_bool().unwrap_or(false)) {
        bail!("This media is DRM-protected. KuDownloader does not bypass DRM.");
    }
    let usable: Vec<&Value> = formats
        .iter()
        .filter(|f| !f["has_drm"].as_bool().unwrap_or(false))
        .filter(|f| f["ext"] != "mhtml" && f["format_note"] != "storyboard")
        .collect();
    let size_of = |f: &Value| f["filesize"].as_i64().or_else(|| f["filesize_approx"].as_i64());
    let is_none = |x: &Value| x.as_str().is_none_or(|c| c == "none");

    let best_audio = usable
        .iter()
        .filter(|f| is_none(&f["vcodec"]) && !is_none(&f["acodec"]))
        .max_by(|a, b| {
            f64_of(&a["abr"]).unwrap_or(0.0).partial_cmp(&f64_of(&b["abr"]).unwrap_or(0.0)).unwrap()
        })
        .copied();
    let audio_size = best_audio.and_then(size_of);

    let mut by_height: BTreeMap<u32, VideoQuality> = BTreeMap::new();
    for f in usable.iter().filter(|f| !is_none(&f["vcodec"])) {
        let Some(h) = f["height"].as_u64().map(|h| h as u32).filter(|h| *h > 0) else { continue };
        let progressive = !is_none(&f["acodec"]);
        if !ffmpeg && !progressive {
            // Without FFmpeg separate video and audio streams can't be merged.
            continue;
        }
        let size = size_of(f).map(|s| if progressive { s } else { s + audio_size.unwrap_or(0) });
        let hdr = f["dynamic_range"].as_str().is_some_and(|d| d != "SDR");
        let entry = by_height.entry(h).or_insert_with(|| VideoQuality {
            height: h,
            label: height_label(h),
            ..Default::default()
        });
        let better = entry.size.unwrap_or(0) < size.unwrap_or(0) || entry.ext.is_empty();
        if better {
            entry.size = size.or(entry.size);
            entry.ext = f["ext"].as_str().unwrap_or("").to_string();
            entry.vcodec = f["vcodec"].as_str().unwrap_or("").split('.').next().unwrap_or("").to_string();
            entry.fps = f64_of(&f["fps"]);
        }
        entry.hdr |= hdr;
    }
    info.video = by_height.into_values().rev().collect();
    let has_audio = best_audio.is_some() || usable.iter().any(|f| !is_none(&f["acodec"]));
    if has_audio {
        info.audio = audio_ladder(info.duration, ffmpeg, best_audio);
    }
    if info.video.is_empty() && info.audio.is_empty() {
        if let Some(f) = usable.first() {
            // Single-format sources (e.g. a direct file handled by the generic extractor).
            info.video.push(VideoQuality {
                height: f["height"].as_u64().unwrap_or(0) as u32,
                label: "Original".into(),
                ext: f["ext"].as_str().unwrap_or("").to_string(),
                size: size_of(f),
                ..Default::default()
            });
        } else if !info.is_live {
            bail!("No downloadable media was found at this address.");
        }
    }
    let langs = |k: &str| -> Vec<String> {
        v[k].as_object().map(|o| o.keys().filter(|k| *k != "live_chat").cloned().collect()).unwrap_or_default()
    };
    info.subtitles = langs("subtitles");
    info.auto_subtitles = langs("automatic_captions");
    Ok(info)
}

fn height_label(h: u32) -> String {
    match h {
        4320 => "4320p 8K".into(),
        2160 => "2160p 4K".into(),
        1440 => "1440p QHD".into(),
        1080 => "1080p Full HD".into(),
        720 => "720p HD".into(),
        h => format!("{h}p"),
    }
}

fn audio_ladder(duration: Option<f64>, ffmpeg: bool, best: Option<&Value>) -> Vec<AudioQuality> {
    if ffmpeg {
        [320u32, 256, 192, 128]
            .iter()
            .map(|kbps| AudioQuality {
                bitrate: *kbps,
                ext: "mp3".into(),
                acodec: "mp3".into(),
                size: duration.map(|d| (d * *kbps as f64 * 125.0) as i64),
            })
            .collect()
    } else {
        let b = best.map(|f| AudioQuality {
            bitrate: f64_of(&f["abr"]).unwrap_or(128.0) as u32,
            ext: f["ext"].as_str().unwrap_or("m4a").to_string(),
            acodec: f["acodec"].as_str().unwrap_or("").to_string(),
            size: f["filesize"].as_i64().or_else(|| f["filesize_approx"].as_i64()),
        });
        b.into_iter().collect()
    }
}

#[derive(Debug, Clone)]
pub enum YtEvent {
    Progress { done: i64, total: i64, speed: i64, eta: Option<i64>, playlist_index: Option<String> },
    Processing,
    File(PathBuf),
    Log(String),
}

pub struct YtJob {
    pub url: String,
    pub dir: PathBuf,
    pub media: MediaOptions,
    pub speed_limit: Option<u64>,
    /// Output file name (without extension) chosen by the user.
    pub name_stem: Option<String>,
}

pub fn build_args(job: &YtJob, ffmpeg: bool) -> Vec<String> {
    let m = &job.media;
    let mut a: Vec<String> = vec![
        "--newline".into(),
        "--progress".into(),
        "--no-mtime".into(),
        "--retries".into(),
        "10".into(),
        "--fragment-retries".into(),
        "10".into(),
        "-N".into(),
        "4".into(),
        "--progress-template".into(),
        "download:KUP|%(progress.status)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(info.playlist_index)s".into(),
        "--print".into(),
        "after_move:KUF|%(filepath)s".into(),
        "-P".into(),
        job.dir.to_string_lossy().into_owned(),
    ];
    let audio = m.mode == "audio";
    if let Some(fid) = m.format_id.as_deref().filter(|f| !f.is_empty()) {
        a.extend(["-f".into(), fid.into()]);
    } else if audio {
        if ffmpeg {
            let fmt = m.container.clone().unwrap_or_else(|| "mp3".into());
            a.extend(["-f".into(), "ba/b".into(), "-x".into(), "--audio-format".into(), fmt]);
            if let Some(b) = m.audio_bitrate {
                a.extend(["--audio-quality".into(), format!("{b}K")]);
            }
        } else {
            a.extend(["-f".into(), "ba[ext=m4a]/ba/b".into()]);
        }
    } else {
        let h = m.height.unwrap_or(0);
        if ffmpeg {
            a.extend(["-f".into(), "bv*+ba/b".into()]);
            let container = m.container.clone().unwrap_or_else(|| "mp4".into());
            let mut sort = Vec::new();
            if h > 0 {
                sort.push(format!("res:{h}"));
            }
            if container == "mp4" {
                sort.push("ext:mp4:m4a".into());
            }
            if !sort.is_empty() {
                a.extend(["-S".into(), sort.join(",")]);
            }
            a.extend(["--merge-output-format".into(), container]);
        } else if h > 0 {
            a.extend(["-f".into(), format!("b[height<={h}]/b")]);
        } else {
            a.extend(["-f".into(), "b".into()]);
        }
    }
    if m.subtitles {
        let langs = m.sub_langs.clone().filter(|l| !l.is_empty()).unwrap_or_else(|| "en.*,en".into());
        a.extend(["--write-subs".into(), "--write-auto-subs".into(), "--sub-langs".into(), langs]);
        if m.embed_subtitles && ffmpeg && !audio {
            a.push("--embed-subs".into());
        }
    }
    if m.embed_thumbnail && ffmpeg {
        a.push("--embed-thumbnail".into());
    } else if m.write_thumbnail {
        a.push("--write-thumbnail".into());
    }
    if let Some(l) = job.speed_limit.filter(|l| *l > 0) {
        a.extend(["--limit-rate".into(), l.to_string()]);
    }
    let stem = job
        .name_stem
        .as_deref()
        .map(crate::classify::sanitize_filename)
        .filter(|s| !s.is_empty() && !m.playlist)
        .map(|s| s.replace('%', "%%"));
    if m.playlist {
        a.push("--yes-playlist".into());
        if let Some(items) = m.playlist_items.as_deref().filter(|i| !i.is_empty()) {
            a.extend(["--playlist-items".into(), items.into()]);
        }
        a.extend([
            "-o".into(),
            "%(playlist_title,playlist|Playlist).120B/%(playlist_index|0)03d - %(title).150B.%(ext)s".into(),
        ]);
    } else {
        a.push("--no-playlist".into());
        a.extend(["-o".into(), format!("{}.%(ext)s", stem.unwrap_or_else(|| "%(title).180B".into()))]);
    }
    a.push("--".into());
    a.push(job.url.clone());
    a
}

fn num(s: &str) -> Option<f64> {
    (s != "NA" && s != "None").then(|| s.parse::<f64>().ok()).flatten()
}

/// Track multi-stream downloads (video + audio) as one monotonic progress.
#[derive(Default)]
struct Aggregate {
    base: i64,
    last_done: i64,
    last_total: i64,
}

impl Aggregate {
    fn update(&mut self, done: i64, total: i64) -> (i64, i64) {
        if done + 1024 * 1024 < self.last_done || (self.last_total > 0 && total > 0 && total != self.last_total && done < self.last_done) {
            // A new stream started.
            self.base += self.last_total.max(self.last_done);
        }
        self.last_done = done;
        self.last_total = total;
        (self.base + done, self.base + total.max(done))
    }
}

/// Run a yt-dlp download until completion or cancellation.
pub async fn run(
    env: &YtEnv,
    job: &YtJob,
    mut cancel: oneshot::Receiver<()>,
    mut on_event: impl FnMut(YtEvent) + Send,
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(&job.dir).with_context(|| format!("creating {}", job.dir.display()))?;
    let mut cmd = env.base_command();
    cmd.args(build_args(job, env.ffmpeg.is_some()));
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().context("starting yt-dlp")?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr"))?;
    let (err_tx, mut err_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            let _ = err_tx.send(l);
        }
    });
    let mut lines = BufReader::new(stdout).lines();
    let mut files = Vec::new();
    let mut agg = Aggregate::default();
    let mut errors: Vec<String> = Vec::new();
    let mut stdout_open = true;
    let mut stderr_open = true;
    loop {
        tokio::select! {
            _ = &mut cancel => {
                kill_tree(pid, &mut child).await;
                bail!("cancelled");
            }
            line = lines.next_line(), if stdout_open => match line {
                Ok(Some(l)) => {
                    if let Some(rest) = l.strip_prefix("KUP|") {
                        let p: Vec<&str> = rest.split('|').collect();
                        if p.len() >= 7 {
                            if p[0] == "finished" {
                                on_event(YtEvent::Processing);
                                continue;
                            }
                            let done = num(p[1]).unwrap_or(0.0) as i64;
                            let total = num(p[2]).or_else(|| num(p[3])).unwrap_or(0.0) as i64;
                            let (d, t) = agg.update(done, total);
                            on_event(YtEvent::Progress {
                                done: d,
                                total: t,
                                speed: num(p[4]).unwrap_or(0.0) as i64,
                                eta: num(p[5]).map(|e| e as i64),
                                playlist_index: (p[6] != "NA").then(|| p[6].to_string()),
                            });
                        }
                    } else if let Some(f) = l.strip_prefix("KUF|") {
                        let pb = PathBuf::from(f.trim());
                        on_event(YtEvent::File(pb.clone()));
                        files.push(pb);
                        agg = Aggregate::default();
                    } else if !l.trim().is_empty() {
                        on_event(YtEvent::Log(l));
                    }
                }
                _ => stdout_open = false,
            },
            line = err_rx.recv(), if stderr_open => match line {
                Some(l) => {
                    if let Some(e) = l.strip_prefix("ERROR:") {
                        errors.push(e.trim().to_string());
                    }
                    if !l.trim().is_empty() {
                        on_event(YtEvent::Log(l));
                    }
                }
                None => stderr_open = false,
            },
            status = child.wait(), if !stdout_open && !stderr_open => {
                let status = status?;
                if status.success() {
                    return Ok(files);
                }
                let e = errors.last().cloned().unwrap_or_else(|| format!("yt-dlp exited with {status}"));
                bail!("{}", friendly_error(&e));
            }
        }
    }
}

async fn kill_tree(pid: Option<u32>, child: &mut tokio::process::Child) {
    if let Some(pid) = pid {
        #[cfg(windows)]
        {
            let mut k = Command::new("taskkill");
            k.args(["/PID", &pid.to_string(), "/T", "/F"]).stdout(Stdio::null()).stderr(Stdio::null());
            hide_window(&mut k);
            let _ = k.status().await;
        }
        #[cfg(unix)]
        {
            let _ = Command::new("kill").args(["-TERM", &format!("-{pid}")]).status().await;
        }
    }
    let _ = child.kill().await;
}

/// Self-update a standalone yt-dlp binary.
pub async fn self_update(bin: &Path) -> Result<String> {
    let mut cmd = Command::new(bin);
    cmd.args(["-U", "--no-colors"]).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    hide_window(&mut cmd);
    let out = tokio::time::timeout(Duration::from_secs(180), cmd.output()).await??;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let summary = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("yt-dlp update finished")
        .to_string();
    if !out.status.success() {
        bail!("{summary}");
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_formats_into_ladder() {
        let v = json!({
            "title": "Clip", "duration": 100.0, "extractor_key": "Youtube",
            "formats": [
                {"format_id": "140", "ext": "m4a", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 129.0, "filesize": 1_600_000},
                {"format_id": "137", "ext": "mp4", "vcodec": "avc1.640028", "acodec": "none", "height": 1080, "filesize": 50_000_000},
                {"format_id": "248", "ext": "webm", "vcodec": "vp9", "acodec": "none", "height": 1080, "filesize": 40_000_000},
                {"format_id": "18", "ext": "mp4", "vcodec": "avc1", "acodec": "mp4a", "height": 360, "filesize": 9_000_000},
                {"format_id": "sb0", "ext": "mhtml", "vcodec": "none", "acodec": "none", "format_note": "storyboard"}
            ],
            "subtitles": {"en": [], "live_chat": []}
        });
        let info = parse_info(&v, "u", true).unwrap();
        assert_eq!(info.video.len(), 2);
        assert_eq!(info.video[0].height, 1080);
        assert_eq!(info.video[0].size, Some(51_600_000));
        assert_eq!(info.audio[0].bitrate, 320);
        assert_eq!(info.audio[0].size, Some(4_000_000));
        assert_eq!(info.subtitles, vec!["en".to_string()]);
        let no_ff = parse_info(&v, "u", false).unwrap();
        assert_eq!(no_ff.video.len(), 1, "only progressive formats without ffmpeg");
    }

    #[test]
    fn rejects_drm() {
        let v = json!({"title": "x", "formats": [{"format_id": "a", "has_drm": true, "vcodec": "avc1", "height": 720}]});
        let e = parse_info(&v, "u", true).unwrap_err().to_string();
        assert!(e.contains("DRM"));
    }

    #[test]
    fn builds_video_args() {
        let job = YtJob {
            url: "https://example.com/v".into(),
            dir: PathBuf::from("out"),
            media: MediaOptions { mode: "video".into(), height: Some(720), container: Some("mp4".into()), ..Default::default() },
            speed_limit: Some(1000),
            name_stem: Some("My: video".into()),
        };
        let a = build_args(&job, true);
        assert!(a.windows(2).any(|w| w[0] == "-S" && w[1] == "res:720,ext:mp4:m4a"));
        assert!(a.windows(2).any(|w| w[0] == "-o" && w[1] == "My_ video.%(ext)s"));
        assert_eq!(a.last().unwrap(), "https://example.com/v");
    }

    #[test]
    fn aggregate_is_monotonic_across_streams() {
        let mut g = Aggregate::default();
        assert_eq!(g.update(50, 100), (50, 100));
        assert_eq!(g.update(100, 100), (100, 100));
        assert_eq!(g.update(0, 20), (100, 120));
        assert_eq!(g.update(20, 20), (120, 120));
    }
}
