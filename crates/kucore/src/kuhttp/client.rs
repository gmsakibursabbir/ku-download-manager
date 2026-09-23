//! Shared HTTP client, credential scoping, redirects and server probing.

use super::config::KuHttpConfig;
use super::error::KuError;
use super::response::{self, ContentRange};
use super::retry::parse_retry_after;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT_ENCODING, AUTHORIZATION, COOKIE, IF_RANGE, LOCATION, RANGE, REFERER, USER_AGENT};
use reqwest::{Client, Response, StatusCode};
use std::collections::HashSet;
use url::Url;

/// One pooled client per engine: keep-alive, HTTP/2 via ALPN, no automatic
/// redirects (handled manually for credential scoping) and no transparent
/// decompression (byte ranges must address the raw representation).
pub fn build_client(cfg: &KuHttpConfig) -> Result<Client, KuError> {
    let mut b = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_gzip()
        .pool_max_idle_per_host(32)
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .tcp_keepalive(std::time::Duration::from_secs(60))
        .tcp_nodelay(true)
        .connect_timeout(cfg.connect_timeout)
        .read_timeout(cfg.read_timeout)
        .danger_accept_invalid_certs(!cfg.verify_tls);
    if let Some(p) = &cfg.proxy {
        let mut proxy = reqwest::Proxy::all(p.url.trim()).map_err(|e| KuError::InvalidUrl(format!("proxy: {e}")))?;
        if let Some(u) = &p.username {
            proxy = proxy.basic_auth(u, p.password.as_deref().unwrap_or(""));
        }
        b = b.proxy(proxy);
    }
    b.build().map_err(|e| KuError::Network(e.to_string()))
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Origin {
    scheme: String,
    host: String,
    port: Option<u16>,
}

impl Origin {
    fn of(u: &Url) -> Origin {
        Origin { scheme: u.scheme().to_string(), host: u.host_str().unwrap_or("").to_ascii_lowercase(), port: u.port_or_known_default() }
    }
}

/// Request parameters with credentials bound to the origin they were given for.
#[derive(Clone)]
pub struct RequestCtx {
    pub original: Url,
    origin: Origin,
    user_agent: HeaderValue,
    referer: Option<HeaderValue>,
    /// Custom headers, cookies and authorization: original origin only.
    private: HeaderMap,
}

impl RequestCtx {
    pub fn new(
        url: &str,
        headers: &[(String, String)],
        cookies: Option<&str>,
        referer: Option<&str>,
        user_agent: &str,
        basic_auth: Option<(&str, &str)>,
    ) -> Result<RequestCtx, KuError> {
        let original = Url::parse(url.trim()).map_err(|e| KuError::InvalidUrl(e.to_string()))?;
        if !matches!(original.scheme(), "http" | "https") {
            return Err(KuError::InvalidUrl("only http and https are supported".into()));
        }
        let hv = |v: &str, what: &str| HeaderValue::from_str(v).map_err(|_| KuError::InvalidHeader(what.to_string()));
        let mut private = HeaderMap::new();
        for (k, v) in headers {
            let name = HeaderName::from_bytes(k.trim().as_bytes()).map_err(|_| KuError::InvalidHeader(k.clone()))?;
            // Headers the engine controls cannot be overridden.
            if [RANGE, IF_RANGE, ACCEPT_ENCODING, reqwest::header::HOST, reqwest::header::CONTENT_LENGTH].contains(&name) {
                continue;
            }
            private.append(name, hv(v.trim(), k)?);
        }
        if let Some(c) = cookies.filter(|c| !c.is_empty()) {
            private.insert(COOKIE, hv(c, "Cookie")?);
        }
        if let Some((u, p)) = basic_auth {
            use base64::Engine as _;
            let token = base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}"));
            let mut v = hv(&format!("Basic {token}"), "Authorization")?;
            v.set_sensitive(true);
            private.insert(AUTHORIZATION, v);
        }
        for v in private.values_mut() {
            v.set_sensitive(true);
        }
        Ok(RequestCtx {
            origin: Origin::of(&original),
            original,
            user_agent: hv(user_agent, "User-Agent")?,
            referer: referer.filter(|r| !r.is_empty()).map(|r| hv(r, "Referer")).transpose()?,
            private,
        })
    }

    /// Credentials go only to the origin they were supplied for (or its
    /// https upgrade). Redirects elsewhere get a clean request.
    pub fn sends_credentials_to(&self, u: &Url) -> bool {
        let o = Origin::of(u);
        o == self.origin || (o.host == self.origin.host && self.origin.scheme == "http" && o.scheme == "https" && o.port == Some(443))
    }

    pub fn headers_for(&self, u: &Url) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(USER_AGENT, self.user_agent.clone());
        h.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
        if let Some(r) = &self.referer {
            h.insert(REFERER, r.clone());
        }
        if self.sends_credentials_to(u) {
            for (k, v) in &self.private {
                h.append(k, v.clone());
            }
        }
        h
    }
}

/// Validators identifying a representation.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

impl Validators {
    fn from(h: &HeaderMap) -> Validators {
        let get = |n| h.get(n).and_then(|v: &HeaderValue| v.to_str().ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        Validators { etag: get(reqwest::header::ETAG), last_modified: get(reqwest::header::LAST_MODIFIED) }
    }

    /// If-Range value: only strong ETags are allowed by RFC 9110; fall back
    /// to Last-Modified.
    pub fn if_range(&self) -> Option<HeaderValue> {
        match &self.etag {
            Some(e) if !e.starts_with("W/") => HeaderValue::from_str(e).ok(),
            _ => self.last_modified.as_deref().and_then(|l| HeaderValue::from_str(l).ok()),
        }
    }

    pub fn has_any(&self) -> bool {
        self.etag.is_some() || self.last_modified.is_some()
    }

    /// Returns a reason when `other` identifies a different representation.
    pub fn changed(&self, other: &Validators) -> Option<String> {
        let norm = |e: &str| e.trim_start_matches("W/").to_string();
        if let (Some(a), Some(b)) = (&self.etag, &other.etag) {
            if norm(a) != norm(b) {
                return Some("ETag changed".into());
            }
        }
        if let (Some(a), Some(b)) = (&self.last_modified, &other.last_modified) {
            if a != b {
                return Some("Last-Modified changed".into());
            }
        }
        if self.etag.is_some() != other.etag.is_some() && self.last_modified.is_some() != other.last_modified.is_some() {
            return Some("validators changed".into());
        }
        None
    }
}

/// Follow redirects manually: loop detection, http(s) only, credentials
/// stripped when leaving the original origin. Returns the final response
/// and the final URL.
pub async fn send(
    client: &Client,
    ctx: &RequestCtx,
    start: &Url,
    range: Option<(u64, Option<u64>)>,
    if_range: Option<&HeaderValue>,
    max_redirects: usize,
) -> Result<(Response, Url, Vec<Url>), KuError> {
    let mut url = start.clone();
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    loop {
        let mut req = client.get(url.clone()).headers(ctx.headers_for(&url));
        if let Some((a, b)) = range {
            let v = match b {
                Some(b) => format!("bytes={a}-{b}"),
                None => format!("bytes={a}-"),
            };
            req = req.header(RANGE, v);
            if let Some(ir) = if_range {
                req = req.header(IF_RANGE, ir.clone());
            }
        }
        let resp = req.send().await.map_err(|e| KuError::from_reqwest(&e))?;
        let st = resp.status();
        if !matches!(st.as_u16(), 301 | 302 | 303 | 307 | 308) {
            return Ok((resp, url, chain));
        }
        if chain.len() >= max_redirects {
            return Err(KuError::TooManyRedirects);
        }
        let loc = resp
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| KuError::BadRedirect("missing Location".into()))?;
        let next = url.join(loc).map_err(|e| KuError::BadRedirect(e.to_string()))?;
        if !matches!(next.scheme(), "http" | "https") {
            return Err(KuError::BadRedirect(format!("unsupported scheme {}", next.scheme())));
        }
        if !seen.insert(next.to_string()) || next == *start {
            return Err(KuError::RedirectLoop);
        }
        chain.push(next.clone());
        url = next;
    }
}

/// What a range probe learned about the server.
#[derive(Clone, Debug)]
pub struct Probe {
    pub final_url: Url,
    pub redirects: usize,
    pub total: Option<u64>,
    /// A real `bytes=0-0` request returned a valid 206.
    pub ranges: bool,
    pub validators: Validators,
    pub content_type: Option<String>,
    pub filename: Option<String>,
    /// Checksum announced by the server for the whole representation.
    pub server_checksum: Option<(String, String)>,
    pub http_version: String,
}

pub fn status_error(resp: &Response) -> KuError {
    KuError::Http { status: resp.status().as_u16(), retry_after: parse_retry_after(resp.headers()) }
}

/// Probe with an actual `Range: bytes=0-0` request — `Accept-Ranges` alone is
/// not trusted.
pub async fn probe(client: &Client, ctx: &RequestCtx, cfg: &KuHttpConfig) -> Result<Probe, KuError> {
    let (resp, final_url, chain) = send(client, ctx, &ctx.original, Some((0, Some(0))), None, cfg.max_redirects).await?;
    let st = resp.status();
    let h = resp.headers().clone();
    let get = |n: HeaderName| h.get(n).and_then(|v| v.to_str().ok()).map(str::to_string);
    let mut p = Probe {
        redirects: chain.len(),
        total: None,
        ranges: false,
        validators: Validators::from(&h),
        content_type: get(reqwest::header::CONTENT_TYPE),
        filename: get(reqwest::header::CONTENT_DISPOSITION)
            .as_deref()
            .and_then(crate::classify::filename_from_disposition)
            .or_else(|| crate::classify::filename_from_url(final_url.as_str()))
            .or_else(|| crate::classify::filename_from_url(ctx.original.as_str())),
        server_checksum: super::integrity::server_checksum(&h),
        http_version: format!("{:?}", resp.version()),
        final_url,
    };
    match st {
        StatusCode::PARTIAL_CONTENT => {
            let cr = get(reqwest::header::CONTENT_RANGE).and_then(|v| response::parse_content_range(&v));
            match cr {
                Some(ContentRange { start: 0, end: 0, total: Some(t) }) => {
                    // The body must be exactly the one byte we asked for.
                    let body = resp.bytes().await.map_err(|e| KuError::from_reqwest(&e))?;
                    p.ranges = body.len() == 1;
                    p.total = Some(t);
                }
                Some(ContentRange { total: Some(t), .. }) => p.total = Some(t),
                _ => {}
            }
        }
        StatusCode::OK => {
            // Server ignored Range: single-stream only. Do not read the body.
            p.total = resp.content_length();
        }
        StatusCode::RANGE_NOT_SATISFIABLE => {
            // An empty representation: "bytes */0".
            let cr = get(reqwest::header::CONTENT_RANGE).and_then(|v| response::parse_unsatisfied_total(&v));
            if cr == Some(0) {
                p.total = Some(0);
            } else {
                return Err(status_error(&resp));
            }
        }
        _ => return Err(status_error(&resp)),
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_origin_scoped() {
        let ctx = RequestCtx::new(
            "http://files.example.com/a.zip",
            &[("X-Api-Key".into(), "secret".into())],
            Some("sid=1"),
            None,
            "UA",
            Some(("u", "p")),
        )
        .unwrap();
        let same = ctx.headers_for(&Url::parse("http://files.example.com/b").unwrap());
        assert!(same.contains_key(AUTHORIZATION) && same.contains_key(COOKIE) && same.contains_key("x-api-key"));
        let upgraded = ctx.headers_for(&Url::parse("https://files.example.com/b").unwrap());
        assert!(upgraded.contains_key(AUTHORIZATION));
        let other = ctx.headers_for(&Url::parse("https://cdn.other.net/b").unwrap());
        assert!(!other.contains_key(AUTHORIZATION) && !other.contains_key(COOKIE) && !other.contains_key("x-api-key"));
        assert!(other.contains_key(USER_AGENT));
    }

    #[test]
    fn rejects_header_injection() {
        let r = RequestCtx::new("https://e.com/", &[("X-A".into(), "v\r\nInjected: 1".into())], None, None, "UA", None);
        assert!(matches!(r, Err(KuError::InvalidHeader(_))));
        assert!(RequestCtx::new("file:///etc/passwd", &[], None, None, "UA", None).is_err());
    }

    #[test]
    fn validator_changes() {
        let a = Validators { etag: Some("\"1\"".into()), last_modified: None };
        let b = Validators { etag: Some("\"2\"".into()), last_modified: None };
        assert!(a.changed(&b).is_some());
        assert!(a.changed(&a.clone()).is_none());
        let w = Validators { etag: Some("W/\"1\"".into()), last_modified: None };
        assert!(w.if_range().is_none());
        assert!(a.if_range().is_some());
    }
}
