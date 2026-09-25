//! Detect installed web browsers so the UI can offer extension setup.
//!
//! Windows: every browser that registers itself with the system
//! (`Clients\StartMenuInternet`: Chrome, Edge, Brave, Helium, Zen, Floorp, …)
//! plus well-known install folders. The engine family is read from the
//! install layout, so unlisted Chromium/Firefox forks work too.
//! Linux/macOS: known executables on PATH.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Browser {
    /// Stable slug of the display name ("google-chrome", "helium", "zen").
    pub id: String,
    pub name: String,
    /// "chromium" or "firefox"
    pub family: String,
    pub path: String,
    /// Page listing extensions (for manual install).
    pub extensions_url: String,
    /// Where this browser looks for the native messaging host (Windows:
    /// registry path under HKCU; elsewhere a directory).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_location: Option<String>,
}

fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty() && *p != "browser").collect::<Vec<_>>().join("-")
}

fn extensions_url(family: &str, exe: &Path) -> String {
    if family == "firefox" {
        return "about:addons".into();
    }
    let stem = exe.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    match stem.as_str() {
        "msedge" | "microsoft-edge" | "microsoft-edge-stable" => "edge://extensions/",
        "brave" | "brave-browser" => "brave://extensions/",
        "vivaldi" | "vivaldi-stable" => "vivaldi://extensions/",
        "opera" | "launcher" => "opera://extensions/",
        _ => "chrome://extensions/",
    }
    .into()
}

/// "chromium" / "firefox" from the files next to the executable.
#[cfg_attr(not(windows), allow(dead_code))]
fn family_of(exe: &Path) -> Option<&'static str> {
    let dir = exe.parent()?;
    if dir.join("omni.ja").is_file() || dir.join("application.ini").is_file() || dir.join("xul.dll").is_file() {
        return Some("firefox");
    }
    let chromium_marker = |d: &Path| ["chrome.dll", "msedge.dll", "opera_browser.dll", "resources.pak", "chrome_elf.dll"].iter().any(|f| d.join(f).is_file());
    if chromium_marker(dir) || dir.join("chrome_proxy.exe").is_file() {
        return Some("chromium");
    }
    // Chromium keeps its payload in a version-numbered folder beside the exe.
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(|c: char| c.is_ascii_digit())) && chromium_marker(&p) {
            return Some("chromium");
        }
    }
    None
}

/// Chromium builds read the host from `HKCU\Software\<Vendor>\<Product>\NativeMessagingHosts`,
/// where Vendor\Product is the install folder (`…\imput\Helium\Application\chrome.exe`).
#[cfg(windows)]
fn chromium_host_key(exe: &Path) -> String {
    let parts: Vec<String> = exe.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    if let Some(i) = parts.iter().rposition(|p| p.eq_ignore_ascii_case("Application")) {
        let roots = ["program files", "program files (x86)", "local", "programs"];
        let start = parts[..i].iter().rposition(|p| roots.contains(&p.to_lowercase().as_str())).map(|r| r + 1).unwrap_or(i.saturating_sub(1));
        if start < i {
            return format!(r"Software\{}\NativeMessagingHosts", parts[start..i].join(r"\"));
        }
    }
    // Opera and other layouts without "Application" use Chrome's key.
    r"Software\Google\Chrome\NativeMessagingHosts".into()
}

#[cfg(windows)]
fn firefox_host_key(_exe: &Path) -> String {
    // Firefox and its forks (Zen, Floorp, Waterfox, LibreWolf) read Mozilla's key.
    r"Software\Mozilla\NativeMessagingHosts".into()
}

#[cfg(windows)]
fn registered() -> Vec<(String, PathBuf)> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY};
    use winreg::RegKey;
    let mut out = Vec::new();
    let sources = [(HKEY_CURRENT_USER, KEY_READ), (HKEY_LOCAL_MACHINE, KEY_READ), (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_32KEY)];
    for (hive, flags) in sources {
        let Ok(root) = RegKey::predef(hive).open_subkey_with_flags(r"SOFTWARE\Clients\StartMenuInternet", flags) else { continue };
        for sub in root.enum_keys().flatten() {
            let Ok(k) = root.open_subkey_with_flags(&sub, flags) else { continue };
            let name: String = k.get_value("").unwrap_or_else(|_| sub.clone());
            let cmd: Option<String> = k.open_subkey_with_flags(r"shell\open\command", flags).ok().and_then(|c| c.get_value("").ok());
            let Some(cmd) = cmd else { continue };
            // `"C:\…\browser.exe" --args` or an unquoted path.
            let exe = if let Some(rest) = cmd.trim().strip_prefix('"') { rest.split('"').next().unwrap_or("").to_string() } else { cmd.split(" -").next().unwrap_or("").trim().to_string() };
            out.push((name, PathBuf::from(exe)));
        }
    }
    out
}

#[cfg(windows)]
const FALLBACK: &[(&str, &str)] = &[
    ("Google Chrome", r"Google\Chrome\Application\chrome.exe"),
    ("Microsoft Edge", r"Microsoft\Edge\Application\msedge.exe"),
    ("Brave", r"BraveSoftware\Brave-Browser\Application\brave.exe"),
    ("Vivaldi", r"Vivaldi\Application\vivaldi.exe"),
    ("Opera", r"Programs\Opera\opera.exe"),
    ("Opera GX", r"Programs\Opera GX\opera.exe"),
    ("Chromium", r"Chromium\Application\chrome.exe"),
    ("Helium", r"imput\Helium\Application\chrome.exe"),
    ("Thorium", r"Thorium\Application\thorium.exe"),
    ("Mozilla Firefox", r"Mozilla Firefox\firefox.exe"),
    ("Zen", r"Zen Browser\zen.exe"),
    ("Floorp", r"Ablaze Floorp\floorp.exe"),
    ("LibreWolf", r"LibreWolf\librewolf.exe"),
    ("Waterfox", r"Waterfox\waterfox.exe"),
];

#[cfg(windows)]
pub fn detect() -> Vec<Browser> {
    let mut candidates = registered();
    let roots = [std::env::var_os("ProgramFiles"), std::env::var_os("ProgramFiles(x86)"), std::env::var_os("LOCALAPPDATA")];
    for root in roots.into_iter().flatten() {
        for (name, rel) in FALLBACK {
            candidates.push((name.to_string(), PathBuf::from(&root).join(rel)));
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (name, exe) in candidates {
        if !exe.is_file() || !seen.insert(exe.to_string_lossy().to_lowercase()) {
            continue;
        }
        let Some(family) = family_of(&exe) else { continue }; // Internet Explorer etc.
        let host_location = Some(if family == "firefox" { firefox_host_key(&exe) } else { chromium_host_key(&exe) });
        out.push(Browser { id: slug(&name), name, family: family.into(), extensions_url: extensions_url(family, &exe), path: exe.display().to_string(), host_location });
    }
    // The same slug twice (e.g. two Firefox channels): keep ids unique.
    let mut ids = std::collections::HashMap::<String, usize>::new();
    for b in &mut out {
        let n = ids.entry(b.id.clone()).or_default();
        *n += 1;
        if *n > 1 {
            b.id = format!("{}-{n}", b.id);
        }
    }
    out
}

#[cfg(not(windows))]
const UNIX: &[(&str, &str, &[&str])] = &[
    ("Google Chrome", "chromium", &["google-chrome", "google-chrome-stable"]),
    ("Microsoft Edge", "chromium", &["microsoft-edge", "microsoft-edge-stable"]),
    ("Brave", "chromium", &["brave-browser", "brave"]),
    ("Vivaldi", "chromium", &["vivaldi", "vivaldi-stable"]),
    ("Opera", "chromium", &["opera"]),
    ("Chromium", "chromium", &["chromium", "chromium-browser"]),
    ("Helium", "chromium", &["helium", "helium-browser"]),
    ("Thorium", "chromium", &["thorium-browser", "thorium"]),
    ("Mozilla Firefox", "firefox", &["firefox", "firefox-esr"]),
    ("Zen", "firefox", &["zen", "zen-browser"]),
    ("Floorp", "firefox", &["floorp"]),
    ("LibreWolf", "firefox", &["librewolf"]),
    ("Waterfox", "firefox", &["waterfox"]),
];

#[cfg(not(windows))]
pub fn detect() -> Vec<Browser> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    let mut out = Vec::new();
    for (name, family, bins) in UNIX {
        let found = bins.iter().flat_map(|b| dirs.iter().map(move |d| d.join(b))).find(|p| p.is_file());
        if let Some(p) = found {
            out.push(Browser { id: slug(name), name: name.to_string(), family: family.to_string(), extensions_url: extensions_url(family, &p), path: p.display().to_string(), host_location: None });
        }
    }
    out
}

pub fn by_id(id: &str) -> Option<Browser> {
    detect().into_iter().find(|b| b.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Google Chrome"), "google-chrome");
        assert_eq!(slug("Zen Browser"), "zen");
        assert_eq!(slug("Mozilla Firefox"), "mozilla-firefox");
    }

    #[cfg(windows)]
    #[test]
    fn host_keys_follow_install_folder() {
        let k = |p: &str| chromium_host_key(Path::new(p));
        assert_eq!(k(r"C:\Users\a\AppData\Local\imput\Helium\Application\chrome.exe"), r"Software\imput\Helium\NativeMessagingHosts");
        assert_eq!(k(r"C:\Program Files\Google\Chrome\Application\chrome.exe"), r"Software\Google\Chrome\NativeMessagingHosts");
        assert_eq!(k(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"), r"Software\Microsoft\Edge\NativeMessagingHosts");
        assert_eq!(k(r"C:\Users\a\AppData\Local\BraveSoftware\Brave-Browser\Application\brave.exe"), r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts");
        assert_eq!(k(r"C:\Users\a\AppData\Local\Vivaldi\Application\vivaldi.exe"), r"Software\Vivaldi\NativeMessagingHosts");
        assert_eq!(k(r"C:\Users\a\AppData\Local\Programs\Opera\opera.exe"), r"Software\Google\Chrome\NativeMessagingHosts");
    }
}

#[cfg(test)]
mod probe {
    /// `cargo test -p kucore --lib browsers::probe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn list_installed() {
        for b in super::detect() {
            println!("{:<18} {:<9} {:<22} {:<55} {}", b.id, b.family, b.extensions_url, b.path, b.host_location.unwrap_or_default());
        }
    }
}

/// Where a Chromium browser on Windows looks for extensions that other
/// programs offer (Google's "external extensions": the browser asks the user
/// to enable them on its next start). `None` for browsers without it.
#[cfg(windows)]
fn external_extensions_key(b: &Browser) -> Option<(&'static str, &'static str)> {
    let stem = Path::new(&b.path).file_stem()?.to_string_lossy().to_lowercase();
    const WEB_STORE: &str = "https://clients2.google.com/service/update2/crx";
    match stem.as_str() {
        "chrome" => Some((r"Software\Google\Chrome\Extensions", WEB_STORE)),
        "brave" => Some((r"Software\BraveSoftware\Brave-Browser\Extensions", WEB_STORE)),
        "msedge" => Some((r"Software\Microsoft\Edge\Extensions", "https://edge.microsoft.com/extensionwebstorebase/v1/crx")),
        _ => None,
    }
}

/// Offer the store-published extension to a Chromium browser (Windows):
/// written under the current user, the browser shows "New extension added"
/// and the user enables it. Returns whether the browser supports it.
#[cfg(windows)]
pub fn offer_store_extension(b: &Browser, id: &str) -> std::io::Result<bool> {
    use winreg::enums::HKEY_CURRENT_USER;
    if id.len() != 32 || !id.bytes().all(|c| (b'a'..=b'p').contains(&c)) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a store extension id"));
    }
    let Some((root, update_url)) = external_extensions_key(b) else { return Ok(false) };
    let (key, _) = winreg::RegKey::predef(HKEY_CURRENT_USER).create_subkey(format!(r"{root}\{id}"))?;
    key.set_value("update_url", &update_url)?;
    Ok(true)
}

#[cfg(not(windows))]
pub fn offer_store_extension(_b: &Browser, _id: &str) -> std::io::Result<bool> {
    Ok(false)
}

/// The store page for this browser's family, when the extension is listed there.
pub fn store_page(b: &Browser, chrome: &str, edge: &str, firefox: &str) -> Option<(String, String)> {
    let stem = Path::new(&b.path).file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    if b.family == "firefox" {
        return (!firefox.is_empty()).then(|| (String::new(), format!("https://addons.mozilla.org/firefox/addon/{firefox}/")));
    }
    if stem.contains("msedge") || stem.contains("microsoft-edge") {
        if !edge.is_empty() {
            return Some((edge.to_string(), format!("https://microsoftedge.microsoft.com/addons/detail/{edge}")));
        }
    }
    (!chrome.is_empty()).then(|| (chrome.to_string(), format!("https://chromewebstore.google.com/detail/{chrome}")))
}
