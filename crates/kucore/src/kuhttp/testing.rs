//! Fault-injecting HTTP server for KuHTTP tests and benchmarks.
//!
//! Content is generated deterministically from `(seed, offset)`, so any size
//! (10 GB+) can be served without memory and verified by streaming.

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const CHUNK: usize = 64 * 1024;

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Deterministic content: byte at `offset` of resource `seed`.
pub fn fill(seed: u64, offset: u64, buf: &mut [u8]) {
    let mut i = 0usize;
    let mut off = offset;
    while i < buf.len() {
        let block = off / 8;
        let word = splitmix(seed ^ block.wrapping_mul(0x2545_F491_4F6C_DD1D)).to_le_bytes();
        let within = (off % 8) as usize;
        let n = (8 - within).min(buf.len() - i);
        buf[i..i + n].copy_from_slice(&word[within..within + n]);
        i += n;
        off += n as u64;
    }
}

pub fn content(seed: u64, len: u64) -> Vec<u8> {
    let mut v = vec![0u8; len as usize];
    fill(seed, 0, &mut v);
    v
}

/// Compare a file with the generated content, streaming.
pub fn file_matches(path: &std::path::Path, seed: u64, len: u64) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    if f.metadata().map(|m| m.len()).unwrap_or(0) != len {
        return false;
    }
    let mut a = vec![0u8; 1 << 20];
    let mut b = vec![0u8; 1 << 20];
    let mut off = 0u64;
    loop {
        let n = match f.read(&mut a) {
            Ok(0) => return off == len,
            Ok(n) => n,
            Err(_) => return false,
        };
        fill(seed, off, &mut b[..n]);
        if a[..n] != b[..n] {
            return false;
        }
        off += n as u64;
    }
}

#[derive(Clone, Debug)]
pub struct Resource {
    pub len: u64,
    pub seed: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// Honour Range requests.
    pub ranges: bool,
    /// Send `Accept-Ranges: bytes` even when ranges are ignored (a liar).
    pub advertise_ranges: bool,
    /// Full responses use chunked encoding without Content-Length.
    pub chunked: bool,
    /// Per-response throughput cap, bytes/s (0 = unlimited).
    pub per_conn_rate: u64,
    /// A slow connection: the response starting exactly at `.0` is capped
    /// at `.1` bytes/s (work stolen from it is served normally).
    pub slow_start: Option<(u64, u64)>,
    /// Probability per 64 KiB chunk that the connection is dropped.
    pub fail_prob: f64,
    /// Statuses returned by the next requests (with Retry-After: 1).
    pub statuses: VecDeque<u16>,
    /// More concurrent responses than this get 503.
    pub max_concurrent: u32,
    /// Ranged responses (except the 0-0 probe) report a wrong start.
    pub bad_content_range: bool,
    /// The next N bodies stop after `truncate_bytes`.
    pub truncate_next: u32,
    pub truncate_bytes: u64,
    /// The next N bodies stall (no data) after `stall_bytes`.
    pub stall_next: u32,
    pub stall_bytes: u64,
    pub content_disposition: Option<String>,
    pub digest: Option<String>,
    pub require_auth: Option<String>,
    /// Signed URL: each `/sign` issues a new signature; old ones get 403.
    pub signed: bool,
}

impl Resource {
    pub fn new(len: u64, seed: u64) -> Resource {
        Resource {
            len,
            seed,
            etag: Some(format!("\"{seed:x}-{len}\"")),
            last_modified: Some("Wed, 01 Jan 2025 00:00:00 GMT".into()),
            ranges: true,
            advertise_ranges: true,
            chunked: false,
            per_conn_rate: 0,
            slow_start: None,
            fail_prob: 0.0,
            statuses: VecDeque::new(),
            max_concurrent: 0,
            bad_content_range: false,
            truncate_next: 0,
            truncate_bytes: 0,
            stall_next: 0,
            stall_bytes: 0,
            content_disposition: None,
            digest: None,
            require_auth: None,
            signed: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RequestLog {
    pub path: String,
    pub range: Option<String>,
    pub authorization: Option<String>,
    pub cookie: Option<String>,
    pub custom: Option<String>,
    pub status: u16,
}

#[derive(Default)]
pub struct ServerState {
    pub resources: Mutex<HashMap<String, Resource>>,
    pub active: AtomicU32,
    pub peak_active: AtomicU32,
    pub requests: AtomicU64,
    pub log: Mutex<Vec<RequestLog>>,
    sig: AtomicU64,
    rng: AtomicU64,
    epoch: AtomicU64,
}

impl ServerState {
    pub fn set(&self, name: &str, r: Resource) {
        self.resources.lock().unwrap().insert(name.to_string(), r);
    }

    pub fn update(&self, name: &str, f: impl FnOnce(&mut Resource)) {
        if let Some(r) = self.resources.lock().unwrap().get_mut(name) {
            f(r);
        }
    }

    /// Abort every response currently being streamed (clients must reconnect).
    pub fn drop_connections(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }

    /// Invalidate signed URLs handed out so far.
    pub fn rotate_signature(&self) {
        self.sig.fetch_add(1000, Ordering::SeqCst);
    }

    pub fn reset_peak(&self) {
        self.peak_active.store(self.active.load(Ordering::SeqCst), Ordering::SeqCst);
    }

    fn random(&self) -> f64 {
        let x = splitmix(self.rng.fetch_add(1, Ordering::Relaxed) ^ 0xDEAD_BEEF);
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub struct TestServer {
    pub port: u16,
    pub state: Arc<ServerState>,
}

impl TestServer {
    pub async fn start() -> TestServer {
        let state = Arc::new(ServerState::default());
        let app = Router::new()
            .route("/r/{name}", get(serve))
            .route("/redir/{n}/{name}", get(redir))
            .route("/loop/{n}", get(redirect_loop))
            .route("/xredir/{port}/{name}", get(cross_redirect))
            .route("/sign/{name}", get(sign))
            .route("/s/{sig}/{name}", get(serve_signed))
            .with_state(state.clone());
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            let _ = axum::serve(l, app).await;
        });
        TestServer { port, state }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
}

fn redirect(to: String) -> Response {
    Response::builder().status(StatusCode::FOUND).header(header::LOCATION, to).body(Body::empty()).unwrap()
}

async fn redir(Path((n, name)): Path<(u32, String)>) -> Response {
    if n == 0 {
        redirect(format!("/r/{name}"))
    } else {
        redirect(format!("/redir/{}/{name}", n - 1))
    }
}

async fn redirect_loop(Path(n): Path<u32>) -> Response {
    redirect(format!("/loop/{}", (n + 1) % 3))
}

async fn cross_redirect(Path((port, name)): Path<(u16, String)>, State(s): State<Arc<ServerState>>, h: HeaderMap) -> Response {
    log(&s, format!("/xredir/{port}/{name}"), &h, 302);
    redirect(format!("http://127.0.0.1:{port}/r/{name}"))
}

async fn sign(Path(name): Path<String>, State(s): State<Arc<ServerState>>) -> Response {
    let sig = s.sig.fetch_add(1, Ordering::SeqCst) + 1;
    redirect(format!("/s/{sig}/{name}"))
}

async fn serve_signed(Path((sig, name)): Path<(u64, String)>, State(s): State<Arc<ServerState>>, h: HeaderMap) -> Response {
    if sig != s.sig.load(Ordering::SeqCst) {
        log(&s, format!("/s/{sig}/{name}"), &h, 403);
        return Response::builder().status(StatusCode::FORBIDDEN).body(Body::empty()).unwrap();
    }
    serve(Path(name), State(s), h).await
}

fn log(s: &ServerState, path: String, h: &HeaderMap, status: u16) {
    let g = |n: &str| h.get(n).and_then(|v| v.to_str().ok()).map(str::to_string);
    s.log.lock().unwrap().push(RequestLog {
        path,
        range: g("range"),
        authorization: g("authorization"),
        cookie: g("cookie"),
        custom: g("x-api-key"),
        status,
    });
}

/// `dyn-{len}-{seed}-{rate}-{fail_ppm}.bin`: resources created on first use
/// (used by the out-of-process benchmark server).
pub fn dynamic_resource(name: &str) -> Option<Resource> {
    let rest = name.strip_prefix("dyn-")?.strip_suffix(".bin")?;
    let p: Vec<u64> = rest.split('-').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let [len, seed, rate, fail_ppm] = p[..] else { return None };
    let mut r = Resource::new(len, seed);
    r.per_conn_rate = rate;
    r.fail_prob = fail_ppm as f64 / 1_000_000.0;
    Some(r)
}

struct ActiveGuard(Arc<ServerState>);
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

fn parse_range(v: &str, len: u64) -> Option<(u64, u64)> {
    let r = v.strip_prefix("bytes=")?;
    if r.contains(',') {
        return None;
    }
    let (a, b) = r.split_once('-')?;
    let a: u64 = a.parse().ok()?;
    let b: u64 = if b.is_empty() { len.checked_sub(1)? } else { b.parse::<u64>().ok()?.min(len.checked_sub(1)?) };
    (a <= b && a < len).then_some((a, b))
}

async fn serve(Path(name): Path<String>, State(s): State<Arc<ServerState>>, h: HeaderMap) -> Response {
    s.requests.fetch_add(1, Ordering::SeqCst);
    let path = format!("/r/{name}");
    let (res, forced, truncate, stall) = {
        let mut map = s.resources.lock().unwrap();
        if !map.contains_key(&name) {
            if let Some(r) = dynamic_resource(&name) {
                map.insert(name.clone(), r);
            }
        }
        let Some(r) = map.get_mut(&name) else {
            drop(map);
            log(&s, path, &h, 404);
            return Response::builder().status(404).body(Body::empty()).unwrap();
        };
        let forced = r.statuses.pop_front();
        let truncate = if r.truncate_next > 0 && forced.is_none() {
            r.truncate_next -= 1;
            Some(r.truncate_bytes)
        } else {
            None
        };
        let stall = if r.stall_next > 0 && forced.is_none() && truncate.is_none() {
            r.stall_next -= 1;
            Some(r.stall_bytes)
        } else {
            None
        };
        (r.clone(), forced, truncate, stall)
    };
    if let Some(req) = &res.require_auth {
        if h.get("authorization").and_then(|v| v.to_str().ok()) != Some(req.as_str()) {
            log(&s, path, &h, 401);
            return Response::builder().status(401).header("www-authenticate", "Basic").body(Body::empty()).unwrap();
        }
    }
    if let Some(code) = forced {
        log(&s, path, &h, code);
        return Response::builder().status(code).header(header::RETRY_AFTER, "1").body(Body::empty()).unwrap();
    }
    let active = s.active.fetch_add(1, Ordering::SeqCst) + 1;
    let guard = ActiveGuard(s.clone());
    if res.max_concurrent > 0 && active > res.max_concurrent {
        log(&s, path, &h, 503);
        drop(guard);
        return Response::builder().status(503).header(header::RETRY_AFTER, "1").body(Body::empty()).unwrap();
    }
    s.peak_active.fetch_max(active, Ordering::SeqCst);

    let mut range = h.get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|v| parse_range(v, res.len));
    if let Some(ir) = h.get(header::IF_RANGE).and_then(|v| v.to_str().ok()) {
        if Some(ir) != res.etag.as_deref() && Some(ir) != res.last_modified.as_deref() {
            range = None; // representation changed: full response
        }
    }
    if !res.ranges {
        range = None;
    }
    if h.contains_key(header::RANGE) && res.ranges && range.is_none() && h.get(header::IF_RANGE).is_none() {
        log(&s, path, &h, 416);
        return Response::builder().status(416).header(header::CONTENT_RANGE, format!("bytes */{}", res.len)).body(Body::empty()).unwrap();
    }
    let (start, end, status) = match range {
        Some((a, b)) => (a, b + 1, StatusCode::PARTIAL_CONTENT),
        None => (0, res.len, StatusCode::OK),
    };
    log(&s, path, &h, status.as_u16());
    let mut b = Response::builder().status(status).header(header::CONTENT_TYPE, "application/octet-stream");
    if let Some(e) = &res.etag {
        b = b.header(header::ETAG, e);
    }
    if let Some(l) = &res.last_modified {
        b = b.header(header::LAST_MODIFIED, l);
    }
    if res.ranges || res.advertise_ranges {
        b = b.header(header::ACCEPT_RANGES, "bytes");
    }
    if let Some(cd) = &res.content_disposition {
        b = b.header(header::CONTENT_DISPOSITION, cd);
    }
    if let Some(d) = &res.digest {
        b = b.header("digest", d);
    }
    if status == StatusCode::PARTIAL_CONTENT {
        let shift = if res.bad_content_range && start > 0 { 1 } else { 0 };
        b = b.header(header::CONTENT_RANGE, format!("bytes {}-{}/{}", start + shift, end - 1, res.len));
        b = b.header(header::CONTENT_LENGTH, (end - start).to_string());
    } else if !res.chunked {
        b = b.header(header::CONTENT_LENGTH, res.len.to_string());
    }
    let rate = match res.slow_start {
        Some((a, r)) if start == a => r,
        _ => res.per_conn_rate,
    };
    let state = s.clone();
    let seed = res.seed;
    let fail_prob = res.fail_prob;
    let limit_end = truncate.map(|t| (start + t).min(end));
    let stall_at = stall.map(|t| start + t);
    let epoch = s.epoch.load(Ordering::SeqCst);
    let t0 = tokio::time::Instant::now();
    let stream = futures_util::stream::unfold((start, guard), move |(pos, guard)| {
        let state = state.clone();
        async move {
            if let Some(le) = limit_end {
                if pos >= le {
                    // Close early while the headers promised more.
                    return Some((Err(std::io::Error::other("truncated")), (end, guard)));
                }
            }
            if pos >= end {
                return None;
            }
            if stall_at.is_some_and(|st| pos >= st) {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
            if state.epoch.load(Ordering::SeqCst) != epoch {
                return Some((Err(std::io::Error::other("connection dropped")), (end, guard)));
            }
            if fail_prob > 0.0 && state.random() < fail_prob {
                return Some((Err(std::io::Error::other("injected failure")), (end, guard)));
            }
            let mut stop = (pos + CHUNK as u64).min(end);
            if let Some(le) = limit_end {
                stop = stop.min(le.max(pos + 1));
            }
            let mut buf = vec![0u8; (stop - pos) as usize];
            fill(seed, pos, &mut buf);
            if rate > 0 {
                // Pace against a deadline so coarse timers do not skew the rate.
                let due = t0 + Duration::from_secs_f64((stop - start) as f64 / rate as f64);
                tokio::time::sleep_until(due).await;
            }
            Some((Ok::<_, std::io::Error>(Bytes::from(buf)), (stop, guard)))
        }
    });
    b.body(Body::from_stream(stream)).unwrap()
}

