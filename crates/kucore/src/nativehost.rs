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
    /// Detected browser this entry belongs to (see browsers::detect).
    pub browser_id: Option<String>,
    pub installed: bool,
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
    if !p.is_file() {
        return None;
    }
    // Browsers start the host by the path in the manifest, so it must outlive
    // this run and be reachable from outside a sandbox.
    #[cfg(target_os = "linux")]
    {
        if let Ok(id) = std::env::var("FLATPAK_ID") {
            return flatpak_wrapper(&id).ok().or(Some(p));
        }
        if std::env::var_os("APPIMAGE").is_some() {
            return stable_copy(&p).ok().or(Some(p));
        }
    }
    Some(p)
}

/// AppImage: its mount point changes every run; keep a copy of the host in
/// the data folder (refreshed when the bundled one differs).
#[cfg(target_os = "linux")]
fn stable_copy(src: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let dir = paths::data_dir().join("bin");
    std::fs::create_dir_all(&dir)?;
    let dst = dir.join("ku-native-host");
    let same = std::fs::metadata(&dst).ok().zip(std::fs::metadata(src).ok()).is_some_and(|(a, b)| a.len() == b.len() && a.modified().ok() >= b.modified().ok());
    if !same {
        let tmp = dir.join(".ku-native-host.new");
        std::fs::copy(src, &tmp)?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&tmp, &dst)?;
    }
    Ok(dst)
}

/// Flatpak: browsers run outside the sandbox, so the manifest points at a
/// script that starts the host inside it.
#[cfg(target_os = "linux")]
fn flatpak_wrapper(app_id: &str) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let dir = paths::data_dir().join("bin");
    std::fs::create_dir_all(&dir)?;
    let script = dir.join("ku-native-host.sh");
    let body = format!("#!/bin/sh\nexec flatpak run --command=ku-native-host {app_id} \"$@\"\n");
    if std::fs::read_to_string(&script).ok().as_deref() != Some(body.as_str()) {
        std::fs::write(&script, body)?;
    }
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
    Ok(script)
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

/// (name, key or dir, firefox?, detected browser id)
#[cfg(windows)]
type Target = (String, String, bool, Option<String>);

#[cfg(windows)]
const WIN_KEYS: &[(&str, &str, bool)] = &[
    ("Chrome", r"Software\Google\Chrome\NativeMessagingHosts", false),
    ("Chromium", r"Software\Chromium\NativeMessagingHosts", false),
    ("Edge", r"Software\Microsoft\Edge\NativeMessagingHosts", false),
    ("Brave", r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts", false),
    ("Vivaldi", r"Software\Vivaldi\NativeMessagingHosts", false),
    ("Firefox", r"Software\Mozilla\NativeMessagingHosts", true),
];

/// Every place a browser on this machine reads the host from: detected
/// browsers (Helium, Zen, Opera, …) first, then the well-known keys so a
/// browser installed later works too.
#[cfg(windows)]
fn win_targets() -> Vec<Target> {
    let mut out: Vec<Target> = Vec::new();
    let mut add = |name: String, key: String, ff: bool, id: Option<String>| {
        if let Some(t) = out.iter_mut().find(|t| t.1.eq_ignore_ascii_case(&key)) {
            // Several browsers can share a key (Firefox forks): keep the first name.
            if t.3.is_none() && id.is_some() {
                t.3 = id;
                t.0 = name;
            }
        } else {
            out.push((name, key, ff, id));
        }
    };
    for b in crate::browsers::detect() {
        if let Some(k) = b.host_location.clone() {
            add(b.name.clone(), k, b.family == "firefox", Some(b.id.clone()));
        }
    }
    for (n, k, ff) in WIN_KEYS {
        add(n.to_string(), k.to_string(), *ff, None);
    }
    out
}

#[cfg(windows)]
pub fn register(extra_ids: &[String]) -> Result<HostStatus> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let host = host_executable().context("ku-native-host executable not found next to KuDownloader")?;
    let (chrome, firefox) = write_manifests(&host, extra_ids)?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for (_, base, is_ff, _) in win_targets() {
        let (key, _) = hkcu.create_subkey(format!(r"{base}\{NATIVE_HOST_NAME}"))?;
        let manifest = if is_ff { &firefox } else { &chrome };
        key.set_value("", &manifest.to_string_lossy().to_string())?;
    }
    Ok(status())
}

#[cfg(windows)]
pub fn unregister() -> Result<HostStatus> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for (_, base, _, _) in win_targets() {
        let _ = hkcu.delete_subkey_all(format!(r"{base}\{NATIVE_HOST_NAME}"));
    }
    Ok(status())
}

#[cfg(windows)]
pub fn status() -> HostStatus {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let browsers = win_targets()
        .into_iter()
        .map(|(name, base, _, id)| {
            let key = format!(r"{base}\{NATIVE_HOST_NAME}");
            let manifest: Option<String> = hkcu.open_subkey(&key).ok().and_then(|k| k.get_value("").ok());
            BrowserRegistration {
                browser: name,
                registered: manifest.as_deref().is_some_and(|m| Path::new(m).is_file()),
                location: format!(r"HKCU\{key}"),
                installed: id.is_some(),
                browser_id: id,
            }
        })
        .collect();
    base_status(browsers)
}

#[cfg(not(windows))]
fn unix_dirs() -> Vec<(&'static str, PathBuf, bool)> {
    let mut v = unix_base_dirs();
    #[cfg(not(target_os = "macos"))]
    {
        let home = dirs::home_dir().unwrap_or_default();
        for (n, d) in [("LibreWolf", ".librewolf"), ("Zen", ".zen"), ("Floorp", ".floorp"), ("Waterfox", ".waterfox")] {
            v.push((n, home.join(d).join("native-messaging-hosts"), true));
        }
    }
    v
}

#[cfg(not(windows))]
fn unix_base_dirs() -> Vec<(&'static str, PathBuf, bool)> {
    let home = dirs::home_dir().unwrap_or_default();
    #[cfg(target_os = "macos")]
    let (cfg, ff) = (home.join("Library/Application Support"), home.join("Library/Application Support/Mozilla/NativeMessagingHosts"));
    #[cfg(not(target_os = "macos"))]
    let (cfg, ff) = (dirs::config_dir().unwrap_or_else(|| home.join(".config")), home.join(".mozilla/native-messaging-hosts"));
    #[cfg(target_os = "macos")]
    let chrome = [("Chrome", "Google/Chrome"), ("Chromium", "Chromium"), ("Edge", "Microsoft Edge"), ("Brave", "BraveSoftware/Brave-Browser"), ("Vivaldi", "Vivaldi")];
    #[cfg(not(target_os = "macos"))]
    let chrome = [("Chrome", "google-chrome"), ("Chromium", "chromium"), ("Edge", "microsoft-edge"), ("Brave", "BraveSoftware/Brave-Browser"), ("Vivaldi", "vivaldi"), ("Opera", "opera"), ("Thorium", "thorium"), ("Helium", "net.imput.helium")];
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
    let detected = crate::browsers::detect();
    let browsers = unix_dirs()
        .into_iter()
        .map(|(name, dir, _)| {
            let f = dir.join(format!("{NATIVE_HOST_NAME}.json"));
            let installed = dir.parent().is_some_and(|p| p.exists());
            // "Chrome" ↔ detected "Google Chrome", "Firefox" ↔ "Mozilla Firefox".
            let browser_id = detected.iter().find(|b| b.name.contains(name)).map(|b| b.id.clone());
            BrowserRegistration { browser: name.into(), registered: f.is_file(), location: f.display().to_string(), browser_id, installed }
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
