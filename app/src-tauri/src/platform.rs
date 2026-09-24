//! Which window chrome to draw: OS, Linux desktop and its button layout, so
//! the title bar matches Windows, macOS, GNOME, KDE, Cinnamon, XFCE, MATE and
//! tiling compositors (niri, Hyprland, Sway, i3…).

use serde::Serialize;

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    /// "windows" | "macos" | "linux"
    pub os: String,
    /// Lower-case desktop id on Linux ("gnome", "kde", "cinnamon", "xfce",
    /// "mate", "budgie", "pantheon", "niri", "hyprland", "sway", …), else "".
    pub desktop: String,
    /// Tiling compositor: no window buttons, the compositor draws borders.
    pub tiling: bool,
    /// Window buttons ("minimize" | "maximize" | "close") on each side.
    pub left: Vec<String>,
    pub right: Vec<String>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const TILING: &[&str] = &["niri", "hyprland", "sway", "i3", "river", "bspwm", "awesome", "dwm", "qtile", "herbstluftwm", "xmonad", "wayfire", "cosmic-tiling"];

fn std_buttons() -> (Vec<String>, Vec<String>) {
    (vec![], vec!["minimize".into(), "maximize".into(), "close".into()])
}

/// GNOME-style layout string: "appmenu:minimize,maximize,close" (left:right).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_gnome_layout(s: &str) -> (Vec<String>, Vec<String>) {
    let s = s.trim().trim_matches('\'').trim_matches('"');
    let (l, r) = s.split_once(':').unwrap_or(("", s));
    let side = |x: &str| x.split(',').map(str::trim).filter(|b| matches!(*b, "minimize" | "maximize" | "close")).map(String::from).collect();
    (side(l), side(r))
}

/// KDE kwinrc letters: I = minimize, A = maximize, X = close.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_kde_side(s: &str) -> Vec<String> {
    s.chars()
        .filter_map(|c| match c {
            'I' => Some("minimize"),
            'A' => Some("maximize"),
            'X' => Some("close"),
            _ => None,
        })
        .map(String::from)
        .collect()
}

/// xfwm4 "O|HMC": H = hide (minimize), M = maximize, C = close; '|' is the title.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_xfce_layout(s: &str) -> (Vec<String>, Vec<String>) {
    let (l, r) = s.trim().split_once('|').unwrap_or(("", s.trim()));
    let side = |x: &str| {
        x.chars()
            .filter_map(|c| match c {
                'H' => Some("minimize"),
                'M' => Some("maximize"),
                'C' => Some("close"),
                _ => None,
            })
            .map(String::from)
            .collect()
    };
    (side(l), side(r))
}

#[cfg(target_os = "linux")]
fn command_output(cmd: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(cmd).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

#[cfg(target_os = "linux")]
fn linux_desktop() -> String {
    if std::env::var_os("NIRI_SOCKET").is_some() {
        return "niri".into();
    }
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        return "hyprland".into();
    }
    if std::env::var_os("SWAYSOCK").is_some() {
        return "sway".into();
    }
    let raw = std::env::var("XDG_CURRENT_DESKTOP").or_else(|_| std::env::var("XDG_SESSION_DESKTOP")).or_else(|_| std::env::var("DESKTOP_SESSION")).unwrap_or_default().to_lowercase();
    // "ubuntu:GNOME", "X-Cinnamon", "KDE", "XFCE", "niri", "Hyprland"…
    for part in raw.split(':') {
        let p = part.trim_start_matches("x-");
        for known in ["gnome", "kde", "plasma", "cinnamon", "xfce", "mate", "budgie", "pantheon", "unity", "lxqt", "lxde", "deepin", "cosmic"] {
            if p.contains(known) {
                return if known == "plasma" { "kde".into() } else { known.into() };
            }
        }
        if TILING.contains(&p) {
            return p.to_string();
        }
    }
    raw
}

#[cfg(target_os = "linux")]
fn kde_layout() -> (Vec<String>, Vec<String>) {
    let config = std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")));
    let path = config.map(|d| d.join("kwinrc"));
    let text = path.and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let mut in_section = false;
    let (mut left, mut right) = (None, None);
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_section = l == "[org.kde.kdecoration2]";
            continue;
        }
        if in_section {
            if let Some(v) = l.strip_prefix("ButtonsOnLeft=") {
                left = Some(parse_kde_side(v));
            } else if let Some(v) = l.strip_prefix("ButtonsOnRight=") {
                right = Some(parse_kde_side(v));
            }
        }
    }
    // Breeze default: left "MS", right "HIAX".
    (left.unwrap_or_default(), right.unwrap_or_else(|| std_buttons().1))
}

pub fn detect() -> PlatformInfo {
    let os = std::env::consts::OS.to_string();
    #[cfg(target_os = "linux")]
    {
        let desktop = linux_desktop();
        let tiling = TILING.iter().any(|t| desktop == *t);
        let (left, right) = if tiling {
            (vec![], vec![])
        } else {
            match desktop.as_str() {
                "gnome" | "budgie" | "pantheon" | "unity" | "cosmic" => command_output("gsettings", &["get", "org.gnome.desktop.wm.preferences", "button-layout"])
                    .map(|s| parse_gnome_layout(&s))
                    .unwrap_or_else(|| (vec![], vec!["close".into()])),
                "cinnamon" => command_output("gsettings", &["get", "org.cinnamon.desktop.wm.preferences", "button-layout"]).map(|s| parse_gnome_layout(&s)).unwrap_or_else(std_buttons),
                "mate" => command_output("gsettings", &["get", "org.mate.Marco.general", "button-layout"]).map(|s| parse_gnome_layout(&s)).unwrap_or_else(std_buttons),
                "xfce" => command_output("xfconf-query", &["-c", "xfwm4", "-p", "/general/button_layout"]).map(|s| parse_xfce_layout(&s)).unwrap_or_else(std_buttons),
                "kde" => kde_layout(),
                _ => std_buttons(),
            }
        };
        return PlatformInfo { os, desktop, tiling, left, right };
    }
    #[cfg(target_os = "macos")]
    {
        // Native traffic lights (Overlay title bar); nothing to draw.
        return PlatformInfo { os, ..Default::default() };
    }
    #[allow(unreachable_code)]
    {
        let (left, right) = std_buttons();
        PlatformInfo { os, desktop: String::new(), tiling: false, left, right }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts() {
        assert_eq!(parse_gnome_layout("'appmenu:close'"), (vec![], vec!["close".to_string()]));
        assert_eq!(parse_gnome_layout("'close,minimize,maximize:'"), (vec!["close".into(), "minimize".into(), "maximize".into()], vec![]));
        assert_eq!(parse_gnome_layout(":minimize,maximize,close"), (vec![], vec!["minimize".into(), "maximize".into(), "close".into()]));
        assert_eq!(parse_kde_side("HIAX"), vec!["minimize", "maximize", "close"]);
        assert_eq!(parse_kde_side("MS"), Vec::<String>::new());
        assert_eq!(parse_xfce_layout("O|HMC"), (vec![], vec!["minimize".into(), "maximize".into(), "close".into()]));
        assert_eq!(parse_xfce_layout("CMH|O"), (vec!["close".into(), "maximize".into(), "minimize".into()], vec![]));
    }
}
