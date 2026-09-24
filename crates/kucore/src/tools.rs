//! On-demand media tools: yt-dlp and ffmpeg are downloaded from their
//! official GitHub releases when first needed (instead of being bundled),
//! verified against the publisher's SHA-256 list, and installed into
//! `<data>/bin` where `paths::find_binary` looks for them.

use anyhow::{anyhow, bail, Context, Result};
use ku_proto::paths;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    YtDlp,
    Ffmpeg,
}

impl Tool {
    pub fn parse(s: &str) -> Option<Tool> {
        match s {
            "yt-dlp" | "ytdlp" => Some(Tool::YtDlp),
            "ffmpeg" => Some(Tool::Ffmpeg),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tool::YtDlp => "yt-dlp",
            Tool::Ffmpeg => "ffmpeg",
        }
    }
}

struct Source {
    asset: &'static str,
    url: String,
    sums_url: String,
    archive: bool,
}

const YTDLP: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const FFMPEG: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest";

fn source(tool: Tool) -> Result<Source> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let s = |base: &str, asset: &'static str, sums: &str, archive: bool| Source { asset, url: format!("{base}/{asset}"), sums_url: format!("{base}/{sums}"), archive };
    Ok(match tool {
        Tool::YtDlp => {
            let asset = match (os, arch) {
                ("windows", _) => "yt-dlp.exe",
                ("linux", "aarch64") => "yt-dlp_linux_aarch64",
                ("linux", _) => "yt-dlp_linux",
                ("macos", _) => "yt-dlp_macos",
                _ => bail!("No yt-dlp build for this system; install it with your package manager."),
            };
            s(YTDLP, asset, "SHA2-256SUMS", false)
        }
        Tool::Ffmpeg => {
            // "shared" builds are ~80 MB instead of ~185 MB and include ffprobe.
            let asset = match (os, arch) {
                ("windows", "aarch64") => "ffmpeg-master-latest-winarm64-gpl-shared.zip",
                ("windows", _) => "ffmpeg-master-latest-win64-gpl-shared.zip",
                ("linux", "aarch64") => "ffmpeg-master-latest-linuxarm64-gpl-shared.tar.xz",
                ("linux", _) => "ffmpeg-master-latest-linux64-gpl-shared.tar.xz",
                _ => bail!("Install ffmpeg with Homebrew: brew install ffmpeg"),
            };
            s(FFMPEG, asset, "checksums.sha256", true)
        }
    })
}

/// Expected SHA-256 for `asset` from a `sha256sum`-style list.
pub fn expected_sha256(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let hash = it.next()?;
        let name = it.next()?.trim_start_matches('*');
        (name == asset && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())).then(|| hash.to_ascii_lowercase())
    })
}

/// Download, verify and install `tool`. `progress(done, total)` is called as
/// bytes arrive. Returns the installed executable. Network failures are
/// retried twice (DNS hiccups, dropped connections); checksum errors are not.
pub async fn install(tool: Tool, client: &reqwest::Client, mut progress: impl FnMut(u64, Option<u64>) + Send) -> Result<PathBuf> {
    let mut last = None;
    for attempt in 0..3u64 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(2 * attempt)).await;
        }
        match install_once(tool, client, &mut progress).await {
            Ok(p) => return Ok(p),
            Err(e) if e.downcast_ref::<reqwest::Error>().is_some() => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| anyhow!("download failed")))
}

async fn install_once(tool: Tool, client: &reqwest::Client, progress: &mut (impl FnMut(u64, Option<u64>) + Send)) -> Result<PathBuf> {
    let src = source(tool)?;
    let dir = paths::tools_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;

    let sums = client.get(&src.sums_url).send().await?.error_for_status()?.text().await?;
    let want = expected_sha256(&sums, src.asset).ok_or_else(|| anyhow!("The publisher's checksum list has no entry for {}", src.asset))?;

    let part = dir.join(format!(".{}.part", src.asset));
    let mut resp = client.get(&src.url).send().await?.error_for_status()?;
    let total = resp.content_length();
    let mut file = std::fs::File::create(&part)?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    while let Some(chunk) = resp.chunk().await? {
        hasher.update(&chunk);
        file.write_all(&chunk)?;
        done += chunk.len() as u64;
        progress(done, total);
    }
    file.flush()?;
    drop(file);
    let got = format!("{:x}", hasher.finalize());
    if got != want {
        let _ = std::fs::remove_file(&part);
        bail!("Checksum mismatch for {} — the download was discarded.", src.asset);
    }

    let exe = paths::exe_name(tool.name());
    let installed = if src.archive {
        let dest = dir.join(tool.name());
        let staging = dir.join(format!("{}.new", tool.name()));
        let _ = std::fs::remove_dir_all(&staging);
        let extracted = if src.asset.ends_with(".zip") { extract_bin(&part, &staging) } else { extract_tar_xz(&part, &staging) };
        let _ = std::fs::remove_file(&part);
        extracted?;
        if !staging.join(&exe).is_file() && !staging.join("bin").join(&exe).is_file() {
            let _ = std::fs::remove_dir_all(&staging);
            bail!("{} did not contain {exe}", src.asset);
        }
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&staging, &dest)?;
        if dest.join(&exe).is_file() { dest.join(exe) } else { dest.join("bin").join(exe) }
    } else {
        let dest = dir.join(&exe);
        let _ = std::fs::remove_file(&dest);
        std::fs::rename(&part, &dest)?;
        dest
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(installed)
}

/// Library folder of an on-demand Linux ffmpeg (`<tools>/ffmpeg/lib`), for
/// `LD_LIBRARY_PATH` when yt-dlp runs it.
pub fn ffmpeg_lib_dir() -> Option<PathBuf> {
    let d = paths::tools_dir().join("ffmpeg").join("lib");
    d.is_dir().then_some(d)
}

/// Linux release tarball: keep `bin/` and `lib/` (the shared build's
/// binaries load their libraries from `../lib`).
#[cfg(target_os = "linux")]
fn extract_tar_xz(path: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let mut ar = tar::Archive::new(xz2::read::XzDecoder::new(std::fs::File::open(path)?));
    for entry in ar.entries()? {
        let mut entry = entry?;
        let p = entry.path()?.into_owned();
        // <root>/{bin,lib}/… → dest/{bin,lib}/…
        let mut parts = p.components().skip(1);
        let Some(top) = parts.next() else { continue };
        let top = top.as_os_str().to_string_lossy().to_string();
        if top != "bin" && top != "lib" {
            continue;
        }
        let rest: PathBuf = parts.collect();
        if rest.as_os_str().is_empty() || rest.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            continue;
        }
        let out = dest.join(&top).join(rest);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&out)?;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn extract_tar_xz(_: &Path, _: &Path) -> Result<()> {
    bail!("tar.xz archives are only used on Linux")
}

/// Flatten `<root>/bin/*` of a release zip into `dest`.
fn extract_bin(zip_path: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let Some(path) = entry.enclosed_name() else { continue };
        let parent_is_bin = path.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "bin");
        let Some(name) = path.file_name().filter(|_| parent_is_bin) else { continue };
        let mut out = std::fs::File::create(dest.join(name))?;
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_lines() {
        let sums = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  yt-dlp.exe\n\
                    AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA *ffmpeg.zip\n";
        assert_eq!(expected_sha256(sums, "yt-dlp.exe").unwrap(), "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
        assert_eq!(expected_sha256(sums, "ffmpeg.zip").unwrap(), "a".repeat(64));
        assert!(expected_sha256(sums, "yt-dlp").is_none());
    }

    #[test]
    fn extracts_only_bin() {
        let dir = std::env::temp_dir().join(format!("ku-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let zp = dir.join("a.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zp).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            for (n, body) in [("ffmpeg-x/bin/ffmpeg.exe", "exe"), ("ffmpeg-x/bin/avcodec.dll", "dll"), ("ffmpeg-x/doc/readme.txt", "doc"), ("ffmpeg-x/LICENSE", "lic")] {
                w.start_file(n, o).unwrap();
                w.write_all(body.as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        let out = dir.join("out");
        extract_bin(&zp, &out).unwrap();
        let mut names: Vec<_> = std::fs::read_dir(&out).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names, ["avcodec.dll", "ffmpeg.exe"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod live {
    /// Real download from the official releases:
    /// `cargo test -p kucore --lib tools::live -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn install_both() {
        let dir = std::env::temp_dir().join(format!("ku-tools-live-{}", std::process::id()));
        std::env::set_var("KU_DATA_DIR", &dir);
        let client = reqwest::Client::new();
        for t in [super::Tool::YtDlp, super::Tool::Ffmpeg] {
            let p = super::install(t, &client, |_, _| {}).await.unwrap();
            let out = std::process::Command::new(&p).arg(if t == super::Tool::YtDlp { "--version" } else { "-version" }).output().unwrap();
            let first = String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or_default().to_string();
            println!("{} -> {} :: {first}", t.name(), p.display());
            assert!(out.status.success());
            assert_eq!(ku_proto::paths::find_binary(t.name(), None).as_deref(), Some(p.as_path()));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
