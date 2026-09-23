//! Types and helpers shared by every KuDownloader process: the desktop app
//! (which hosts KuCore), the `ku` CLI and the browser native messaging host.
//!
//! This crate deliberately has no async runtime and no HTTP library so the
//! CLI and the native host stay tiny and start instantly.

pub mod client;
pub mod launcher;
pub mod model;
pub mod paths;

pub use model::*;

/// Native messaging host name registered with browsers.
pub const NATIVE_HOST_NAME: &str = "com.kuduy.kudownloader";
/// Firefox add-on id of the KuDownloader extension.
pub const FIREFOX_EXTENSION_ID: &str = "kudownloader@kuduy.digital";
/// Chromium extension id derived from the public key in `extension/manifest.chrome.json`.
pub const CHROME_EXTENSION_ID: &str = include_str!("../../../extension/chrome-extension-id.txt");
/// Version of the local API protocol.
pub const API_VERSION: u32 = 1;
