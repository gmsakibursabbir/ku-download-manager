//! KuAirSend: send files, folders, text and links between KuDownloader
//! installs on the same network, at full LAN / Wi-Fi speed, with no cloud.
//!
//! ```text
//! discovery  UDP multicast 224.0.0.168:53318  {"app":"kudownloader", alias, avatar, fingerprint, port, announce}
//!            ← answered with POST /kuairsend/v1/hello (or a UDP reply)
//! transfer   POST /kuairsend/v1/offer                    files or text → per-file tokens once accepted
//!            POST /kuairsend/v1/file?session&id&token     raw bytes, several files in parallel
//!            POST /kuairsend/v1/cancel?session
//! download   POST /kuairsend/v1/offer with `download`      a link for the receiver to download
//!            itself, now or at a set time (older versions see it as a text message)
//! ```
//!
//! Every connection is mutual TLS with each device's self-signed certificate.
//! A device's fingerprint is the SHA-256 of its certificate and is checked on
//! both ends, so a "trusted device" really is that device. Only KuDownloader
//! takes part; other apps' packets are ignored. Nothing is written to disk
//! until the receiver accepts, unless the sender is trusted or auto-accept is on.

use crate::core::Core;
use crate::types::CoreEvent;
use anyhow::{anyhow, bail, Context, Result};
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Extension, Json, Router};
use futures_util::{StreamExt, TryStreamExt};
use ku_proto::paths;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub const PORT: u16 = 53318;
const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 168);
const APP: &str = "kudownloader";
const PROTO: u32 = 1;
const API: &str = "/kuairsend/v1";
/// Peers not heard from for this long drop off the list.
const PEER_TTL: Duration = Duration::from_secs(100);
const ANNOUNCE_EVERY: Duration = Duration::from_secs(30);
/// How long a sender waits for the receiver to accept.
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(300);
/// An accepted transfer with no data for this long is dropped.
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const HISTORY_MAX: usize = 100;
/// File names carried in events (the count is always complete).
const FILES_LISTED: usize = 200;
const PARALLEL_FILES: usize = 4;
/// Devices remembered at once (the least recently seen is dropped first).
const MAX_PEERS: usize = 256;
/// Simultaneous connections to the KuAirSend server.
const MAX_CONNECTIONS: usize = 64;
/// Wrong PINs from one device before it has to wait.
const PIN_TRIES: u32 = 5;
const PIN_LOCKOUT: Duration = Duration::from_secs(60);
/// At most one message popup per device this often.
const MESSAGE_GAP: Duration = Duration::from_secs(2);
pub const AVATARS: [&str; 16] = [
    "cat", "fox", "frog", "panda", "bunny", "penguin", "pig", "chick", "dog", "bear", "koala", "owl", "monkey", "tiger", "mouse", "cow",
];

// ───────────────────────── wire types ─────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct DeviceInfo {
    pub app: String,
    pub proto: u32,
    pub alias: String,
    pub avatar: String,
    /// "windows" | "macos" | "linux" | "android" | "ios"
    pub os: String,
    pub version: String,
    pub fingerprint: String,
    pub port: u16,
    /// Set on multicast announcements that want an answer.
    #[serde(skip_serializing_if = "is_false")]
    pub announce: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
struct OfferFile {
    id: String,
    /// Relative path; folders are sent as "folder/sub/file".
    name: String,
    size: u64,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Offer {
    from: DeviceInfo,
    files: Vec<OfferFile>,
    /// A text message or link instead of files.
    text: Option<String>,
    pin: Option<String>,
    /// "Download this": the receiver downloads the link itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    download: Option<RemoteDownload>,
}

/// A download handed to another device (for example from a phone to a PC).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct RemoteDownload {
    pub url: String,
    pub filename: Option<String>,
    /// When to start (ms since the epoch); absent or past = now.
    pub at: Option<i64>,
    pub referer: Option<String>,
    /// The sender's cookies for the link (a signed-in browser session).
    pub cookies: Vec<ku_proto::BrowserCookie>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct OfferReply {
    session: String,
    tokens: HashMap<String, String>,
}

// ───────────────────────── UI types ─────────────────────────

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AirPeer {
    pub fingerprint: String,
    pub alias: String,
    pub avatar: String,
    pub os: String,
    pub version: String,
    pub ip: String,
    pub port: u16,
    pub trusted: bool,
    #[serde(skip)]
    seen: Option<Instant>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AirFile {
    pub name: String,
    pub size: u64,
    pub done: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AirTransfer {
    pub id: String,
    /// "send" | "receive"
    pub direction: String,
    pub peer: String,
    pub peer_fingerprint: String,
    pub peer_avatar: String,
    /// "waiting" | "transferring" | "done" | "declined" | "pin" | "failed" | "cancelled"
    pub state: String,
    pub files: Vec<AirFile>,
    pub file_count: usize,
    pub total: u64,
    pub done: u64,
    /// Bytes per second.
    pub speed: u64,
    pub started: i64,
    pub finished: Option<i64>,
    pub error: Option<String>,
    /// Received files: the folder they were saved to.
    pub folder: Option<String>,
    pub text: Option<String>,
    /// A download handed over (the link is also in `text`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download: Option<RemoteDownload>,
}

impl AirTransfer {
    fn finished(&self) -> bool {
        matches!(self.state.as_str(), "done" | "declined" | "pin" | "failed" | "cancelled")
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AirRequest {
    pub id: String,
    pub peer: String,
    pub peer_fingerprint: String,
    pub peer_avatar: String,
    pub peer_os: String,
    pub files: Vec<AirFile>,
    pub file_count: usize,
    pub total: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AirDownloadRequest {
    pub id: String,
    pub peer: String,
    pub peer_fingerprint: String,
    pub peer_avatar: String,
    pub peer_os: String,
    pub download: RemoteDownload,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AirMessage {
    pub id: String,
    pub peer: String,
    pub peer_fingerprint: String,
    pub peer_avatar: String,
    pub text: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AirStatus {
    pub running: bool,
    pub alias: String,
    pub avatar: String,
    pub fingerprint: String,
    pub port: u16,
    pub addresses: Vec<String>,
    pub folder: String,
    /// Multicast discovery could not start; devices can still be added by IP.
    pub discovery_error: Option<String>,
}

// ───────────────────────── state ─────────────────────────

struct Identity {
    cert: Vec<u8>,
    key: Vec<u8>,
    fingerprint: String,
}

struct InFile {
    offer: OfferFile,
    token: String,
    /// Index in the transfer's listed files.
    index: Option<usize>,
    path: Option<PathBuf>,
    done: bool,
}

struct Incoming {
    id: String,
    accepted: bool,
    folder: PathBuf,
    files: HashMap<String, InFile>,
    last_activity: Instant,
}

struct Outgoing {
    abort: tokio::task::AbortHandle,
    /// Receiver's address and session, once it accepted (to cancel it there).
    remote: Option<(String, String, String)>,
}

#[derive(Default)]
struct Inner {
    peers: HashMap<String, AirPeer>,
    incoming: Option<Incoming>,
    decisions: HashMap<String, oneshot::Sender<bool>>,
    outgoing: HashMap<String, Outgoing>,
    active: HashMap<String, AirTransfer>,
    /// Per transfer: (speed sample time, bytes then, last event time).
    meters: HashMap<String, (Instant, u64, Instant)>,
    history: VecDeque<AirTransfer>,
    /// Devices being checked after an announcement (fingerprint → when).
    verifying: HashMap<String, Instant>,
    /// Wrong PINs per device: (count, last attempt).
    pin_fails: HashMap<String, (u32, Instant)>,
    last_message: HashMap<String, Instant>,
}

struct Running {
    tasks: Vec<JoinHandle<()>>,
    udp: Option<Arc<UdpSocket>>,
    discovery_error: Option<String>,
}

pub struct AirSend {
    core: Arc<Core>,
    /// Device certificate and history.
    dir: PathBuf,
    id: Identity,
    port: AtomicU16,
    inner: Mutex<Inner>,
    running: Mutex<Option<Running>>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn uid() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn fingerprint_of(cert: &[u8]) -> String {
    hex(&Sha256::digest(cert))
}

fn os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        "android" => "android",
        _ => "linux",
    }
}

fn computer_name() -> String {
    let n = gethostname::gethostname().to_string_lossy().trim().to_string();
    let n = n.strip_suffix(".local").unwrap_or(&n).to_string();
    if n.is_empty() {
        "KuDownloader".into()
    } else {
        n
    }
}

/// IPv4 addresses of the active network interfaces (no loopback / link-local).
fn local_ipv4() -> Vec<Ipv4Addr> {
    let mut out: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            IpAddr::V4(v) if !v.is_link_local() && !v.is_unspecified() => Some(v),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn plain_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(v6)),
        v4 => v4,
    }
}

// ───────────────────────── TLS ─────────────────────────

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn load_identity(dir: &Path) -> Result<Identity> {
    std::fs::create_dir_all(dir)?;
    let (cp, kp) = (dir.join("device.crt"), dir.join("device.key"));
    let (cert, key) = match (std::fs::read(&cp), std::fs::read(&kp)) {
        (Ok(c), Ok(k)) if !c.is_empty() && !k.is_empty() => (c, k),
        _ => {
            let ck = rcgen::generate_simple_self_signed(vec!["kuairsend.local".into()]).context("creating the KuAirSend device certificate")?;
            let (c, k) = (ck.cert.der().to_vec(), ck.key_pair.serialize_der());
            std::fs::write(&kp, &k)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&kp, std::fs::Permissions::from_mode(0o600));
            }
            std::fs::write(&cp, &c)?;
            (c, k)
        }
    };
    let fingerprint = fingerprint_of(&cert);
    Ok(Identity { cert, key, fingerprint })
}

fn cert_chain(id: &Identity) -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>) {
    (vec![CertificateDer::from(id.cert.clone())], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(id.key.clone())))
}

/// Server side: every client must present a certificate (any); its
/// fingerprint is then compared with the device it claims to be.
#[derive(Debug)]
struct AnyClientCert(Arc<CryptoProvider>);

impl ClientCertVerifier for AnyClientCert {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: UnixTime) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Client side: the receiver must present the certificate of the device we
/// mean to reach. With no expected fingerprint (adding a device by IP) any
/// certificate is accepted and remembered, to be matched against its answer.
#[derive(Debug)]
struct PinnedServer {
    provider: Arc<CryptoProvider>,
    expected: Option<String>,
    seen: Mutex<Option<String>>,
}

impl ServerCertVerifier for PinnedServer {
    fn verify_server_cert(&self, cert: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint_of(cert);
        if self.expected.as_ref().is_some_and(|e| *e != fp) {
            return Err(rustls::Error::General("not the expected KuAirSend device".into()));
        }
        *self.seen.lock().unwrap() = Some(fp);
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(m, c, d, &self.provider.signature_verification_algorithms)
    }
    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(m, c, d, &self.provider.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

fn acceptor(id: &Identity) -> Result<tokio_rustls::TlsAcceptor> {
    let p = provider();
    let (chain, key) = cert_chain(id);
    let mut cfg = rustls::ServerConfig::builder_with_provider(p.clone())
        .with_safe_default_protocol_versions()?
        .with_client_cert_verifier(Arc::new(AnyClientCert(p)))
        .with_single_cert(chain, key)?;
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(cfg)))
}

fn client(id: &Identity, expected: Option<&str>) -> Result<(reqwest::Client, Arc<PinnedServer>)> {
    let p = provider();
    let verifier = Arc::new(PinnedServer { provider: p.clone(), expected: expected.map(str::to_string), seen: Mutex::new(None) });
    let (chain, key) = cert_chain(id);
    let mut cfg = rustls::ClientConfig::builder_with_provider(p)
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_client_auth_cert(chain, key)?;
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    let c = reqwest::Client::builder()
        .use_preconfigured_tls(cfg)
        .no_proxy()
        .http1_only()
        .tcp_nodelay(true)
        .connect_timeout(Duration::from_secs(4))
        .build()?;
    Ok((c, verifier))
}

// ───────────────────────── files ─────────────────────────

/// A relative path that stays inside the receive folder.
fn safe_relative(name: &str) -> PathBuf {
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    let mut out = PathBuf::new();
    for part in name.split(['/', '\\']) {
        let clean: String = part.chars().map(|c| if c.is_control() || r#"<>:"|?*"#.contains(c) { '_' } else { c }).collect();
        let clean = clean.trim().trim_end_matches(['.', ' ']).to_string();
        if clean.is_empty() || clean == "." || clean == ".." {
            continue;
        }
        let stem = clean.split('.').next().unwrap_or("").to_ascii_lowercase();
        out.push(if RESERVED.contains(&stem.as_str()) { format!("_{clean}") } else { clean });
    }
    if out.as_os_str().is_empty() {
        out.push("file");
    }
    out
}

/// `path`, or "name (2).ext" … when it exists or another file in this transfer took it.
fn unique_path(path: PathBuf, taken: &HashSet<PathBuf>) -> PathBuf {
    let free = |p: &Path| !p.exists() && !taken.contains(p);
    if free(&path) {
        return path;
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..10_000).map(|n| path.with_file_name(format!("{stem} ({n}){ext}"))).find(|p| free(p)).unwrap_or(path)
}

/// Files to send: plain files as themselves, folders walked recursively
/// (named "folder/sub/file" so the receiver recreates the tree).
fn collect_files(paths: &[String]) -> Result<Vec<(PathBuf, String, u64)>> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<(PathBuf, String, u64)>) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = format!("{prefix}/{}", e.file_name().to_string_lossy());
            let ty = e.file_type()?;
            if ty.is_dir() {
                walk(&e.path(), &name, out)?;
            } else if ty.is_file() {
                out.push((e.path(), name, e.metadata()?.len()));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for p in paths {
        let path = PathBuf::from(p);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        let meta = std::fs::metadata(&path).with_context(|| format!("“{name}” can't be read"))?;
        if meta.is_dir() {
            walk(&path, &name, &mut out)?;
        } else {
            out.push((path, name, meta.len()));
        }
    }
    Ok(out)
}

// ───────────────────────── service ─────────────────────────

#[derive(Clone)]
struct Caller {
    ip: IpAddr,
    /// SHA-256 of the client certificate presented on this connection.
    fingerprint: String,
}

type Svc = Arc<AirSend>;

impl AirSend {
    pub fn new(core: Arc<Core>) -> Result<Arc<Self>> {
        Self::open(core, paths::data_dir().join("airsend"))
    }

    /// With its identity and history in `dir` (tests run two devices in one process).
    pub fn open(core: Arc<Core>, dir: PathBuf) -> Result<Arc<Self>> {
        let id = load_identity(&dir)?;
        let history = std::fs::read(dir.join("history.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<VecDeque<AirTransfer>>(&b).ok())
            .unwrap_or_default();
        Ok(Arc::new(AirSend { core, dir, id, port: AtomicU16::new(PORT), inner: Mutex::new(Inner { history, ..Default::default() }), running: Mutex::new(None) }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn is_running(&self) -> bool {
        self.running.lock().unwrap().is_some()
    }

    fn alias(&self) -> String {
        let n = self.core.settings().airsend_name.trim().to_string();
        if n.is_empty() {
            computer_name()
        } else {
            n
        }
    }

    fn avatar(&self) -> String {
        let a = self.core.settings().airsend_avatar;
        if AVATARS.contains(&a.as_str()) {
            a
        } else {
            let b = u8::from_str_radix(self.id.fingerprint.get(..2).unwrap_or("0"), 16).unwrap_or(0);
            AVATARS[b as usize % AVATARS.len()].into()
        }
    }

    fn folder(&self) -> PathBuf {
        let s = self.core.settings();
        if s.airsend_folder.trim().is_empty() {
            Path::new(&s.download_dir).join("KuAirSend")
        } else {
            PathBuf::from(s.airsend_folder.trim())
        }
    }

    fn info(&self, announce: bool) -> DeviceInfo {
        DeviceInfo {
            app: APP.into(),
            proto: PROTO,
            alias: self.alias(),
            avatar: self.avatar(),
            os: os_name().into(),
            version: env!("CARGO_PKG_VERSION").into(),
            fingerprint: self.id.fingerprint.clone(),
            port: self.port.load(Ordering::Relaxed),
            announce,
        }
    }

    pub fn status(&self) -> AirStatus {
        let run = self.running.lock().unwrap();
        AirStatus {
            running: run.is_some(),
            alias: self.alias(),
            avatar: self.avatar(),
            fingerprint: self.id.fingerprint.clone(),
            port: self.port.load(Ordering::Relaxed),
            addresses: local_ipv4().iter().map(|a| a.to_string()).collect(),
            folder: self.folder().display().to_string(),
            discovery_error: run.as_ref().and_then(|r| r.discovery_error.clone()),
        }
    }

    pub fn peers(&self) -> Vec<AirPeer> {
        let mut v: Vec<AirPeer> = self.lock().peers.values().cloned().collect();
        v.sort_by(|a, b| a.alias.to_lowercase().cmp(&b.alias.to_lowercase()).then(a.fingerprint.cmp(&b.fingerprint)));
        v
    }

    /// Active transfers (newest first), then the history.
    pub fn transfers(&self) -> Vec<AirTransfer> {
        let g = self.lock();
        let mut active: Vec<AirTransfer> = g.active.values().cloned().collect();
        active.sort_by_key(|t| std::cmp::Reverse(t.started));
        active.extend(g.history.iter().cloned());
        active
    }

    pub fn clear_history(&self) {
        self.lock().history.clear();
        self.save_history();
    }

    fn save_history(&self) {
        let json = serde_json::to_vec(&self.lock().history).unwrap_or_default();
        let _ = std::fs::write(self.dir.join("history.json"), json);
    }

    fn emit_peers(&self) {
        self.core.emit(CoreEvent::AirSendPeers { peers: self.peers() });
    }

    // ── start / stop ──

    pub async fn start(self: &Arc<Self>) -> Result<AirStatus> {
        if self.is_running() {
            return Ok(self.status());
        }
        let listener = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, PORT)).await {
            Ok(l) => l,
            // Another app holds the usual port: any free port works, it is announced.
            Err(_) => TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).await.context("KuAirSend could not open a network port")?,
        };
        self.port.store(listener.local_addr()?.port(), Ordering::Relaxed);
        let tls = acceptor(&self.id)?;
        let (udp, discovery_error) = match bind_multicast() {
            Ok(u) => (Some(Arc::new(u)), None),
            Err(e) => {
                tracing::warn!("KuAirSend discovery: {e:#}");
                (None, Some("Nearby devices can't be found automatically on this network. Add them by IP address.".to_string()))
            }
        };
        let mut tasks = vec![tokio::spawn(self.clone().serve(listener, tls))];
        if let Some(u) = &udp {
            tasks.push(tokio::spawn(self.clone().listen(u.clone())));
        }
        tasks.push(tokio::spawn(self.clone().housekeeping(udp.clone())));
        *self.running.lock().unwrap() = Some(Running { tasks, udp, discovery_error });
        tracing::info!("KuAirSend listening on port {}", self.port.load(Ordering::Relaxed));
        Ok(self.status())
    }

    pub async fn stop(self: &Arc<Self>) {
        let run = self.running.lock().unwrap().take();
        if let Some(r) = run {
            for t in r.tasks {
                t.abort();
            }
        }
        let (incoming, outgoing): (Option<String>, Vec<String>) = {
            let g = self.lock();
            (g.incoming.as_ref().map(|i| i.id.clone()), g.outgoing.keys().cloned().collect())
        };
        if let Some(id) = incoming {
            self.end_incoming(&id, "cancelled", None);
        }
        for id in outgoing {
            self.cancel(&id).await;
        }
        self.lock().peers.clear();
        self.emit_peers();
    }

    // ── discovery ──

    fn add_peer(&self, info: DeviceInfo, ip: IpAddr) -> Option<AirPeer> {
        if info.app != APP || info.fingerprint.is_empty() || info.fingerprint == self.id.fingerprint || info.port == 0 {
            return None;
        }
        let trusted = self.core.settings().airsend_trusted.contains(&info.fingerprint);
        let peer = AirPeer {
            alias: if info.alias.trim().is_empty() { "KuDownloader".into() } else { info.alias.trim().chars().take(40).collect() },
            avatar: if AVATARS.contains(&info.avatar.as_str()) { info.avatar } else { AVATARS[0].into() },
            os: info.os,
            version: info.version,
            ip: plain_ip(ip).to_string(),
            port: info.port,
            trusted,
            seen: Some(Instant::now()),
            fingerprint: info.fingerprint,
        };
        let changed = {
            let mut g = self.lock();
            if g.peers.len() >= MAX_PEERS && !g.peers.contains_key(&peer.fingerprint) {
                let oldest = g.peers.values().min_by_key(|p| p.seen).map(|p| p.fingerprint.clone());
                if let Some(fp) = oldest {
                    g.peers.remove(&fp);
                }
            }
            let old = g.peers.insert(peer.fingerprint.clone(), peer.clone());
            !matches!(old, Some(o) if o.alias == peer.alias && o.avatar == peer.avatar && o.ip == peer.ip && o.port == peer.port && o.trusted == peer.trusted)
        };
        if changed {
            self.emit_peers();
        }
        Some(peer)
    }

    async fn announce(&self, udp: &UdpSocket, announce: bool) {
        let msg = serde_json::to_vec(&self.info(announce)).unwrap_or_default();
        let target = SocketAddr::from((GROUP, PORT));
        let ifaces = local_ipv4();
        if ifaces.is_empty() {
            let _ = udp.send_to(&msg, target).await;
        }
        for ip in ifaces {
            // One copy per network (Wi-Fi and Ethernet can both be up).
            let _ = socket2::SockRef::from(udp).set_multicast_if_v4(&ip);
            let _ = udp.send_to(&msg, target).await;
        }
    }

    async fn listen(self: Arc<Self>, udp: Arc<UdpSocket>) {
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            let (n, from) = match udp.recv_from(&mut buf).await {
                Ok(x) => x,
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    continue;
                }
            };
            let Ok(info) = serde_json::from_slice::<DeviceInfo>(&buf[..n]) else { continue };
            if info.app != APP || info.fingerprint.is_empty() || info.fingerprint == self.id.fingerprint || info.port == 0 {
                continue;
            }
            // Anyone on the network can send these packets, so they only prompt a
            // check: a device is added or moved only after it proves, over TLS, that
            // it holds the certificate its fingerprint names.
            let ip = plain_ip(from.ip()).to_string();
            let known = {
                let mut g = self.lock();
                match g.peers.get_mut(&info.fingerprint) {
                    Some(p) if p.ip == ip && p.port == info.port => {
                        p.seen = Some(Instant::now());
                        true
                    }
                    _ => false,
                }
            };
            if known && !info.announce {
                continue;
            }
            let fresh = {
                let mut g = self.lock();
                let now = Instant::now();
                g.verifying.retain(|_, t| now.duration_since(*t) < Duration::from_secs(3));
                if g.verifying.len() >= MAX_PEERS || g.verifying.contains_key(&info.fingerprint) {
                    false
                } else {
                    g.verifying.insert(info.fingerprint.clone(), now);
                    true
                }
            };
            if !fresh {
                continue;
            }
            let (me, udp, wants_reply) = (self.clone(), udp.clone(), info.announce);
            tokio::spawn(async move {
                // Verifies both ends; if TLS can't get through, a UDP answer lets
                // the other device check us instead.
                if me.hello(&ip, info.port, Some(&info.fingerprint)).await.is_err() && wants_reply {
                    me.announce(&udp, false).await;
                }
            });
        }
    }

    /// Introduce ourselves to a device and learn who it is.
    async fn hello(&self, ip: &str, port: u16, fingerprint: Option<&str>) -> Result<AirPeer> {
        let (c, verifier) = client(&self.id, fingerprint)?;
        let info: DeviceInfo = c
            .post(format!("https://{ip}:{port}{API}/hello"))
            .timeout(Duration::from_secs(4))
            .json(&self.info(false))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        // The answer must come from the certificate it names.
        if verifier.seen.lock().unwrap().as_deref() != Some(info.fingerprint.as_str()) {
            bail!("the device's certificate does not match its id");
        }
        let ip: IpAddr = ip.parse()?;
        self.add_peer(info, ip).ok_or_else(|| anyhow!("That is not a KuDownloader device."))
    }

    /// Find devices now: a fresh announcement, and when that can't work (or
    /// nobody answered) a direct check of the local /24 networks.
    pub async fn refresh(self: &Arc<Self>) {
        let udp = self.running.lock().unwrap().as_ref().map(|r| r.udp.clone());
        let Some(udp) = udp else { return };
        if let Some(u) = &udp {
            self.announce(u, true).await;
        }
        let known: Vec<(String, u16, String)> = self.lock().peers.values().map(|p| (p.ip.clone(), p.port, p.fingerprint.clone())).collect();
        for (ip, port, fp) in known {
            let me = self.clone();
            tokio::spawn(async move {
                if me.hello(&ip, port, Some(&fp)).await.is_err() {
                    me.lock().peers.remove(&fp);
                    me.emit_peers();
                }
            });
        }
        if udp.is_none() || self.lock().peers.is_empty() {
            self.scan().await;
        }
    }

    async fn scan(self: &Arc<Self>) {
        let mine = local_ipv4();
        let hosts: Vec<String> = mine
            .iter()
            .flat_map(|ip| {
                let o = ip.octets();
                (1..=254u8).map(move |h| Ipv4Addr::new(o[0], o[1], o[2], h))
            })
            .filter(|h| !mine.contains(h))
            .map(|h| h.to_string())
            .collect();
        futures_util::stream::iter(hosts)
            .for_each_concurrent(64, |ip| {
                let me = self.clone();
                async move {
                    let _ = tokio::time::timeout(Duration::from_millis(1500), me.hello(&ip, PORT, None)).await;
                }
            })
            .await;
    }

    /// Add a device by address ("192.168.1.20" or "192.168.1.20:53318").
    pub async fn add_address(self: &Arc<Self>, address: &str) -> Result<AirPeer> {
        let a = address.trim();
        let (host, port) = match a.rsplit_once(':') {
            Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.parse().unwrap()),
            _ => (a.to_string(), PORT),
        };
        let ip: IpAddr = host.parse().map_err(|_| anyhow!("Enter an IP address like 192.168.1.20"))?;
        self.hello(&ip.to_string(), port, None)
            .await
            .map_err(|e| anyhow!("No KuDownloader with KuAirSend on at {a} ({e})"))
    }

    async fn housekeeping(self: Arc<Self>, udp: Option<Arc<UdpSocket>>) {
        if let Some(u) = &udp {
            for d in [100, 600, 2000] {
                tokio::time::sleep(Duration::from_millis(d)).await;
                self.announce(u, true).await;
            }
        }
        let mut last = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if let Some(u) = &udp {
                if last.elapsed() >= ANNOUNCE_EVERY {
                    self.announce(u, true).await;
                    last = Instant::now();
                }
            }
            let expired = {
                let mut g = self.lock();
                let before = g.peers.len();
                g.peers.retain(|_, p| p.seen.is_some_and(|s| s.elapsed() < PEER_TTL));
                g.peers.len() != before
            };
            if expired {
                self.emit_peers();
            }
            let idle = self.lock().incoming.as_ref().filter(|i| i.accepted && i.last_activity.elapsed() > IDLE_TIMEOUT).map(|i| i.id.clone());
            if let Some(id) = idle {
                self.end_incoming(&id, "failed", Some("The sender stopped responding.".into()));
            }
        }
    }

    // ── transfers bookkeeping ──

    fn begin(&self, t: AirTransfer) {
        let id = t.id.clone();
        self.lock().active.insert(id.clone(), t);
        self.update(&id, true, |_| {});
    }

    /// Change a transfer and publish it (progress at most 4× a second).
    fn update(&self, id: &str, force: bool, f: impl FnOnce(&mut AirTransfer)) {
        let (snapshot, finished) = {
            let mut g = self.lock();
            let Inner { active, meters, history, .. } = &mut *g;
            let Some(t) = active.get_mut(id) else { return };
            f(t);
            let now = Instant::now();
            let m = meters.entry(id.to_string()).or_insert((now, t.done, now - Duration::from_secs(1)));
            let dt = now.duration_since(m.0).as_secs_f64();
            if dt >= 0.5 {
                t.speed = (t.done.saturating_sub(m.1) as f64 / dt) as u64;
                m.0 = now;
                m.1 = t.done;
            }
            let finished = t.finished();
            if !force && !finished && now.duration_since(m.2) < Duration::from_millis(250) {
                return;
            }
            m.2 = now;
            if finished {
                t.speed = 0;
                t.finished.get_or_insert(now_ms());
                let done = active.remove(id).unwrap();
                meters.remove(id);
                history.push_front(done.clone());
                history.truncate(HISTORY_MAX);
                (done, true)
            } else {
                (t.clone(), false)
            }
        };
        if finished {
            self.save_history();
        }
        self.core.emit(CoreEvent::AirSendTransfer { transfer: Box::new(snapshot) });
    }

    // ── receiving ──

    async fn offer(self: Arc<Self>, caller: Caller, offer: Offer) -> Result<Option<OfferReply>, StatusCode> {
        let from = offer.from;
        if from.app != APP || from.fingerprint != caller.fingerprint {
            return Err(StatusCode::FORBIDDEN);
        }
        let Some(peer) = self.add_peer(from, caller.ip) else { return Err(StatusCode::FORBIDDEN) };
        let s = self.core.settings();
        let pin = s.airsend_pin.trim();
        if !pin.is_empty() {
            let mut g = self.lock();
            let locked = g.pin_fails.get(&peer.fingerprint).is_some_and(|(n, at)| *n >= PIN_TRIES && at.elapsed() < PIN_LOCKOUT);
            if locked {
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
            if offer.pin.as_deref().map(str::trim) != Some(pin) {
                let e = g.pin_fails.entry(peer.fingerprint.clone()).or_insert((0, Instant::now()));
                if e.1.elapsed() >= PIN_LOCKOUT {
                    e.0 = 0;
                }
                e.0 += 1;
                e.1 = Instant::now();
                return Err(StatusCode::UNAUTHORIZED);
            }
            g.pin_fails.remove(&peer.fingerprint);
        }
        if let Some(dl) = offer.download {
            return self.remote_download(peer, dl).await;
        }
        // A message or link: shown right away, nothing to store.
        if let Some(text) = offer.text.filter(|t| !t.trim().is_empty()) {
            {
                let mut g = self.lock();
                let now = Instant::now();
                if g.last_message.get(&peer.fingerprint).is_some_and(|t| now.duration_since(*t) < MESSAGE_GAP) {
                    return Err(StatusCode::TOO_MANY_REQUESTS);
                }
                g.last_message.insert(peer.fingerprint.clone(), now);
            }
            let text: String = text.chars().take(100_000).collect();
            let id = uid();
            self.begin(AirTransfer {
                id: id.clone(),
                direction: "receive".into(),
                peer: peer.alias.clone(),
                peer_fingerprint: peer.fingerprint.clone(),
                peer_avatar: peer.avatar.clone(),
                state: "done".into(),
                started: now_ms(),
                text: Some(text.clone()),
                ..Default::default()
            });
            self.core.emit(CoreEvent::AirSendMessage {
                message: Box::new(AirMessage { id, peer: peer.alias, peer_fingerprint: peer.fingerprint, peer_avatar: peer.avatar, text }),
            });
            return Ok(None);
        }
        if offer.files.is_empty() || offer.files.len() > 100_000 {
            return Err(StatusCode::BAD_REQUEST);
        }
        let id = uid();
        let folder = self.folder();
        let listed: Vec<AirFile> = offer.files.iter().take(FILES_LISTED).map(|f| AirFile { name: f.name.clone(), size: f.size, done: false }).collect();
        let total: u64 = offer.files.iter().fold(0u64, |a, f| a.saturating_add(f.size));
        let file_count = offer.files.len();
        // Refuse up front rather than fail at 99%.
        if free_space(&folder).is_some_and(|free| total > free) {
            return Err(StatusCode::INSUFFICIENT_STORAGE);
        }
        {
            let mut g = self.lock();
            if g.incoming.is_some() {
                return Err(StatusCode::CONFLICT);
            }
            let files = offer
                .files
                .into_iter()
                .enumerate()
                .map(|(i, f)| (f.id.clone(), InFile { token: uid(), index: (i < FILES_LISTED).then_some(i), path: None, done: false, offer: f }))
                .collect();
            g.incoming = Some(Incoming { id: id.clone(), accepted: false, folder: folder.clone(), files, last_activity: Instant::now() });
        }
        let auto = s.airsend_auto_accept || peer.trusted;
        self.begin(AirTransfer {
            id: id.clone(),
            direction: "receive".into(),
            peer: peer.alias.clone(),
            peer_fingerprint: peer.fingerprint.clone(),
            peer_avatar: peer.avatar.clone(),
            state: if auto { "transferring" } else { "waiting" }.into(),
            files: listed.clone(),
            file_count,
            total,
            started: now_ms(),
            folder: Some(folder.display().to_string()),
            ..Default::default()
        });
        if !auto {
            let (tx, rx) = oneshot::channel();
            self.lock().decisions.insert(id.clone(), tx);
            self.core.emit(CoreEvent::AirSendRequest {
                request: Box::new(AirRequest {
                    id: id.clone(),
                    peer: peer.alias.clone(),
                    peer_fingerprint: peer.fingerprint.clone(),
                    peer_avatar: peer.avatar.clone(),
                    peer_os: peer.os.clone(),
                    files: listed,
                    file_count,
                    total,
                }),
            });
            // If the sender gives up while we wait, this future is dropped: forget the offer.
            let guard = OfferGuard { me: self.clone(), id: id.clone(), armed: true };
            let accepted = matches!(tokio::time::timeout(ACCEPT_TIMEOUT, rx).await, Ok(Ok(true)));
            guard.disarm();
            if !accepted {
                self.end_incoming(&id, "declined", None);
                return Err(StatusCode::FORBIDDEN);
            }
        }
        let tokens = {
            let mut g = self.lock();
            let Some(inc) = g.incoming.as_mut().filter(|i| i.id == id) else { return Err(StatusCode::GONE) };
            inc.accepted = true;
            inc.last_activity = Instant::now();
            inc.files.iter().map(|(k, f)| (k.clone(), f.token.clone())).collect()
        };
        self.update(&id, true, |t| t.state = "transferring".into());
        Ok(Some(OfferReply { session: id, tokens }))
    }

    /// A link another device wants this one to download (now or later).
    async fn remote_download(self: &Arc<Self>, peer: AirPeer, mut dl: RemoteDownload) -> Result<Option<OfferReply>, StatusCode> {
        dl.url = dl.url.trim().to_string();
        if dl.url.len() > 8192 || !crate::classify::scheme_allowed(&dl.url) || dl.cookies.len() > 300 {
            return Err(StatusCode::BAD_REQUEST);
        }
        dl.filename = dl.filename.map(|f| crate::classify::sanitize_filename(&f)).filter(|f| !f.is_empty());
        let s = self.core.settings();
        let auto = s.airsend_auto_accept || peer.trusted;
        // Only questions are rate-limited; a trusted phone may hand over many links.
        if !auto {
            let mut g = self.lock();
            let now = Instant::now();
            if g.last_message.get(&peer.fingerprint).is_some_and(|t| now.duration_since(*t) < MESSAGE_GAP) {
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
            g.last_message.insert(peer.fingerprint.clone(), now);
        }
        let id = uid();
        self.begin(AirTransfer {
            id: id.clone(),
            direction: "receive".into(),
            peer: peer.alias.clone(),
            peer_fingerprint: peer.fingerprint.clone(),
            peer_avatar: peer.avatar.clone(),
            state: "waiting".into(),
            started: now_ms(),
            text: Some(dl.url.clone()),
            download: Some(dl.clone()),
            ..Default::default()
        });
        if !auto {
            let (tx, rx) = oneshot::channel();
            self.lock().decisions.insert(id.clone(), tx);
            self.core.emit(CoreEvent::AirSendDownload {
                request: Box::new(AirDownloadRequest {
                    id: id.clone(),
                    peer: peer.alias.clone(),
                    peer_fingerprint: peer.fingerprint.clone(),
                    peer_avatar: peer.avatar.clone(),
                    peer_os: peer.os.clone(),
                    download: dl.clone(),
                }),
            });
            let accepted = matches!(tokio::time::timeout(ACCEPT_TIMEOUT, rx).await, Ok(Ok(true)));
            self.lock().decisions.remove(&id);
            if !accepted {
                self.update(&id, true, |t| t.state = "declined".into());
                return Err(StatusCode::FORBIDDEN);
            }
        }
        match self.start_remote_download(&dl, &peer.alias).await {
            Ok(()) => {
                self.update(&id, true, |t| t.state = "done".into());
                Ok(None)
            }
            Err(e) => {
                let msg = format!("{e:#}");
                self.update(&id, true, |t| {
                    t.state = "failed".into();
                    t.error = Some(msg);
                });
                Err(StatusCode::UNPROCESSABLE_ENTITY)
            }
        }
    }

    /// Add a handed-over download: started now, or put in a queue of its own
    /// that a one-off schedule starts at the requested time.
    async fn start_remote_download(&self, dl: &RemoteDownload, from: &str) -> Result<()> {
        let later = dl.at.filter(|t| *t > now_ms() + 30_000);
        let queue_id = match later {
            Some(at) => Some(self.schedule_queue(at, from)?),
            None => None,
        };
        let req = ku_proto::AddRequest {
            url: dl.url.clone(),
            filename: dl.filename.clone(),
            queue_id,
            start: Some(later.is_none()),
            source: Some("airsend".into()),
            options: ku_proto::DownloadOptions { referer: dl.referer.clone(), cookies: dl.cookies.clone(), ..Default::default() },
            ..Default::default()
        };
        self.core.add(req).await?;
        Ok(())
    }

    /// The queue for downloads due at `at` (local time, to the minute), with
    /// the one-off schedule that starts it.
    fn schedule_queue(&self, at: i64, from: &str) -> Result<String> {
        let when = chrono::DateTime::from_timestamp_millis(at).ok_or_else(|| anyhow!("Invalid start time"))?.with_timezone(&chrono::Local);
        let (date, time) = (when.format("%Y-%m-%d").to_string(), when.format("%H:%M").to_string());
        let id = format!("airsend-{}", when.format("%Y%m%d-%H%M"));
        if !self.core.queues().iter().any(|q| q.id == id) {
            self.core.save_queue(crate::types::Queue { id: id.clone(), name: format!("KuAirSend · {date} {time}"), max_concurrent: 2, ..Default::default() })?;
            self.core.save_schedule(crate::types::Schedule {
                id: String::new(),
                name: format!("KuAirSend · {from}"),
                enabled: true,
                queue_id: id.clone(),
                start: time,
                date: Some(date),
                ..Default::default()
            })?;
        }
        Ok(id)
    }

    /// The user's answer to an incoming offer.
    pub async fn decide(&self, id: &str, accept: bool, trust: bool) -> Result<()> {
        if accept && trust {
            let fp = self.lock().active.get(id).map(|t| t.peer_fingerprint.clone());
            if let Some(fp) = fp {
                self.set_trusted(&fp, true).await?;
            }
        }
        let tx = self.lock().decisions.remove(id);
        match tx {
            Some(tx) => {
                let _ = tx.send(accept);
                Ok(())
            }
            None => bail!("The sender is no longer waiting."),
        }
    }

    pub async fn set_trusted(&self, fingerprint: &str, trusted: bool) -> Result<()> {
        let mut s = self.core.settings();
        s.airsend_trusted.retain(|f| f != fingerprint);
        if trusted {
            s.airsend_trusted.push(fingerprint.to_string());
        }
        self.core.save_settings(s).await?;
        if let Some(p) = self.lock().peers.get_mut(fingerprint) {
            p.trusted = trusted;
        }
        self.emit_peers();
        Ok(())
    }

    /// Stop an incoming transfer: partial files are removed, finished ones kept.
    fn end_incoming(&self, id: &str, state: &str, error: Option<String>) {
        let inc = {
            let mut g = self.lock();
            if let Some(tx) = g.decisions.remove(id) {
                let _ = tx.send(false);
            }
            if g.incoming.as_ref().is_some_and(|i| i.id == id) {
                g.incoming.take()
            } else {
                None
            }
        };
        if let Some(inc) = inc {
            for f in inc.files.values().filter(|f| !f.done) {
                if let Some(p) = &f.path {
                    let _ = std::fs::remove_file(part_path(p));
                }
            }
        }
        let state = state.to_string();
        self.update(id, true, |t| {
            if !t.finished() {
                t.state = state;
                t.error = error;
            }
        });
    }

    async fn receive_file(self: Arc<Self>, q: FileQuery, body: Body) -> Result<(), StatusCode> {
        let (dest, index, size) = {
            let mut g = self.lock();
            let inc = g.incoming.as_mut().filter(|i| i.id == q.session && i.accepted).ok_or(StatusCode::FORBIDDEN)?;
            let taken: HashSet<PathBuf> = inc.files.values().filter_map(|f| f.path.clone()).collect();
            let folder = inc.folder.clone();
            let f = inc.files.get_mut(&q.id).ok_or(StatusCode::FORBIDDEN)?;
            if f.token != q.token {
                return Err(StatusCode::FORBIDDEN);
            }
            if f.done || f.path.is_some() {
                return Err(StatusCode::CONFLICT);
            }
            let dest = unique_path(folder.join(safe_relative(&f.offer.name)), &taken);
            f.path = Some(dest.clone());
            let size = f.offer.size;
            inc.last_activity = Instant::now();
            (dest, f.index, size)
        };
        let part = part_path(&dest);
        let fail = |me: &AirSend, msg: String| {
            let _ = std::fs::remove_file(&part);
            me.end_incoming(&q.session, "failed", Some(msg));
        };
        if let Some(dir) = dest.parent() {
            if let Err(e) = tokio::fs::create_dir_all(dir).await {
                fail(self.as_ref(), format!("Can't create {}: {e}", dir.display()));
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
        let file = match tokio::fs::File::create(&part).await {
            Ok(f) => f,
            Err(e) => {
                fail(self.as_ref(), format!("Can't write {}: {e}", dest.display()));
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };
        let mut out = tokio::io::BufWriter::with_capacity(1 << 20, file);
        let mut stream = body.into_data_stream();
        let mut written = 0u64;
        loop {
            let chunk = match tokio::time::timeout(IDLE_TIMEOUT, stream.next()).await {
                Ok(None) => break,
                Ok(Some(Ok(c))) => c,
                Ok(Some(Err(_))) | Err(_) => {
                    fail(self.as_ref(), "The connection was lost.".into());
                    return Err(StatusCode::BAD_REQUEST);
                }
            };
            written += chunk.len() as u64;
            if written > size {
                fail(self.as_ref(), "The sender sent more data than it announced.".into());
                return Err(StatusCode::PAYLOAD_TOO_LARGE);
            }
            if let Err(e) = out.write_all(&chunk).await {
                fail(self.as_ref(), format!("Writing failed: {e}"));
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            // Still wanted? (cancelled by either side)
            let alive = {
                let mut g = self.lock();
                match g.incoming.as_mut().filter(|i| i.id == q.session) {
                    Some(i) => {
                        i.last_activity = Instant::now();
                        true
                    }
                    None => false,
                }
            };
            if !alive {
                drop(out);
                let _ = std::fs::remove_file(&part);
                return Err(StatusCode::GONE);
            }
            let n = chunk.len() as u64;
            self.update(&q.session, false, |t| t.done += n);
        }
        if written != size {
            fail(self.as_ref(), "The file arrived incomplete.".into());
            return Err(StatusCode::BAD_REQUEST);
        }
        if let Err(e) = out.flush().await {
            fail(self.as_ref(), format!("Writing failed: {e}"));
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        drop(out);
        if let Err(e) = tokio::fs::rename(&part, &dest).await {
            fail(self.as_ref(), format!("Can't save {}: {e}", dest.display()));
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        let all_done = {
            let mut g = self.lock();
            let Some(inc) = g.incoming.as_mut().filter(|i| i.id == q.session) else { return Ok(()) };
            if let Some(f) = inc.files.get_mut(&q.id) {
                f.done = true;
            }
            let all = inc.files.values().all(|f| f.done);
            if all {
                g.incoming = None;
            }
            all
        };
        self.update(&q.session, all_done, |t| {
            if let Some(i) = index {
                if let Some(f) = t.files.get_mut(i) {
                    f.done = true;
                }
            }
            if all_done {
                t.state = "done".into();
                t.done = t.total;
            }
        });
        Ok(())
    }

    // ── sending ──

    /// Send files/folders or a text/link to a nearby device. Returns the transfer id.
    pub fn send(self: &Arc<Self>, fingerprint: &str, paths: Vec<String>, text: Option<String>, pin: Option<String>) -> Result<String> {
        self.send_with(fingerprint, paths, text, None, pin)
    }

    /// Ask another device to download a link itself (now or at `download.at`).
    pub fn send_download(self: &Arc<Self>, fingerprint: &str, download: RemoteDownload, pin: Option<String>) -> Result<String> {
        if !crate::classify::scheme_allowed(download.url.trim()) {
            bail!("Unsupported address. KuDownloader accepts http, https, ftp, sftp and magnet links.");
        }
        self.send_with(fingerprint, Vec::new(), None, Some(download), pin)
    }

    fn send_with(self: &Arc<Self>, fingerprint: &str, paths: Vec<String>, text: Option<String>, download: Option<RemoteDownload>, pin: Option<String>) -> Result<String> {
        if !self.is_running() {
            bail!("Turn on KuAirSend first.");
        }
        let peer = self.lock().peers.get(fingerprint).cloned().ok_or_else(|| anyhow!("That device is no longer nearby."))?;
        let id = uid();
        self.begin(AirTransfer {
            id: id.clone(),
            direction: "send".into(),
            peer: peer.alias.clone(),
            peer_fingerprint: peer.fingerprint.clone(),
            peer_avatar: peer.avatar.clone(),
            state: "waiting".into(),
            started: now_ms(),
            text: text.clone().or_else(|| download.as_ref().map(|d| d.url.clone())),
            download: download.clone(),
            ..Default::default()
        });
        let (me, tid) = (self.clone(), id.clone());
        let task = tokio::spawn(async move {
            let (state, error) = match me.clone().send_inner(&tid, &peer, paths, text, download, pin).await {
                Ok(state) => (state.to_string(), None),
                Err(e) => ("failed".to_string(), Some(format!("{e:#}"))),
            };
            me.lock().outgoing.remove(&tid);
            me.update(&tid, true, |t| {
                t.state = state;
                t.error = error;
                if t.state == "done" {
                    t.done = t.total;
                }
            });
        });
        self.lock().outgoing.insert(id.clone(), Outgoing { abort: task.abort_handle(), remote: None });
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    async fn send_inner(self: Arc<Self>, id: &str, peer: &AirPeer, paths: Vec<String>, text: Option<String>, download: Option<RemoteDownload>, pin: Option<String>) -> Result<&'static str> {
        // Older versions don't know `download`: they show the link as a message.
        let text = text.or_else(|| download.as_ref().map(|d| d.url.clone()));
        let (c, _) = client(&self.id, Some(&peer.fingerprint))?;
        let base = format!("https://{}:{}{API}", peer.ip, peer.port);
        let files = match &text {
            Some(_) => Vec::new(),
            None => tokio::task::spawn_blocking(move || collect_files(&paths)).await??,
        };
        if text.is_none() && files.is_empty() {
            bail!("There is nothing to send (the folder is empty).");
        }
        let offer_files: Vec<OfferFile> = files.iter().map(|(_, name, size)| OfferFile { id: uid(), name: name.clone(), size: *size }).collect();
        let total: u64 = offer_files.iter().map(|f| f.size).sum();
        self.update(id, true, |t| {
            t.files = offer_files.iter().take(FILES_LISTED).map(|f| AirFile { name: f.name.clone(), size: f.size, done: false }).collect();
            t.file_count = offer_files.len();
            t.total = total;
        });
        let offer = Offer { from: self.info(false), files: offer_files.clone(), text, pin, download };
        let resp = c
            .post(format!("{base}/offer"))
            .timeout(ACCEPT_TIMEOUT + Duration::from_secs(30))
            .json(&offer)
            .send()
            .await
            .with_context(|| format!("Could not reach {}", peer.alias))?;
        match resp.status() {
            StatusCode::NO_CONTENT => return Ok("done"),
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED => return Ok("pin"),
            StatusCode::FORBIDDEN => return Ok("declined"),
            StatusCode::CONFLICT => bail!("{} is receiving from someone else. Try again in a moment.", peer.alias),
            StatusCode::TOO_MANY_REQUESTS => bail!("{} asked to wait (too many attempts). Try again in a minute.", peer.alias),
            StatusCode::INSUFFICIENT_STORAGE => bail!("{} does not have enough free space for this.", peer.alias),
            StatusCode::UNPROCESSABLE_ENTITY => bail!("{} could not start the download.", peer.alias),
            s => bail!("{} refused the transfer ({s}).", peer.alias),
        }
        let reply: OfferReply = resp.json().await?;
        if let Some(o) = self.lock().outgoing.get_mut(id) {
            o.remote = Some((base.clone(), reply.session.clone(), peer.fingerprint.clone()));
        }
        self.update(id, true, |t| t.state = "transferring".into());
        let jobs: Vec<(usize, PathBuf, OfferFile, String)> = files
            .into_iter()
            .zip(offer_files)
            .enumerate()
            .filter_map(|(i, ((path, _, _), f))| reply.tokens.get(&f.id).map(|tok| (i, path, f.clone(), tok.clone())))
            .collect();
        futures_util::stream::iter(jobs)
            .map(|(i, path, f, token)| {
                let (me, c, base, session) = (self.clone(), c.clone(), base.clone(), reply.session.clone());
                async move { me.upload(&c, id, &base, &session, i, &path, &f, &token).await }
            })
            .buffer_unordered(PARALLEL_FILES)
            .try_collect::<Vec<()>>()
            .await?;
        Ok("done")
    }

    #[allow(clippy::too_many_arguments)]
    async fn upload(self: Arc<Self>, c: &reqwest::Client, id: &str, base: &str, session: &str, index: usize, path: &Path, f: &OfferFile, token: &str) -> Result<()> {
        let file = tokio::fs::File::open(path).await.with_context(|| format!("“{}” can't be read", f.name))?;
        let (me, tid) = (self.clone(), id.to_string());
        let stream = tokio_util::io::ReaderStream::with_capacity(file, 256 * 1024).inspect_ok(move |chunk| {
            let n = chunk.len() as u64;
            me.update(&tid, false, |t| t.done += n);
        });
        let resp = c
            .post(format!("{base}/file"))
            .query(&[("session", session), ("id", &f.id), ("token", token)])
            .header(reqwest::header::CONTENT_LENGTH, f.size)
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await
            .with_context(|| format!("Sending “{}” failed", f.name))?;
        if !resp.status().is_success() {
            bail!("The other device stopped the transfer ({}).", resp.status());
        }
        self.update(id, false, |t| {
            if let Some(x) = t.files.get_mut(index) {
                x.done = true;
            }
        });
        Ok(())
    }

    /// Cancel a transfer in either direction.
    pub async fn cancel(self: &Arc<Self>, id: &str) {
        let out = self.lock().outgoing.remove(id);
        if let Some(o) = out {
            o.abort.abort();
            if let Some((base, session, fp)) = o.remote {
                if let Ok((c, _)) = client(&self.id, Some(&fp)) {
                    let _ = c.post(format!("{base}/cancel")).query(&[("session", session)]).timeout(Duration::from_secs(4)).send().await;
                }
            }
            self.update(id, true, |t| t.state = "cancelled".into());
            return;
        }
        self.end_incoming(id, "cancelled", None);
    }

    // ── HTTPS server ──

    async fn serve(self: Arc<Self>, listener: TcpListener, tls: tokio_rustls::TlsAcceptor) {
        let app = Router::new()
            .route(&format!("{API}/hello"), post(hello_handler))
            .route(&format!("{API}/offer"), post(offer_handler).layer(DefaultBodyLimit::max(16 << 20)))
            .route(&format!("{API}/file"), post(file_handler).layer(DefaultBodyLimit::disable()))
            .route(&format!("{API}/cancel"), post(cancel_handler))
            .with_state(self.clone());
        let slots = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
        loop {
            let Ok((tcp, addr)) = listener.accept().await else {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            // Full: drop the connection rather than queue it.
            let Ok(permit) = slots.clone().try_acquire_owned() else { continue };
            let _ = tcp.set_nodelay(true);
            let (tls, app) = (tls.clone(), app.clone());
            tokio::spawn(async move {
                let _permit = permit;
                let Ok(Ok(stream)) = tokio::time::timeout(Duration::from_secs(10), tls.accept(tcp)).await else { return };
                let fingerprint = stream.get_ref().1.peer_certificates().and_then(|c| c.first()).map(|c| fingerprint_of(c)).unwrap_or_default();
                let app = app.layer(Extension(Caller { ip: plain_ip(addr.ip()), fingerprint }));
                let svc = hyper_util::service::TowerToHyperService::new(app);
                let _ = hyper::server::conn::http1::Builder::new()
                    .timer(hyper_util::rt::TokioTimer::new())
                    .header_read_timeout(Duration::from_secs(10))
                    .serve_connection(hyper_util::rt::TokioIo::new(stream), svc).await;
            });
        }
    }
}

/// Free bytes on the disk that holds `dir` (or its nearest existing parent).
fn free_space(dir: &Path) -> Option<u64> {
    let existing = dir.ancestors().find(|p| p.exists())?;
    fs4::available_space(existing).ok()
}

fn part_path(dest: &Path) -> PathBuf {
    PathBuf::from(format!("{}.kuairsend-part", dest.display()))
}

/// Drops a pending offer if its HTTP request goes away before the user answers.
struct OfferGuard {
    me: Arc<AirSend>,
    id: String,
    armed: bool,
}

impl OfferGuard {
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for OfferGuard {
    fn drop(&mut self) {
        if self.armed {
            self.me.end_incoming(&self.id, "cancelled", None);
        }
    }
}

#[derive(Deserialize)]
struct FileQuery {
    session: String,
    id: String,
    token: String,
}

#[derive(Deserialize)]
struct SessionQuery {
    session: String,
}

async fn hello_handler(State(me): State<Svc>, Extension(caller): Extension<Caller>, Json(info): Json<DeviceInfo>) -> Result<Json<DeviceInfo>, StatusCode> {
    if info.fingerprint != caller.fingerprint {
        return Err(StatusCode::FORBIDDEN);
    }
    me.add_peer(info, caller.ip).ok_or(StatusCode::FORBIDDEN)?;
    Ok(Json(me.info(false)))
}

async fn offer_handler(State(me): State<Svc>, Extension(caller): Extension<Caller>, Json(offer): Json<Offer>) -> axum::response::Response {
    use axum::response::IntoResponse;
    match me.offer(caller, offer).await {
        Ok(Some(reply)) => Json(reply).into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(code) => code.into_response(),
    }
}

async fn file_handler(State(me): State<Svc>, Extension(caller): Extension<Caller>, Query(q): Query<FileQuery>, body: Body) -> StatusCode {
    // Only the device that made the offer may upload into it.
    let owner = me.lock().active.get(&q.session).map(|t| t.peer_fingerprint.clone());
    if owner.as_deref() != Some(caller.fingerprint.as_str()) {
        return StatusCode::FORBIDDEN;
    }
    match me.receive_file(q, body).await {
        Ok(()) => StatusCode::OK,
        Err(code) => code,
    }
}

async fn cancel_handler(State(me): State<Svc>, Extension(caller): Extension<Caller>, Query(q): Query<SessionQuery>) -> StatusCode {
    let owner = me.lock().active.get(&q.session).map(|t| t.peer_fingerprint.clone());
    if owner.as_deref() == Some(caller.fingerprint.as_str()) {
        me.end_incoming(&q.session, "cancelled", Some("The sender cancelled.".into()));
    }
    StatusCode::OK
}

fn bind_multicast() -> Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_reuse_address(true)?;
    #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos"))))]
    let _ = s.set_reuse_port(true);
    s.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT)).into())?;
    s.set_multicast_loop_v4(true)?;
    s.set_multicast_ttl_v4(1)?;
    let mut joined = 0;
    for ip in local_ipv4() {
        if s.join_multicast_v4(&GROUP, &ip).is_ok() {
            joined += 1;
        }
    }
    if joined == 0 {
        s.join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)?;
    }
    s.set_nonblocking(true)?;
    Ok(UdpSocket::from_std(s.into())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn received_names_stay_inside_the_folder() {
        assert_eq!(safe_relative("../../etc/passwd"), PathBuf::from("etc").join("passwd"));
        assert_eq!(safe_relative("C:\\Windows\\x.dll"), PathBuf::from("C_").join("Windows").join("x.dll"));
        assert_eq!(safe_relative("album/cover.jpg"), PathBuf::from("album").join("cover.jpg"));
        assert_eq!(safe_relative("con.txt"), PathBuf::from("_con.txt"));
        assert_eq!(safe_relative("  ..  "), PathBuf::from("file"));
        assert_eq!(safe_relative("a<b>.txt"), PathBuf::from("a_b_.txt"));
    }

    #[test]
    fn taken_names_get_a_number() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("photo.jpg");
        std::fs::write(&p, b"x").unwrap();
        let mut taken = HashSet::new();
        let a = unique_path(p.clone(), &taken);
        assert_eq!(a, dir.path().join("photo (2).jpg"));
        taken.insert(a);
        assert_eq!(unique_path(p, &taken), dir.path().join("photo (3).jpg"));
    }
}
