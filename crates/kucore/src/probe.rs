//! Lightweight HTTP probe: resolves redirects, file name, size, range support
//! and content type before a download is handed to an engine.

use crate::classify;
use crate::settings::Settings;
use ku_proto::{DownloadOptions, ProbeInfo};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::time::Duration;

pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";

pub fn user_agent(opts: &DownloadOptions, s: &Settings) -> String {
    opts.user_agent
        .clone()
        .filter(|u| !u.trim().is_empty())
        .or_else(|| (!s.user_agent.trim().is_empty()).then(|| s.user_agent.clone()))
        .unwrap_or_else(|| DEFAULT_USER_AGENT.to_string())
}

/// `Cookie` header value for cookies that apply to `url`.
pub fn cookie_header(opts: &DownloadOptions) -> Option<String> {
    let v: Vec<String> = opts.cookies.iter().map(|c| format!("{}={}", c.name, c.value)).collect();
    (!v.is_empty()).then(|| v.join("; "))
}

fn client(s: &Settings, opts: &DownloadOptions) -> reqwest::Result<reqwest::Client> {
    let mut b = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(10))
        .danger_accept_invalid_certs(!s.check_certificate);
    let proxy = opts.proxy.clone().filter(|p| !p.is_empty()).unwrap_or_else(|| s.proxy.clone());
    if !proxy.trim().is_empty() {
        let mut p = reqwest::Proxy::all(proxy.trim())?;
        if !s.proxy_user.is_empty() {
            p = p.basic_auth(&s.proxy_user, &s.proxy_pass);
        }
        b = b.proxy(p);
    }
    b.build()
}

pub async fn probe(url: &str, opts: &DownloadOptions, s: &Settings) -> ProbeInfo {
    let mut info = ProbeInfo { url: url.to_string(), final_url: url.to_string(), ..Default::default() };
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        info.filename = classify::filename_from_url(url);
        return info;
    }
    let c = match client(s, opts) {
        Ok(c) => c,
        Err(e) => {
            info.error = Some(format!("Invalid proxy configuration: {e}"));
            return info;
        }
    };
    let mut headers = HeaderMap::new();
    for h in &opts.headers {
        if let Some((k, v)) = h.split_once(':') {
            if let (Ok(k), Ok(v)) = (HeaderName::from_bytes(k.trim().as_bytes()), HeaderValue::from_str(v.trim())) {
                headers.insert(k, v);
            }
        }
    }
    if let Some(ck) = cookie_header(opts).and_then(|c| HeaderValue::from_str(&c).ok()) {
        headers.insert(reqwest::header::COOKIE, ck);
    }
    if let Some(r) = opts.referer.as_deref().and_then(|r| HeaderValue::from_str(r).ok()) {
        headers.insert(reqwest::header::REFERER, r);
    }
    let mut req = c
        .get(url)
        .headers(headers)
        .header(reqwest::header::USER_AGENT, user_agent(opts, s))
        .header(reqwest::header::RANGE, "bytes=0-0")
        .header(reqwest::header::ACCEPT_ENCODING, "identity");
    if let (Some(u), p) = (opts.username.as_deref(), opts.password.as_deref()) {
        req = req.basic_auth(u, p);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            info.error = Some(describe_reqwest_error(&e));
            return info;
        }
    };
    let status = resp.status();
    info.status = Some(status.as_u16());
    info.final_url = resp.url().to_string();
    let h = resp.headers();
    let get = |n: reqwest::header::HeaderName| h.get(n).and_then(|v| v.to_str().ok()).map(str::to_string);
    info.mime = get(reqwest::header::CONTENT_TYPE).map(|m| m.split(';').next().unwrap_or("").trim().to_string());
    let disposition = get(reqwest::header::CONTENT_DISPOSITION);
    let attachment = disposition.as_deref().is_some_and(|d| d.to_ascii_lowercase().contains("attachment"));
    info.filename = disposition
        .as_deref()
        .and_then(classify::filename_from_disposition)
        .or_else(|| classify::filename_from_url(&info.final_url))
        .or_else(|| classify::filename_from_url(url));
    if status.as_u16() == 206 {
        info.resumable = Some(true);
        info.size = get(reqwest::header::CONTENT_RANGE)
            .and_then(|r| r.rsplit('/').next().and_then(|t| t.trim().parse().ok()));
    } else if status.is_success() {
        let accepts = get(reqwest::header::ACCEPT_RANGES).is_some_and(|v| v.contains("bytes"));
        info.resumable = Some(accepts);
        info.size = resp.content_length().map(|l| l as i64).filter(|l| *l > 0);
    } else {
        info.error = Some(describe_status(status.as_u16()));
    }
    drop(resp);
    if status.is_success() {
        let (engine, kind) = classify::route_from_probe(info.mime.as_deref(), attachment, info.filename.as_deref());
        info.engine = Some(engine);
        info.kind = Some(kind);
        // A web page has no meaningful file name for media extraction.
        if engine == ku_proto::Engine::Ytdlp {
            info.filename = None;
            info.size = None;
        }
    }
    info
}

pub fn describe_status(code: u16) -> String {
    match code {
        401 => "The server requires authentication (HTTP 401). Add credentials in the download options.".into(),
        403 => "The server refused access (HTTP 403). The link may have expired or require browser cookies.".into(),
        404 => "The file was not found on the server (HTTP 404).".into(),
        410 => "The file is no longer available (HTTP 410).".into(),
        416 => "The server rejected the requested byte range (HTTP 416).".into(),
        429 => "The server is rate limiting requests (HTTP 429).".into(),
        500..=599 => format!("The server reported an internal error (HTTP {code})."),
        _ => format!("The server responded with HTTP {code}."),
    }
}

pub fn describe_reqwest_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "The server did not respond in time.".into()
    } else if e.is_connect() {
        "Could not connect to the server. Check your network connection or proxy.".into()
    } else if e.is_redirect() {
        "Too many redirects.".into()
    } else {
        format!("Request failed: {e}")
    }
}
