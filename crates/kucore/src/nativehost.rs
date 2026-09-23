//! Registration of the native messaging host with installed browsers.
//!
//! Only per-user locations are touched (HKCU on Windows, the home directory on
//! Linux/macOS); no administrator rights are needed.

use anyhow::{Context, Result};
use ku_proto::{paths, CHROME_EXTENSION_ID, FIREFOX_EXTENSION_ID, NATIVE_HOST_NAME};
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRegistration {
    pub browser: String,
    pub registered: bool,
    pub location: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HostStatus {
    pub host_path: Option<String>,
    pub host_exists: bool,
    pub chrome_extension_id: String,
    pub firefox_extension_id: String,
    pub browsers: Vec<BrowserRegistration>,
}

pub fn host_executable() -> Option<PathBuf> {
    let p = paths::current_exe_dir()?.join(paths::exe_name("ku-native-host"));
    p.is_file().then_some(p)
}

fn manifest_dir() -> PathBuf {
    paths::data_dir().join("native-messaging")
}

fn chrome_origins(extra: &[String]) -> Vec<String> {
    std::iter::once(CHROME_EXTENSION_ID.trim().to_string())
        .chain(extra.iter().map(|s| s.trim().to_string()))
        .filter(|id| id.len() == 32 && id.chars().all(|c| ('a'..='p').contains(&c)))
        .map(|id| format!("chrome-extension://{id}/"))
        .collect()
}

fn write_manifests(host: &Path, extra_ids: &[String]) -> Result<(PathBuf, PathBuf)> {
    let dir = manifest_dir();
    std::fs::create_dir_all(&dir)?;
    let chrome = dir.join("chrome.json");
    let firefox = dir.join("firefox.json");
    let path = host.to_string_lossy();
    let common = |extra: serde_json::Value| {
        let mut v = json!({
            "name": NATIVE_HOST_NAME,
            "description": "KuDownloader browser integration",
            "path": path,
            "type": "stdio",
        });
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        serde_json::to_string_pretty(&v).unwrap()
    };
    std::fs::write(&chrome, common(json!({ "allowed_origins": chrome_origins(extra_ids) })))?;
    std::fs::write(&firefox, common(json!({ "allowed_extensions": [FIREFOX_EXTENSION_ID] })))?;
    Ok((chrome, firefox))
}

#[cfg(windows)]
const WIN_KEYS: &[(&str, &str, bool)] = &[
    ("Chrome", r"Software\Google\Chrome\NativeMessagingHosts", false),
    ("Chromium", r"Software\Chromium\NativeMessagingHosts", false),
    ("Edge", r"Software\Microsoft\Edge\NativeMessagingHosts", false),
    ("Brave", r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts", false),
    ("Vivaldi", r"Software\Vivaldi\NativeMessagingHosts", false),
    ("Firefox", r"Software\Mozilla\NativeMessagingHosts", true),
];

#[cfg(windows)]
pub fn register(extra_ids: &[String]) -> Result<HostStatus> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let host = host_executable().context("ku-native-host executable not found next to KuDownloader")?;
    let (chrome, firefox) = write_manifests(&host, extra_ids)?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for (_, base, is_ff) in WIN_KEYS {
        let (key, _) = hkcu.create_subkey(format!(r"{base}\{NATIVE_HOST_NAME}"))?;
        let manifest = if *is_ff { &firefox } else { &chrome };
        key.set_value("", &manifest.to_string_lossy().to_string())?;
    }
    Ok(status())
}

#[cfg(windows)]
pub fn unregister() -> Result<HostStatus> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for (_, base, _) in WIN_KEYS {
        let _ = hkcu.delete_subkey_all(format!(r"{base}\{NATIVE_HOST_NAME}"));
    }
    Ok(status())
}

#[cfg(windows)]
pub fn status() -> HostStatus {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let browsers = WIN_KEYS
        .iter()
        .map(|(name, base, _)| {
            let key = format!(r"{base}\{NATIVE_HOST_NAME}");
            let manifest: Option<String> = hkcu.open_subkey(&key).ok().and_then(|k| k.get_value("").ok());
            BrowserRegistration {
                browser: name.to_string(),
                registered: manifest.as_deref().is_some_and(|m| Path::new(m).is_file()),
                location: format!(r"HKCU\{key}"),
            }
        })
        .collect();
    base_status(browsers)
}

#[cfg(not(windows))]
fn unix_dirs() -> Vec<(&'static str, PathBuf, bool)> {
    let home = dirs::home_dir().unwrap_or_default();
    #[cfg(target_os = "macos")]
    let (cfg, ff) = (home.join("Library/Application Support"), home.join("Library/Application Support/Mozilla/NativeMessagingHosts"));
    #[cfg(not(target_os = "macos"))]
    let (cfg, ff) = (dirs::config_dir().unwrap_or_else(|| home.join(".config")), home.join(".mozilla/native-messaging-hosts"));
    #[cfg(target_os = "macos")]
    let chrome = [("Chrome", "Google/Chrome"), ("Chromium", "Chromium"), ("Edge", "Microsoft Edge"), ("Brave", "BraveSoftware/Brave-Browser"), ("Vivaldi", "Vivaldi")];
    #[cfg(not(target_os = "macos"))]
    let chrome = [("Chrome", "google-chrome"), ("Chromium", "chromium"), ("Edge", "microsoft-edge"), ("Brave", "BraveSoftware/Brave-Browser"), ("Vivaldi", "vivaldi")];
    let mut v: Vec<_> = chrome.iter().map(|(n, d)| (*n, cfg.join(d).join("NativeMessagingHosts"), false)).collect();
    v.push(("Firefox", ff, true));
    v
}

#[cfg(not(windows))]
pub fn register(extra_ids: &[String]) -> Result<HostStatus> {
    let host = host_executable().context("ku-native-host executable not found next to KuDownloader")?;
    let (chrome, firefox) = write_manifests(&host, extra_ids)?;
    for (_, dir, is_ff) in unix_dirs() {
        // Only register for browsers that are installed (their profile root exists).
        if dir.parent().is_some_and(|p| p.exists()) {
            std::fs::create_dir_all(&dir)?;
            std::fs::copy(if is_ff { &firefox } else { &chrome }, dir.join(format!("{NATIVE_HOST_NAME}.json")))?;
        }
    }
    Ok(status())
}

#[cfg(not(windows))]
pub fn unregister() -> Result<HostStatus> {
    for (_, dir, _) in unix_dirs() {
        let _ = std::fs::remove_file(dir.join(format!("{NATIVE_HOST_NAME}.json")));
    }
    Ok(status())
}

#[cfg(not(windows))]
pub fn status() -> HostStatus {
    let browsers = unix_dirs()
        .into_iter()
        .map(|(name, dir, _)| {
            let f = dir.join(format!("{NATIVE_HOST_NAME}.json"));
            BrowserRegistration { browser: name.into(), registered: f.is_file(), location: f.display().to_string() }
        })
        .collect();
    base_status(browsers)
}

fn base_status(browsers: Vec<BrowserRegistration>) -> HostStatus {
    let host = host_executable();
    HostStatus {
        host_exists: host.is_some(),
        host_path: host.map(|h| h.display().to_string()),
        chrome_extension_id: CHROME_EXTENSION_ID.trim().to_string(),
        firefox_extension_id: FIREFOX_EXTENSION_ID.into(),
        browsers,
    }
}
