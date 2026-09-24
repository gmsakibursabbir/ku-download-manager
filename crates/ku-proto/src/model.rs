use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Aria2,
    Ytdlp,
    /// KuDownloader's native HTTP engine (experimental).
    Kuhttp,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Http,
    Ftp,
    Sftp,
    Torrent,
    Magnet,
    Metalink,
    Media,
}

impl Kind {
    pub fn is_bittorrent(self) -> bool {
        matches!(self, Kind::Torrent | Kind::Magnet)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Waiting for a free slot or for its queue to start.
    Queued,
    Downloading,
    /// Post-processing (merging media streams, verifying).
    Processing,
    Paused,
    Seeding,
    Completed,
    Error,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Queued => "queued",
            Status::Downloading => "downloading",
            Status::Processing => "processing",
            Status::Paused => "paused",
            Status::Seeding => "seeding",
            Status::Completed => "completed",
            Status::Error => "error",
        }
    }
    pub fn parse(s: &str) -> Status {
        match s {
            "downloading" => Status::Downloading,
            "processing" => Status::Processing,
            "paused" => Status::Paused,
            "seeding" => Status::Seeding,
            "completed" => Status::Completed,
            "error" => Status::Error,
            _ => Status::Queued,
        }
    }
    /// Occupies an engine slot.
    pub fn is_running(self) -> bool {
        matches!(self, Status::Downloading | Status::Processing)
    }
    pub fn is_finished(self) -> bool {
        matches!(self, Status::Completed | Status::Seeding)
    }
}

/// A cookie as exported by the browser extension (`chrome.cookies` shape).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct BrowserCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub host_only: bool,
    pub expiration_date: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct MediaOptions {
    /// "video" or "audio".
    pub mode: String,
    /// Maximum video height, e.g. 1080. `None` = best available.
    pub height: Option<u32>,
    /// Target audio bitrate in kbps for audio-only downloads.
    pub audio_bitrate: Option<u32>,
    /// mp4 / mkv / webm for video; mp3 / m4a / opus / flac for audio.
    pub container: Option<String>,
    /// Explicit yt-dlp format selector; overrides height.
    pub format_id: Option<String>,
    pub subtitles: bool,
    pub sub_langs: Option<String>,
    pub embed_subtitles: bool,
    pub write_thumbnail: bool,
    pub embed_thumbnail: bool,
    pub playlist: bool,
    /// yt-dlp `--playlist-items` spec, e.g. "1,3,5-7".
    pub playlist_items: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct DownloadOptions {
    /// Extra request headers, each `Name: value`.
    pub headers: Vec<String>,
    /// Cookies supplied by the browser extension for the download URL.
    pub cookies: Vec<BrowserCookie>,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    /// `algo=hexdigest`, e.g. `sha-256=9f86d0…`.
    pub checksum: Option<String>,
    /// Per-download proxy, overrides the global one.
    pub proxy: Option<String>,
    /// Bytes per second, 0/None = unlimited.
    pub speed_limit: Option<u64>,
    pub media: Option<MediaOptions>,
    /// Base64 encoded .torrent file content.
    pub torrent_data: Option<String>,
    /// Base64 encoded .metalink/.meta4 file content.
    pub metalink_data: Option<String>,
    /// aria2 `select-file` spec for torrents (1-based indexes).
    pub select_files: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub length: i64,
    pub completed: i64,
    pub selected: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct DownloadMeta {
    pub mime: Option<String>,
    /// Whether the server supports byte ranges (None = unknown).
    pub resumable: Option<bool>,
    pub thumbnail: Option<String>,
    pub media_title: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub info_hash: Option<String>,
    pub num_seeders: Option<i64>,
    pub files: Vec<FileEntry>,
    /// Human readable note from the smart connection controller.
    pub smart_note: Option<String>,
    pub verified: Option<String>,
    pub retries: u32,
    pub playlist_index: Option<String>,
    /// Server identity of the finished file ("size|last-modified|etag"),
    /// compared by synchronization queues.
    pub remote_stamp: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub id: String,
    pub engine: Engine,
    pub kind: Kind,
    pub url: String,
    pub mirrors: Vec<String>,
    pub name: String,
    pub dir: String,
    /// Absolute path of the (main) output file once known.
    pub file_path: Option<String>,
    pub status: Status,
    pub total: i64,
    pub done: i64,
    pub uploaded: i64,
    pub speed: i64,
    pub upload_speed: i64,
    /// Requested connections, 0 = Smart.
    pub connections: u32,
    /// Live connection (or peer) count.
    pub active_connections: u32,
    pub category: String,
    /// `None` = started directly; `Some(queue)` = managed by that queue.
    pub queue_id: Option<String>,
    pub position: i64,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub error: Option<String>,
    pub gid: Option<String>,
    pub source: String,
    pub options: DownloadOptions,
    pub meta: DownloadMeta,
}

impl Download {
    pub fn eta(&self) -> Option<i64> {
        (self.speed > 0 && self.total > 0 && self.total >= self.done)
            .then(|| (self.total - self.done) / self.speed)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AddRequest {
    pub url: String,
    pub mirrors: Vec<String>,
    pub dir: Option<String>,
    pub filename: Option<String>,
    /// 0 = Smart, None = use the default from settings.
    pub connections: Option<u32>,
    pub category: Option<String>,
    pub queue_id: Option<String>,
    /// Start right away (default true). Ignored when `queue_id` is set.
    pub start: Option<bool>,
    /// Force an engine instead of automatic selection.
    pub engine: Option<Engine>,
    pub options: DownloadOptions,
    pub size_hint: Option<i64>,
    /// ui / browser / cli / clipboard / api / batch
    pub source: Option<String>,
    /// Ask the user in the app before adding (browser interception).
    pub prompt: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ProbeInfo {
    pub url: String,
    pub final_url: String,
    pub filename: Option<String>,
    pub size: Option<i64>,
    pub mime: Option<String>,
    pub resumable: Option<bool>,
    pub status: Option<u16>,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
    /// "aria2" or "ytdlp"
    pub engine: Option<Engine>,
    pub kind: Option<Kind>,
    pub category: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct VideoQuality {
    pub height: u32,
    pub label: String,
    pub fps: Option<f64>,
    pub ext: String,
    pub vcodec: String,
    /// Estimated size including the best matching audio track.
    pub size: Option<i64>,
    pub hdr: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AudioQuality {
    pub bitrate: u32,
    pub ext: String,
    pub acodec: String,
    pub size: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct PlaylistEntry {
    pub index: u32,
    pub id: String,
    pub title: String,
    pub url: String,
    pub duration: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct MediaInfo {
    pub url: String,
    pub webpage_url: String,
    pub extractor: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub view_count: Option<i64>,
    pub upload_date: Option<String>,
    pub thumbnail: Option<String>,
    pub is_live: bool,
    pub is_playlist: bool,
    pub playlist_count: Option<u32>,
    pub entries: Vec<PlaylistEntry>,
    pub video: Vec<VideoQuality>,
    pub audio: Vec<AudioQuality>,
    pub subtitles: Vec<String>,
    pub auto_subtitles: Vec<String>,
    pub ffmpeg_available: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct MediaRequest {
    pub url: String,
    pub dir: Option<String>,
    pub media: MediaOptions,
    pub cookies: Vec<BrowserCookie>,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
    pub queue_id: Option<String>,
    pub title: Option<String>,
    pub thumbnail: Option<String>,
    pub size_hint: Option<i64>,
    pub source: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct GrabLink {
    pub url: String,
    pub text: Option<String>,
    pub kind: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct GrabRequest {
    pub page_url: Option<String>,
    pub links: Vec<GrabLink>,
    pub referer: Option<String>,
    pub cookies: Vec<BrowserCookie>,
    pub user_agent: Option<String>,
}

/// Settings the browser extension needs; fetched through the native host.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct BrowserConfig {
    pub intercept: bool,
    pub confirm: bool,
    pub min_size: u64,
    pub extensions: Vec<String>,
    pub skip_domains: Vec<String>,
    pub hover_button: bool,
    pub media_detection: bool,
    pub adapters: Vec<String>,
    pub app_version: String,
}

/// Contents of `api.json`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiInfo {
    pub port: u16,
    pub token: String,
    pub pid: u32,
    pub version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Stats {
    pub download_speed: i64,
    pub upload_speed: i64,
    pub active: u32,
    pub queued: u32,
    pub paused: u32,
    pub completed: u32,
    pub errors: u32,
    pub total: u32,
}

/// Generic API envelope for errors.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiError {
    pub error: String,
}
