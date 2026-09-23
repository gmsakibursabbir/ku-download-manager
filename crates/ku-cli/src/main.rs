//! `ku` — control KuDownloader from the terminal.

use clap::{Parser, Subcommand};
use ku_proto::client::{Client, ClientError};
use ku_proto::{launcher, AddRequest, Download, DownloadOptions, MediaOptions, MediaRequest, Stats, Status};
use serde_json::{json, Value};
use std::process::ExitCode;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "ku", version, about = "Control KuDownloader from the command line")]
struct Cli {
    /// Do not start KuDownloader automatically if it is not running.
    #[arg(long, global = true)]
    no_launch: bool,
    /// Print machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Add one or more downloads (HTTP, FTP, SFTP, magnet, .torrent, media pages).
    Add {
        urls: Vec<String>,
        /// Destination folder.
        #[arg(short, long)]
        dir: Option<String>,
        /// Output file name (single URL only).
        #[arg(short, long)]
        output: Option<String>,
        /// Connections: smart, 1-32.
        #[arg(short, long)]
        connections: Option<String>,
        /// Add to a queue instead of starting now.
        #[arg(short, long)]
        queue: Option<String>,
        /// Add paused.
        #[arg(long)]
        paused: bool,
        #[arg(long)]
        referer: Option<String>,
        #[arg(long)]
        user_agent: Option<String>,
        /// Extra header "Name: value" (repeatable).
        #[arg(short = 'H', long = "header")]
        headers: Vec<String>,
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        password: Option<String>,
        /// Expected checksum, e.g. sha-256=<hex>.
        #[arg(long)]
        checksum: Option<String>,
        /// Additional mirror URL for the same file (repeatable).
        #[arg(long = "mirror")]
        mirrors: Vec<String>,
        /// Speed limit, e.g. 2M or 500K.
        #[arg(long)]
        limit: Option<String>,
        /// Read URLs from a file (one per line), "-" for stdin.
        #[arg(short = 'i', long)]
        input: Option<String>,
    },
    /// Download media with yt-dlp (video or audio).
    Media {
        url: String,
        /// Maximum height, e.g. 1080.
        #[arg(long)]
        quality: Option<u32>,
        /// Audio only (mp3 by default).
        #[arg(long)]
        audio: bool,
        /// Audio bitrate in kbps.
        #[arg(long, default_value_t = 320)]
        bitrate: u32,
        /// Container / audio format (mp4, mkv, webm, mp3, m4a, opus).
        #[arg(long)]
        format: Option<String>,
        #[arg(long)]
        subs: bool,
        #[arg(long)]
        playlist: bool,
        #[arg(short, long)]
        dir: Option<String>,
    },
    /// Show media information and available qualities.
    Info {
        url: String,
        #[arg(long)]
        playlist: bool,
    },
    /// List downloads.
    #[command(alias = "ls")]
    List {
        /// Filter by status (downloading, queued, paused, completed, error, seeding).
        #[arg(short, long)]
        status: Option<String>,
    },
    /// Pause downloads ("all" for everything).
    Pause { ids: Vec<String> },
    /// Resume downloads ("all" for everything).
    Resume { ids: Vec<String> },
    /// Remove downloads from the list.
    #[command(alias = "rm")]
    Remove {
        ids: Vec<String>,
        /// Also delete downloaded files.
        #[arg(long)]
        delete_files: bool,
    },
    /// Wait until the given downloads finish; exit code 1 if any failed.
    Wait { ids: Vec<String> },
    /// Overall status.
    Status,
    /// Start or stop a queue.
    Queue {
        /// start or stop
        action: String,
        #[arg(default_value = "main")]
        queue: String,
    },
    /// Bring the KuDownloader window to the front.
    Show,
}

fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_ascii_uppercase();
    let (num, mul) = match s.chars().last()? {
        'K' => (&s[..s.len() - 1], 1024),
        'M' => (&s[..s.len() - 1], 1024 * 1024),
        'G' => (&s[..s.len() - 1], 1024 * 1024 * 1024),
        _ => (s.as_str(), 1),
    };
    num.trim().parse::<f64>().ok().map(|n| (n * mul as f64) as u64)
}

fn human(n: i64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else { format!("{v:.1} {}", units[i]) }
}

fn client(no_launch: bool) -> Result<Client, String> {
    let r = if no_launch { Client::discover() } else { launcher::ensure_running(Duration::from_secs(20)) };
    r.map_err(|e| match e {
        ClientError::NotRunning => "KuDownloader is not running. Start it or omit --no-launch.".into(),
        e => e.to_string(),
    })
}

fn resolve_ids(c: &Client, ids: &[String]) -> Result<Vec<String>, String> {
    let all: Vec<Download> = c.get("/v1/downloads").map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for id in ids {
        let m: Vec<&Download> = all.iter().filter(|d| d.id.starts_with(id.as_str())).collect();
        match m.len() {
            0 => return Err(format!("no download matches \"{id}\"")),
            1 => out.push(m[0].id.clone()),
            _ => return Err(format!("\"{id}\" is ambiguous; use more characters")),
        }
    }
    Ok(out)
}

fn run(cli: Cli) -> Result<(), String> {
    let c = client(cli.no_launch)?;
    let err = |e: ClientError| e.to_string();
    match cli.cmd {
        Cmd::Add { mut urls, dir, output, connections, queue, paused, referer, user_agent, headers, user, password, checksum, mirrors, limit, input } => {
            if let Some(path) = input {
                let text = if path == "-" {
                    let mut s = String::new();
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).map_err(|e| e.to_string())?;
                    s
                } else {
                    std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?
                };
                urls.extend(text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from));
            }
            if urls.is_empty() {
                return Err("no URLs given".into());
            }
            let connections = match connections.as_deref() {
                None => None,
                Some("smart") | Some("auto") => Some(0),
                Some(n) => Some(n.parse::<u32>().map_err(|_| "connections must be \"smart\" or 1-32")?.clamp(1, 32)),
            };
            let template = AddRequest {
                url: String::new(),
                mirrors,
                dir,
                connections,
                queue_id: queue,
                start: Some(!paused),
                source: Some("cli".into()),
                options: DownloadOptions {
                    headers,
                    referer,
                    user_agent,
                    username: user,
                    password,
                    checksum,
                    speed_limit: limit.as_deref().and_then(parse_size),
                    ..Default::default()
                },
                ..Default::default()
            };
            if urls.len() == 1 {
                let req = AddRequest { url: urls.remove(0), filename: output, ..template };
                let v: Value = c.post("/v1/downloads", &req).map_err(err)?;
                let d: Download = serde_json::from_value(v["download"].clone()).map_err(|e| e.to_string())?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&d).unwrap());
                } else {
                    println!("Added {} → {} [{}]", &d.id[..8], d.name, d.dir);
                }
            } else {
                let v: Value = c.post("/v1/downloads/batch", &json!({"urls": urls, "template": template})).map_err(err)?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&v).unwrap());
                } else {
                    for d in v["added"].as_array().into_iter().flatten() {
                        println!("Added {} → {}", &d["id"].as_str().unwrap_or("")[..8], d["name"].as_str().unwrap_or(""));
                    }
                    for f in v["failed"].as_array().into_iter().flatten() {
                        eprintln!("Failed {}: {}", f["url"].as_str().unwrap_or(""), f["error"].as_str().unwrap_or(""));
                    }
                }
            }
        }
        Cmd::Media { url, quality, audio, bitrate, format, subs, playlist, dir } => {
            let req = MediaRequest {
                url,
                dir,
                media: MediaOptions {
                    mode: if audio { "audio".into() } else { "video".into() },
                    height: quality,
                    audio_bitrate: audio.then_some(bitrate),
                    container: format,
                    subtitles: subs,
                    embed_subtitles: subs,
                    playlist,
                    ..Default::default()
                },
                source: Some("cli".into()),
                ..Default::default()
            };
            let d: Download = c.post("/v1/media/download", &req).map_err(err)?;
            println!("Added {} → {}", &d.id[..8], d.name);
        }
        Cmd::Info { url, playlist } => {
            let v: Value = c.with_timeout(Duration::from_secs(150)).post("/v1/media/analyze", &json!({"url": url, "playlist": playlist})).map_err(err)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
                return Ok(());
            }
            println!("{}", v["title"].as_str().unwrap_or(""));
            if let Some(u) = v["uploader"].as_str() {
                println!("by {u}");
            }
            if v["isPlaylist"].as_bool() == Some(true) {
                println!("Playlist · {} items", v["playlistCount"]);
                for e in v["entries"].as_array().into_iter().flatten().take(50) {
                    println!("  {:>3}. {}", e["index"], e["title"].as_str().unwrap_or(""));
                }
            }
            println!("\nVideo");
            for q in v["video"].as_array().into_iter().flatten() {
                let size = q["size"].as_i64().map(human).unwrap_or_else(|| "—".into());
                println!("  {:<16} {:<5} {:>10}", q["label"].as_str().unwrap_or(""), q["ext"].as_str().unwrap_or(""), size);
            }
            println!("\nAudio");
            for q in v["audio"].as_array().into_iter().flatten() {
                let size = q["size"].as_i64().map(human).unwrap_or_else(|| "—".into());
                println!("  {:>4} kbps {:<5} {:>10}", q["bitrate"], q["ext"].as_str().unwrap_or(""), size);
            }
        }
        Cmd::List { status } => {
            let mut all: Vec<Download> = c.get("/v1/downloads").map_err(err)?;
            if let Some(s) = status {
                all.retain(|d| d.status.as_str() == s);
            }
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&all).unwrap());
                return Ok(());
            }
            if all.is_empty() {
                println!("No downloads.");
            }
            for d in all {
                let pct = if d.total > 0 { format!("{:>5.1}%", d.done as f64 * 100.0 / d.total as f64) } else { "    —".into() };
                let speed = if d.speed > 0 { format!("{}/s", human(d.speed)) } else { String::new() };
                println!("{}  {:<11} {}  {:>10}  {:>11}  {}", &d.id[..8], d.status.as_str(), pct, human(d.total), speed, d.name);
            }
        }
        Cmd::Pause { ids } | Cmd::Resume { ids } if ids.is_empty() => return Err("give download ids or \"all\"".into()),
        Cmd::Pause { ids } => pause_resume(&c, &ids, "pause")?,
        Cmd::Resume { ids } => pause_resume(&c, &ids, "resume")?,
        Cmd::Remove { ids, delete_files } => {
            for id in resolve_ids(&c, &ids)? {
                let _: Value = c.delete(&format!("/v1/downloads/{id}?deleteFiles={delete_files}")).map_err(err)?;
                println!("Removed {}", &id[..8]);
            }
        }
        Cmd::Wait { ids } => {
            let ids = resolve_ids(&c, &ids)?;
            let mut failed = false;
            loop {
                let mut pending = 0;
                for id in &ids {
                    let d: Download = c.get(&format!("/v1/downloads/{id}")).map_err(err)?;
                    match d.status {
                        Status::Completed | Status::Seeding => {}
                        Status::Error => failed = true,
                        _ => pending += 1,
                    }
                }
                if pending == 0 {
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
            if failed {
                return Err("one or more downloads failed".into());
            }
        }
        Cmd::Status => {
            let s: Stats = c.get("/v1/stats").map_err(err)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&s).unwrap());
            } else {
                println!(
                    "↓ {}/s  ↑ {}/s  · {} active, {} queued, {} paused, {} completed, {} failed",
                    human(s.download_speed), human(s.upload_speed), s.active, s.queued, s.paused, s.completed, s.errors
                );
            }
        }
        Cmd::Queue { action, queue } => {
            if action != "start" && action != "stop" {
                return Err("queue action must be start or stop".into());
            }
            let _: Value = c.post(&format!("/v1/queues/{queue}/{action}"), &json!({})).map_err(err)?;
            println!("Queue {queue}: {action}ed");
        }
        Cmd::Show => {
            let _: Value = c.post("/v1/ui/show", &json!({})).map_err(err)?;
        }
    }
    Ok(())
}

fn pause_resume(c: &Client, ids: &[String], action: &str) -> Result<(), String> {
    if ids.iter().any(|i| i == "all") {
        let _: Value = c.post(&format!("/v1/downloads-all/{action}"), &json!({})).map_err(|e| e.to_string())?;
        return Ok(());
    }
    for id in resolve_ids(c, ids)? {
        let _: Value = c.post(&format!("/v1/downloads/{id}/{action}"), &json!({})).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ku: {e}");
            ExitCode::FAILURE
        }
    }
}
