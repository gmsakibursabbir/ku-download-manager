//! KuCore — the KuDownloader engine.
//!
//! ```text
//! UI / CLI / browser ──▶ KuCore ──▶ aria2 (HTTP, FTP, SFTP, BitTorrent, Metalink)
//!                               └─▶ yt-dlp (media sites, HLS/DASH)
//! ```

pub mod api;
pub mod aria2;
pub mod browsers;
pub mod classify;
pub mod core;
pub mod db;
pub mod grab;
pub mod hash;
pub mod kuhttp;
pub mod landing;
pub mod nativehost;
pub mod power;
pub mod probe;
pub mod scan;
pub mod settings;
pub mod smart;
pub mod tools;
pub mod torrent;
pub mod types;
pub mod ytdlp;

pub use crate::core::Core;
pub use ku_proto;
pub use settings::Settings;
pub use types::*;
