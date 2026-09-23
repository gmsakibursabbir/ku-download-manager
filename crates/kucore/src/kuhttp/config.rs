use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct ProxyConfig {
    /// http://, https:// or socks5:// URL.
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Test hook: return an error to fail a positional write at `offset`.
pub type WriteFault = Arc<dyn Fn(u64, usize) -> Option<std::io::Error> + Send + Sync>;

#[derive(Clone)]
pub struct KuHttpConfig {
    /// Lower / upper bound for adaptive connection counts.
    pub min_connections: u32,
    pub max_connections: u32,
    /// Files smaller than this use a single connection.
    pub small_file_threshold: u64,
    /// Never create segments smaller than this (HTTP overhead).
    pub min_segment_size: u64,
    /// Bytes buffered per worker before a positional write.
    pub write_buffer: usize,
    pub max_retries: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub connect_timeout: Duration,
    /// Maximum silence while reading a response body.
    pub read_timeout: Duration,
    /// Coalesced progress event interval.
    pub progress_interval: Duration,
    /// How often synced progress is written to the resume state.
    pub persist_interval: Duration,
    pub max_redirects: usize,
    pub user_agent: String,
    pub verify_tls: bool,
    pub proxy: Option<ProxyConfig>,
    /// Restart from scratch when the remote file changed (otherwise fail).
    pub restart_on_change: bool,
    /// Windows: mark temp files sparse to avoid NTFS zero-filling.
    pub sparse_files: bool,
    #[doc(hidden)]
    pub write_fault: Option<WriteFault>,
}

impl Default for KuHttpConfig {
    fn default() -> Self {
        KuHttpConfig {
            min_connections: 1,
            max_connections: 8,
            small_file_threshold: 10 * 1024 * 1024,
            min_segment_size: 2 * 1024 * 1024,
            write_buffer: 512 * 1024,
            max_retries: 10,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(15),
            read_timeout: Duration::from_secs(30),
            progress_interval: Duration::from_millis(500),
            persist_interval: Duration::from_secs(3),
            max_redirects: 10,
            user_agent: crate::probe::DEFAULT_USER_AGENT.to_string(),
            verify_tls: true,
            proxy: None,
            restart_on_change: true,
            sparse_files: true,
            write_fault: None,
        }
    }
}

impl std::fmt::Debug for KuHttpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deliberately omits proxy credentials.
        f.debug_struct("KuHttpConfig")
            .field("min_connections", &self.min_connections)
            .field("max_connections", &self.max_connections)
            .field("proxy", &self.proxy.as_ref().map(|p| crate::kuhttp::redact(&p.url)))
            .finish_non_exhaustive()
    }
}
