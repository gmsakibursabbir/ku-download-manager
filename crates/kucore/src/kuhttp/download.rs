//! Download orchestration: probe → (resume | fresh) → workers → verify →
//! finalize. Falls back from segmented to single-stream automatically.

use super::client::{self, Probe, RequestCtx, Validators};
use super::config::KuHttpConfig;
use super::connection::ConnStats;
use super::error::KuError;
use super::events::{KuEvent, SegmentInfo, State, Status};
use super::integrity::{self, Checksum};
use super::limiter::RateLimiter;
use super::recovery::{self, Decision};
use super::resume::{self, ResumeState};
use super::retry::RetryPolicy;
use super::scheduler::{self, Controller};
use super::segment::SegmentMap;
use super::storage::{self, Storage};
use super::worker::{self, Exit};
use reqwest::Client;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinSet;
use url::Url;

#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    Run,
    Pause,
    Cancel { delete: bool },
}

/// What KuCore asks KuHTTP to download.
#[derive(Clone, Debug, Default)]
pub struct DownloadRequest {
    pub id: String,
    pub url: String,
    pub dest_dir: PathBuf,
    /// Final file name; derived from the server when None.
    pub file_name: Option<String>,
    pub headers: Vec<(String, String)>,
    pub cookies: Option<String>,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
    pub basic_auth: Option<(String, String)>,
    pub checksum: Option<String>,
    /// Fixed connection count (benchmarks/testing); None = adaptive.
    pub connections: Option<u32>,
    /// Bytes/s, 0 = unlimited.
    pub speed_limit: u64,
    pub overwrite: bool,
}

/// State shared by the workers of one download attempt.
pub struct Job {
    pub id: String,
    pub client: Client,
    pub ctx: RequestCtx,
    pub cfg: KuHttpConfig,
    pub url: RwLock<Url>,
    pub total: Option<u64>,
    pub validators: Validators,
    pub segmented: bool,
    pub ranges: bool,
    pub map: Mutex<SegmentMap>,
    pub unknown_done: AtomicU64,
    pub unknown_eof: AtomicBool,
    pub storage: Storage,
    pub limiter: Arc<RateLimiter>,
    pub global: Arc<RateLimiter>,
    pub control: watch::Receiver<Control>,
    pub target: AtomicU32,
    pub stats: ConnStats,
    pub controller: Mutex<Controller>,
    pub events: broadcast::Sender<KuEvent>,
    pub retry: RetryPolicy,
    pub min_split: u64,
    http_version: Mutex<Option<String>>,
    refresh_lock: tokio::sync::Mutex<()>,
}

impl Job {
    pub fn stopping(&self) -> bool {
        *self.control.borrow() != Control::Run
    }

    /// Sleep, returning early (true) if paused/cancelled meanwhile.
    pub async fn sleep_or_stop(&self, d: Duration) -> bool {
        let mut rx = self.control.clone();
        tokio::select! {
            _ = tokio::time::sleep(d) => self.stopping(),
            _ = rx.changed() => true,
        }
    }

    pub fn redirected(&self) -> bool {
        *self.url.read().unwrap() != self.ctx.original
    }

    /// Resolve the original URL again (expired signed redirect targets).
    /// Serialized: if another worker already refreshed `failed`, reuse it.
    pub async fn refresh_url(&self, failed: &Url) -> Result<(), KuError> {
        let _guard = self.refresh_lock.lock().await;
        if *self.url.read().unwrap() != *failed {
            return Ok(());
        }
        let p = client::probe(&self.client, &self.ctx, &self.cfg).await?;
        if p.total != self.total {
            return Err(KuError::ResourceChanged("size changed".into()));
        }
        if let Some(why) = self.validators.changed(&p.validators) {
            return Err(KuError::ResourceChanged(why));
        }
        *self.url.write().unwrap() = p.final_url;
        Ok(())
    }

    pub fn note_version(&self, v: reqwest::Version) {
        let mut g = self.http_version.lock().unwrap();
        if g.is_none() {
            *g = Some(format!("{v:?}"));
        }
    }

    pub fn downloaded(&self) -> u64 {
        if self.total.is_some() {
            self.map.lock().unwrap().downloaded()
        } else {
            self.unknown_done.load(Ordering::SeqCst)
        }
    }
}

enum AttemptEnd {
    Done,
    Stopped(Control),
    /// Start over with a new probe (resource changed / range fallback).
    Restart { reason: String, single_stream: bool },
    Fatal(KuError),
}

pub struct Runner {
    pub req: DownloadRequest,
    pub client: Client,
    pub cfg: KuHttpConfig,
    pub global: Arc<RateLimiter>,
    pub limiter: Arc<RateLimiter>,
    pub control: watch::Receiver<Control>,
    pub status: Arc<Mutex<Status>>,
    pub segments: Arc<Mutex<Vec<SegmentInfo>>>,
    pub events: broadcast::Sender<KuEvent>,
}

impl Runner {
    fn emit(&self, e: KuEvent) {
        let _ = self.events.send(e);
    }

    fn set_state(&self, state: State, error: Option<String>, message: Option<String>) -> Status {
        let mut s = self.status.lock().unwrap();
        s.state = state;
        if error.is_some() {
            s.error = error;
        }
        if message.is_some() {
            s.message = message;
        }
        if state.is_terminal() {
            s.speed = 0;
            s.eta = None;
            s.connections = 0;
            s.active_segments = 0;
        }
        s.clone()
    }

    pub async fn run(self) -> Status {
        let id = self.req.id.clone();
        self.emit(KuEvent::Started { id: id.clone() });
        let ctx = match RequestCtx::new(
            &self.req.url,
            &self.req.headers,
            self.req.cookies.as_deref(),
            self.req.referer.as_deref(),
            self.req.user_agent.as_deref().unwrap_or(&self.cfg.user_agent),
            self.req.basic_auth.as_ref().map(|(u, p)| (u.as_str(), p.as_str())),
        ) {
            Ok(c) => c,
            Err(e) => return self.fail(e, false),
        };
        let checksum = match self.req.checksum.as_deref().map(Checksum::parse).transpose() {
            Ok(c) => c,
            Err(e) => return self.fail(e, false),
        };
        let retry = RetryPolicy { max_retries: self.cfg.max_retries, initial: self.cfg.initial_backoff, max: self.cfg.max_backoff };
        let mut force_single = false;
        let mut restarts = 0;
        loop {
            self.set_state(State::Probing, None, None);
            self.emit(KuEvent::Probing { id: id.clone() });
            let probe = match self.probe_with_retry(&ctx, &retry).await {
                Ok(p) => p,
                Err(KuError::Paused) => return self.paused(),
                Err(KuError::Cancelled) => return self.cancelled(None),
                Err(e) => {
                    let transient = e.is_transient();
                    return self.fail(e, transient);
                }
            };
            match self.attempt(&ctx, &probe, &retry, force_single, checksum.as_ref()).await {
                AttemptEnd::Done => return self.status.lock().unwrap().clone(),
                AttemptEnd::Stopped(Control::Cancel { delete }) => return self.cancelled(Some(delete)),
                AttemptEnd::Stopped(_) => return self.paused(),
                AttemptEnd::Fatal(e) => {
                    let recoverable = e.is_transient() || e == KuError::DiskFull;
                    if e == KuError::DiskFull {
                        let st = self.set_state(State::Paused, Some(e.to_string()), None);
                        self.emit(KuEvent::Paused { id: id.clone() });
                        return st;
                    }
                    return self.fail(e, recoverable);
                }
                AttemptEnd::Restart { reason, single_stream } => {
                    restarts += 1;
                    force_single |= single_stream;
                    self.emit(KuEvent::Restarted { id: id.clone(), reason: reason.clone() });
                    tracing::info!("kuhttp {id}: restarting ({reason})");
                    if restarts > 3 {
                        return self.fail(KuError::ResourceChanged(format!("{reason}; gave up after 3 restarts")), true);
                    }
                }
            }
        }
    }

    async fn probe_with_retry(&self, ctx: &RequestCtx, retry: &RetryPolicy) -> Result<Probe, KuError> {
        let mut attempt = 0;
        loop {
            match client::probe(&self.client, ctx, &self.cfg).await {
                Ok(p) => return Ok(p),
                Err(e) if e.is_transient() && attempt < retry.max_retries => {
                    attempt += 1;
                    self.emit(KuEvent::Retrying { id: self.req.id.clone(), attempt, reason: e.to_string() });
                    let mut rx = self.control.clone();
                    tokio::select! {
                        _ = tokio::time::sleep(retry.delay(attempt, e.retry_after())) => {}
                        _ = rx.changed() => {}
                    }
                    match &*self.control.borrow() {
                        Control::Pause => return Err(KuError::Paused),
                        Control::Cancel { .. } => return Err(KuError::Cancelled),
                        Control::Run => {}
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn attempt(&self, ctx: &RequestCtx, probe: &Probe, retry: &RetryPolicy, force_single: bool, checksum: Option<&Checksum>) -> AttemptEnd {
        let id = &self.req.id;
        let name = self
            .req
            .file_name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .or_else(|| probe.filename.clone())
            .unwrap_or_else(|| "download".into());
        let final_path = match storage::safe_join(&self.req.dest_dir, &name) {
            Ok(p) => p,
            Err(e) => return AttemptEnd::Fatal(e),
        };
        let temp = storage::temp_path_for(&final_path);
        let state_path = storage::state_path_for(&temp);
        let total = probe.total;
        let ranges = probe.ranges && total.is_some() && !force_single;

        // Resume only when the saved state provably matches the server.
        let saved = resume::load(&state_path).filter(|s| s.url == self.req.url);
        let temp_len = std::fs::metadata(&temp).ok().map(|m| m.len());
        let mut resumed: Option<SegmentMap> = None;
        if let Some(st) = &saved {
            match recovery::decide(st, probe, temp_len).await {
                Decision::Resume(m) if ranges => resumed = Some(m),
                Decision::Resume(_) => {}
                Decision::Restart(reason) => {
                    self.emit(KuEvent::Restarted { id: id.clone(), reason: format!("previous data discarded: {reason}") });
                    self.status.lock().unwrap().message = Some(format!("Started over: {reason}."));
                }
            }
        }
        let fresh = resumed.is_none();
        if fresh {
            resume::remove(&state_path);
        }
        // Disk space: needed = what is still missing.
        // A resumed temp file is already preallocated; a fresh one needs `total`.
        if let (Some(t), true) = (total, fresh) {
            if storage::available_space(&self.req.dest_dir).is_some_and(|free| free < t) {
                return AttemptEnd::Fatal(KuError::DiskFull);
            }
        }
        let storage = match Storage::open(&temp, total, fresh, self.cfg.sparse_files, self.cfg.write_fault.clone()) {
            Ok(s) => s,
            Err(e) => return AttemptEnd::Fatal(e),
        };
        let segmented_ok = ranges && total.is_some_and(|t| t >= self.cfg.small_file_threshold || self.req.connections.is_some_and(|c| c > 1));
        let fixed = self.req.connections;
        let initial = match fixed {
            Some(n) => n.clamp(1, 64),
            None if segmented_ok => scheduler::initial_connections(total, true, self.cfg.small_file_threshold, self.cfg.min_connections, self.cfg.max_connections),
            None => 1,
        };
        let initial = if segmented_ok { initial } else { 1 };
        let max = if segmented_ok { fixed.unwrap_or(self.cfg.max_connections).max(initial) } else { 1 };
        let min_split = self.cfg.min_segment_size.max(2 * self.cfg.write_buffer as u64);
        let map = match resumed {
            Some(m) => {
                self.emit(KuEvent::Resumed { id: id.clone(), downloaded: m.downloaded() });
                m
            }
            None => SegmentMap::new(total.unwrap_or(u64::MAX), if segmented_ok { initial } else { 1 }, min_split),
        };
        if saved.is_some() && !fresh {
            // Content check for servers without validators.
            if !probe.validators.has_any() {
                if let Err(why) = recovery::spot_check(&self.client, ctx, &self.cfg, probe, &map, &storage).await {
                    drop(storage);
                    let _ = std::fs::remove_file(&temp);
                    resume::remove(&state_path);
                    return AttemptEnd::Restart { reason: format!("content changed ({why})"), single_stream: false };
                }
            }
        }
        {
            let mut s = self.status.lock().unwrap();
            s.total = total;
            s.segmented = segmented_ok;
            s.file_name = Some(name.clone());
            s.temp_path = Some(temp.display().to_string());
            s.target_connections = initial;
            s.http_version = Some(probe.http_version.clone());
        }
        let job = Arc::new(Job {
            id: id.clone(),
            client: self.client.clone(),
            ctx: ctx.clone(),
            cfg: self.cfg.clone(),
            url: RwLock::new(probe.final_url.clone()),
            total,
            validators: probe.validators.clone(),
            segmented: segmented_ok,
            ranges,
            map: Mutex::new(map),
            unknown_done: AtomicU64::new(0),
            unknown_eof: AtomicBool::new(false),
            storage: storage.clone(),
            limiter: self.limiter.clone(),
            global: self.global.clone(),
            control: self.control.clone(),
            target: AtomicU32::new(initial),
            stats: ConnStats::default(),
            controller: Mutex::new(Controller::new(initial, self.cfg.min_connections, max, fixed.is_some())),
            events: self.events.clone(),
            retry: *retry,
            min_split,
            http_version: Mutex::new(None),
            refresh_lock: tokio::sync::Mutex::new(()),
        });
        let state_template = ResumeState {
            version: resume::STATE_VERSION,
            id: id.clone(),
            url: self.req.url.clone(),
            final_name: name.clone(),
            total,
            validators: probe.validators.clone(),
            segmented: segmented_ok,
            ranges: Vec::new(),
            created_at: saved.as_ref().map(|s| s.created_at).unwrap_or_else(resume::now_secs),
            updated_at: 0,
        };
        if total.is_some() && ranges {
            let _ = persist(&job, &state_path, &state_template).await;
        }

        self.set_state(State::Downloading, None, None);
        let mut workers = JoinSet::new();
        let mut next_wid = 0u32;
        let started = Instant::now();
        let start_bytes = job.downloaded();
        let mut last_persist = Instant::now();
        let mut last_speed_event = (Instant::now(), 0u64);
        let mut fatal: Option<KuError> = None;
        let mut ticker = tokio::time::interval(self.cfg.progress_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut control = self.control.clone();

        let spawn = |workers: &mut JoinSet<Exit>, next_wid: &mut u32| {
            job.stats.add_worker();
            let j = job.clone();
            let wid = *next_wid;
            *next_wid += 1;
            workers.spawn(async move { worker::run(j, wid).await });
        };
        for _ in 0..initial {
            spawn(&mut workers, &mut next_wid);
        }

        loop {
            tokio::select! {
                Some(res) = workers.join_next() => {
                    match res {
                        Ok(Exit::Fatal(e)) => {
                            if fatal.is_none() {
                                fatal = Some(e);
                            }
                        }
                        Ok(_) => {}
                        Err(e) => fatal = Some(KuError::Io(format!("worker crashed: {e}"))),
                    }
                    let _ = self.events.send(KuEvent::ConnectionRemoved { id: id.clone(), connections: job.stats.workers() });
                    if fatal.is_some() {
                        // Stop the remaining workers (they flush and exit).
                        break;
                    }
                    if workers.is_empty() {
                        let complete = if total.is_some() { job.map.lock().unwrap().is_complete() } else { job.unknown_eof.load(Ordering::SeqCst) };
                        if complete || self.stopping() {
                            break;
                        }
                        // Work remains (e.g. retired too many): keep one going.
                        spawn(&mut workers, &mut next_wid);
                    }
                }
                _ = ticker.tick() => {
                    let now = Instant::now();
                    let downloaded = job.downloaded();
                    let splittable = job.map.lock().unwrap().has_work(min_split);
                    let target = job.controller.lock().unwrap().tick(now, downloaded, splittable);
                    let prev = job.target.swap(target, Ordering::SeqCst);
                    if target > prev {
                        let _ = self.events.send(KuEvent::ConnectionAdded { id: id.clone(), connections: target });
                    }
                    while !self.stopping() && job.stats.workers() < target && job.map.lock().unwrap().has_work(min_split) {
                        spawn(&mut workers, &mut next_wid);
                    }
                    let speed = job.controller.lock().unwrap().speed() as u64;
                    self.publish(&job, downloaded, speed, started, start_bytes);
                    if last_speed_event.1 == 0 || speed.abs_diff(last_speed_event.1) * 4 > last_speed_event.1 && now.duration_since(last_speed_event.0) > Duration::from_secs(2) {
                        last_speed_event = (now, speed.max(1));
                        let _ = self.events.send(KuEvent::SpeedChanged { id: id.clone(), speed });
                    }
                    if ranges && now.duration_since(last_persist) >= self.cfg.persist_interval {
                        last_persist = now;
                        if let Err(e) = persist(&job, &state_path, &state_template).await {
                            fatal = Some(e);
                            break;
                        }
                    }
                }
                _ = control.changed() => {
                    if self.stopping() {
                        // Workers observe the control channel and flush.
                        while workers.join_next().await.is_some() {}
                        break;
                    }
                }
            }
        }
        // Let the rest stop and flush.
        if fatal.is_some() || self.stopping() {
            if fatal.is_some() && !self.stopping() {
                // Tell workers to stop by aborting their loops via the stop flag.
                workers.abort_all();
            }
            while workers.join_next().await.is_some() {}
        }
        if ranges {
            let _ = persist(&job, &state_path, &state_template).await;
        }
        let ctl = self.control.borrow().clone();
        if ctl != Control::Run {
            drop(storage);
            if let Control::Cancel { delete: true } = ctl {
                let _ = std::fs::remove_file(&temp);
                resume::remove(&state_path);
            }
            return AttemptEnd::Stopped(ctl);
        }
        if let Some(e) = fatal {
            return match e {
                KuError::RangeIgnored | KuError::BadContentRange(_) if segmented_ok || ranges => {
                    // Re-probe: changed resource → restart; otherwise single stream.
                    match client::probe(&self.client, ctx, &self.cfg).await {
                        Ok(p) if p.total != total || probe.validators.changed(&p.validators).is_some() => {
                            let _ = std::fs::remove_file(&temp);
                            resume::remove(&state_path);
                            AttemptEnd::Restart { reason: "the file changed on the server".into(), single_stream: false }
                        }
                        _ => {
                            let _ = std::fs::remove_file(&temp);
                            resume::remove(&state_path);
                            AttemptEnd::Restart { reason: "the server does not handle byte ranges reliably; using a single connection".into(), single_stream: true }
                        }
                    }
                }
                KuError::ResourceChanged(reason) if self.cfg.restart_on_change => {
                    drop(storage);
                    let _ = std::fs::remove_file(&temp);
                    resume::remove(&state_path);
                    AttemptEnd::Restart { reason, single_stream: false }
                }
                e => AttemptEnd::Fatal(e),
            };
        }
        self.finish(&job, &storage, &temp, &state_path, &final_path, checksum, probe, started, start_bytes).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn finish(
        &self,
        job: &Arc<Job>,
        storage: &Storage,
        temp: &std::path::Path,
        state_path: &std::path::Path,
        final_path: &std::path::Path,
        checksum: Option<&Checksum>,
        probe: &Probe,
        started: Instant,
        start_bytes: u64,
    ) -> AttemptEnd {
        let id = &self.req.id;
        // 1–3: every range complete, map consistent.
        let size = match job.total {
            Some(t) => {
                let map = job.map.lock().unwrap();
                if let Err(e) = map.check() {
                    return AttemptEnd::Fatal(KuError::Io(format!("internal segment map error: {e}")));
                }
                if !map.is_complete() {
                    return AttemptEnd::Fatal(KuError::SizeMismatch { expected: t, actual: map.downloaded() });
                }
                t
            }
            None => {
                let n = job.unknown_done.load(Ordering::SeqCst);
                if let Err(e) = storage.set_len(n) {
                    return AttemptEnd::Fatal(e);
                }
                n
            }
        };
        storage.densify();
        // 2: all writes durable.
        if let Err(e) = storage.sync().await {
            return AttemptEnd::Fatal(e);
        }
        // 4: on-disk size equals the expected size.
        let on_disk = storage.len();
        if on_disk != size {
            return AttemptEnd::Fatal(KuError::SizeMismatch { expected: size, actual: on_disk });
        }
        // 6: checksum (user supplied first, else server announced).
        let want = checksum.cloned().or_else(|| probe.server_checksum.clone().map(|(algo, hex)| Checksum { algo, hex }));
        if let Some(w) = &want {
            self.set_state(State::Verifying, None, None);
            let _ = self.events.send(KuEvent::Verifying { id: id.clone() });
            if let Err(e) = integrity::verify(temp, w).await {
                let st = self.set_state(State::FailedVerification, Some(e.to_string()), None);
                let _ = self.events.send(KuEvent::VerificationFailed { id: id.clone(), reason: e.to_string() });
                *self.status.lock().unwrap() = st;
                // The temp file stays for inspection; the final name is never used.
                resume::remove(state_path);
                return AttemptEnd::Done;
            }
        }
        // 7: atomic rename.
        self.set_state(State::Finalizing, None, None);
        let fin = match storage::finalize(temp, final_path, self.req.overwrite) {
            Ok(p) => p,
            Err(e) => return AttemptEnd::Fatal(e),
        };
        resume::remove(state_path);
        {
            let mut s = self.status.lock().unwrap();
            s.downloaded = size;
            s.total = Some(size);
            s.percent = Some(100.0);
            s.final_path = Some(fin.display().to_string());
            let secs = started.elapsed().as_secs_f64().max(0.001);
            s.average_speed = ((size - start_bytes.min(size)) as f64 / secs) as u64;
            s.retries = job.stats.retries.load(Ordering::Relaxed);
        }
        self.set_state(State::Completed, None, None);
        let _ = self.events.send(KuEvent::Completed { id: id.clone(), path: fin.display().to_string(), size });
        AttemptEnd::Done
    }

    fn publish(&self, job: &Job, downloaded: u64, speed: u64, started: Instant, start_bytes: u64) {
        let (segments, active, done, failed) = {
            let map = job.map.lock().unwrap();
            let segs: Vec<SegmentInfo> = if job.total.is_some() {
                map.segments()
                    .iter()
                    .map(|s| SegmentInfo {
                        start: s.start,
                        end: s.end,
                        downloaded: s.pos - s.start,
                        active: s.worker.is_some(),
                        done: s.pos == s.end,
                        speed: s.speed as u64,
                        retries: s.retries,
                    })
                    .collect()
            } else {
                Vec::new()
            };
            (segs, map.active_count(), map.done_count(), map.failed_count())
        };
        let status = {
            let mut s = self.status.lock().unwrap();
            s.downloaded = downloaded;
            s.percent = job.total.filter(|t| *t > 0).map(|t| downloaded as f64 * 100.0 / t as f64);
            s.speed = speed;
            let secs = started.elapsed().as_secs_f64();
            s.average_speed = if secs > 0.5 { ((downloaded.saturating_sub(start_bytes)) as f64 / secs) as u64 } else { 0 };
            // ETA from a blend of current and average speed (stable, but responsive).
            let basis = if s.average_speed > 0 { (speed as f64 * 0.5 + s.average_speed as f64 * 0.5).max(1.0) } else { speed as f64 };
            s.eta = match job.total {
                Some(t) if basis >= 1.0 && speed > 0 => Some((t.saturating_sub(downloaded)) as f64 / basis),
                _ => None,
            };
            s.connections = job.stats.workers();
            s.target_connections = job.target.load(Ordering::SeqCst);
            s.active_segments = active;
            s.completed_segments = done;
            s.failed_segments = failed;
            s.retries = job.stats.retries.load(Ordering::Relaxed);
            if let Some(v) = job.http_version.lock().unwrap().clone() {
                s.http_version = Some(v);
            }
            s.clone()
        };
        *self.segments.lock().unwrap() = segments.clone();
        let _ = self.events.send(KuEvent::Progress { id: self.req.id.clone(), status: Box::new(status), segments });
    }

    fn stopping(&self) -> bool {
        *self.control.borrow() != Control::Run
    }

    fn fail(&self, e: KuError, recoverable: bool) -> Status {
        let state = if recoverable { State::Recoverable } else { State::Failed };
        let st = self.set_state(state, Some(e.to_string()), None);
        self.emit(KuEvent::Failed { id: self.req.id.clone(), error: e.to_string(), recoverable });
        st
    }

    fn paused(&self) -> Status {
        let st = self.set_state(State::Paused, None, None);
        self.emit(KuEvent::Paused { id: self.req.id.clone() });
        st
    }

    fn cancelled(&self, _deleted: Option<bool>) -> Status {
        let st = self.set_state(State::Cancelled, None, None);
        self.emit(KuEvent::Cancelled { id: self.req.id.clone() });
        st
    }
}

/// Snapshot positions → fsync data → atomically write the state. Positions
/// recorded are therefore always backed by durable bytes.
async fn persist(job: &Job, path: &std::path::Path, template: &ResumeState) -> Result<(), KuError> {
    let ranges = job.map.lock().unwrap().persisted();
    job.storage.sync().await?;
    let mut s = template.clone();
    s.ranges = ranges;
    s.updated_at = resume::now_secs();
    let p = path.to_path_buf();
    tokio::task::spawn_blocking(move || resume::save(&p, &s)).await.map_err(|e| KuError::Io(e.to_string()))?
}
