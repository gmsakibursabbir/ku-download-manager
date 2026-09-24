//! Links that lead to a page instead of the file: share links (Google Drive,
//! Dropbox, SourceForge, GitHub blob pages) are rewritten to the direct file
//! URL, and HTML landing pages are searched for the real target (meta
//! refresh, Google Drive's "can't scan for viruses" form, SourceForge's
//! direct link).

use url::Url;

fn host_is(u: &Url, host: &str) -> bool {
    u.host_str().is_some_and(|h| h == host || h.ends_with(&format!(".{host}")))
}

/// Direct-download form of a known share link, if any.
pub fn rewrite(raw: &str) -> Option<String> {
    let mut u = Url::parse(raw.trim()).ok()?;
    if host_is(&u, "drive.google.com") || (host_is(&u, "docs.google.com") && u.path().starts_with("/uc")) {
        // /file/d/<id>/view · /open?id=<id> · /uc?id=<id>
        let segs: Vec<&str> = u.path_segments().map(|s| s.collect()).unwrap_or_default();
        let id = segs.iter().position(|s| *s == "d").and_then(|i| segs.get(i + 1).map(|s| s.to_string())).or_else(|| u.query_pairs().find(|(k, _)| k == "id").map(|(_, v)| v.into_owned()))?;
        if id.len() < 10 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return None;
        }
        return Some(format!("https://drive.usercontent.google.com/download?id={id}&export=download&confirm=t"));
    }
    if host_is(&u, "dropbox.com") {
        let pairs: Vec<(String, String)> = u.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
        if pairs.iter().any(|(k, v)| k == "dl" && v == "1") || pairs.iter().any(|(k, _)| k == "raw") {
            return None;
        }
        let mut q = u.query_pairs_mut();
        q.clear();
        for (k, v) in pairs.iter().filter(|(k, _)| k != "dl") {
            q.append_pair(k, v);
        }
        q.append_pair("dl", "1");
        drop(q);
        return Some(u.to_string());
    }
    if host_is(&u, "sourceforge.net") && !host_is(&u, "downloads.sourceforge.net") {
        // /projects/<p>/files/<path…>/download → downloads.sourceforge.net/project/<p>/<path…>
        let segs: Vec<&str> = u.path_segments().map(|s| s.collect()).unwrap_or_default();
        if segs.len() >= 4 && segs[0] == "projects" && segs[2] == "files" && segs.last() == Some(&"download") {
            let rest = segs[3..segs.len() - 1].join("/");
            if !rest.is_empty() {
                return Some(format!("https://downloads.sourceforge.net/project/{}/{rest}", segs[1]));
            }
        }
        return None;
    }
    if u.host_str() == Some("github.com") {
        // /<owner>/<repo>/blob/<ref>/<path> → raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>
        let segs: Vec<&str> = u.path_segments().map(|s| s.collect()).unwrap_or_default();
        if segs.len() >= 5 && segs[2] == "blob" {
            return Some(format!("https://raw.githubusercontent.com/{}/{}/{}", segs[0], segs[1], segs[3..].join("/")));
        }
    }
    None
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        let before_ok = at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric() && lower.as_bytes()[at - 1] != b'-';
        let rest = &tag[at + name.len()..];
        let rest_trim = rest.trim_start();
        if before_ok && rest_trim.starts_with('=') {
            let v = rest_trim[1..].trim_start();
            let (q, body) = match v.chars().next() {
                Some(c @ ('"' | '\'')) => (Some(c), &v[1..]),
                _ => (None, v),
            };
            let end = match q {
                Some(c) => body.find(c).unwrap_or(body.len()),
                None => body.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(body.len()),
            };
            return Some(&body[..end]);
        }
        from = at + name.len();
    }
    None
}

fn tags<'a>(html: &'a str, name: &str) -> Vec<&'a str> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{name}");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = lower[from..].find(&open) {
        let start = from + i;
        let end = lower[start..].find('>').map(|e| start + e + 1).unwrap_or(html.len());
        out.push(&html[start..end]);
        from = end;
    }
    out
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&").replace("&#38;", "&").replace("&quot;", "\"").replace("&#x2F;", "/").replace("&#47;", "/")
}

/// The real file link on a landing page, if the page is one.
pub fn find_in_html(base: &Url, html: &str) -> Option<String> {
    // Google Drive "can't scan this file for viruses": a GET form with hidden inputs.
    for form in tags(html, "form") {
        if attr(form, "id") == Some("download-form") || attr(form, "action").is_some_and(|a| a.contains("drive.usercontent.google.com/download")) {
            let action = base.join(&decode_entities(attr(form, "action")?)).ok()?;
            let start = html.find(form)?;
            let end = html[start..].to_ascii_lowercase().find("</form>").map(|e| start + e).unwrap_or(html.len());
            let mut u = action;
            {
                let mut q = u.query_pairs_mut();
                for input in tags(&html[start..end], "input") {
                    if let (Some(n), Some(v)) = (attr(input, "name"), attr(input, "value")) {
                        q.append_pair(n, &decode_entities(v));
                    }
                }
            }
            return Some(u.to_string());
        }
    }
    // <meta http-equiv="refresh" content="5; url=https://…">
    for meta in tags(html, "meta") {
        if attr(meta, "http-equiv").is_some_and(|v| v.eq_ignore_ascii_case("refresh")) {
            let Some(content) = attr(meta, "content") else { continue };
            let lower = content.to_ascii_lowercase();
            if let Some(i) = lower.find("url=") {
                let target = content[i + 4..].trim().trim_matches(|c| c == '\'' || c == '"');
                if let Ok(u) = base.join(&decode_entities(target)) {
                    if matches!(u.scheme(), "http" | "https") && u.as_str() != base.as_str() {
                        return Some(u.to_string());
                    }
                }
            }
        }
    }
    // SourceForge download page: <a class="direct-download" href="…"> or data-release-url.
    if host_is(base, "sourceforge.net") {
        for a in tags(html, "a") {
            let direct = attr(a, "class").is_some_and(|c| c.split_whitespace().any(|x| x == "direct-download")) || attr(a, "data-release-url").is_some();
            if direct {
                if let Some(h) = attr(a, "href").or_else(|| attr(a, "data-release-url")) {
                    if let Ok(u) = base.join(&decode_entities(h)) {
                        return Some(u.to_string());
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_share_links() {
        assert_eq!(
            rewrite("https://drive.google.com/file/d/1AbCdEfGhIjKlMnOpQrStUv/view?usp=sharing").as_deref(),
            Some("https://drive.usercontent.google.com/download?id=1AbCdEfGhIjKlMnOpQrStUv&export=download&confirm=t")
        );
        assert_eq!(rewrite("https://drive.google.com/open?id=1AbCdEfGhIjKlMnOpQrStUv").as_deref(), Some("https://drive.usercontent.google.com/download?id=1AbCdEfGhIjKlMnOpQrStUv&export=download&confirm=t"));
        assert_eq!(rewrite("https://www.dropbox.com/scl/fi/abc/file.zip?rlkey=xyz&dl=0").as_deref(), Some("https://www.dropbox.com/scl/fi/abc/file.zip?rlkey=xyz&dl=1"));
        assert_eq!(rewrite("https://www.dropbox.com/s/abc/file.zip?dl=1"), None);
        assert_eq!(
            rewrite("https://sourceforge.net/projects/sevenzip/files/7-Zip/24.08/7z2408-x64.exe/download").as_deref(),
            Some("https://downloads.sourceforge.net/project/sevenzip/7-Zip/24.08/7z2408-x64.exe")
        );
        assert_eq!(rewrite("https://github.com/o/r/blob/main/dist/app.zip").as_deref(), Some("https://raw.githubusercontent.com/o/r/main/dist/app.zip"));
        assert_eq!(rewrite("https://github.com/o/r/releases/download/v1/app.zip"), None);
        assert_eq!(rewrite("https://example.com/file.zip"), None);
    }

    #[test]
    fn finds_targets_in_pages() {
        let base = Url::parse("https://drive.usercontent.google.com/download?id=X&export=download").unwrap();
        let drive = r#"<html><form id="download-form" action="https://drive.usercontent.google.com/download" method="get">
            <input type="submit" value="Download anyway"><input type="hidden" name="id" value="1AbC"><input type="hidden" name="export" value="download">
            <input type="hidden" name="confirm" value="t"><input type="hidden" name="uuid" value="u-1"></form></html>"#;
        assert_eq!(find_in_html(&base, drive).as_deref(), Some("https://drive.usercontent.google.com/download?id=1AbC&export=download&confirm=t&uuid=u-1"));

        let base = Url::parse("https://mirror.example.org/get/file").unwrap();
        let meta = r#"<head><META HTTP-EQUIV="Refresh" CONTENT="3; URL=/files/app-1.2.tar.gz"></head>"#;
        assert_eq!(find_in_html(&base, meta).as_deref(), Some("https://mirror.example.org/files/app-1.2.tar.gz"));

        let base = Url::parse("https://sourceforge.net/projects/p/files/x.zip/download").unwrap();
        let sf = r#"<a href="https://downloads.sourceforge.net/project/p/x.zip?ts=1&amp;use_mirror=netix" class="button direct-download">direct link</a>"#;
        assert_eq!(find_in_html(&base, sf).as_deref(), Some("https://downloads.sourceforge.net/project/p/x.zip?ts=1&use_mirror=netix"));

        assert_eq!(find_in_html(&base, "<html><body>Just a page</body></html>"), None);
    }
}
