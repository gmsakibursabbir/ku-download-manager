use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum KuError {
    /// Non-success HTTP status.
    Http { status: u16, retry_after: Option<Duration> },
    /// Connection refused/reset, DNS, TLS…
    Network(String),
    Timeout,
    /// The server answered a range request with the full body (200).
    RangeIgnored,
    /// Content-Range missing or not matching what was requested.
    BadContentRange(String),
    /// The body ended before the promised length.
    Truncated { expected: u64, got: u64 },
    /// Size / ETag / Last-Modified / content no longer match.
    ResourceChanged(String),
    TooManyRedirects,
    RedirectLoop,
    BadRedirect(String),
    InvalidUrl(String),
    InvalidHeader(String),
    DiskFull,
    Io(String),
    SizeMismatch { expected: u64, actual: u64 },
    VerificationFailed { algo: String, expected: String, actual: String },
    Cancelled,
    Paused,
}

impl KuError {
    /// Worth retrying the same request after a backoff.
    pub fn is_transient(&self) -> bool {
        match self {
            KuError::Http { status, .. } => matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504),
            KuError::Network(_) | KuError::Timeout | KuError::Truncated { .. } | KuError::BadContentRange(_) => true,
            _ => false,
        }
    }

    /// The server is asking us to slow down.
    pub fn is_throttle(&self) -> bool {
        matches!(self, KuError::Http { status: 429 | 503, .. })
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            KuError::Http { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    pub fn from_io(e: &std::io::Error) -> KuError {
        if is_disk_full(e) {
            KuError::DiskFull
        } else {
            KuError::Io(e.to_string())
        }
    }

    pub fn from_reqwest(e: &reqwest::Error) -> KuError {
        if e.is_timeout() {
            KuError::Timeout
        } else if e.is_builder() {
            KuError::InvalidUrl(e.to_string())
        } else {
            // Never include the URL: it may carry signed tokens.
            let mut msg = e.without_url_ref();
            if msg.is_empty() {
                msg = "connection failed".into();
            }
            KuError::Network(msg)
        }
    }
}

trait WithoutUrl {
    fn without_url_ref(&self) -> String;
}

impl WithoutUrl for reqwest::Error {
    fn without_url_ref(&self) -> String {
        let mut s = String::new();
        let mut src: Option<&dyn std::error::Error> = std::error::Error::source(self);
        while let Some(e) = src {
            s = e.to_string();
            src = e.source();
        }
        if s.is_empty() {
            if self.is_connect() {
                s = "could not connect".into();
            } else if self.is_body() || self.is_decode() {
                s = "the connection was interrupted".into();
            }
        }
        s
    }
}

pub fn is_disk_full(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::StorageFull {
        return true;
    }
    match e.raw_os_error() {
        // ERROR_HANDLE_DISK_FULL, ERROR_DISK_FULL on Windows
        #[cfg(windows)]
        Some(39) | Some(112) => true,
        // ENOSPC, EDQUOT
        #[cfg(unix)]
        Some(28) | Some(122) => true,
        _ => false,
    }
}

impl std::fmt::Display for KuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KuError::Http { status, .. } => write!(f, "{}", crate::probe::describe_status(*status)),
            KuError::Network(m) => write!(f, "Network error: {m}"),
            KuError::Timeout => write!(f, "The server stopped responding."),
            KuError::RangeIgnored => write!(f, "The server ignored a byte-range request."),
            KuError::BadContentRange(m) => write!(f, "The server sent an invalid range response ({m})."),
            KuError::Truncated { expected, got } => write!(f, "The connection ended early ({got} of {expected} bytes)."),
            KuError::ResourceChanged(m) => write!(f, "The file changed on the server ({m})."),
            KuError::TooManyRedirects => write!(f, "Too many redirects."),
            KuError::RedirectLoop => write!(f, "The server redirects in a loop."),
            KuError::BadRedirect(m) => write!(f, "Invalid redirect: {m}"),
            KuError::InvalidUrl(m) => write!(f, "Invalid address: {m}"),
            KuError::InvalidHeader(m) => write!(f, "Invalid request header: {m}"),
            KuError::DiskFull => write!(f, "Not enough disk space. Free some space and resume the download."),
            KuError::Io(m) => write!(f, "Disk error: {m}"),
            KuError::SizeMismatch { expected, actual } => write!(f, "Size check failed: expected {expected} bytes, got {actual}."),
            KuError::VerificationFailed { algo, expected, actual } => {
                write!(f, "Checksum mismatch ({algo}): expected {expected}, got {actual}. The file was not saved.")
            }
            KuError::Cancelled => write!(f, "Cancelled."),
            KuError::Paused => write!(f, "Paused."),
        }
    }
}

impl std::error::Error for KuError {}
