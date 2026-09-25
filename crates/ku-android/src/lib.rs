//! KuDownloader for Android: the Kotlin interface talks to the same engine as
//! the desktop app through a small JSON bridge.
//!
//! ```text
//! Kotlin ──call(method, json)──▶ dispatch ──▶ KuCore / KuAirSend
//!        ◀──nextEvents()──────── CoreEvent batches
//!        ──shouldBlock(url)────▶ ad blocker (EasyList, EasyPrivacy)
//! ```
//!
//! Every call returns `{"ok": value}` or `{"error": "message"}`.

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine as _;
use ku_proto::{paths, AddRequest, BrowserCookie, DownloadOptions, Engine, Kind, MediaRequest, ProbeInfo};
use kucore::airsend::AirSend;
use kucore::db::Db;
use kucore::{Core, CoreEvent, Queue, Schedule, Settings};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::broadcast::{self, error::RecvError};

pub mod adblock_engine;
#[path = "../../../app/src-tauri/src/i18n.rs"]
pub mod i18n;
mod jni_api;

struct App {
    rt: Runtime,
    core: Arc<Core>,
    air: Option<Arc<AirSend>>,
    events: Mutex<broadcast::Receiver<CoreEvent>>,
}

static APP: OnceLock<App> = OnceLock::new();

fn app() -> Result<&'static App> {
    APP.get().ok_or_else(|| anyhow!("KuDownloader is still starting."))
}

/// Start the engine once per process. `config` carries the folders and the
/// environment the bundled tools need (Python, FFmpeg, aria2 libraries).
pub fn init(config: &str) -> Result<()> {
    if APP.get().is_some() {
        return Ok(());
    }
    let cfg: Value = serde_json::from_str(config).context("bad configuration")?;
    if let Some(env) = cfg["env"].as_object() {
        for (k, v) in env {
            if let Some(v) = v.as_str() {
                // Single-threaded here: nothing else runs before the engine starts.
                std::env::set_var(k, v);
            }
        }
    }
    let data = paths::data_dir();
    std::fs::create_dir_all(&data).with_context(|| format!("creating {}", data.display()))?;
    logging(&data);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(cfg["threads"].as_u64().unwrap_or(4).clamp(2, 8) as usize)
        .thread_name("kucore")
        .enable_all()
        .build()?;
    let db = Db::open(&paths::db_path()).context("opening the database")?;
    let core = Core::open(db).context("starting KuCore")?;
    first_run_defaults(&rt, &core, &cfg);
    let events = Mutex::new(core.subscribe());
    rt.block_on(async { core.start() });
    let air = AirSend::new(core.clone()).map_err(|e| tracing::error!("KuAirSend unavailable: {e:#}")).ok();
    if let Some(a) = air.clone().filter(|_| core.settings().airsend_enabled) {
        rt.spawn(async move {
            if let Err(e) = a.start().await {
                tracing::warn!("KuAirSend: {e:#}");
            }
        });
    }
    let data_dir = data.clone();
    rt.spawn(async move { adblock_engine::load_cached(&data_dir).await });
    APP.set(App { rt, core, air, events }).map_err(|_| anyhow!("already started"))?;
    Ok(())
}

fn logging(data: &std::path::Path) {
    let path = data.join("kudownloader.log");
    // Keep the log small: start over once it passes 2 MB.
    if std::fs::metadata(&path).map(|m| m.len() > 2 << 20).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
            .with_ansi(false)
            .with_writer(Mutex::new(file))
            .try_init();
    }
}

/// Phone defaults on the first start: the native HTTP engine (no helper
/// process), fewer parallel downloads, the phone's name for KuAirSend and the
/// bundled tools' locations.
fn first_run_defaults(rt: &Runtime, core: &Arc<Core>, cfg: &Value) {
    let mut s = core.settings();
    let before = s.clone();
    let tools = &cfg["tools"];
    for (field, key) in [(&mut s.ytdlp_path, "ytdlp"), (&mut s.ffmpeg_path, "ffmpeg"), (&mut s.aria2_path, "aria2")] {
        // The bundled tools live in the app's library folder, which moves on
        // every update: always point at the current one.
        if let Some(p) = tools[key].as_str() {
            let custom = !field.is_empty() && !field.contains("/youtubedl-android/") && !field.contains("/lib/");
            if !custom {
                *field = p.to_string();
            }
        }
    }
    if !s.onboarded {
        s.http_engine = "kuhttp".into();
        s.max_concurrent = 3;
        s.minimize_to_tray = false;
        s.start_with_os = false;
        s.show_progress_window = false;
        s.api_enabled = false;
        s.virus_scan = "off".into();
        s.clipboard_monitor = true;
        if let Some(name) = cfg["deviceName"].as_str().filter(|n| !n.trim().is_empty()) {
            if s.airsend_name.is_empty() {
                s.airsend_name = name.trim().to_string();
            }
        }
    }
    if s != before {
        if let Err(e) = rt.block_on(core.save_settings(s)) {
            tracing::warn!("saving phone defaults: {e:#}");
        }
    }
}

/// Up to `timeout_ms` for the next events; returns a JSON array (maybe empty).
pub fn next_events(timeout_ms: u64) -> String {
    let Ok(a) = app() else {
        std::thread::sleep(Duration::from_millis(timeout_ms.min(500)));
        return "[]".into();
    };
    let mut rx = a.events.lock().unwrap_or_else(|p| p.into_inner());
    let mut out: Vec<Value> = Vec::new();
    let first = a.rt.block_on(async { tokio::time::timeout(Duration::from_millis(timeout_ms), rx.recv()).await });
    let push = |out: &mut Vec<Value>, r: Result<CoreEvent, RecvError>| -> bool {
        match r {
            Ok(e) => {
                out.push(serde_json::to_value(&e).unwrap_or(Value::Null));
                true
            }
            // The interface fell behind: it reloads everything.
            Err(RecvError::Lagged(_)) => {
                out.push(json!({"type": "resync"}));
                true
            }
            Err(RecvError::Closed) => false,
        }
    };
    if let Ok(r) = first {
        if push(&mut out, r) {
            while out.len() < 256 {
                match rx.try_recv() {
                    Ok(e) => {
                        push(&mut out, Ok(e));
                    }
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        push(&mut out, Err(RecvError::Lagged(n)));
                    }
                    Err(_) => break,
                }
            }
        }
    }
    Value::Array(out).to_string()
}

pub fn call(method: &str, args: &str) -> String {
    let args: Value = if args.trim().is_empty() { Value::Null } else { serde_json::from_str(args).unwrap_or(Value::Null) };
    match app().and_then(|a| dispatch(a, method, &args)) {
        Ok(v) => json!({ "ok": v }).to_string(),
        Err(e) => json!({ "error": format!("{e:#}") }).to_string(),
    }
}

fn arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T> {
    serde_json::from_value(args.get(key).cloned().unwrap_or(Value::Null)).with_context(|| format!("argument {key}"))
}

fn opt<T: DeserializeOwned>(args: &Value, key: &str) -> Option<T> {
    args.get(key).filter(|v| !v.is_null()).and_then(|v| serde_json::from_value(v.clone()).ok())
}

fn to<T: serde::Serialize>(v: T) -> Result<Value> {
    Ok(serde_json::to_value(v)?)
}

fn air(a: &App) -> Result<&Arc<AirSend>> {
    a.air.as_ref().ok_or_else(|| anyhow!("KuAirSend is unavailable on this device."))
}

fn dispatch(a: &App, method: &str, args: &Value) -> Result<Value> {
    let core = &a.core;
    let rt = &a.rt;
    match method {
        "appInfo" => Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "dataDir": paths::data_dir().display().to_string(),
            "platform": std::env::consts::OS,
            "defaultDownloadDir": paths::default_download_dir().display().to_string(),
        })),
        "listDownloads" => to(core.list()),
        "getDownload" => to(core.get(&arg::<String>(args, "id")?)),
        "stats" => to(core.stats()),
        "addDownload" => to(rt.block_on(core.add(arg::<AddRequest>(args, "req")?))?),
        "addBatch" => {
            let (ok, failed) = rt.block_on(core.add_batch(arg(args, "urls")?, arg(args, "template")?));
            Ok(json!({
                "added": ok,
                "failed": failed.into_iter().map(|(u, e)| json!({"url": u, "error": e})).collect::<Vec<_>>(),
            }))
        }
        "probeUrl" => to(rt.block_on(probe(core, arg(args, "url")?, opt(args, "options").unwrap_or_default()))?),
        "checkDuplicate" => Ok(core.check_duplicate(&arg::<String>(args, "url")?, opt::<String>(args, "dir").as_deref(), opt::<String>(args, "filename").as_deref())),
        "pause" => to(rt.block_on(core.pause(&arg::<Vec<String>>(args, "ids")?))?),
        "resume" => to(rt.block_on(core.resume(&arg::<Vec<String>>(args, "ids")?))?),
        "redownload" => to(rt.block_on(core.redownload(&arg::<Vec<String>>(args, "ids")?))?),
        "remove" => to(rt.block_on(core.remove(&arg::<Vec<String>>(args, "ids")?, opt(args, "deleteFiles").unwrap_or(false)))?),
        "pauseAll" => to(rt.block_on(core.pause_all())?),
        "resumeAll" => to(rt.block_on(core.resume_all())?),
        "clearFinished" => to(core.clear_finished()?),
        "editDownload" => to(rt.block_on(core.edit(&arg::<String>(args, "id")?, arg(args, "patch")?))?),
        "moveToQueue" => to(core.move_to_queue(&arg::<Vec<String>>(args, "ids")?, opt(args, "queueId"))?),
        "reorder" => to(core.reorder(&arg::<String>(args, "id")?, &arg::<String>(args, "direction")?)?),
        "getDetails" => rt.block_on(core.details(&arg::<String>(args, "id")?)),
        "logs" => to(core.logs_for(&arg::<String>(args, "id")?)),
        "verifyHash" => to(rt.block_on(core.verify(&arg::<String>(args, "id")?, &arg::<String>(args, "algo")?))?),

        "getSettings" => to(core.settings()),
        "saveSettings" => {
            let old = core.settings();
            let s = rt.block_on(core.save_settings(arg::<Settings>(args, "settings")?))?;
            if old.airsend_name != s.airsend_name || old.airsend_avatar != s.airsend_avatar {
                if let Some(air) = a.air.clone() {
                    rt.spawn(async move { air.refresh().await });
                }
            }
            to(s)
        }
        "setProfile" => to(rt.block_on(core.set_profile(&arg::<String>(args, "id")?))?),

        "listQueues" => to(core.queues()),
        "saveQueue" => to(core.save_queue(arg::<Queue>(args, "queue")?)?),
        "deleteQueue" => to(core.delete_queue(&arg::<String>(args, "id")?)?),
        "startQueue" => to(rt.block_on(core.start_queue(&arg::<String>(args, "id")?, None))?),
        "stopQueue" => to(rt.block_on(core.stop_queue(&arg::<String>(args, "id")?))?),
        "listSchedules" => to(core.schedules()),
        "saveSchedule" => to(core.save_schedule(arg::<Schedule>(args, "schedule")?)?),
        "deleteSchedule" => to(core.delete_schedule(&arg::<String>(args, "id")?)?),
        "setAfterAll" => {
            core.set_after_all(&arg::<String>(args, "action")?);
            Ok(Value::Null)
        }
        "getAfterAll" => to(core.after_all()),
        "cancelPower" => {
            core.cancel_power();
            Ok(Value::Null)
        }

        "mediaAnalyze" => {
            let cookies: Vec<BrowserCookie> = opt(args, "cookies").unwrap_or_default();
            to(rt.block_on(core.analyze(&arg::<String>(args, "url")?, opt(args, "playlist").unwrap_or(false), cookies, opt(args, "referer")))?)
        }
        "mediaDownload" => to(rt.block_on(core.add_media(arg::<MediaRequest>(args, "req")?))?),
        "torrentInfo" => torrent_info(opt(args, "path"), opt(args, "data")),
        "engineInfo" => Ok(rt.block_on(core.engine_info())),
        "updateYtdlp" => to(rt.block_on(core.update_ytdlp())?),
        "grabPage" => to(rt.block_on(kucore::grab::grab_page(&arg::<String>(args, "url")?, &core.settings()))?),
        "extractLinks" => {
            let base = url::Url::parse(&arg::<String>(args, "base")?)?;
            to(kucore::grab::extract_links(&arg::<String>(args, "html")?, &base))
        }

        "airStatus" => to(air(a)?.status()),
        "airSetEnabled" => {
            let enabled: bool = arg(args, "enabled")?;
            let air = air(a)?;
            let status = rt.block_on(async {
                if enabled {
                    air.start().await
                } else {
                    air.stop().await;
                    Ok(air.status())
                }
            })?;
            let mut s = core.settings();
            if s.airsend_enabled != enabled {
                s.airsend_enabled = enabled;
                rt.block_on(core.save_settings(s))?;
            }
            to(status)
        }
        "airPeers" => to(air(a)?.peers()),
        "airTransfers" => to(air(a)?.transfers()),
        "airSend" => to(air(a)?.send(&arg::<String>(args, "fingerprint")?, opt(args, "paths").unwrap_or_default(), opt(args, "text"), opt(args, "pin"))?),
        "airSendDownload" => to(air(a)?.send_download(&arg::<String>(args, "fingerprint")?, arg(args, "download")?, opt(args, "pin"))?),
        "airCancel" => {
            rt.block_on(air(a)?.cancel(&arg::<String>(args, "id")?));
            Ok(Value::Null)
        }
        "airDecide" => to(rt.block_on(air(a)?.decide(&arg::<String>(args, "id")?, arg(args, "accept")?, opt(args, "trust").unwrap_or(false)))?),
        "airRefresh" => {
            rt.block_on(air(a)?.refresh());
            Ok(Value::Null)
        }
        "airAdd" => to(rt.block_on(air(a)?.add_address(&arg::<String>(args, "address")?))?),
        "airTrust" => to(rt.block_on(air(a)?.set_trusted(&arg::<String>(args, "fingerprint")?, arg(args, "trusted")?))?),
        "airClearHistory" => {
            air(a)?.clear_history();
            Ok(Value::Null)
        }

        "setStrings" => {
            i18n::set(arg::<HashMap<String, String>>(args, "strings")?);
            Ok(Value::Null)
        }
        "adblockUpdate" => {
            let lists: Vec<String> = opt(args, "lists").unwrap_or_default();
            to(rt.block_on(adblock_engine::update(&paths::data_dir(), &lists, &core.settings()))?)
        }
        "adblockStatus" => Ok(adblock_engine::status()),
        "adblockClear" => {
            adblock_engine::clear(&paths::data_dir());
            Ok(Value::Null)
        }
        "shutdown" => {
            rt.block_on(core.shutdown());
            Ok(Value::Null)
        }
        _ => bail!("Unknown request: {method}"),
    }
}

/// Same as the desktop's address check: media sites and non-HTTP links skip the probe.
async fn probe(core: &Arc<Core>, url: String, opts: DownloadOptions) -> Result<ProbeInfo> {
    let s = core.settings();
    let url = url.trim().to_string();
    if !kucore::classify::scheme_allowed(&url) {
        bail!("Unsupported address. KuDownloader accepts http, https, ftp, sftp and magnet links.");
    }
    let mut info = match kucore::classify::classify(&url, &s.categories) {
        kucore::classify::Route::Ytdlp => ProbeInfo { url: url.clone(), final_url: url.clone(), engine: Some(Engine::Ytdlp), kind: Some(Kind::Media), ..Default::default() },
        kucore::classify::Route::Aria2(k) if k != Kind::Http => ProbeInfo {
            url: url.clone(),
            final_url: url.clone(),
            engine: Some(Engine::Aria2),
            kind: Some(k),
            filename: kucore::classify::filename_from_url(&url),
            ..Default::default()
        },
        _ => kucore::probe::probe(&url, &opts, &s).await,
    };
    if let Some(n) = &info.filename {
        info.category = Some(kucore::classify::category_for(n, &s.categories, info.kind.unwrap_or(Kind::Http)));
    }
    Ok(info)
}

fn torrent_info(path: Option<String>, data: Option<String>) -> Result<Value> {
    let bytes = match (path, data) {
        (Some(p), _) => {
            if std::fs::metadata(&p)?.len() > 20 << 20 {
                bail!("This .torrent file is too large.");
            }
            std::fs::read(&p)?
        }
        (None, Some(d)) => base64::engine::general_purpose::STANDARD.decode(d.trim())?,
        _ => bail!("No torrent given"),
    };
    let info = kucore::torrent::parse(&bytes)?;
    Ok(json!({ "info": info, "data": base64::engine::general_purpose::STANDARD.encode(&bytes) }))
}

/// Translate an engine message into the interface language.
pub fn te(msg: &str) -> String {
    i18n::te(msg)
}
