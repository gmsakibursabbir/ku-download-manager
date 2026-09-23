//! URL Grabber: fetch one page and list the links and media it references.

use crate::classify;
use crate::settings::Settings;
use anyhow::{bail, Result};
use ku_proto::GrabLink;
use std::collections::HashSet;
use url::Url;

const MAX_PAGE: usize = 8 * 1024 * 1024;

/// Extract `href`/`src` attribute values without a full HTML parser.
pub fn extract_links(html: &str, base: &Url) -> Vec<GrabLink> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    for attr in ["href", "src", "data-src", "data-href"] {
        let needle = format!("{attr}=");
        let mut from = 0;
        while let Some(pos) = lower[from..].find(&needle) {
            let start = from + pos + needle.len();
            from = start;
            // Make sure this is a whole attribute name (preceded by whitespace).
            let before = from - needle.len();
            if before > 0 && !bytes[before - 1].is_ascii_whitespace() {
                continue;
            }
            let (value, end) = match bytes.get(start) {
                Some(b'"') | Some(b'\'') => {
                    let q = bytes[start] as char;
                    match html[start + 1..].find(q) {
                        Some(e) => (&html[start + 1..start + 1 + e], start + 1 + e),
                        None => continue,
                    }
                }
                Some(_) => {
                    let e = html[start..].find(|c: char| c.is_whitespace() || c == '>').unwrap_or(html.len() - start);
                    (&html[start..start + e], start + e)
                }
                None => break,
            };
            from = end;
            let v = decode_entities(value.trim());
            if v.is_empty() || v.starts_with('#') {
                continue;
            }
            let Ok(abs) = base.join(&v) else { continue };
            let s = abs.to_string();
            if !classify::scheme_allowed(&s) || !seen.insert(s.clone()) {
                continue;
            }
            let ext = classify::extension_of(abs.path());
            let text = anchor_text(html, end);
            out.push(GrabLink { url: s, text, kind: ext });
        }
    }
    out
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&").replace("&#38;", "&").replace("&quot;", "\"").replace("&#39;", "'")
}

/// Text of an `<a>` element following the attribute, trimmed and short.
fn anchor_text(html: &str, attr_end: usize) -> Option<String> {
    let rest = &html[attr_end..];
    let open_end = rest.find('>')?;
    let after = &rest[open_end + 1..];
    let close = after.find('<')?;
    let t: String = after[..close].split_whitespace().collect::<Vec<_>>().join(" ");
    (!t.is_empty() && t.len() < 200).then(|| decode_entities(&t))
}

pub async fn grab_page(url: &str, s: &Settings) -> Result<Vec<GrabLink>> {
    let base = Url::parse(url.trim())?;
    if !matches!(base.scheme(), "http" | "https") {
        bail!("Enter an http or https page address.");
    }
    let mut b = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .danger_accept_invalid_certs(!s.check_certificate);
    if !s.proxy.trim().is_empty() {
        b = b.proxy(reqwest::Proxy::all(s.proxy.trim())?);
    }
    let resp = b
        .build()?
        .get(base.clone())
        .header(reqwest::header::USER_AGENT, crate::probe::user_agent(&Default::default(), s))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(crate::probe::describe_reqwest_error(&e)))?;
    if !resp.status().is_success() {
        bail!("{}", crate::probe::describe_status(resp.status().as_u16()));
    }
    let final_url = resp.url().clone();
    let ct = resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    if !ct.is_empty() && !ct.contains("html") && !ct.contains("xml") {
        bail!("This address is a file ({ct}), not a web page. Add it as a download instead.");
    }
    let body = resp.bytes().await?;
    let body = &body[..body.len().min(MAX_PAGE)];
    Ok(extract_links(&String::from_utf8_lossy(body), &final_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_resolves() {
        let html = r##"<a href="/files/a.zip">Archive A</a> <A HREF='b.pdf'>B</a>
            <img src="https://cdn.x/i.png"> <a href="javascript:void(0)">x</a> <a href="#top">t</a>
            <a class="x" href=c.iso>C</a> <a href="/files/a.zip">dup</a> <a data-href="q?x=1&amp;y=2">q</a>"##;
        let base = Url::parse("https://example.com/dir/page.html").unwrap();
        let links = extract_links(html, &base);
        let urls: Vec<&str> = links.iter().map(|l| l.url.as_str()).collect();
        assert!(urls.contains(&"https://example.com/files/a.zip"));
        assert!(urls.contains(&"https://example.com/dir/b.pdf"));
        assert!(urls.contains(&"https://cdn.x/i.png"));
        assert!(urls.contains(&"https://example.com/dir/c.iso"));
        assert!(urls.contains(&"https://example.com/dir/q?x=1&y=2"));
        assert_eq!(urls.iter().filter(|u| u.ends_with("a.zip")).count(), 1);
        assert!(!urls.iter().any(|u| u.starts_with("javascript")));
        assert_eq!(links[0].text.as_deref(), Some("Archive A"));
        assert_eq!(links[0].kind.as_deref(), Some("zip"));
    }
}
