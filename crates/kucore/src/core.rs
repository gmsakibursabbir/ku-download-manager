//! KuCore: owns download state, decides what runs, drives the engines and
//! publishes batched events. All state transitions are persisted immediately.

use crate::aria2::{self, Aria2, SpawnConfig};
use crate::classify::{self, Route};
use crate::db::Db;
use crate::probe;
use crate::settings::Settings;
use crate::smart::{self, Smart, SmartAction};
use crate::types::*;
use crate::ytdlp::{self, YtEnv, YtEvent, YtJob};
use anyhow::{anyhow, bail, Context, Result};
use ku_proto::*;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, oneshot, Notify};

mod kuhttp_bridge;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

const LOG_LINES: usize = 300;
const STATUS_KEYS: &[&str] = &[
    "gid", "status", "totalLength", "completedLength", "uploadLength", "downloadSpeed", "uploadSpeed",
    "connections", "numSeeders", "errorCode", "errorMessage", "followedBy", "infoHash", "bittorrent", "dir",
];

#[derive(Default)]
struct State {
    downloads: HashMap<String, Download>,
    queues: Vec<Queue>,
    schedules: Vec<Schedule>,
    smart: HashMap<String, Smart>,
    retry_at: HashMap<String, Instant>,
    /// Queues that had work since they were started (drain detection).
    queue_busy: HashSet<String>,
    /// Schedules whose window is currently active → previous profile.
    schedule_active: HashMap<String, Option<String>>,
    any_busy: bool,
    aria2_crashes: VecDeque<Instant>,
}

enum After {
    Completed { id: String, name: String, path: Option<String> },
    Failed { id: String, name: String, message: String },
    Purge(String),
    Retune { gid: String, connections: u32 },
}

pub struct Core {
    pub db: Db,
    settings: RwLock<Settings>,
    st: Mutex<State>,
    aria: tokio::sync::RwLock<Option<Arc<Aria2>>>,
    aria_start: tokio::sync::Mutex<()>,
    events: broadcast::Sender<CoreEvent>,
    wake: Notify,
    /// On-demand tool downloads in flight (one per tool).
    tool_jobs: Mutex<std::collections::HashSet<String>>,
    yt: Mutex<HashMap<String, oneshot::Sender<()>>>,
    logs: Mutex<HashMap<String, VecDeque<LogLine>>>,
    power: Mutex<Option<oneshot::Sender<()>>>,
    after_all: Mutex<String>,
    shutting_down: AtomicBool,
    tick: AtomicU64,
    quit_hook: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    tmp_dir: PathBuf,
    /// Created on first use (experimental native HTTP engine).
    kuhttp: Mutex<Option<Arc<crate::kuhttp::KuHttpEngine>>>,
}

impl Core {
    pub fn open(db: Db) -> Result<Arc<Core>> {
        let settings = db.load_settings()?;
        let (events, _) = broadcast::channel(2048);
        let core = Arc::new(Core {
            db,
            settings: RwLock::new(settings),
            st: Mutex::new(State::default()),
            aria: tokio::sync::RwLock::new(None),
            aria_start: tokio::sync::Mutex::new(()),
            events,
            wake: Notify::new(),
            tool_jobs: Mutex::new(std::collections::HashSet::new()),
            yt: Mutex::new(HashMap::new()),
            logs: Mutex::new(HashMap::new()),
            power: Mutex::new(None),
            after_all: Mutex::new("none".into()),
            shutting_down: AtomicBool::new(false),
            tick: AtomicU64::new(0),
            quit_hook: Mutex::new(None),
            tmp_dir: paths::data_dir().join("tmp"),
            kuhttp: Mutex::new(None),
        });
        core.recover()?;
        Ok(core)
    }

    /// Start background loops. Must be called inside a Tokio runtime.
    pub fn start(self: &Arc<Self>) {
        let _ = std::fs::remove_dir_all(&self.tmp_dir);
        let c = self.clone();
        tokio::spawn(async move { c.main_loop().await });
        let c = self.clone();
        tokio::spawn(async move { c.scheduler_loop().await });
        self.wake.notify_one();
    }

    pub fn set_quit_hook(&self, f: impl Fn() + Send + Sync + 'static) {
        *lock(&self.quit_hook) = Some(Box::new(f));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.events.subscribe()
    }

    pub fn emit(&self, e: CoreEvent) {
        if let CoreEvent::Completed { id, name, path } = &e {
            self.scan_finished(id, name, path.clone());
        }
        let _ = self.events.send(e);
    }

    /// Optional virus scan of a finished file; only problems are reported.
    fn scan_finished(&self, id: &str, name: &str, path: Option<String>) {
        let s = self.settings();
        if !matches!(s.virus_scan.as_str(), "defender" | "custom") {
            return;
        }
        let file = path.map(std::path::PathBuf::from).or_else(|| self.get(id).map(|d| std::path::Path::new(&d.dir).join(&d.name)));
        // Playlists point at a folder: scan files only.
        let Some(file) = file.filter(|f| f.is_file()) else { return };
        let Ok(rt) = tokio::runtime::Handle::try_current() else { return };
        let (tx, id, name) = (self.events.clone(), id.to_string(), name.to_string());
        rt.spawn(async move {
            let (level, title, message) = match crate::scan::scan(&s.virus_scan, &s.virus_scanner, &s.virus_scanner_args, &file).await {
                Ok(crate::scan::Outcome::Clean) => return,
                Ok(crate::scan::Outcome::Threat(m)) => ("error", format!("Threat found in {name}"), m),
                Err(e) => ("warning", format!("Could not scan {name}"), e.to_string()),
            };
            let _ = tx.send(CoreEvent::Notice { level: level.into(), title, message, download_id: Some(id) });
        });
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    // ───────────────────────────── recovery ─────────────────────────────

    /// Crash/restart recovery: reload everything, validate partial files and
    /// requeue what was running so it resumes.
    fn recover(&self) -> Result<()> {
        let mut st = lock(&self.st);
        st.queues = self.db.load_queues()?;
        st.schedules = self.db.load_schedules()?;
        for mut d in self.db.load_downloads()? {
            let mut dirty = false;
            match d.status {
                Status::Downloading | Status::Processing => {
                    d.status = Status::Queued;
                    dirty = true;
                }
                Status::Seeding => {
                    d.status = Status::Completed;
                    dirty = true;
                }
                _ => {}
            }
            if matches!(d.status, Status::Queued | Status::Paused | Status::Error) && d.engine == Engine::Aria2 && d.done > 0 {
                // A partial file without its control file cannot be resumed.
                let target = d.file_path.clone().map(PathBuf::from).unwrap_or_else(|| Path::new(&d.dir).join(&d.name));
                let control = PathBuf::from(format!("{}.aria2", target.display()));
                if !target.exists() || (!control.exists() && !d.kind.is_bittorrent()) {
                    d.done = 0;
                    dirty = true;
                }
            }
            d.speed = 0;
            d.upload_speed = 0;
            d.active_connections = 0;
            if dirty {
                self.db.upsert_download(&d)?;
            }
            st.downloads.insert(d.id.clone(), d);
        }
        let running: Vec<String> = st.queues.iter().filter(|q| q.running).map(|q| q.id.clone()).collect();
        st.queue_busy.extend(running);
        Ok(())
    }

    // ───────────────────────────── queries ─────────────────────────────

/// Same address, ignoring the fragment and a trailing slash.
    fn same_url(a: &str, b: &str) -> bool {
        let norm = |u: &str| u.trim().split('#').next().unwrap_or("").trim_end_matches('/').to_string();
        norm(a) == norm(b)
    }

    /// Before adding: an existing download of the same link, and whether a
    /// file with the target name is already in the folder.
    pub fn check_duplicate(&self, url: &str, dir: Option<&str>, filename: Option<&str>) -> serde_json::Value {
        let existing = {
            let st = lock(&self.st);
            let mut v: Vec<&Download> = st.downloads.values().filter(|d| Self::same_url(&d.url, url)).collect();
            v.sort_by_key(|d| std::cmp::Reverse(d.created_at));
            v.first().map(|d| (*d).clone())
        };
        let path = match (dir.filter(|d| !d.trim().is_empty()), filename.filter(|n| !n.trim().is_empty())) {
            (Some(d), Some(n)) => Some(std::path::Path::new(d.trim()).join(n.trim())),
            _ => None,
        };
        let file_exists = path.as_ref().is_some_and(|p| p.is_file());
        serde_json::json!({
            "existing": existing,
            "existingFileExists": existing.as_ref().is_some_and(|d| std::path::Path::new(d.file_path.as_deref().unwrap_or("")).is_file() || std::path::Path::new(&d.dir).join(&d.name).is_file()),
            "fileExists": file_exists,
            "path": path.map(|p| p.display().to_string()),
            "policy": self.settings().file_exists,
        })
    }

    pub fn list(&self) -> Vec<Download> {
        let st = lock(&self.st);
        let mut v: Vec<Download> = st.downloads.values().cloned().collect();
        v.sort_by_key(|d| d.created_at);
        v
    }

    pub fn get(&self, id: &str) -> Option<Download> {
        lock(&self.st).downloads.get(id).cloned()
    }

    pub fn queues(&self) -> Vec<Queue> {
        lock(&self.st).queues.clone()
    }

    pub fn schedules(&self) -> Vec<Schedule> {
        lock(&self.st).schedules.clone()
    }

    pub fn stats(&self) -> Stats {
        let st = lock(&self.st);
        let mut s = Stats::default();
        for d in st.downloads.values() {
            s.total += 1;
            s.download_speed += d.speed;
            s.upload_speed += d.upload_speed;
            match d.status {
                Status::Downloading | Status::Processing => s.active += 1,
                Status::Queued => s.queued += 1,
                Status::Paused => s.paused += 1,
                Status::Completed | Status::Seeding => s.completed += 1,
                Status::Error => s.errors += 1,
            }
        }
        s
    }

    pub fn log(&self, id: &str, level: &str, message: impl Into<String>) {
        let mut logs = lock(&self.logs);
        let q = logs.entry(id.to_string()).or_default();
        if q.len() >= LOG_LINES {
            q.pop_front();
        }
        q.push_back(LogLine { ts: now_ms(), level: level.into(), message: message.into() });
    }

    pub fn logs_for(&self, id: &str) -> Vec<LogLine> {
        lock(&self.logs).get(id).map(|q| q.iter().cloned().collect()).unwrap_or_default()
    }

    fn commit(&self, d: &Download) {
        if let Err(e) = self.db.upsert_download(d) {
            tracing::error!("persisting {}: {e:#}", d.id);
        }
        self.emit(CoreEvent::Upsert { download: Box::new(d.clone()) });
    }

    /// Mutate one download under the lock, persist and publish it.
    fn update<R>(&self, id: &str, f: impl FnOnce(&mut Download) -> R) -> Option<R> {
        let mut st = lock(&self.st);
        let d = st.downloads.get_mut(id)?;
        let r = f(d);
        let snapshot = d.clone();
        drop(st);
        self.commit(&snapshot);
        Some(r)
    }

    // ───────────────────────────── settings ─────────────────────────────

    pub async fn save_settings(self: &Arc<Self>, mut s: Settings) -> Result<Settings> {
        s.sanitize();
        let old = self.settings();
        self.db.save_settings(&s)?;
        *self.settings.write().unwrap_or_else(|p| p.into_inner()) = s.clone();
        if old.profile() != s.profile() || old.active_profile != s.active_profile {
            self.apply_speed_limits().await;
        }
        if old.proxy != s.proxy || old.user_agent != s.user_agent || old.check_certificate != s.check_certificate || old.timeout != s.timeout {
            self.kuhttp_reset();
        }
        if old.aria2_path != s.aria2_path || old.bt_listen_port != s.bt_listen_port || old.enable_dht != s.enable_dht {
            // Restart aria2 lazily with the new binary/ports; downloads resume.
            self.restart_aria2().await;
        }
        self.emit(CoreEvent::SettingsChanged);
        self.wake.notify_one();
        Ok(s)
    }

    pub async fn set_profile(self: &Arc<Self>, id: &str) -> Result<()> {
        let mut s = self.settings();
        if !s.profiles.iter().any(|p| p.id == id) {
            bail!("Unknown bandwidth profile");
        }
        s.active_profile = id.to_string();
        self.save_settings(s).await.map(|_| ())
    }

    async fn apply_speed_limits(&self) {
        let p = self.settings().profile();
        self.kuhttp_set_limit(p.download);
        if let Some(a) = self.aria.read().await.clone() {
            let _ = a
                .change_global(json!({
                    "max-overall-download-limit": p.download.to_string(),
                    "max-overall-upload-limit": p.upload.to_string(),
                }))
                .await;
        }
    }

    // ───────────────────────────── aria2 lifecycle ─────────────────────────────

    async fn aria2(self: &Arc<Self>) -> Result<Arc<Aria2>> {
        if let Some(a) = self.aria.read().await.clone() {
            return Ok(a);
        }
        let _guard = self.aria_start.lock().await;
        if let Some(a) = self.aria.read().await.clone() {
            return Ok(a);
        }
        if self.shutting_down.load(Ordering::SeqCst) {
            bail!("KuDownloader is shutting down");
        }
        let s = self.settings();
        let bin = paths::find_binary("aria2c", Some(&s.aria2_path)).ok_or_else(|| {
            anyhow!("The aria2 engine (aria2c) was not found. Reinstall KuDownloader or set its path in Settings › Advanced.")
        })?;
        let data = paths::data_dir();
        let (a, mut child) = Aria2::spawn(SpawnConfig {
            binary: &bin,
            data_dir: &data,
            download_dir: &s.download_dir,
            listen_port: &s.bt_listen_port,
            enable_dht: s.enable_dht,
            max_peers: s.bt_max_peers,
        })
        .await?;
        tracing::info!("aria2 {} started from {}", a.version, bin.display());
        let a = Arc::new(a);
        *self.aria.write().await = Some(a.clone());
        self.apply_speed_limits().await;
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let mut lines = tokio::io::BufReader::new(stderr).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    if !l.trim().is_empty() {
                        tracing::warn!("aria2: {l}");
                    }
                }
            });
        }
        let core = self.clone();
        let this = a.clone();
        tokio::spawn(async move {
            let status = child.wait().await;
            core.on_aria2_exit(&this, status.ok()).await;
        });
        Ok(a)
    }

    async fn on_aria2_exit(self: &Arc<Self>, which: &Arc<Aria2>, status: Option<std::process::ExitStatus>) {
        {
            let mut cur = self.aria.write().await;
            if cur.as_ref().is_some_and(|c| Arc::ptr_eq(c, which)) {
                *cur = None;
            } else {
                return; // an intentional restart already replaced it
            }
        }
        if self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        tracing::warn!("aria2 exited unexpectedly: {status:?}");
        let (ids, give_up) = {
            let mut st = lock(&self.st);
            let now = Instant::now();
            st.aria2_crashes.push_back(now);
            while st.aria2_crashes.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(120)) {
                st.aria2_crashes.pop_front();
            }
            let give_up = st.aria2_crashes.len() >= 4;
            let ids: Vec<String> = st
                .downloads
                .values()
                .filter(|d| d.engine == Engine::Aria2 && matches!(d.status, Status::Downloading | Status::Seeding))
                .map(|d| d.id.clone())
                .collect();
            (ids, give_up)
        };
        for id in &ids {
            self.update(id, |d| {
                d.gid = None;
                d.speed = 0;
                if give_up {
                    d.status = Status::Error;
                    d.error = Some("The aria2 engine keeps stopping unexpectedly. Check disk space and antivirus software, then retry.".into());
                } else {
                    d.status = if d.status == Status::Seeding { Status::Completed } else { Status::Queued };
                }
            });
        }
        if !give_up {
            self.emit(CoreEvent::Notice {
                level: "warning".into(),
                title: "Download engine restarted".into(),
                message: "aria2 stopped unexpectedly. KuDownloader restarted it and is resuming your downloads.".into(),
                download_id: None,
            });
        }
        self.wake.notify_one();
    }

    async fn restart_aria2(self: &Arc<Self>) {
        let old = self.aria.write().await.take();
        if let Some(a) = old {
            let ids: Vec<String> = {
                let st = lock(&self.st);
                st.downloads
                    .values()
                    .filter(|d| d.engine == Engine::Aria2 && d.status == Status::Downloading)
                    .map(|d| d.id.clone())
                    .collect()
            };
            a.shutdown().await;
            for id in ids {
                self.update(&id, |d| {
                    d.status = Status::Queued;
                    d.gid = None;
                });
            }
        }
        self.wake.notify_one();
    }

    // ───────────────────────────── adding ─────────────────────────────

    fn resolve_dir(&self, s: &Settings, category: &str, explicit: Option<&str>) -> String {
        if let Some(d) = explicit.filter(|d| !d.trim().is_empty()) {
            return d.trim().to_string();
        }
        let base = PathBuf::from(&s.download_dir);
        if s.use_categories {
            if let Some(c) = s.categories.iter().find(|c| c.id == category) {
                let f = Path::new(&c.folder);
                if f.is_absolute() {
                    return c.folder.clone();
                }
                if !c.folder.is_empty() {
                    return base.join(f).to_string_lossy().into_owned();
                }
            }
        }
        base.to_string_lossy().into_owned()
    }

    /// Pick a name that collides with neither an existing file nor another download.
    fn unique_name(&self, st: &State, dir: &str, name: &str, policy: &str) -> Result<String> {
        let taken = |n: &str| {
            let p = Path::new(dir).join(n);
            p.exists()
                || Path::new(&format!("{}.aria2", p.display())).exists()
                || st.downloads.values().any(|d| !d.status.is_finished() && d.dir == dir && d.name == n)
        };
        if !taken(name) || policy == "overwrite" {
            return Ok(name.to_string());
        }
        if policy == "skip" {
            bail!("\"{name}\" already exists in {dir}. Change the file name or the “If the file exists” setting.");
        }
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
            _ => (name.to_string(), String::new()),
        };
        (1..10_000)
            .map(|i| format!("{stem} ({i}){ext}"))
            .find(|n| !taken(n))
            .ok_or_else(|| anyhow!("Could not find a free file name for {name}"))
    }

    fn next_position(st: &State, queue: Option<&str>) -> i64 {
        st.downloads.values().filter(|d| d.queue_id.as_deref() == queue).map(|d| d.position).max().unwrap_or(0) + 1
    }

    pub async fn add(self: &Arc<Self>, req: AddRequest) -> Result<Download> {
        let s = self.settings();
        // Share links (Drive, Dropbox, SourceForge…) → direct file URL.
        let mut url = req.url.trim().to_string();
        if let Some(direct) = crate::landing::rewrite(&url) {
            url = direct;
        }
        let has_file = req.options.torrent_data.is_some() || req.options.metalink_data.is_some();
        if !has_file && !classify::scheme_allowed(&url) {
            bail!("Unsupported address. KuDownloader accepts http, https, ftp, sftp and magnet links.");
        }
        if let Some(c) = &req.options.checksum {
            let (a, h) = crate::hash::parse_checksum(c)?;
            let _ = (a, h);
        }
        let mut route = if req.options.torrent_data.is_some() {
            Route::Aria2(Kind::Torrent)
        } else if req.options.metalink_data.is_some() {
            Route::Aria2(Kind::Metalink)
        } else {
            match req.engine {
                Some(Engine::Ytdlp) => Route::Ytdlp,
                Some(Engine::Aria2 | Engine::Kuhttp) => match classify::classify(&url, &s.categories) {
                    Route::Aria2(k) => Route::Aria2(k),
                    _ => Route::Aria2(Kind::Http),
                },
                None => classify::classify(&url, &s.categories),
            }
        };
        let mut probe_info = None;
        if matches!(route, Route::Unknown | Route::Aria2(Kind::Http)) && !has_file {
            let p = probe::probe(&url, &req.options, &s).await;
            // A landing page resolved to its file: download that.
            if p.url != url && p.status.is_some_and(|c| (200..300).contains(&c)) {
                url = p.url.clone();
                route = classify::classify(&url, &s.categories);
                if route == Route::Ytdlp {
                    route = Route::Aria2(Kind::Http);
                }
            }
            if route == Route::Unknown {
                route = match (p.engine, p.kind) {
                    (Some(Engine::Ytdlp), _) => Route::Ytdlp,
                    (Some(Engine::Aria2), Some(k)) => Route::Aria2(k),
                    _ => Route::Aria2(Kind::Http),
                };
            }
            probe_info = Some(p);
        }
        if route == Route::Ytdlp {
            let media = MediaOptions {
                mode: "video".into(),
                height: Some(s.video_height),
                container: Some(s.video_container.clone()),
                subtitles: s.subtitles,
                sub_langs: Some(s.sub_langs.clone()),
                embed_subtitles: s.subtitles,
                embed_thumbnail: s.embed_thumbnail,
                ..Default::default()
            };
            return self
                .add_media(MediaRequest {
                    url,
                    dir: req.dir,
                    media: req.options.media.clone().unwrap_or(media),
                    cookies: req.options.cookies.clone(),
                    referer: req.options.referer.clone(),
                    user_agent: req.options.user_agent.clone(),
                    queue_id: req.queue_id,
                    title: req.filename,
                    size_hint: req.size_hint,
                    source: req.source,
                    thumbnail: None,
                })
                .await;
        }
        let Route::Aria2(kind) = route else { unreachable!() };
        let lower_url = url.to_ascii_lowercase();
        let http_like = kind == Kind::Http && (lower_url.starts_with("http://") || lower_url.starts_with("https://")) && req.mirrors.is_empty();
        // KuHTTP is the default HTTP engine (docs/kuhttp.md); aria2 on request or for mirrors.
        // Without aria2 (e.g. macOS without Homebrew) plain HTTP(S) still works through KuHTTP.
        let no_aria2 = paths::find_binary("aria2c", Some(&s.aria2_path)).is_none();
        let engine = if http_like && (no_aria2 || req.engine == Some(Engine::Kuhttp) || (req.engine.is_none() && s.http_engine == "kuhttp")) {
            Engine::Kuhttp
        } else {
            Engine::Aria2
        };

        let mut torrent_meta = None;
        if let Some(b64) = &req.options.torrent_data {
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64).context("invalid torrent data")?;
            torrent_meta = Some(crate::torrent::parse(&bytes)?);
        }
        let name = req
            .filename
            .as_deref()
            .map(classify::sanitize_filename)
            .filter(|n| !n.is_empty())
            .or_else(|| torrent_meta.as_ref().map(|t| classify::sanitize_filename(&t.name)))
            .or_else(|| probe_info.as_ref().and_then(|p| p.filename.clone()))
            .or_else(|| {
                if kind == Kind::Magnet {
                    magnet_name(&url)
                } else {
                    classify::filename_from_url(&url)
                }
            })
            .unwrap_or_else(|| "download".into());
        let category = req
            .category
            .clone()
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| classify::category_for(&name, &s.categories, kind));
        let dir = self.resolve_dir(&s, &category, req.dir.as_deref());

        let mut meta = DownloadMeta::default();
        if let Some(p) = &probe_info {
            meta.mime = p.mime.clone();
            meta.resumable = p.resumable;
        }
        if let Some(t) = &torrent_meta {
            meta.info_hash = Some(t.info_hash.clone());
            meta.files = t
                .files
                .iter()
                .map(|f| FileEntry { path: f.path.clone(), length: f.length, completed: 0, selected: true })
                .collect();
        }
        let total = probe_info.as_ref().and_then(|p| p.size).or(req.size_hint).or(torrent_meta.as_ref().map(|t| t.total)).unwrap_or(0);
        let queue_id = req.queue_id.clone().filter(|q| !q.is_empty());
        let start = req.start.unwrap_or(true);
        let d = {
            let mut st = lock(&self.st);
            if kind != Kind::Magnet && !has_file {
                if let Some(dup) = st.downloads.values().find(|d| d.url == url && !d.status.is_finished() && d.status != Status::Error) {
                    bail!("This address is already in your download list as \"{}\".", dup.name);
                }
            }
            let name = if kind.is_bittorrent() { name } else { self.unique_name(&st, &dir, &name, &s.file_exists)? };
            if let Some(q) = &queue_id {
                if !st.queues.iter().any(|x| &x.id == q) {
                    bail!("Unknown queue");
                }
            }
            let d = Download {
                id: uuid::Uuid::new_v4().to_string(),
                engine,
                kind,
                url: if url.is_empty() { format!("torrent:{name}") } else { url.clone() },
                mirrors: req.mirrors.iter().map(|m| m.trim().to_string()).filter(|m| classify::scheme_allowed(m)).collect(),
                name,
                dir,
                file_path: None,
                status: if start || queue_id.is_some() { Status::Queued } else { Status::Paused },
                total,
                done: 0,
                uploaded: 0,
                speed: 0,
                upload_speed: 0,
                connections: req.connections.unwrap_or(s.default_connections).min(32),
                active_connections: 0,
                category,
                position: Self::next_position(&st, queue_id.as_deref()),
                queue_id,
                created_at: now_ms(),
                completed_at: None,
                error: None,
                gid: None,
                source: req.source.clone().unwrap_or_else(|| "ui".into()),
                options: req.options.clone(),
                meta,
            };
            st.downloads.insert(d.id.clone(), d.clone());
            d
        };
        if let Some(p) = &probe_info {
            if let Some(e) = &p.error {
                self.log(&d.id, "warn", format!("Probe: {e}"));
            } else {
                self.log(&d.id, "info", format!(
                    "Probe: {} · {} · ranges {}",
                    p.mime.as_deref().unwrap_or("unknown type"),
                    p.size.map(|s| format!("{s} bytes")).unwrap_or_else(|| "size unknown".into()),
                    match p.resumable { Some(true) => "supported", Some(false) => "not supported", None => "unknown" }
                ));
            }
        }
        self.commit(&d);
        self.wake.notify_one();
        Ok(d)
    }

    pub async fn add_media(self: &Arc<Self>, req: MediaRequest) -> Result<Download> {
        let s = self.settings();
        let url = req.url.trim().to_string();
        if !url.starts_with("http://") && !url.starts_with("https://") {
            bail!("Media downloads need an http or https address.");
        }
        let audio = req.media.mode == "audio";
        let ext = req.media.container.clone().unwrap_or_else(|| if audio { s.audio_format.clone() } else { s.video_container.clone() });
        let dir = req.dir.clone().filter(|d| !d.trim().is_empty()).unwrap_or_else(|| {
            if audio {
                self.resolve_dir(&s, "music", None)
            } else {
                s.video_dir.clone()
            }
        });
        let title = req.title.clone().filter(|t| !t.trim().is_empty());
        let queue_id = req.queue_id.clone().filter(|q| !q.is_empty());
        let d = {
            let mut st = lock(&self.st);
            if let Some(dup) = st.downloads.values().find(|d| {
                d.url == url && d.options.media.as_ref() == Some(&req.media) && !d.status.is_finished() && d.status != Status::Error
            }) {
                bail!("This media is already in your download list as \"{}\".", dup.name);
            }
            let name = match &title {
                Some(t) if req.media.playlist => classify::sanitize_filename(t),
                Some(t) => format!("{}.{ext}", classify::sanitize_filename(t)),
                None => url.clone(),
            };
            let d = Download {
                id: uuid::Uuid::new_v4().to_string(),
                engine: Engine::Ytdlp,
                kind: Kind::Media,
                url: url.clone(),
                mirrors: Vec::new(),
                name,
                dir,
                file_path: None,
                status: Status::Queued,
                total: req.size_hint.unwrap_or(0),
                done: 0,
                uploaded: 0,
                speed: 0,
                upload_speed: 0,
                connections: 0,
                active_connections: 0,
                category: if audio { "music".into() } else { "video".into() },
                position: Self::next_position(&st, queue_id.as_deref()),
                queue_id,
                created_at: now_ms(),
                completed_at: None,
                error: None,
                gid: None,
                source: req.source.clone().unwrap_or_else(|| "ui".into()),
                options: DownloadOptions {
                    cookies: req.cookies.clone(),
                    referer: req.referer.clone(),
                    user_agent: req.user_agent.clone(),
                    media: Some(req.media.clone()),
                    ..Default::default()
                },
                meta: DownloadMeta { thumbnail: req.thumbnail.clone(), media_title: title.clone(), ..Default::default() },
            };
            st.downloads.insert(d.id.clone(), d.clone());
            d
        };
        self.commit(&d);
        if title.is_none() {
            // Fill in title/thumbnail in the background; the download itself
            // does not wait for it.
            let core = self.clone();
            let id = d.id.clone();
            let playlist = req.media.playlist;
            tokio::spawn(async move {
                let Some(d) = core.get(&id) else { return };
                if let Ok(info) = core.analyze_with(&d.url, playlist, &d.options).await {
                    core.update(&id, |d| {
                        if d.file_path.is_none() {
                            d.name = if playlist { classify::sanitize_filename(&info.title) } else { format!("{}.{ext}", classify::sanitize_filename(&info.title)) };
                        }
                        d.meta.media_title = Some(info.title.clone());
                        d.meta.thumbnail = info.thumbnail.clone();
                        d.meta.uploader = info.uploader.clone();
                        d.meta.duration = info.duration;
                    });
                }
            });
        }
        self.wake.notify_one();
        Ok(d)
    }

    pub async fn add_batch(self: &Arc<Self>, urls: Vec<String>, template: AddRequest) -> (Vec<Download>, Vec<(String, String)>) {
        let mut ok = Vec::new();
        let mut failed = Vec::new();
        let mut seen = HashSet::new();
        for u in urls.into_iter().map(|u| u.trim().to_string()).filter(|u| !u.is_empty()) {
            if !seen.insert(u.clone()) {
                continue;
            }
            let req = AddRequest { url: u.clone(), filename: None, size_hint: None, ..template.clone() };
            match self.add(req).await {
                Ok(d) => ok.push(d),
                Err(e) => failed.push((u, e.to_string())),
            }
        }
        (ok, failed)
    }

    // ───────────────────────────── media ─────────────────────────────


    fn yt_env(&self, opts: &DownloadOptions) -> Result<YtEnv> {
        let s = self.settings();
        let ytdlp = paths::find_binary("yt-dlp", Some(&s.ytdlp_path)).ok_or_else(|| {
            anyhow!("The media engine (yt-dlp) is not installed yet. Download it from the Video Downloader or Settings › Advanced.")
        })?;
        let cookies_file = ytdlp::write_cookie_file(&self.tmp_dir, &opts.cookies)?;
        Ok(YtEnv {
            ytdlp,
            ffmpeg: paths::find_binary("ffmpeg", Some(&s.ffmpeg_path)),
            proxy: opts.proxy.clone().filter(|p| !p.is_empty()).unwrap_or(s.proxy.clone()),
            user_agent: opts.user_agent.clone().filter(|u| !u.is_empty()),
            referer: opts.referer.clone(),
            cookies_file,
            user_cookies_file: Some(std::path::PathBuf::from(s.cookies_file.trim())).filter(|p| !p.as_os_str().is_empty() && p.is_file()),
            cookies_from_browser: Some(s.cookies_from_browser.trim().to_string()).filter(|b| ALLOWED_COOKIE_BROWSERS.contains(&b.as_str())),
        })
    }

    async fn analyze_with(&self, url: &str, playlist: bool, opts: &DownloadOptions) -> Result<MediaInfo> {
        let env = self.yt_env(opts)?;
        let r = ytdlp::analyze(&env, url, playlist).await;
        if let Some(c) = &env.cookies_file {
            let _ = std::fs::remove_file(c);
        }
        r
    }

    pub async fn analyze(&self, url: &str, playlist: bool, cookies: Vec<BrowserCookie>, referer: Option<String>) -> Result<MediaInfo> {
        let url = url.trim();
        if !url.starts_with("http://") && !url.starts_with("https://") {
            bail!("Enter an http or https address.");
        }
        let opts = DownloadOptions { cookies, referer, ..Default::default() };
        self.analyze_with(url, playlist, &opts).await
    }

    // ───────────────────────────── control ─────────────────────────────

    pub async fn pause(self: &Arc<Self>, ids: &[String]) -> Result<()> {
        for id in ids {
            let Some(d) = self.get(id) else { continue };
            match d.status {
                Status::Downloading | Status::Processing | Status::Seeding => {
                    self.stop_engine(&d).await;
                    self.update(id, |d| {
                        d.status = if d.status == Status::Seeding { Status::Completed } else { Status::Paused };
                        d.speed = 0;
                        d.upload_speed = 0;
                        d.active_connections = 0;
                    });
                    self.log(id, "info", "Paused");
                }
                Status::Queued => {
                    self.update(id, |d| d.status = Status::Paused);
                }
                _ => {}
            }
        }
        self.wake.notify_one();
        Ok(())
    }

    /// Stop the engine side of a download, keeping partial data for resume.
    async fn stop_engine(self: &Arc<Self>, d: &Download) {
        match d.engine {
            Engine::Aria2 => {
                if let (Some(gid), Some(a)) = (&d.gid, self.aria.read().await.clone()) {
                    if d.status == Status::Seeding {
                        a.purge(gid).await;
                    } else if let Err(e) = a.force_pause(gid).await {
                        if !aria2::is_gid_not_found(&e) {
                            tracing::warn!("pause {gid}: {e:#}");
                        }
                    }
                }
            }
            Engine::Ytdlp => {
                if let Some(tx) = lock(&self.yt).remove(&d.id) {
                    let _ = tx.send(());
                }
            }
            Engine::Kuhttp => self.stop_kuhttp(&d.id, false, false),
        }
    }

    pub async fn resume(self: &Arc<Self>, ids: &[String]) -> Result<()> {
        for id in ids {
            self.update(id, |d| {
                if matches!(d.status, Status::Paused | Status::Error | Status::Queued) {
                    if d.status == Status::Error {
                        d.meta.retries = 0;
                    }
                    d.status = Status::Queued;
                    d.error = None;
                }
            });
            lock(&self.st).retry_at.remove(id);
        }
        self.wake.notify_one();
        Ok(())
    }

    /// Download finished (or failed) files again from scratch, replacing the
    /// old copy; the entry keeps its queue, folder and name.
    pub async fn redownload(self: &Arc<Self>, ids: &[String]) -> Result<()> {
        for id in ids {
            let Some(d) = self.get(id) else { continue };
            if d.kind.is_bittorrent() || d.status.is_running() {
                continue;
            }
            if d.engine == Engine::Aria2 {
                if let (Some(gid), Some(a)) = (&d.gid, self.aria.read().await.clone()) {
                    a.purge(gid).await;
                }
            }
            delete_download_files(&d);
            self.update(id, |d| {
                d.status = Status::Queued;
                d.done = 0;
                d.speed = 0;
                d.gid = None;
                d.error = None;
                d.completed_at = None;
                d.meta.retries = 0;
                d.meta.verified = None;
            });
            lock(&self.st).retry_at.remove(id);
        }
        self.wake.notify_one();
        Ok(())
    }

    /// Synchronization queues: re-check finished HTTP files and download the
    /// ones that changed on the server.
    async fn sync_queues(self: &Arc<Self>) {
        let now = now_ms();
        let due: Vec<Queue> = lock(&self.st).queues.iter().filter(|q| q.sync_minutes > 0 && now - q.last_sync >= q.sync_minutes as i64 * 60_000).cloned().collect();
        for mut q in due {
            q.last_sync = now;
            {
                let mut st = lock(&self.st);
                if let Some(x) = st.queues.iter_mut().find(|x| x.id == q.id) {
                    x.last_sync = now;
                }
            }
            let _ = self.db.save_queue(&q);
            let items: Vec<Download> = self
                .list()
                .into_iter()
                .filter(|d| d.queue_id.as_deref() == Some(q.id.as_str()) && d.status == Status::Completed && d.kind == Kind::Http && d.url.starts_with("http"))
                .collect();
            let s = self.settings();
            let mut changed = Vec::new();
            for d in items {
                let p = probe::probe(&d.url, &d.options, &s).await;
                if p.error.is_some() || !p.status.is_some_and(|c| (200..300).contains(&c)) {
                    continue;
                }
                let stamp = format!("{}|{}|{}", p.size.unwrap_or(-1), p.last_modified.unwrap_or_default(), p.etag.unwrap_or_default());
                match d.meta.remote_stamp.as_deref() {
                    None => {
                        // First check: remember what the server has.
                        self.update(&d.id, |x| x.meta.remote_stamp = Some(stamp.clone()));
                    }
                    Some(old) if old != stamp => {
                        self.log(&d.id, "info", "Changed on the server — downloading the new version");
                        self.update(&d.id, |x| x.meta.remote_stamp = Some(stamp.clone()));
                        changed.push(d.id.clone());
                    }
                    _ => {}
                }
            }
            if !changed.is_empty() {
                let _ = self.redownload(&changed).await;
                let _ = self.start_queue(&q.id, None).await;
                self.emit(CoreEvent::Notice { level: "info".into(), title: format!("{} updated in “{}”", changed.len(), q.name), message: "Files that changed on the server are being downloaded again.".into(), download_id: None });
            }
            self.emit(CoreEvent::QueuesChanged);
        }
    }

    pub async fn remove(self: &Arc<Self>, ids: &[String], delete_files: bool) -> Result<()> {
        for id in ids {
            let Some(d) = self.get(id) else { continue };
            if d.status.is_running() || d.status == Status::Seeding {
                self.stop_engine(&d).await;
            }
            if d.engine == Engine::Aria2 {
                if let (Some(gid), Some(a)) = (&d.gid, self.aria.read().await.clone()) {
                    a.purge(gid).await;
                }
            }
            if d.engine == Engine::Kuhttp {
                // Removes the .kudownload temp and state files when asked to.
                self.stop_kuhttp(&d.id, true, delete_files);
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            if d.engine == Engine::Ytdlp {
                // Give the killed process a moment to release file handles.
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            if delete_files {
                delete_download_files(&d);
            }
            {
                let mut st = lock(&self.st);
                st.downloads.remove(id);
                st.smart.remove(id);
                st.retry_at.remove(id);
            }
            lock(&self.logs).remove(id);
        }
        self.db.delete_downloads(ids)?;
        self.emit(CoreEvent::Removed { ids: ids.to_vec() });
        self.wake.notify_one();
        Ok(())
    }

    pub async fn pause_all(self: &Arc<Self>) -> Result<()> {
        let ids: Vec<String> = lock(&self.st)
            .downloads
            .values()
            .filter(|d| d.status.is_running() || d.status == Status::Queued)
            .map(|d| d.id.clone())
            .collect();
        self.pause(&ids).await
    }

    pub async fn resume_all(self: &Arc<Self>) -> Result<()> {
        let ids: Vec<String> = lock(&self.st)
            .downloads
            .values()
            .filter(|d| d.status == Status::Paused || (d.status == Status::Error && d.queue_id.is_none()))
            .map(|d| d.id.clone())
            .collect();
        self.resume(&ids).await
    }

    pub fn clear_finished(&self) -> Result<Vec<String>> {
        let ids: Vec<String> = {
            let mut st = lock(&self.st);
            let ids: Vec<String> = st.downloads.values().filter(|d| d.status == Status::Completed).map(|d| d.id.clone()).collect();
            for id in &ids {
                st.downloads.remove(id);
            }
            ids
        };
        self.db.delete_downloads(&ids)?;
        self.emit(CoreEvent::Removed { ids: ids.clone() });
        Ok(ids)
    }

    /// Change editable properties of a download that is not running.
    pub async fn edit(self: &Arc<Self>, id: &str, patch: Value) -> Result<Download> {
        let d = self.get(id).ok_or_else(|| anyhow!("Download not found"))?;
        let running = d.status.is_running();
        let mut out = None;
        let mut retune = None;
        self.update(id, |d| {
            if let Some(c) = patch["connections"].as_u64() {
                d.connections = (c as u32).min(32);
                if running && d.engine == Engine::Aria2 {
                    retune = d.gid.clone().map(|g| (g, d.connections));
                }
            }
            if !running {
                if let Some(n) = patch["name"].as_str().map(classify::sanitize_filename).filter(|n| !n.is_empty()) {
                    if d.done == 0 {
                        d.name = n;
                    }
                }
                if let Some(dir) = patch["dir"].as_str().filter(|x| !x.trim().is_empty()) {
                    if d.done == 0 {
                        d.dir = dir.trim().to_string();
                    }
                }
                if let Some(u) = patch["url"].as_str().filter(|u| classify::scheme_allowed(u)) {
                    // Refresh an expired link; the partial data is kept.
                    d.url = u.trim().to_string();
                }
            }
            if let Some(l) = patch["speedLimit"].as_u64() {
                d.options.speed_limit = (l > 0).then_some(l);
            }
            if let Some(c) = patch["checksum"].as_str() {
                d.options.checksum = (!c.trim().is_empty()).then(|| c.trim().to_string());
            }
            out = Some(d.clone());
        });
        if let Some((gid, n)) = retune {
            if let Some(a) = self.aria.read().await.clone() {
                let n = if n == 0 { 8 } else { n };
                let _ = a.change_option(&gid, json!({"split": n.to_string(), "max-connection-per-server": n.min(16).to_string()})).await;
            }
        }
        if let (Some(l), Engine::Kuhttp) = (patch["speedLimit"].as_u64(), d.engine) {
            self.kuhttp_download_limit(id, l);
        }
        if let (Some(l), Some(gid), Some(a)) = (patch["speedLimit"].as_u64(), d.gid.as_ref(), self.aria.read().await.clone()) {
            let _ = a.change_option(gid, json!({"max-download-limit": l.to_string()})).await;
        }
        out.ok_or_else(|| anyhow!("Download not found"))
    }

    // ───────────────────────────── queues ─────────────────────────────

    pub fn save_queue(&self, mut q: Queue) -> Result<Queue> {
        if q.id.is_empty() {
            q.id = uuid::Uuid::new_v4().to_string();
        }
        q.max_concurrent = q.max_concurrent.clamp(1, 32);
        q.name = q.name.trim().to_string();
        if q.name.is_empty() {
            bail!("A queue needs a name");
        }
        let mut st = lock(&self.st);
        match st.queues.iter_mut().find(|x| x.id == q.id) {
            Some(existing) => {
                q.running = existing.running;
                *existing = q.clone();
            }
            None => {
                q.position = st.queues.len() as i64;
                q.running = false;
                st.queues.push(q.clone());
            }
        }
        self.db.save_queue(&q)?;
        drop(st);
        self.emit(CoreEvent::QueuesChanged);
        self.wake.notify_one();
        Ok(q)
    }

    pub fn delete_queue(&self, id: &str) -> Result<()> {
        if id == MAIN_QUEUE {
            bail!("The main queue cannot be deleted");
        }
        let moved: Vec<Download> = {
            let mut st = lock(&self.st);
            st.queues.retain(|q| q.id != id);
            st.downloads
                .values_mut()
                .filter(|d| d.queue_id.as_deref() == Some(id))
                .map(|d| {
                    d.queue_id = Some(MAIN_QUEUE.into());
                    d.clone()
                })
                .collect()
        };
        for d in &moved {
            self.commit(d);
        }
        self.db.delete_queue(id)?;
        self.emit(CoreEvent::QueuesChanged);
        Ok(())
    }

    fn set_queue_running(&self, id: &str, running: bool, after: Option<&str>) -> Result<()> {
        let mut st = lock(&self.st);
        let q = st.queues.iter_mut().find(|q| q.id == id).ok_or_else(|| anyhow!("Unknown queue"))?;
        q.running = running;
        if let Some(a) = after {
            q.after = a.to_string();
        } else if !running {
            q.after = "none".into();
        }
        let q = q.clone();
        if running {
            st.queue_busy.insert(id.to_string());
        } else {
            st.queue_busy.remove(id);
        }
        self.db.save_queue(&q)?;
        drop(st);
        self.emit(CoreEvent::QueuesChanged);
        Ok(())
    }

    pub async fn start_queue(self: &Arc<Self>, id: &str, after: Option<&str>) -> Result<()> {
        // Failed items get another chance when the user starts the queue.
        let failed: Vec<String> = lock(&self.st)
            .downloads
            .values()
            .filter(|d| d.queue_id.as_deref() == Some(id) && matches!(d.status, Status::Error | Status::Paused))
            .map(|d| d.id.clone())
            .collect();
        self.set_queue_running(id, true, after)?;
        self.resume(&failed).await?;
        self.wake.notify_one();
        Ok(())
    }

    pub async fn stop_queue(self: &Arc<Self>, id: &str) -> Result<()> {
        self.set_queue_running(id, false, None)?;
        let running: Vec<Download> = lock(&self.st)
            .downloads
            .values()
            .filter(|d| d.queue_id.as_deref() == Some(id) && d.status.is_running())
            .cloned()
            .collect();
        for d in running {
            self.stop_engine(&d).await;
            self.update(&d.id, |d| {
                d.status = Status::Queued;
                d.speed = 0;
                d.active_connections = 0;
            });
        }
        Ok(())
    }

    /// Assign downloads to a queue (None = no queue, start directly).
    pub fn move_to_queue(&self, ids: &[String], queue: Option<String>) -> Result<()> {
        let queue = queue.filter(|q| !q.is_empty());
        let changed: Vec<Download> = {
            let mut st = lock(&self.st);
            if let Some(q) = &queue {
                if !st.queues.iter().any(|x| &x.id == q) {
                    bail!("Unknown queue");
                }
            }
            let mut pos = Self::next_position(&st, queue.as_deref());
            let mut out = Vec::new();
            for id in ids {
                if let Some(d) = st.downloads.get_mut(id) {
                    if d.queue_id != queue {
                        d.queue_id = queue.clone();
                        d.position = pos;
                        pos += 1;
                        if d.status == Status::Paused && queue.is_some() {
                            d.status = Status::Queued;
                        }
                        out.push(d.clone());
                    }
                }
            }
            out
        };
        for d in &changed {
            self.commit(d);
        }
        self.wake.notify_one();
        Ok(())
    }

    /// Reorder within a queue: "up", "down", "top", "bottom".
    pub fn reorder(&self, id: &str, direction: &str) -> Result<()> {
        let changed: Vec<Download> = {
            let mut st = lock(&self.st);
            let q = st.downloads.get(id).ok_or_else(|| anyhow!("Download not found"))?.queue_id.clone();
            let mut items: Vec<(String, i64)> = st
                .downloads
                .values()
                .filter(|d| d.queue_id == q && !d.status.is_finished())
                .map(|d| (d.id.clone(), d.position))
                .collect();
            items.sort_by_key(|(_, p)| *p);
            let Some(i) = items.iter().position(|(x, _)| x == id) else { return Ok(()) };
            let item = items.remove(i);
            let j = match direction {
                "up" => i.saturating_sub(1),
                "down" => (i + 1).min(items.len()),
                "top" => 0,
                _ => items.len(),
            };
            items.insert(j, item);
            let mut out = Vec::new();
            for (n, (x, old)) in items.iter().enumerate() {
                let np = n as i64 + 1;
                if *old != np {
                    if let Some(d) = st.downloads.get_mut(x) {
                        d.position = np;
                        out.push(d.clone());
                    }
                }
            }
            out
        };
        for d in &changed {
            self.commit(d);
        }
        self.wake.notify_one();
        Ok(())
    }

    // ───────────────────────────── schedules ─────────────────────────────

    pub fn save_schedule(&self, mut s: Schedule) -> Result<Schedule> {
        if s.id.is_empty() {
            s.id = uuid::Uuid::new_v4().to_string();
        }
        for t in std::iter::once(&s.start).chain(s.stop.iter()) {
            if chrono::NaiveTime::parse_from_str(t, "%H:%M").is_err() {
                bail!("Times must be in HH:MM format");
            }
        }
        if let Some(d) = s.date.as_deref().filter(|d| !d.is_empty()) {
            chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| anyhow!("Dates must be YYYY-MM-DD"))?;
        } else {
            s.date = None;
        }
        s.days.retain(|d| (1..=7).contains(d));
        if !matches!(s.after.as_str(), "none" | "shutdown" | "sleep" | "quit") {
            s.after = "none".into();
        }
        let mut st = lock(&self.st);
        if !st.queues.iter().any(|q| q.id == s.queue_id) {
            bail!("Unknown queue");
        }
        match st.schedules.iter_mut().find(|x| x.id == s.id) {
            Some(e) => {
                // Editing the time re-arms the schedule for today.
                if e.start != s.start {
                    s.last_start = None;
                }
                *e = s.clone();
            }
            None => st.schedules.push(s.clone()),
        }
        self.db.save_schedule(&s)?;
        drop(st);
        self.emit(CoreEvent::SchedulesChanged);
        Ok(s)
    }

    pub fn delete_schedule(&self, id: &str) -> Result<()> {
        lock(&self.st).schedules.retain(|s| s.id != id);
        self.db.delete_schedule(id)?;
        self.emit(CoreEvent::SchedulesChanged);
        Ok(())
    }

    async fn scheduler_loop(self: Arc<Self>) {
        let mut iv = tokio::time::interval(Duration::from_secs(15));
        loop {
            iv.tick().await;
            if self.shutting_down.load(Ordering::SeqCst) {
                return;
            }
            self.run_schedules(chrono::Local::now().naive_local()).await;
            let me = self.clone();
            tokio::spawn(async move { me.sync_queues().await });
        }
    }

    pub async fn run_schedules(self: &Arc<Self>, now: chrono::NaiveDateTime) {
        use chrono::Datelike;
        let today = now.format("%Y-%m-%d").to_string();
        let hm = now.format("%H:%M").to_string();
        let weekday = now.weekday().number_from_monday() as u8;
        let schedules = self.schedules();
        for mut s in schedules.into_iter().filter(|s| s.enabled) {
            let day_ok = match &s.date {
                Some(d) => *d == today,
                None => s.days.is_empty() || s.days.contains(&weekday),
            };
            let in_window = match &s.stop {
                Some(stop) if s.start <= *stop => hm >= s.start && hm < *stop,
                Some(stop) => hm >= s.start || hm < *stop,
                // Start-only schedules: catch up for an hour if the app was off.
                None => hm >= s.start && hm < add_minutes(&s.start, 60),
            };
            let active = lock(&self.st).schedule_active.contains_key(&s.id);
            if day_ok && in_window && s.last_start.as_deref() != Some(&today) && !active {
                tracing::info!("schedule {} starting queue {}", s.name, s.queue_id);
                let prev = self.settings().active_profile;
                if let Some(p) = s.profile.clone().filter(|p| !p.is_empty()) {
                    let _ = self.set_profile(&p).await;
                }
                if self.start_queue(&s.queue_id, Some(&s.after)).await.is_ok() {
                    s.last_start = Some(today.clone());
                    if s.stop.is_some() {
                        lock(&self.st).schedule_active.insert(s.id.clone(), Some(prev));
                    }
                    let _ = self.save_schedule_state(&s);
                    self.emit(CoreEvent::Notice {
                        level: "info".into(),
                        title: "Scheduled downloads started".into(),
                        message: format!("“{}” started its queue.", s.name),
                        download_id: None,
                    });
                }
            } else if active && !in_window {
                let prev = lock(&self.st).schedule_active.remove(&s.id).flatten();
                let _ = self.stop_queue(&s.queue_id).await;
                if s.profile.is_some() {
                    if let Some(p) = prev {
                        let _ = self.set_profile(&p).await;
                    }
                }
                s.last_stop = Some(today.clone());
                let _ = self.save_schedule_state(&s);
            }
        }
    }

    fn save_schedule_state(&self, s: &Schedule) -> Result<()> {
        let mut st = lock(&self.st);
        if let Some(e) = st.schedules.iter_mut().find(|x| x.id == s.id) {
            e.last_start = s.last_start.clone();
            e.last_stop = s.last_stop.clone();
        }
        drop(st);
        self.db.save_schedule(s)?;
        self.emit(CoreEvent::SchedulesChanged);
        Ok(())
    }

    // ───────────────────────────── power ─────────────────────────────

    pub fn set_after_all(&self, action: &str) {
        let a = if matches!(action, "shutdown" | "sleep" | "quit") { action } else { "none" };
        *lock(&self.after_all) = a.to_string();
    }

    pub fn after_all(&self) -> String {
        lock(&self.after_all).clone()
    }

    fn begin_power(self: &Arc<Self>, action: &str) {
        if action == "none" {
            return;
        }
        if action == "quit" {
            if let Some(h) = lock(&self.quit_hook).as_ref() {
                h();
            }
            return;
        }
        let (tx, rx) = oneshot::channel::<()>();
        if let Some(old) = lock(&self.power).replace(tx) {
            let _ = old.send(());
        }
        const SECONDS: u32 = 60;
        self.emit(CoreEvent::PowerCountdown { action: action.into(), seconds: SECONDS });
        let core = self.clone();
        let action = action.to_string();
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(SECONDS as u64)) => {
                    lock(&core.power).take();
                    core.flush_progress();
                    if let Err(e) = crate::power::perform(&action) {
                        core.emit(CoreEvent::Notice { level: "error".into(), title: "Power action failed".into(), message: e.to_string(), download_id: None });
                    }
                }
                _ = rx => core.emit(CoreEvent::PowerCancelled),
            }
        });
    }

    pub fn cancel_power(&self) {
        if let Some(tx) = lock(&self.power).take() {
            let _ = tx.send(());
        }
    }

    // ───────────────────────────── main loop ─────────────────────────────

    async fn main_loop(self: Arc<Self>) {
        let mut last_flush = Instant::now();
        loop {
            if self.shutting_down.load(Ordering::SeqCst) {
                return;
            }
            self.pump().await;
            let busy = {
                let st = lock(&self.st);
                st.downloads.values().any(|d| d.status.is_running() || d.status == Status::Seeding) || !st.retry_at.is_empty()
            };
            if busy {
                let t = self.tick.fetch_add(1, Ordering::Relaxed);
                self.poll_aria2(t).await;
                self.poll_kuhttp();
                self.publish_progress();
                if last_flush.elapsed() >= Duration::from_secs(5) {
                    self.flush_progress();
                    last_flush = Instant::now();
                }
            }
            self.check_drained();
            // Event driven when idle; 1 Hz while something is transferring.
            let wait = if busy { Duration::from_secs(1) } else { Duration::from_secs(30) };
            let _ = tokio::time::timeout(wait, self.wake.notified()).await;
        }
    }

    /// Start whatever is allowed to run.
    async fn pump(self: &Arc<Self>) {
        let s = self.settings();
        let starts: Vec<Download> = {
            let mut st = lock(&self.st);
            let now = Instant::now();
            let ready = |d: &Download, st: &State| d.status == Status::Queued && st.retry_at.get(&d.id).is_none_or(|t| *t <= now);
            let mut chosen: Vec<String> = Vec::new();
            let direct_running = st.downloads.values().filter(|d| d.queue_id.is_none() && d.status.is_running()).count();
            let mut direct: Vec<&Download> = st.downloads.values().filter(|d| d.queue_id.is_none() && ready(d, &st)).collect();
            direct.sort_by_key(|d| (d.position, d.created_at));
            let slots = (s.max_concurrent as usize).saturating_sub(direct_running);
            chosen.extend(direct.iter().take(slots).map(|d| d.id.clone()));
            for q in st.queues.iter().filter(|q| q.running) {
                let running = st.downloads.values().filter(|d| d.queue_id.as_deref() == Some(&q.id) && d.status.is_running()).count();
                let mut items: Vec<&Download> = st.downloads.values().filter(|d| d.queue_id.as_deref() == Some(&q.id) && ready(d, &st)).collect();
                items.sort_by_key(|d| (d.position, d.created_at));
                let slots = (q.max_concurrent as usize).saturating_sub(running);
                chosen.extend(items.iter().take(slots).map(|d| d.id.clone()));
            }
            let mut out = Vec::new();
            for id in chosen {
                st.retry_at.remove(&id);
                if let Some(d) = st.downloads.get_mut(&id) {
                    d.status = Status::Downloading;
                    d.error = None;
                    out.push(d.clone());
                }
            }
            out
        };
        for d in starts {
            self.commit(&d);
            let core = self.clone();
            tokio::spawn(async move {
                let id = d.id.clone();
                let r = match d.engine {
                    Engine::Aria2 => core.start_aria2(d).await,
                    Engine::Ytdlp => core.start_ytdlp(d),
                    Engine::Kuhttp => core.start_kuhttp(d),
                };
                if let Err(e) = r {
                    let msg = e.to_string();
                    core.log(&id, "error", &msg);
                    core.update(&id, |d| {
                        d.status = Status::Error;
                        d.error = Some(msg);
                    });
                    core.wake.notify_one();
                }
            });
        }
    }

    fn aria2_options(&self, d: &Download, gid: &str, s: &Settings) -> Value {
        let mut o = Map::new();
        let mut put = |k: &str, v: String| {
            o.insert(k.into(), Value::String(v));
        };
        put("gid", gid.into());
        put("dir", d.dir.clone());
        if matches!(d.kind, Kind::Http | Kind::Ftp | Kind::Sftp) && !d.name.is_empty() {
            put("out", d.name.clone());
        }
        let n = if d.connections == 0 { smart::initial_connections(d) } else { d.connections };
        put("split", n.to_string());
        put("max-connection-per-server", n.clamp(1, 16).to_string());
        put("user-agent", probe::user_agent(&d.options, s));
        if let Some(r) = d.options.referer.as_ref().filter(|r| !r.is_empty()) {
            put("referer", r.clone());
        }
        match d.kind {
            Kind::Ftp | Kind::Sftp => {
                if let Some(u) = &d.options.username {
                    put("ftp-user", u.clone());
                    put("ftp-passwd", d.options.password.clone().unwrap_or_default());
                }
            }
            _ => {
                if let Some(u) = &d.options.username {
                    put("http-user", u.clone());
                    put("http-passwd", d.options.password.clone().unwrap_or_default());
                }
            }
        }
        if let Some((algo, hex)) = d.options.checksum.as_deref().and_then(|c| crate::hash::parse_checksum(c).ok()) {
            put("checksum", format!("{algo}={hex}"));
            put("check-integrity", "true".into());
        }
        let proxy = d.options.proxy.clone().filter(|p| !p.is_empty()).unwrap_or(s.proxy.clone());
        if !proxy.trim().is_empty() {
            put("all-proxy", proxy.trim().into());
            if !s.proxy_user.is_empty() {
                put("all-proxy-user", s.proxy_user.clone());
                put("all-proxy-passwd", s.proxy_pass.clone());
            }
            if !s.no_proxy.trim().is_empty() {
                put("no-proxy", s.no_proxy.trim().into());
            }
        }
        if let Some(l) = d.options.speed_limit.filter(|l| *l > 0) {
            put("max-download-limit", l.to_string());
        }
        put("max-tries", s.max_tries.to_string());
        put("retry-wait", s.retry_wait.to_string());
        put("timeout", s.timeout.to_string());
        put("connect-timeout", s.connect_timeout.to_string());
        put("check-certificate", s.check_certificate.to_string());
        if let Some(sel) = d.options.select_files.as_ref().filter(|x| !x.is_empty()) {
            put("select-file", sel.clone());
        }
        if s.seed_ratio > 0.0 {
            put("seed-ratio", format!("{:.2}", s.seed_ratio));
        }
        if s.seed_time > 0 || s.seed_ratio > 0.0 {
            if s.seed_time > 0 {
                put("seed-time", s.seed_time.to_string());
            }
        } else {
            put("seed-time", "0".into());
        }
        match s.file_exists.as_str() {
            "overwrite" => {
                put("allow-overwrite", "true".into());
                put("auto-file-renaming", "false".into());
            }
            "skip" => {
                put("allow-overwrite", "false".into());
                put("auto-file-renaming", "false".into());
            }
            _ => {
                put("allow-overwrite", "false".into());
                put("auto-file-renaming", "true".into());
            }
        }
        let mut headers: Vec<Value> = d
            .options
            .headers
            .iter()
            .filter(|h| h.contains(':') && !h.contains(['\r', '\n']))
            .map(|h| Value::String(h.clone()))
            .collect();
        if let Some(c) = probe::cookie_header(&d.options).filter(|c| !c.contains(['\r', '\n'])) {
            headers.push(Value::String(format!("Cookie: {c}")));
        }
        if !headers.is_empty() {
            o.insert("header".into(), Value::Array(headers));
        }
        Value::Object(o)
    }

    async fn start_aria2(self: &Arc<Self>, d: Download) -> Result<()> {
        let a = self.aria2().await?;
        let s = self.settings();
        if let Some(gid) = &d.gid {
            if let Ok(st) = a.tell_status(gid).await {
                match st["status"].as_str() {
                    Some("paused") => {
                        a.unpause(gid).await?;
                        self.log(&d.id, "info", "Resumed");
                        return Ok(());
                    }
                    Some("active" | "waiting") => return Ok(()),
                    _ => a.purge(gid).await,
                }
            }
        }
        std::fs::create_dir_all(&d.dir).with_context(|| format!("Could not create the folder {}", d.dir))?;
        let gid = aria2::new_gid();
        let opts = self.aria2_options(&d, &gid, &s);
        let result = match (&d.options.torrent_data, &d.options.metalink_data) {
            (Some(t), _) => a.add_torrent(t, opts).await,
            (_, Some(m)) => a.add_metalink(m, opts).await,
            _ => {
                let mut uris = vec![d.url.clone()];
                uris.extend(d.mirrors.iter().cloned());
                a.add_uri(&uris, opts).await
            }
        };
        let gid = result.map_err(|e| anyhow!("The download engine rejected this download: {e}"))?;
        if d.connections == 0 {
            lock(&self.st).smart.insert(d.id.clone(), Smart::new(smart::initial_connections(&d)));
        }
        self.log(&d.id, "info", format!("Started with aria2 (gid {gid})"));
        self.update(&d.id, |x| {
            x.gid = Some(gid);
        });
        Ok(())
    }

    fn start_ytdlp(self: &Arc<Self>, d: Download) -> Result<()> {
        let env = self.yt_env(&d.options)?;
        let s = self.settings();
        let media = d.options.media.clone().unwrap_or_default();
        let active_yt = lock(&self.yt).len() as u64 + 1;
        let profile_limit = s.profile().download;
        let speed_limit = d.options.speed_limit.filter(|l| *l > 0).or((profile_limit > 0).then(|| profile_limit / active_yt));
        let name_stem = d.meta.media_title.clone();
        let job = YtJob { url: d.url.clone(), dir: PathBuf::from(&d.dir), media, speed_limit, name_stem };
        let (tx, rx) = oneshot::channel();
        lock(&self.yt).insert(d.id.clone(), tx);
        self.log(&d.id, "info", format!("Started with yt-dlp{}", if env.ffmpeg.is_some() { "" } else { " (FFmpeg not found: merging and conversion unavailable)" }));
        let core = self.clone();
        let id = d.id.clone();
        tokio::spawn(async move {
            let c2 = core.clone();
            let id2 = id.clone();
            let res = ytdlp::run(&env, &job, rx, move |ev| c2.on_yt_event(&id2, ev)).await;
            if let Some(c) = &env.cookies_file {
                let _ = std::fs::remove_file(c);
            }
            lock(&core.yt).remove(&id);
            core.on_yt_finished(&id, res);
        });
        Ok(())
    }

    fn on_yt_event(&self, id: &str, ev: YtEvent) {
        let mut st = lock(&self.st);
        let Some(d) = st.downloads.get_mut(id) else { return };
        match ev {
            YtEvent::Progress { done, total, speed, playlist_index, .. } => {
                if d.status == Status::Processing {
                    d.status = Status::Downloading;
                }
                d.done = done;
                if total > 0 {
                    d.total = total;
                }
                d.speed = speed;
                d.meta.playlist_index = playlist_index;
            }
            YtEvent::Processing => {
                d.status = Status::Processing;
                d.speed = 0;
            }
            YtEvent::File(p) => {
                if let Some(n) = p.file_name() {
                    if !d.options.media.as_ref().is_some_and(|m| m.playlist) {
                        d.name = n.to_string_lossy().into_owned();
                    }
                }
                d.file_path = Some(p.to_string_lossy().into_owned());
                let snap = d.clone();
                drop(st);
                self.commit(&snap);
            }
            YtEvent::Log(l) => {
                drop(st);
                let level = if l.starts_with("ERROR") { "error" } else if l.starts_with("WARNING") { "warn" } else { "info" };
                self.log(id, level, l);
            }
        }
    }

    fn on_yt_finished(self: &Arc<Self>, id: &str, res: Result<Vec<PathBuf>>) {
        let s = self.settings();
        match res {
            Ok(files) => {
                let size: i64 = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len() as i64).sum();
                let mut name = String::new();
                self.update(id, |d| {
                    d.status = Status::Completed;
                    d.completed_at = Some(now_ms());
                    d.speed = 0;
                    if size > 0 {
                        d.total = size;
                    }
                    d.done = d.total;
                    if files.len() > 1 && d.options.media.as_ref().is_some_and(|m| m.playlist) {
                        // For playlists point at the folder.
                        d.file_path = files[0].parent().map(|p| p.to_string_lossy().into_owned());
                    }
                    name = d.name.clone();
                });
                self.log(id, "info", "Completed");
                let path = self.get(id).and_then(|d| d.file_path);
                self.emit(CoreEvent::Completed { id: id.into(), name, path });
            }
            Err(e) if e.to_string() == "cancelled" => {}
            Err(e) => {
                let msg = e.to_string();
                let transient = {
                    let l = msg.to_ascii_lowercase();
                    l.contains("timed out") || l.contains("could not reach") || l.contains("connection") || l.contains("429")
                };
                let mut name = String::new();
                let retried = self
                    .update(id, |d| {
                        name = d.name.clone();
                        d.speed = 0;
                        if transient && d.meta.retries < s.auto_retry {
                            d.meta.retries += 1;
                            d.status = Status::Queued;
                            true
                        } else {
                            d.status = Status::Error;
                            d.error = Some(msg.clone());
                            false
                        }
                    })
                    .unwrap_or(false);
                self.log(id, "error", &msg);
                if retried {
                    let n = self.get(id).map(|d| d.meta.retries).unwrap_or(1);
                    lock(&self.st).retry_at.insert(id.into(), Instant::now() + backoff(n));
                } else {
                    self.emit(CoreEvent::Notice { level: "error".into(), title: format!("Download failed: {name}"), message: msg, download_id: Some(id.into()) });
                }
            }
        }
        self.wake.notify_one();
    }

    async fn poll_aria2(self: &Arc<Self>, tick: u64) {
        let running: Vec<(String, String, bool)> = {
            let st = lock(&self.st);
            st.downloads
                .values()
                .filter(|d| d.engine == Engine::Aria2 && matches!(d.status, Status::Downloading | Status::Seeding))
                .filter_map(|d| d.gid.clone().map(|g| (d.id.clone(), g, d.file_path.is_none())))
                .collect()
        };
        if running.is_empty() {
            return;
        }
        let Some(a) = self.aria.read().await.clone() else { return };
        let calls = running
            .iter()
            .map(|(_, gid, no_path)| {
                let with_files = *no_path || tick.is_multiple_of(5);
                let keys: Vec<&str> = STATUS_KEYS.iter().copied().chain(with_files.then_some("files")).collect();
                ("aria2.tellStatus", vec![json!(gid), json!(keys)])
            })
            .collect();
        let results = match a.multicall(calls).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("aria2 poll failed: {e:#}");
                return;
            }
        };
        let s = self.settings();
        let mut after = Vec::new();
        for ((id, gid, _), r) in running.into_iter().zip(results) {
            match r {
                Ok(v) => self.apply_status(&id, &gid, &v, &s, &mut after),
                Err(e) if e.message.contains("is not found") => {
                    // aria2 lost it (e.g. it was restarted): restart from the control file.
                    self.update(&id, |d| {
                        d.gid = None;
                        d.status = Status::Queued;
                    });
                }
                Err(e) => tracing::warn!("tellStatus {gid}: {e}"),
            }
        }
        for a2 in after {
            match a2 {
                After::Completed { id, name, path } => {
                    self.log(&id, "info", "Completed");
                    self.emit(CoreEvent::Completed { id, name, path });
                }
                After::Failed { id, name, message } => {
                    self.log(&id, "error", &message);
                    self.emit(CoreEvent::Notice { level: "error".into(), title: format!("Download failed: {name}"), message, download_id: Some(id) });
                }
                After::Purge(gid) => {
                    let _ = a.call("aria2.removeDownloadResult", vec![json!(gid)]).await;
                }
                After::Retune { gid, connections } => {
                    let _ = a
                        .change_option(&gid, json!({"split": connections.to_string(), "max-connection-per-server": connections.min(16).to_string()}))
                        .await;
                }
            }
        }
    }

    fn apply_status(&self, id: &str, gid: &str, v: &Value, s: &Settings, after: &mut Vec<After>) {
        let p = aria2::parse_i64;
        let status = v["status"].as_str().unwrap_or("");
        let mut guard = lock(&self.st);
        let st = &mut *guard;
        let mut smart = st.smart.remove(id);
        let Some(d) = st.downloads.get_mut(id) else { return };
        let mut changed_state = false;
        let mut log_lines: Vec<(String, String)> = Vec::new();
        let total = p(&v["totalLength"]);
        let done = p(&v["completedLength"]);
        if total > 0 {
            d.total = total;
        }
        d.done = done;
        d.uploaded = p(&v["uploadLength"]);
        d.speed = p(&v["downloadSpeed"]);
        d.upload_speed = p(&v["uploadSpeed"]);
        d.active_connections = p(&v["connections"]) as u32;
        let bt = v.get("bittorrent").filter(|b| b.is_object());
        if let Some(bt) = bt {
            if let Some(n) = bt["info"]["name"].as_str().filter(|n| !n.is_empty()) {
                if d.name != n {
                    d.name = n.to_string();
                    changed_state = true;
                }
            }
            if let Some(h) = v["infoHash"].as_str() {
                d.meta.info_hash = Some(h.to_string());
            }
            d.meta.num_seeders = Some(p(&v["numSeeders"]));
        }
        if let Some(files) = v["files"].as_array() {
            let entries: Vec<FileEntry> = files
                .iter()
                .map(|f| FileEntry {
                    path: f["path"].as_str().unwrap_or("").to_string(),
                    length: p(&f["length"]),
                    completed: p(&f["completedLength"]),
                    selected: f["selected"] == "true",
                })
                .collect();
            if let Some(first) = entries.first().filter(|f| !f.path.is_empty() && !f.path.starts_with("[METADATA]")) {
                let path = if bt.is_some() && entries.len() > 1 {
                    Path::new(&d.dir).join(&d.name).to_string_lossy().into_owned()
                } else {
                    first.path.clone()
                };
                if d.file_path.as_deref() != Some(&path) {
                    if bt.is_none() {
                        if let Some(n) = Path::new(&path).file_name() {
                            d.name = n.to_string_lossy().into_owned();
                        }
                    }
                    d.file_path = Some(path);
                    changed_state = true;
                }
            }
            if d.kind.is_bittorrent() {
                d.meta.files = entries;
            }
        }
        match status {
            "active" | "waiting" => {
                let seeding = bt.is_some() && total > 0 && done >= total && !d.status.is_finished();
                if seeding && d.status != Status::Seeding {
                    d.status = Status::Seeding;
                    d.completed_at = Some(now_ms());
                    changed_state = true;
                    after.push(After::Completed { id: id.into(), name: d.name.clone(), path: d.file_path.clone() });
                }
                if let Some(sm) = smart.as_mut().filter(|_| d.status == Status::Downloading) {
                    match sm.observe(d, Instant::now()) {
                        SmartAction::None => {}
                        SmartAction::Set { connections, note } => {
                            log_lines.push(("info".into(), note.clone()));
                            d.meta.smart_note = Some(note);
                            after.push(After::Retune { gid: gid.into(), connections });
                        }
                    }
                }
            }
            "complete" => {
                let followed = v["followedBy"].as_array().and_then(|a| a.first()).and_then(|g| g.as_str());
                if let Some(next) = followed {
                    // Metadata (magnet / .torrent URL / metalink) resolved to the real download.
                    d.gid = Some(next.to_string());
                    if d.kind == Kind::Magnet || d.kind == Kind::Http {
                        d.kind = Kind::Torrent;
                    }
                    d.total = 0;
                    d.done = 0;
                    log_lines.push(("info".into(), "Metadata received, starting transfer".into()));
                } else {
                    d.status = Status::Completed;
                    d.completed_at.get_or_insert(now_ms());
                    d.done = d.total.max(done);
                    if d.file_path.is_none() {
                        d.file_path = Some(Path::new(&d.dir).join(&d.name).to_string_lossy().into_owned());
                    }
                    d.speed = 0;
                    d.upload_speed = 0;
                    d.active_connections = 0;
                    if d.options.checksum.is_some() {
                        d.meta.verified = d.options.checksum.clone();
                    }
                    after.push(After::Completed { id: id.into(), name: d.name.clone(), path: d.file_path.clone() });
                }
                after.push(After::Purge(gid.into()));
                changed_state = true;
                smart = None;
            }
            "error" => {
                let code = p(&v["errorCode"]);
                let raw = v["errorMessage"].as_str().unwrap_or("").to_string();
                let lower = raw.to_ascii_lowercase();
                let conn_limited = d.connections == 0
                    && smart.as_ref().is_some_and(|s| s.target > 1)
                    && (lower.contains("503") || lower.contains("429") || lower.contains("too many") || code == 29);
                d.speed = 0;
                d.active_connections = 0;
                changed_state = true;
                after.push(After::Purge(gid.into()));
                if conn_limited {
                    let sm = smart.get_or_insert_with(|| Smart::new(4));
                    let n = (sm.target / 2).max(1);
                    sm.target = n;
                    let note = format!("Server rejected additional connections. KuDownloader reduced the connection count to {n} and will retry automatically.");
                    log_lines.push(("warn".into(), note.clone()));
                    d.meta.smart_note = Some(note);
                    d.connections = 0;
                    d.status = Status::Queued;
                    d.gid = None;
                    st_retry(&mut st.retry_at, id, Duration::from_secs(3));
                } else if aria2::is_retryable(code) && d.meta.retries < s.auto_retry {
                    d.meta.retries += 1;
                    let n = d.meta.retries;
                    log_lines.push(("warn".into(), format!("{} Retrying automatically ({n}/{}).", aria2::describe_error(code, &raw), s.auto_retry)));
                    d.status = Status::Queued;
                    d.gid = None;
                    st_retry(&mut st.retry_at, id, backoff(n));
                } else {
                    let message = aria2::describe_error(code, &raw);
                    d.status = Status::Error;
                    d.error = Some(message.clone());
                    after.push(After::Failed { id: id.into(), name: d.name.clone(), message });
                }
                smart = None;
            }
            "removed" => {
                d.status = Status::Paused;
                d.gid = None;
                d.speed = 0;
                changed_state = true;
                smart = None;
            }
            _ => {}
        }
        if let Some(sm) = smart {
            st.smart.insert(id.to_string(), sm);
        }
        let snapshot = changed_state.then(|| st.downloads.get(id).cloned()).flatten();
        drop(guard);
        for (level, l) in log_lines {
            self.log(id, &level, l);
        }
        if let Some(d) = snapshot {
            self.commit(&d);
        }
    }

    fn publish_progress(&self) {
        let st = lock(&self.st);
        let mut items = Vec::new();
        let (mut down, mut up) = (0, 0);
        for d in st.downloads.values().filter(|d| d.status.is_running() || d.status == Status::Seeding) {
            down += d.speed;
            up += d.upload_speed;
            items.push(ProgressItem {
                id: d.id.clone(),
                status: d.status,
                done: d.done,
                total: d.total,
                speed: d.speed,
                upload_speed: d.upload_speed,
                active_connections: d.active_connections,
                eta: d.eta(),
            });
        }
        drop(st);
        self.emit(CoreEvent::Progress { items, download_speed: down, upload_speed: up });
    }

    fn flush_progress(&self) {
        let items: Vec<(String, i64, i64, i64)> = lock(&self.st)
            .downloads
            .values()
            .filter(|d| d.status.is_running() || d.status == Status::Seeding)
            .map(|d| (d.id.clone(), d.done, d.total, d.uploaded))
            .collect();
        if let Err(e) = self.db.save_progress(&items) {
            tracing::error!("saving progress: {e:#}");
        }
    }

    /// Queue drain / everything-done detection for notifications and power actions.
    fn check_drained(self: &Arc<Self>) {
        let mut done_queues = Vec::new();
        let all_done;
        {
            let mut st = lock(&self.st);
            let busy_ids: Vec<String> = st.queue_busy.iter().cloned().collect();
            for qid in busy_ids {
                let pending = st.downloads.values().any(|d| {
                    d.queue_id.as_deref() == Some(&qid) && (d.status.is_running() || d.status == Status::Queued)
                });
                if !pending {
                    st.queue_busy.remove(&qid);
                    if let Some(q) = st.queues.iter_mut().find(|q| q.id == qid && q.running) {
                        q.running = false;
                        let after = std::mem::replace(&mut q.after, "none".into());
                        let _ = self.db.save_queue(q);
                        done_queues.push((q.id.clone(), q.name.clone(), after));
                    }
                }
            }
            let busy = st.downloads.values().any(|d| d.status.is_running() || d.status == Status::Queued && (d.queue_id.is_none() || st.queues.iter().any(|q| q.running && Some(&q.id) == d.queue_id.as_ref())));
            all_done = st.any_busy && !busy;
            st.any_busy = busy;
        }
        for (id, name, after) in done_queues {
            self.emit(CoreEvent::QueuesChanged);
            self.emit(CoreEvent::QueueDone { queue_id: id, name });
            self.begin_power(&after);
        }
        if all_done {
            let a = self.after_all();
            if a != "none" {
                self.set_after_all("none");
                self.begin_power(&a);
            }
        }
    }

    // ───────────────────────────── details ─────────────────────────────

    pub async fn details(self: &Arc<Self>, id: &str) -> Result<Value> {
        let d = self.get(id).ok_or_else(|| anyhow!("Download not found"))?;
        let mut out = json!({ "log": self.logs_for(id), "smart": lock(&self.st).smart.get(id).map(|s| s.target) });
        if d.engine == Engine::Kuhttp {
            let k = self.kuhttp_details(id);
            out["segments"] = k["segments"].clone();
            out["kuhttp"] = k["kuhttp"].clone();
            return Ok(out);
        }
        if d.engine != Engine::Aria2 {
            return Ok(out);
        }
        let (Some(gid), Some(a)) = (d.gid.clone(), self.aria.read().await.clone()) else { return Ok(out) };
        let mut calls = vec![
            ("aria2.tellStatus", vec![json!(gid), json!(["bitfield", "numPieces", "pieceLength", "bittorrent", "files", "connections"])]),
            ("aria2.getServers", vec![json!(gid)]),
        ];
        if d.kind.is_bittorrent() {
            calls.push(("aria2.getPeers", vec![json!(gid)]));
        }
        let res = a.multicall(calls).await?;
        let mut it = res.into_iter();
        if let Some(Ok(st)) = it.next() {
            out["pieces"] = json!({
                "count": aria2::parse_i64(&st["numPieces"]),
                "length": aria2::parse_i64(&st["pieceLength"]),
                "bitfield": st["bitfield"],
            });
            out["files"] = st["files"].clone();
            if let Some(list) = st["bittorrent"]["announceList"].as_array() {
                let trackers: Vec<Value> = list.iter().filter_map(|t| t.as_array()).flatten().cloned().collect();
                out["trackers"] = Value::Array(trackers);
            }
            out["comment"] = st["bittorrent"]["comment"].clone();
        }
        if let Some(Ok(servers)) = it.next() {
            let conns: Vec<Value> = servers
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|f| f["servers"].as_array().cloned().unwrap_or_default())
                .map(|s| json!({"uri": s["uri"], "currentUri": s["currentUri"], "speed": aria2::parse_i64(&s["downloadSpeed"])}))
                .collect();
            out["connections"] = Value::Array(conns);
        }
        if let Some(Ok(peers)) = it.next() {
            let peers: Vec<Value> = peers
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| json!({
                    "ip": p["ip"], "port": p["port"], "seeder": p["seeder"] == "true",
                    "downloadSpeed": aria2::parse_i64(&p["downloadSpeed"]),
                    "uploadSpeed": aria2::parse_i64(&p["uploadSpeed"]),
                    "bitfield": p["bitfield"],
                }))
                .collect();
            out["peers"] = Value::Array(peers);
        }
        Ok(out)
    }

    pub async fn verify(&self, id: &str, algo: &str) -> Result<String> {
        let d = self.get(id).ok_or_else(|| anyhow!("Download not found"))?;
        let path = d.file_path.clone().map(PathBuf::from).unwrap_or_else(|| Path::new(&d.dir).join(&d.name));
        if !path.is_file() {
            bail!("The file is not on disk anymore: {}", path.display());
        }
        let algo2 = algo.to_string();
        let hash = tokio::task::spawn_blocking(move || crate::hash::hash_file(&path, &algo2)).await??;
        let expected = d.options.checksum.as_deref().and_then(|c| crate::hash::parse_checksum(c).ok());
        let algo_n = crate::hash::normalize_algo(algo).unwrap_or("sha-256");
        if let Some((ea, eh)) = expected.filter(|(a, _)| a == algo_n) {
            if eh != hash {
                self.update(id, |d| d.meta.verified = None);
                bail!("Checksum mismatch: expected {eh}, got {hash}. The file may be corrupted — download it again.");
            }
            self.update(id, |d| d.meta.verified = Some(format!("{ea}={eh}")));
        }
        Ok(hash)
    }

    pub async fn engine_info(self: &Arc<Self>) -> Value {
        let s = self.settings();
        let aria = paths::find_binary("aria2c", Some(&s.aria2_path));
        let yt = paths::find_binary("yt-dlp", Some(&s.ytdlp_path));
        let ff = paths::find_binary("ffmpeg", Some(&s.ffmpeg_path));
        let aria_version = self.aria.read().await.clone().map(|a| a.version.clone());
        let yt_version = match &yt {
            Some(p) => ytdlp::version(p).await,
            None => None,
        };
        json!({
            "aria2": {"path": aria.map(|p| p.display().to_string()), "version": aria_version, "running": aria_version.is_some()},
            "ytdlp": {"path": yt.map(|p| p.display().to_string()), "version": yt_version},
            "ffmpeg": {"path": ff.map(|p| p.display().to_string())},
            "dataDir": paths::data_dir().display().to_string(),
        })
    }

    /// Download and install yt-dlp or ffmpeg from the official releases
    /// (checksum-verified). Progress arrives as `ToolProgress` events.
    pub async fn install_tool(&self, name: &str) -> Result<String> {
        let tool = crate::tools::Tool::parse(name).ok_or_else(|| anyhow!("Unknown tool: {name}"))?;
        // One download per tool: a second request (another screen, a double
        // click) must not write the same files concurrently.
        if !lock(&self.tool_jobs).insert(tool.name().to_string()) {
            bail!("{} is already being downloaded.", tool.name());
        }
        let r = self.install_tool_inner(tool).await;
        lock(&self.tool_jobs).remove(tool.name());
        let (ok, message) = match &r {
            Ok(p) => (true, p.clone()),
            Err(e) => (false, format!("{e:#}")),
        };
        self.emit(CoreEvent::ToolDone { tool: tool.name().into(), ok, message });
        r
    }

    /// Tools currently downloading (for a window that opens mid-download).
    pub fn tool_jobs(&self) -> Vec<String> {
        lock(&self.tool_jobs).iter().cloned().collect()
    }

    async fn install_tool_inner(&self, tool: crate::tools::Tool) -> Result<String> {
        let s = self.settings();
        let mut b = reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).redirect(reqwest::redirect::Policy::limited(10));
        if !s.proxy.trim().is_empty() {
            let mut p = reqwest::Proxy::all(s.proxy.trim())?;
            if !s.proxy_user.is_empty() {
                p = p.basic_auth(&s.proxy_user, &s.proxy_pass);
            }
            b = b.proxy(p);
        }
        let client = b.build()?;
        let mut last = Instant::now() - Duration::from_secs(1);
        let tool_name = tool.name().to_string();
        let path = crate::tools::install(tool, &client, |done, total| {
            if last.elapsed() >= Duration::from_millis(200) || total == Some(done) {
                last = Instant::now();
                self.emit(CoreEvent::ToolProgress { tool: tool_name.clone(), done, total: total.unwrap_or(0) });
            }
        })
        .await?;
        tracing::info!("{} installed at {}", tool.name(), path.display());
        Ok(path.display().to_string())
    }

    pub async fn update_ytdlp(&self) -> Result<String> {
        let s = self.settings();
        let yt = paths::find_binary("yt-dlp", Some(&s.ytdlp_path)).ok_or_else(|| anyhow!("yt-dlp was not found"))?;
        ytdlp::self_update(&yt).await
    }

    /// Persist and stop engines. Running downloads stay "downloading" in the
    /// database so the next start resumes them.
    pub async fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        self.flush_progress();
        let txs: Vec<oneshot::Sender<()>> = lock(&self.yt).drain().map(|(_, t)| t).collect();
        for t in txs {
            let _ = t.send(());
        }
        self.kuhttp_shutdown().await;
        if let Some(a) = self.aria.write().await.take() {
            a.shutdown().await;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// Browsers yt-dlp can read cookies from (`--cookies-from-browser`).
pub const ALLOWED_COOKIE_BROWSERS: &[&str] = &["firefox", "chrome", "chromium", "edge", "brave", "opera", "vivaldi", "whale", "safari"];

fn st_retry(map: &mut HashMap<String, Instant>, id: &str, after: Duration) {
    map.insert(id.to_string(), Instant::now() + after);
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_secs((5u64 << attempt.min(5)).min(120))
}

fn add_minutes(hm: &str, mins: i64) -> String {
    chrono::NaiveTime::parse_from_str(hm, "%H:%M")
        .map(|t| {
            let t2 = t + chrono::Duration::minutes(mins);
            if t2 < t { "23:59".to_string() } else { t2.format("%H:%M").to_string() }
        })
        .unwrap_or_else(|_| hm.to_string())
}

fn magnet_name(url: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == "dn").then(|| {
            let v = v.replace('+', " ");
            classify::sanitize_filename(&percent_encoding::percent_decode_str(&v).decode_utf8_lossy())
        })
    })
}

fn delete_download_files(d: &Download) {
    let main = d.file_path.clone().map(PathBuf::from).unwrap_or_else(|| Path::new(&d.dir).join(&d.name));
    let mut candidates = vec![main.clone(), PathBuf::from(format!("{}.aria2", main.display()))];
    if d.engine == Engine::Ytdlp {
        // yt-dlp partials: "<stem>.<fmt>.<ext>.part", "*.ytdl", fragments.
        let stem = d.meta.media_title.as_deref().map(classify::sanitize_filename).unwrap_or_else(|| {
            Path::new(&d.name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
        });
        if !stem.is_empty() {
            if let Ok(rd) = std::fs::read_dir(&d.dir) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().into_owned();
                    if n.starts_with(&stem) && (n.ends_with(".part") || n.ends_with(".ytdl") || n.contains(".part-Frag")) {
                        candidates.push(e.path());
                    }
                }
            }
        }
    }
    for p in candidates {
        if p.is_file() {
            let _ = std::fs::remove_file(&p);
        } else if p.is_dir() && d.kind.is_bittorrent() && p.starts_with(&d.dir) && p != Path::new(&d.dir) {
            let _ = std::fs::remove_dir_all(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(magnet_name("magnet:?xt=urn:btih:abc&dn=Ubuntu+24.04%20ISO").unwrap(), "Ubuntu 24.04 ISO");
        assert_eq!(add_minutes("23:30", 60), "23:59");
        assert_eq!(add_minutes("02:00", 60), "03:00");
        assert_eq!(backoff(1), Duration::from_secs(10));
        assert_eq!(backoff(10), Duration::from_secs(120));
    }
}
