use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Queued,
    Probing,
    Downloading,
    Verifying,
    Finalizing,
    Completed,
    Paused,
    Cancelled,
    Failed,
    /// Stopped on repeated transient errors; safe to resume later.
    Recoverable,
    FailedVerification,
}

impl State {
    pub fn is_terminal(self) -> bool {
        !matches!(self, State::Queued | State::Probing | State::Downloading | State::Verifying | State::Finalizing)
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SegmentInfo {
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub active: bool,
    pub done: bool,
    pub speed: u64,
    pub retries: u32,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: State,
    pub downloaded: u64,
    /// None when the server does not report a length.
    pub total: Option<u64>,
    /// None when the total is unknown — never a fake percentage.
    pub percent: Option<f64>,
    pub speed: u64,
    pub average_speed: u64,
    /// Seconds; None when unknown or speed is zero.
    pub eta: Option<f64>,
    pub connections: u32,
    pub target_connections: u32,
    pub active_segments: u32,
    pub completed_segments: u32,
    pub failed_segments: u32,
    pub segmented: bool,
    pub retries: u32,
    pub file_name: Option<String>,
    pub final_path: Option<String>,
    pub temp_path: Option<String>,
    pub error: Option<String>,
    pub message: Option<String>,
    pub http_version: Option<String>,
}

impl Status {
    pub fn queued() -> Status {
        Status {
            state: State::Queued,
            downloaded: 0,
            total: None,
            percent: None,
            speed: 0,
            average_speed: 0,
            eta: None,
            connections: 0,
            target_connections: 0,
            active_segments: 0,
            completed_segments: 0,
            failed_segments: 0,
            segmented: false,
            retries: 0,
            file_name: None,
            final_path: None,
            temp_path: None,
            error: None,
            message: None,
            http_version: None,
        }
    }
}

/// Lightweight engine events. Progress (including per-segment detail) is
/// coalesced to one event per `progress_interval` per download.
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum KuEvent {
    Started { id: String },
    Probing { id: String },
    Progress { id: String, status: Box<Status>, segments: Vec<SegmentInfo> },
    SegmentStarted { id: String, start: u64, end: u64 },
    SegmentCompleted { id: String, start: u64, end: u64 },
    ConnectionAdded { id: String, connections: u32 },
    ConnectionRemoved { id: String, connections: u32 },
    SpeedChanged { id: String, speed: u64 },
    Retrying { id: String, attempt: u32, reason: String },
    Paused { id: String },
    Resumed { id: String, downloaded: u64 },
    Restarted { id: String, reason: String },
    Verifying { id: String },
    VerificationFailed { id: String, reason: String },
    Completed { id: String, path: String, size: u64 },
    Failed { id: String, error: String, recoverable: bool },
    Cancelled { id: String },
}
