use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "KuDownloader";

/// Per-user data directory. `KU_DATA_DIR` overrides it (tests, portable mode).
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("KU_DATA_DIR") {
        return PathBuf::from(p);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR_NAME)
}

pub fn db_path() -> PathBuf {
    data_dir().join("kudownloader.db")
}

/// Written by the running app: `{port, token, pid}` for the local API.
pub fn api_file() -> PathBuf {
    data_dir().join("api.json")
}

pub fn default_download_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

/// Locate an engine binary. Order: explicit override, next to the current
/// executable (bundled sidecar), `binaries/` next to it, then `PATH`.
pub fn find_binary(base: &str, override_path: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = override_path.filter(|p| !p.trim().is_empty()) {
        let p = PathBuf::from(p.trim());
        return p.is_file().then_some(p);
    }
    let name = exe_name(base);
    if let Some(dir) = current_exe_dir() {
        for cand in [dir.join(&name), dir.join("binaries").join(&name)] {
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join(&name);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    #[cfg(windows)]
    if let Some(local) = dirs::data_local_dir() {
        let winget = local.join("Microsoft").join("WinGet");
        let cand = winget.join("Links").join(&name);
        if cand.is_file() {
            return Some(cand);
        }
        // winget portable packages live in Packages\<id>\[<subdir>\]<exe> and are
        // only on PATH for processes started after installation.
        if let Ok(pkgs) = std::fs::read_dir(winget.join("Packages")) {
            for pkg in pkgs.flatten() {
                let dir = pkg.path();
                if dir.join(&name).is_file() {
                    return Some(dir.join(&name));
                }
                if let Ok(subs) = std::fs::read_dir(&dir) {
                    for sub in subs.flatten() {
                        let c = sub.path().join(&name);
                        if c.is_file() {
                            return Some(c);
                        }
                        let c = sub.path().join("bin").join(&name);
                        if c.is_file() {
                            return Some(c);
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn current_exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// Path of the desktop app executable, assumed to live next to the CLI and
/// the native host (true for both the installer layout and `target/<profile>`).
pub fn app_executable() -> Option<PathBuf> {
    let dir = current_exe_dir()?;
    ["kudownloader", "KuDownloader"]
        .iter()
        .map(|b| dir.join(exe_name(b)))
        .find(|p| p.is_file())
}
