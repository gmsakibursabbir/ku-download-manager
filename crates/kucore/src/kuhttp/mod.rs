//! KuHTTP — KuDownloader's native adaptive HTTP/HTTPS download engine.
//!
//! ```text
//! probe (real bytes=0-0) ─▶ resume check (size, ETag, Last-Modified, spot samples)
//!        │
//!        ▼
//! segment map ◀── workers (claim / steal / validate 206 / positional write)
//!        │              ▲
//!        │     controller (grow while it helps, back off on 429/503/errors)
//!        ▼
//! fsync → persist state → … → verify size + checksum → atomic rename
//! ```
//!
//! Status: experimental. aria2 stays the production HTTP engine until KuHTTP
//! has proven itself (see `tests/kuhttp.rs` and `examples/kuhttp_bench.rs`).

pub mod client;
pub mod config;
pub mod connection;
pub mod download;
pub mod error;
pub mod events;
pub mod integrity;
pub mod limiter;
pub mod recovery;
pub mod response;
pub mod resume;
pub mod retry;
pub mod scheduler;
pub mod segment;
pub mod storage;
#[cfg(feature = "testing")]
pub mod testing;
pub mod worker;

pub use config::{KuHttpConfig, ProxyConfig};
pub use download::{Control, DownloadRequest};
pub use error::KuError;
pub use events::{KuEvent, State, Status};

use limiter::RateLimiter;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, watch};

/// Remove query and fragment (signed tokens) from URLs before logging.
pub fn redact(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut u) => {
            let _ = u.set_username("");
            let _ = u.set_password(None);
            if u.query().is_some() {
                u.set_query(Some("…"));
            }
            u.set_fragment(None);
            u.to_string()
        }
        Err(_) => "<invalid url>".into(),
    }
}

struct Handle {
    control: watch::Sender<Control>,
    status: Arc<Mutex<Status>>,
    segments: Arc<Mutex<Vec<events::SegmentInfo>>>,
    done: watch::Receiver<bool>,
    limiter: Arc<RateLimiter>,
    task: tokio::task::JoinHandle<()>,
}

pub struct KuHttpEngine {
    client: reqwest::Client,
    config: KuHttpConfig,
    global: Arc<RateLimiter>,
    jobs: Mutex<HashMap<String, Handle>>,
    events: broadcast::Sender<KuEvent>,
}

impl KuHttpEngine {
    pub fn new(config: KuHttpConfig) -> Result<KuHttpEngine, KuError> {
        let client = client::build_client(&config)?;
        let (events, _) = broadcast::channel(4096);
        Ok(KuHttpEngine { client, config, global: Arc::new(RateLimiter::new(0)), jobs: Mutex::new(HashMap::new()), events })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<KuEvent> {
        self.events.subscribe()
    }

    /// Start (or resume, if a `.kudownload.json` state exists) a download.
    pub fn download(&self, req: DownloadRequest) -> Result<(), KuError> {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(h) = jobs.get(&req.id) {
            if !*h.done.borrow() {
                return Ok(()); // already running
            }
        }
        let (control_tx, control_rx) = watch::channel(Control::Run);
        let (done_tx, done_rx) = watch::channel(false);
        let status = Arc::new(Mutex::new(Status::queued()));
        let segments = Arc::new(Mutex::new(Vec::new()));
        let limiter = Arc::new(RateLimiter::new(req.speed_limit));
        let runner = download::Runner {
            req: req.clone(),
            client: self.client.clone(),
            cfg: self.config.clone(),
            global: self.global.clone(),
            limiter: limiter.clone(),
            control: control_rx,
            status: status.clone(),
            segments: segments.clone(),
            events: self.events.clone(),
        };
        let task = tokio::spawn(async move {
            runner.run().await;
            let _ = done_tx.send(true);
        });
        jobs.insert(req.id.clone(), Handle { control: control_tx, status, segments, done: done_rx, limiter, task });
        Ok(())
    }

    pub fn pause(&self, id: &str) {
        if let Some(h) = self.jobs.lock().unwrap().get(id) {
            let _ = h.control.send(Control::Pause);
        }
    }

    /// Resume is a new `download` with the same request: state is on disk.
    pub fn resume(&self, req: DownloadRequest) -> Result<(), KuError> {
        self.download(req)
    }

    pub fn cancel(&self, id: &str, delete_partial: bool) {
        if let Some(h) = self.jobs.lock().unwrap().get(id) {
            let _ = h.control.send(Control::Cancel { delete: delete_partial });
        }
    }

    pub fn status(&self, id: &str) -> Option<Status> {
        self.jobs.lock().unwrap().get(id).map(|h| h.status.lock().unwrap().clone())
    }

    /// Wait until the download reaches a terminal state.
    pub async fn wait(&self, id: &str) -> Option<Status> {
        let (mut done, status) = {
            let jobs = self.jobs.lock().unwrap();
            let h = jobs.get(id)?;
            (h.done.clone(), h.status.clone())
        };
        while !*done.borrow() {
            if done.changed().await.is_err() {
                break;
            }
        }
        let s = status.lock().unwrap().clone();
        Some(s)
    }

    /// Latest segment map snapshot (for inspectors).
    pub fn segments(&self, id: &str) -> Vec<events::SegmentInfo> {
        self.jobs.lock().unwrap().get(id).map(|h| h.segments.lock().unwrap().clone()).unwrap_or_default()
    }

    pub fn set_global_limit(&self, bytes_per_sec: u64) {
        self.global.set_rate(bytes_per_sec);
    }

    pub fn set_download_limit(&self, id: &str, bytes_per_sec: u64) {
        if let Some(h) = self.jobs.lock().unwrap().get(id) {
            h.limiter.set_rate(bytes_per_sec);
        }
    }

    /// Verify a finished file against a checksum (`algo=hex`).
    pub async fn verify(path: &Path, checksum: &str) -> Result<(), KuError> {
        integrity::verify(path, &integrity::Checksum::parse(checksum)?).await
    }

    /// Test hook: kill a download without any cleanup, like a crash.
    #[doc(hidden)]
    pub fn crash_for_test(&self, id: &str) {
        if let Some(h) = self.jobs.lock().unwrap().remove(id) {
            h.task.abort();
        }
    }
}

impl Drop for KuHttpEngine {
    fn drop(&mut self) {
        for (_, h) in self.jobs.lock().unwrap().drain() {
            h.task.abort();
        }
    }
}
