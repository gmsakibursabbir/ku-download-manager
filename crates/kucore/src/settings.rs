use ku_proto::{paths, BrowserConfig};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    /// Sub-folder of the download directory, or an absolute path.
    pub folder: String,
    pub extensions: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct BandwidthProfile {
    pub id: String,
    pub name: String,
    /// Bytes per second, 0 = unlimited.
    pub download: u64,
    pub upload: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    // Appearance
    pub theme: String,
    /// Accent colour: "blue" (default), "violet", "teal", "green", "orange", "pink", "red", "graphite".
    pub accent: String,
    /// Dark palette: "default", "midnight", "black" (OLED).
    pub dark_palette: String,
    /// Interface language: "system", "en", "bn".
    pub language: String,
    pub compact: bool,
    /// Mica / Acrylic window background where the OS supports it.
    pub translucent: bool,
    // General
    pub start_with_os: bool,
    pub minimize_to_tray: bool,
    pub clipboard_monitor: bool,
    pub check_updates: bool,
    // Notifications
    pub notify_complete: bool,
    pub notify_error: bool,
    pub notify_queue_done: bool,
    /// IDM-style progress window when a single download is started.
    pub show_progress_window: bool,
    /// The first-run guide (media tools, browser extension) was completed or skipped.
    pub onboarded: bool,
    // Downloads
    pub download_dir: String,
    pub use_categories: bool,
    pub categories: Vec<Category>,
    pub max_concurrent: u32,
    /// 0 = Smart.
    pub default_connections: u32,
    /// rename / overwrite / skip
    pub file_exists: String,
    /// Scan finished downloads: "off" | "defender" | "custom".
    pub virus_scan: String,
    /// yt-dlp: a Netscape cookies.txt file to use for media sites.
    pub cookies_file: String,
    /// yt-dlp: read cookies from this browser's store ("" = off).
    pub cookies_from_browser: String,
    pub virus_scanner: String,
    /// Arguments for a custom scanner; `{file}` is the downloaded file.
    pub virus_scanner_args: String,
    // Connection
    pub proxy: String,
    pub proxy_user: String,
    pub proxy_pass: String,
    pub no_proxy: String,
    pub user_agent: String,
    pub max_tries: u32,
    pub retry_wait: u32,
    pub timeout: u32,
    pub connect_timeout: u32,
    /// KuCore-level retries after the engine gives up on a transient error.
    pub auto_retry: u32,
    pub check_certificate: bool,
    // Speed
    pub active_profile: String,
    pub profiles: Vec<BandwidthProfile>,
    // Browser
    pub intercept_downloads: bool,
    pub confirm_browser_downloads: bool,
    pub intercept_min_size: u64,
    pub intercept_extensions: Vec<String>,
    pub skip_domains: Vec<String>,
    pub extra_extension_ids: Vec<String>,
    // KuAirSend (local transfers between KuDownloader devices)
    /// Off by default: no network port is opened until the user turns it on.
    pub airsend_enabled: bool,
    /// Name shown to nearby devices; empty = the computer name.
    pub airsend_name: String,
    /// 8-bit animal shown to nearby devices; empty = picked from the device id.
    pub airsend_avatar: String,
    /// Where received files go; empty = <download folder>/KuAirSend.
    pub airsend_folder: String,
    /// Accept from any nearby KuDownloader without asking.
    pub airsend_auto_accept: bool,
    /// Senders must enter this PIN (empty = no PIN).
    pub airsend_pin: String,
    /// Device fingerprints accepted without asking.
    pub airsend_trusted: Vec<String>,
    // Media
    pub hover_button: bool,
    pub media_detection: bool,
    pub adapters: Vec<String>,
    pub video_dir: String,
    pub video_height: u32,
    pub video_container: String,
    pub audio_format: String,
    pub audio_bitrate: u32,
    pub subtitles: bool,
    pub sub_langs: String,
    pub embed_thumbnail: bool,
    pub ytdlp_path: String,
    pub ffmpeg_path: String,
    // Torrent
    pub seed_ratio: f64,
    pub seed_time: u32,
    pub bt_listen_port: String,
    pub enable_dht: bool,
    pub bt_max_peers: u32,
    // Advanced
    pub aria2_path: String,
    pub api_enabled: bool,
    /// Update feed URL; empty = the feed built into the app.
    pub update_endpoint: String,
    /// "aria2" (default) or "kuhttp" (experimental native engine).
    pub http_engine: String,
}

pub fn default_categories() -> Vec<Category> {
    let c = |id: &str, name: &str, folder: &str, exts: &[&str]| Category {
        id: id.into(),
        name: name.into(),
        folder: folder.into(),
        extensions: exts.iter().map(|e| e.to_string()).collect(),
    };
    vec![
        c("archives", "Archives", "Archives", &["zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "cab", "lz", "lzma"]),
        c("images-disk", "Disk images", "ISOs", &["iso", "img", "dmg", "vhd", "vhdx", "vmdk", "qcow2"]),
        c("programs", "Programs", "Programs", &["exe", "msi", "msix", "appx", "deb", "rpm", "appimage", "apk", "pkg", "flatpak", "snap"]),
        c("video", "Video", "Video", &["mp4", "mkv", "webm", "avi", "mov", "wmv", "flv", "m4v", "mpg", "mpeg", "ts", "3gp"]),
        c("music", "Music", "Music", &["mp3", "m4a", "flac", "wav", "ogg", "opus", "aac", "wma", "alac"]),
        c("documents", "Documents", "Documents", &["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "epub", "txt", "csv", "rtf", "md"]),
        c("images", "Images", "Images", &["jpg", "jpeg", "png", "gif", "webp", "svg", "bmp", "tiff", "heic", "avif", "psd"]),
        c("torrents", "Torrents", "Torrents", &[]),
    ]
}

fn default_profiles() -> Vec<BandwidthProfile> {
    let mb = 1024 * 1024;
    vec![
        BandwidthProfile { id: "unlimited".into(), name: "Unlimited".into(), download: 0, upload: 0 },
        BandwidthProfile { id: "balanced".into(), name: "Balanced".into(), download: 10 * mb, upload: mb },
        BandwidthProfile { id: "background".into(), name: "Background".into(), download: 2 * mb, upload: 256 * 1024 },
    ]
}

impl Default for Settings {
    fn default() -> Self {
        let dl = paths::default_download_dir();
        Settings {
            theme: "system".into(),
            accent: "blue".into(),
            dark_palette: "default".into(),
            language: "system".into(),
            compact: false,
            translucent: false,
            start_with_os: false,
            minimize_to_tray: true,
            clipboard_monitor: true,
            check_updates: true,
            notify_complete: true,
            show_progress_window: true,
            onboarded: false,
            notify_error: true,
            notify_queue_done: true,
            download_dir: dl.to_string_lossy().into_owned(),
            use_categories: true,
            categories: default_categories(),
            max_concurrent: 4,
            default_connections: 0,
            file_exists: "rename".into(),
            virus_scan: "off".into(),
            cookies_file: String::new(),
            cookies_from_browser: String::new(),
            virus_scanner: String::new(),
            virus_scanner_args: "\"{file}\"".into(),
            proxy: String::new(),
            proxy_user: String::new(),
            proxy_pass: String::new(),
            no_proxy: String::new(),
            user_agent: String::new(),
            max_tries: 5,
            retry_wait: 5,
            timeout: 60,
            connect_timeout: 30,
            auto_retry: 3,
            check_certificate: true,
            active_profile: "unlimited".into(),
            profiles: default_profiles(),
            intercept_downloads: true,
            confirm_browser_downloads: true,
            intercept_min_size: 0,
            intercept_extensions: Vec::new(),
            skip_domains: Vec::new(),
            extra_extension_ids: Vec::new(),
            airsend_enabled: false,
            airsend_name: String::new(),
            airsend_avatar: String::new(),
            airsend_folder: String::new(),
            airsend_auto_accept: false,
            airsend_pin: String::new(),
            airsend_trusted: Vec::new(),
            hover_button: true,
            media_detection: true,
            adapters: vec!["youtube".into(), "vimeo".into(), "dailymotion".into(), "generic".into()],
            video_dir: dl.join("Video").to_string_lossy().into_owned(),
            video_height: 1080,
            video_container: "mp4".into(),
            // M4A is copied out of YouTube's stream; MP3 has to be re-encoded (slow).
            audio_format: "m4a".into(),
            audio_bitrate: 320,
            subtitles: false,
            sub_langs: "en".into(),
            embed_thumbnail: false,
            ytdlp_path: String::new(),
            ffmpeg_path: String::new(),
            seed_ratio: 0.0,
            seed_time: 0,
            bt_listen_port: "6881-6999".into(),
            enable_dht: true,
            bt_max_peers: 55,
            aria2_path: String::new(),
            api_enabled: true,
            update_endpoint: String::new(),
            // Default since v0.2 (see docs/kuhttp.md); existing settings are kept.
            http_engine: "kuhttp".into(),
        }
    }
}

impl Settings {
    /// Clamp values that would otherwise produce invalid engine options.
    pub fn sanitize(&mut self) {
        self.max_concurrent = self.max_concurrent.clamp(1, 32);
        self.default_connections = self.default_connections.min(32);
        self.max_tries = self.max_tries.min(100);
        self.timeout = self.timeout.clamp(5, 600);
        self.connect_timeout = self.connect_timeout.clamp(5, 300);
        self.retry_wait = self.retry_wait.min(600);
        self.auto_retry = self.auto_retry.min(20);
        self.bt_max_peers = self.bt_max_peers.clamp(1, 1000);
        if !matches!(self.file_exists.as_str(), "rename" | "overwrite" | "skip") {
            self.file_exists = "rename".into();
        }
        if !matches!(self.virus_scan.as_str(), "off" | "defender" | "custom") {
            self.virus_scan = "off".into();
        }
        if !matches!(self.http_engine.as_str(), "aria2" | "kuhttp") {
            self.http_engine = "aria2".into();
        }
        if !matches!(self.theme.as_str(), "light" | "dark" | "system") {
            self.theme = "system".into();
        }
        if self.download_dir.trim().is_empty() {
            self.download_dir = paths::default_download_dir().to_string_lossy().into_owned();
        }
        if self.profiles.is_empty() {
            self.profiles = default_profiles();
        }
        if !self.profiles.iter().any(|p| p.id == self.active_profile) {
            self.active_profile = self.profiles[0].id.clone();
        }
        for list in [&mut self.intercept_extensions, &mut self.skip_domains] {
            for s in list.iter_mut() {
                *s = s.trim().trim_start_matches('.').to_ascii_lowercase();
            }
            list.retain(|s| !s.is_empty());
        }
    }

    pub fn profile(&self) -> BandwidthProfile {
        self.profiles
            .iter()
            .find(|p| p.id == self.active_profile)
            .cloned()
            .unwrap_or_else(|| default_profiles().remove(0))
    }

    pub fn browser_config(&self) -> BrowserConfig {
        BrowserConfig {
            intercept: self.intercept_downloads,
            confirm: self.confirm_browser_downloads,
            min_size: self.intercept_min_size,
            extensions: self.intercept_extensions.clone(),
            skip_domains: self.skip_domains.clone(),
            hover_button: self.hover_button,
            media_detection: self.media_detection,
            adapters: self.adapters.clone(),
            app_version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}
