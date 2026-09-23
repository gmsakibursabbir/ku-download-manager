//! URL classification and smart engine selection.
//!
//! KuCore never depends on specific websites for downloading: this module only
//! decides which engine is best suited. Media hosts go to yt-dlp, everything
//! file-like goes to aria2, ambiguous URLs are probed first.

use crate::settings::Category;
use ku_proto::{Engine, Kind};
use url::Url;

/// Hosts that serve media pages rather than files. Matching is by suffix, so
/// `m.youtube.com` matches `youtube.com`.
const MEDIA_HOSTS: &[&str] = &[
    "youtube.com", "youtu.be", "youtube-nocookie.com", "vimeo.com", "dailymotion.com", "dai.ly",
    "twitch.tv", "soundcloud.com", "bandcamp.com", "tiktok.com", "instagram.com", "facebook.com",
    "fb.watch", "x.com", "twitter.com", "reddit.com", "v.redd.it", "bilibili.com", "nicovideo.jp",
    "rumble.com", "odysee.com", "streamable.com", "mixcloud.com", "ted.com", "vk.com", "ok.ru",
    "bitchute.com", "archive.org/details", "peertube", "kick.com", "threads.net", "pinterest.com",
    "tumblr.com", "loom.com", "coub.com", "9gag.com", "imgur.com/gallery",
];

const STREAM_EXTENSIONS: &[&str] = &["m3u8", "mpd", "ism"];

/// Extensions that are always treated as plain files even without a category.
const FILE_EXTENSIONS: &[&str] = &[
    "bin", "dat", "part", "tar", "gz", "zip", "json", "xml", "jar", "war", "whl", "crate", "nupkg",
    "vsix", "xpi", "crx", "ttf", "otf", "woff", "woff2", "sqlite", "db", "sig", "asc", "sha256",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Aria2(Kind),
    Ytdlp,
    /// Needs an HTTP probe to decide.
    Unknown,
}

pub fn scheme_allowed(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    ["http://", "https://", "ftp://", "sftp://", "magnet:"].iter().any(|p| lower.starts_with(p))
}

pub fn extension_of(path: &str) -> Option<String> {
    let last = path.rsplit('/').next()?;
    let (_, ext) = last.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    (!ext.is_empty() && ext.len() <= 10 && ext.chars().all(|c| c.is_ascii_alphanumeric())).then_some(ext)
}

pub fn is_media_host(u: &Url) -> bool {
    let host = u.host_str().unwrap_or("").trim_start_matches("www.").to_ascii_lowercase();
    let host_path = format!("{host}{}", u.path());
    MEDIA_HOSTS.iter().any(|m| {
        if m.contains('/') {
            host_path.starts_with(m)
        } else if !m.contains('.') {
            host.contains(m)
        } else {
            host == *m || host.ends_with(&format!(".{m}"))
        }
    })
}

/// Classify without network access.
pub fn classify(url: &str, categories: &[Category]) -> Route {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("magnet:") {
        return Route::Aria2(Kind::Magnet);
    }
    let Ok(u) = Url::parse(trimmed) else { return Route::Unknown };
    match u.scheme() {
        "ftp" => return Route::Aria2(Kind::Ftp),
        "sftp" => return Route::Aria2(Kind::Sftp),
        "http" | "https" => {}
        _ => return Route::Unknown,
    }
    let ext = extension_of(u.path());
    match ext.as_deref() {
        Some("torrent") => return Route::Aria2(Kind::Torrent),
        Some("metalink" | "meta4") => return Route::Aria2(Kind::Metalink),
        Some(e) if STREAM_EXTENSIONS.contains(&e) => return Route::Ytdlp,
        _ => {}
    }
    if is_media_host(&u) {
        return Route::Ytdlp;
    }
    if let Some(e) = ext.as_deref() {
        let known = FILE_EXTENSIONS.contains(&e)
            || categories.iter().any(|c| c.extensions.iter().any(|x| x == e));
        if known {
            return Route::Aria2(Kind::Http);
        }
    }
    Route::Unknown
}

/// Decide from the response of a probe request.
pub fn route_from_probe(mime: Option<&str>, attachment: bool, filename: Option<&str>) -> (Engine, Kind) {
    let mime = mime.unwrap_or("").to_ascii_lowercase();
    if mime.contains("bittorrent") || filename.is_some_and(|f| f.to_ascii_lowercase().ends_with(".torrent")) {
        return (Engine::Aria2, Kind::Torrent);
    }
    if mime.contains("metalink") {
        return (Engine::Aria2, Kind::Metalink);
    }
    if mime.contains("mpegurl") || mime.contains("dash+xml") {
        return (Engine::Ytdlp, Kind::Media);
    }
    if !attachment && (mime.starts_with("text/html") || mime.starts_with("application/xhtml")) {
        // A web page: yt-dlp's extractors (including the generic one) can find
        // embedded media. If it finds nothing, the user gets a clear error.
        return (Engine::Ytdlp, Kind::Media);
    }
    (Engine::Aria2, Kind::Http)
}

pub fn category_for(name: &str, categories: &[Category], kind: Kind) -> String {
    if kind.is_bittorrent() {
        return "torrents".into();
    }
    if kind == Kind::Media {
        return "video".into();
    }
    let ext = extension_of(name).unwrap_or_default();
    categories
        .iter()
        .find(|c| c.extensions.iter().any(|e| *e == ext))
        .map(|c| c.id.clone())
        .unwrap_or_default()
}

/// File name from the last URL path segment, percent-decoded and sanitized.
pub fn filename_from_url(url: &str) -> Option<String> {
    let u = Url::parse(url).ok()?;
    let seg = u.path_segments()?.rev().find(|s| !s.is_empty())?;
    let decoded = percent_encoding::percent_decode_str(seg).decode_utf8_lossy();
    let name = sanitize_filename(&decoded);
    (!name.is_empty()).then_some(name)
}

/// Parse `Content-Disposition`, preferring RFC 5987 `filename*`.
pub fn filename_from_disposition(v: &str) -> Option<String> {
    let mut plain = None;
    for part in v.split(';').map(str::trim) {
        let lower = part.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("filename*=").map(|_| &part[10..]) {
            let rest = rest.trim_matches('"');
            let value = rest.splitn(3, '\'').nth(2).unwrap_or(rest);
            let decoded = percent_encoding::percent_decode_str(value).decode_utf8_lossy();
            let n = sanitize_filename(&decoded);
            if !n.is_empty() {
                return Some(n);
            }
        } else if lower.starts_with("filename=") {
            let n = sanitize_filename(part[9..].trim().trim_matches('"'));
            if !n.is_empty() {
                plain = Some(n);
            }
        }
    }
    plain
}

/// Strip path separators and characters invalid on Windows; keep it portable.
pub fn sanitize_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let mut out: String = base
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    out = out.trim().trim_end_matches(['.', ' ']).to_string();
    if out == "." || out == ".." {
        out.clear();
    }
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    const RESERVED: &[&str] = &["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "LPT1", "LPT2", "LPT3"];
    if RESERVED.contains(&stem.as_str()) {
        out.insert(0, '_');
    }
    if out.len() > 240 {
        let ext = extension_of(&out).map(|e| format!(".{e}")).unwrap_or_default();
        let mut cut = 240 - ext.len();
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out = format!("{}{}", &out[..cut], ext);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::default_categories;

    #[test]
    fn routes() {
        let c = default_categories();
        assert_eq!(classify("magnet:?xt=urn:btih:abc", &c), Route::Aria2(Kind::Magnet));
        assert_eq!(classify("https://www.youtube.com/watch?v=x", &c), Route::Ytdlp);
        assert_eq!(classify("https://youtu.be/x", &c), Route::Ytdlp);
        assert_eq!(classify("https://example.com/a.iso", &c), Route::Aria2(Kind::Http));
        assert_eq!(classify("https://example.com/a.torrent", &c), Route::Aria2(Kind::Torrent));
        assert_eq!(classify("https://cdn.example.com/v/master.m3u8", &c), Route::Ytdlp);
        assert_eq!(classify("ftp://example.com/x", &c), Route::Aria2(Kind::Ftp));
        assert_eq!(classify("https://example.com/download?id=3", &c), Route::Unknown);
        assert_eq!(classify("https://notyoutube.com/x", &c), Route::Unknown);
    }

    #[test]
    fn filenames() {
        assert_eq!(filename_from_url("https://e.com/a/My%20File.zip?x=1").unwrap(), "My File.zip");
        assert_eq!(
            filename_from_disposition("attachment; filename=\"a.zip\"; filename*=UTF-8''%E2%82%AC.zip").unwrap(),
            "€.zip"
        );
        assert_eq!(filename_from_disposition("attachment; filename=b.txt").unwrap(), "b.txt");
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("a<b>:c?.txt"), "a_b__c_.txt");
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
    }

    #[test]
    fn categories() {
        let c = default_categories();
        assert_eq!(category_for("ubuntu.iso", &c, Kind::Http), "images-disk");
        assert_eq!(category_for("a.zip", &c, Kind::Http), "archives");
        assert_eq!(category_for("x.unknown", &c, Kind::Http), "");
        assert_eq!(route_from_probe(Some("text/html; charset=utf-8"), false, None).0, Engine::Ytdlp);
        assert_eq!(route_from_probe(Some("text/html"), true, None).0, Engine::Aria2);
    }
}
