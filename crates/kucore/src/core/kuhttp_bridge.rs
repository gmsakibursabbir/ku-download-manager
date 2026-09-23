//! KuCore ↔ KuHTTP bridge. KuHTTP is opt-in (Settings › Advanced ›
//! HTTP engine); aria2 remains the default HTTP engine.

use super::{backoff, lock, now_ms, Core};
use crate::kuhttp::{self, DownloadRequest, KuHttpConfig, KuHttpEngine, State};
use crate::types::CoreEvent;
use anyhow::{anyhow, Result};
use ku_proto::{Download, Status};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

impl Core {
    pub(super) fn kuhttp(&self) -> Result<Arc<KuHttpEngine>> {
        let mut g = lock(&self.kuhttp);
        if let Some(e) = g.as_ref() {
            return Ok(e.clone());
        }
        let s = self.settings();
        let cfg = KuHttpConfig {
            max_retries: s.max_tries.max(3),
            connect_timeout: Duration::from_secs(s.connect_timeout as u64),
            read_timeout: Duration::from_secs(s.timeout as u64),
            user_agent: crate::probe::user_agent(&Default::default(), &s),
            verify_tls: s.check_certificate,
            proxy: (!s.proxy.trim().is_empty()).then(|| kuhttp::ProxyConfig {
                url: s.proxy.trim().to_string(),
                username: (!s.proxy_user.is_empty()).then(|| s.proxy_user.clone()),
                password: (!s.proxy_pass.is_empty()).then(|| s.proxy_pass.clone()),
            }),
            max_connections: if s.default_connections == 0 { 8 } else { s.default_connections.clamp(1, 32) },
            ..KuHttpConfig::default()
        };
        let e = Arc::new(KuHttpEngine::new(cfg).map_err(|e| anyhow!("KuHTTP: {e}"))?);
        e.set_global_limit(s.profile().download);
        *g = Some(e.clone());
        Ok(e)
    }

    /// Drop the engine so the next download picks up new connection settings
    /// (only when nothing is running on it).
    pub(super) fn kuhttp_reset(&self) {
        let busy = lock(&self.st).downloads.values().any(|d| d.engine == ku_proto::Engine::Kuhttp && d.status.is_running());
        if !busy {
            lock(&self.kuhttp).take();
        }
    }

    pub(super) fn kuhttp_set_limit(&self, bps: u64) {
        if let Some(e) = lock(&self.kuhttp).as_ref() {
            e.set_global_limit(bps);
        }
    }

    fn request_for(d: &Download) -> DownloadRequest {
        let headers = d
            .options
            .headers
            .iter()
            .filter_map(|h| h.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
            .collect();
        DownloadRequest {
            id: d.id.clone(),
            url: d.url.clone(),
            dest_dir: PathBuf::from(&d.dir),
            file_name: Some(d.name.clone()),
            headers,
            cookies: crate::probe::cookie_header(&d.options),
            referer: d.options.referer.clone(),
            user_agent: d.options.user_agent.clone().filter(|u| !u.is_empty()),
            basic_auth: d.options.username.clone().map(|u| (u, d.options.password.clone().unwrap_or_default())),
            checksum: d.options.checksum.clone(),
            connections: (d.connections > 0).then_some(d.connections),
            speed_limit: d.options.speed_limit.unwrap_or(0),
            overwrite: false,
        }
    }

    pub(super) fn start_kuhttp(self: &Arc<Self>, d: Download) -> Result<()> {
        let engine = self.kuhttp()?;
        std::fs::create_dir_all(&d.dir).map_err(|e| anyhow!("Could not create the folder {}: {e}", d.dir))?;
        engine.download(Self::request_for(&d)).map_err(|e| anyhow!("{e}"))?;
        self.log(&d.id, "info", "Started with KuHTTP");
        let core = self.clone();
        let id = d.id.clone();
        tokio::spawn(async move {
            let Some(st) = engine.wait(&id).await else { return };
            // A newer run for the same id replaced this one: ignore.
            if engine.status(&id).is_some_and(|cur| !cur.state.is_terminal()) {
                return;
            }
            core.kuhttp_finished(&id, st);
        });
        Ok(())
    }

    fn kuhttp_finished(self: &Arc<Self>, id: &str, st: kuhttp::Status) {
        let s = self.settings();
        let mut notice = None;
        let mut completed = None;
        let mut retry_n = None;
        self.update(id, |d| {
            d.speed = 0;
            d.active_connections = 0;
            match st.state {
                State::Completed => {
                    d.status = Status::Completed;
                    d.completed_at = Some(now_ms());
                    d.total = st.total.unwrap_or(st.downloaded) as i64;
                    d.done = d.total;
                    if let Some(p) = &st.final_path {
                        d.file_path = Some(p.clone());
                        if let Some(n) = std::path::Path::new(p).file_name() {
                            d.name = n.to_string_lossy().into_owned();
                        }
                    }
                    if d.options.checksum.is_some() {
                        d.meta.verified = d.options.checksum.clone();
                    }
                    completed = Some((d.name.clone(), d.file_path.clone()));
                }
                State::Paused if st.error.is_some() => {
                    // Disk full: paused safely, resumable once space is freed.
                    d.status = Status::Paused;
                    d.error = st.error.clone();
                    notice = Some(("Download paused".to_string(), st.error.clone().unwrap_or_default()));
                }
                State::Paused | State::Cancelled => {}
                State::Recoverable if d.meta.retries < s.auto_retry => {
                    d.meta.retries += 1;
                    d.status = Status::Queued;
                    retry_n = Some(d.meta.retries);
                }
                _ => {
                    d.status = Status::Error;
                    d.error = Some(st.error.clone().unwrap_or_else(|| "The download failed.".into()));
                    notice = Some((format!("Download failed: {}", d.name), d.error.clone().unwrap_or_default()));
                }
            }
            if let Some(m) = &st.message {
                d.meta.smart_note = Some(m.clone());
            }
        });
        if let Some(n) = retry_n {
            lock(&self.st).retry_at.insert(id.to_string(), Instant::now() + backoff(n));
            self.log(id, "warn", format!("{} Retrying automatically.", st.error.clone().unwrap_or_default()));
        }
        if let Some((name, path)) = completed {
            self.log(id, "info", "Completed");
            self.emit(CoreEvent::Completed { id: id.into(), name, path });
        }
        if let Some((title, message)) = notice {
            self.log(id, "error", &message);
            self.emit(CoreEvent::Notice { level: "error".into(), title, message, download_id: Some(id.into()) });
        }
        self.wake.notify_one();
    }

    /// Copy live KuHTTP status into the download list (1 Hz, batched).
    pub(super) fn poll_kuhttp(&self) {
        let Some(engine) = lock(&self.kuhttp).clone() else { return };
        let mut st = lock(&self.st);
        for d in st.downloads.values_mut().filter(|d| d.engine == ku_proto::Engine::Kuhttp && d.status.is_running()) {
            let Some(s) = engine.status(&d.id) else { continue };
            d.done = s.downloaded as i64;
            if let Some(t) = s.total {
                d.total = t as i64;
            }
            d.speed = s.speed as i64;
            d.active_connections = s.connections;
            d.status = match s.state {
                State::Verifying | State::Finalizing => Status::Processing,
                _ if d.status == Status::Processing => Status::Downloading,
                _ => d.status,
            };
        }
    }

    pub(super) fn stop_kuhttp(&self, id: &str, cancel: bool, delete: bool) {
        if let Some(e) = lock(&self.kuhttp).as_ref() {
            if cancel {
                e.cancel(id, delete);
            } else {
                e.pause(id);
            }
        }
    }

    /// Pause running KuHTTP jobs so they persist their state; the database
    /// keeps them as downloading, so the next start resumes them.
    pub(super) async fn kuhttp_shutdown(&self) {
        let Some(e) = lock(&self.kuhttp).clone() else { return };
        let ids: Vec<String> = lock(&self.st)
            .downloads
            .values()
            .filter(|d| d.engine == ku_proto::Engine::Kuhttp && d.status.is_running())
            .map(|d| d.id.clone())
            .collect();
        for id in &ids {
            e.pause(id);
        }
        for id in &ids {
            let _ = tokio::time::timeout(Duration::from_secs(5), e.wait(id)).await;
        }
    }

    pub(super) fn kuhttp_download_limit(&self, id: &str, bps: u64) {
        if let Some(e) = lock(&self.kuhttp).as_ref() {
            e.set_download_limit(id, bps);
        }
    }

    pub(super) fn kuhttp_details(&self, id: &str) -> Value {
        let Some(e) = lock(&self.kuhttp).clone() else { return json!({}) };
        let st = e.status(id);
        json!({
            "segments": e.segments(id),
            "kuhttp": st,
        })
    }
}
