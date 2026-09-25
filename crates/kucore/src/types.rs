use ku_proto::{AddRequest, Download, GrabRequest, MediaRequest};
use serde::{Deserialize, Serialize};

pub const MAIN_QUEUE: &str = "main";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Queue {
    pub id: String,
    pub name: String,
    pub max_concurrent: u32,
    pub position: i64,
    /// Persisted so a running queue survives restarts and crashes.
    pub running: bool,
    /// Set when a schedule started the queue: power action once it drains.
    pub after: String,
    /// Synchronization (IDM-style): every N minutes re-check finished files on
    /// the server and download the ones that changed. 0 = off.
    pub sync_minutes: u32,
    /// Last synchronization check (ms since epoch).
    pub last_sync: i64,
}

impl Default for Queue {
    fn default() -> Self {
        Queue {
            id: MAIN_QUEUE.into(),
            name: "Main queue".into(),
            max_concurrent: 2,
            position: 0,
            running: false,
            after: "none".into(),
            sync_minutes: 0,
            last_sync: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub queue_id: String,
    /// Local time "HH:MM".
    pub start: String,
    /// Optional local stop time "HH:MM"; the queue is paused then.
    pub stop: Option<String>,
    /// ISO weekdays (1 = Monday … 7 = Sunday). Empty = every day.
    pub days: Vec<u8>,
    /// One-off date "YYYY-MM-DD"; overrides `days`.
    pub date: Option<String>,
    /// Bandwidth profile applied while the schedule window is active.
    pub profile: Option<String>,
    /// none / shutdown / sleep / quit — once the queue drains.
    pub after: String,
    pub last_start: Option<String>,
    pub last_stop: Option<String>,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule {
            id: String::new(),
            name: "Night downloads".into(),
            enabled: true,
            queue_id: MAIN_QUEUE.into(),
            start: "02:00".into(),
            stop: None,
            days: Vec::new(),
            date: None,
            profile: None,
            after: "none".into(),
            last_start: None,
            last_stop: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProgressItem {
    pub id: String,
    pub status: ku_proto::Status,
    pub done: i64,
    pub total: i64,
    pub speed: i64,
    pub upload_speed: i64,
    pub active_connections: u32,
    pub eta: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub ts: i64,
    pub level: String,
    pub message: String,
}

/// Events published by KuCore; the desktop shell forwards them to the UI.
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CoreEvent {
    /// Batched live progress (at most once per second).
    #[serde(rename_all = "camelCase")]
    Progress { items: Vec<ProgressItem>, download_speed: i64, upload_speed: i64 },
    Upsert { download: Box<Download> },
    Removed { ids: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Notice { level: String, title: String, message: String, download_id: Option<String> },
    Completed { id: String, name: String, path: Option<String> },
    #[serde(rename_all = "camelCase")]
    QueueDone { queue_id: String, name: String },
    PromptAdd { request: Box<AddRequest> },
    PromptMedia { request: Box<MediaRequest> },
    Grab { request: Box<GrabRequest> },
    ClipboardUrl { url: String },
    PowerCountdown { action: String, seconds: u32 },
    PowerCancelled,
    /// On-demand tool download (yt-dlp / ffmpeg); `total` 0 when unknown.
    ToolProgress { tool: String, done: u64, total: u64 },
    /// An on-demand tool download finished (`ok`) or failed (`message`).
    ToolDone { tool: String, ok: bool, message: String },
    /// KuAirSend: the nearby devices changed.
    AirSendPeers { peers: Vec<crate::airsend::AirPeer> },
    /// KuAirSend: a transfer started, progressed or finished.
    AirSendTransfer { transfer: Box<crate::airsend::AirTransfer> },
    /// KuAirSend: a nearby device wants to send files (Accept / Decline).
    AirSendRequest { request: Box<crate::airsend::AirRequest> },
    /// KuAirSend: a nearby device asks this one to download a link (Accept / Decline).
    AirSendDownload { request: Box<crate::airsend::AirDownloadRequest> },
    /// KuAirSend: a text message or link arrived.
    AirSendMessage { message: Box<crate::airsend::AirMessage> },
    Show,
    SettingsChanged,
    QueuesChanged,
    SchedulesChanged,
}

impl CoreEvent {
    /// Events that ask the user for a decision; buffered until the UI is ready.
    pub fn is_prompt(&self) -> bool {
        matches!(
            self,
            CoreEvent::PromptAdd { .. }
                | CoreEvent::PromptMedia { .. }
                | CoreEvent::Grab { .. }
                | CoreEvent::ClipboardUrl { .. }
                | CoreEvent::PowerCountdown { .. }
                | CoreEvent::AirSendRequest { .. }
                | CoreEvent::AirSendDownload { .. }
                | CoreEvent::AirSendMessage { .. }
        )
    }
}
