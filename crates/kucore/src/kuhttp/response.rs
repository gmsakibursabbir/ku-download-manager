//! Strict validation of range responses. Nothing is written to disk unless
//! the response is exactly the bytes that were requested.

use super::client::status_error;
use super::error::KuError;
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE};
use reqwest::{Response, StatusCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentRange {
    pub start: u64,
    /// Inclusive.
    pub end: u64,
    pub total: Option<u64>,
}

/// Parse `bytes START-END/TOTAL` (TOTAL may be `*`).
pub fn parse_content_range(v: &str) -> Option<ContentRange> {
    let rest = v.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (a, b) = range.trim().split_once('-')?;
    let start: u64 = a.trim().parse().ok()?;
    let end: u64 = b.trim().parse().ok()?;
    let total = match total.trim() {
        "*" => None,
        t => Some(t.parse().ok()?),
    };
    if end < start || total.is_some_and(|t| end >= t) {
        return None;
    }
    Some(ContentRange { start, end, total })
}

/// `bytes */TOTAL` from a 416 response.
pub fn parse_unsatisfied_total(v: &str) -> Option<u64> {
    v.trim().strip_prefix("bytes")?.trim().strip_prefix("*/")?.trim().parse().ok()
}

/// Validate a response to `Range: bytes=start-end` (inclusive end) of a
/// representation of `total` bytes. Returns the range the body will cover
/// (servers may legally return less than requested).
pub fn validate_partial(resp: &Response, start: u64, end: u64, total: u64) -> Result<ContentRange, KuError> {
    let st = resp.status();
    if st == StatusCode::OK {
        return Err(KuError::RangeIgnored);
    }
    if st != StatusCode::PARTIAL_CONTENT {
        return Err(status_error(resp));
    }
    let h = resp.headers();
    if h.get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|c| c.to_ascii_lowercase().starts_with("multipart/byteranges")) {
        return Err(KuError::BadContentRange("multipart response".into()));
    }
    let raw = h.get(CONTENT_RANGE).and_then(|v| v.to_str().ok()).ok_or_else(|| KuError::BadContentRange("missing Content-Range".into()))?;
    let cr = parse_content_range(raw).ok_or_else(|| KuError::BadContentRange(format!("unparsable \"{raw}\"")))?;
    match cr.total {
        Some(t) if t != total => return Err(KuError::ResourceChanged(format!("size changed from {total} to {t} bytes"))),
        None => return Err(KuError::BadContentRange("total size missing".into())),
        _ => {}
    }
    if cr.start != start {
        return Err(KuError::BadContentRange(format!("asked for {start}-{end}, got {}-{}", cr.start, cr.end)));
    }
    if cr.end > end {
        return Err(KuError::BadContentRange(format!("asked for {start}-{end}, got {}-{}", cr.start, cr.end)));
    }
    if let Some(len) = h.get(CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()) {
        if len != cr.end - cr.start + 1 {
            return Err(KuError::BadContentRange(format!("Content-Length {len} does not match range {}-{}", cr.start, cr.end)));
        }
    }
    Ok(cr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_range() {
        assert_eq!(parse_content_range("bytes 0-0/100"), Some(ContentRange { start: 0, end: 0, total: Some(100) }));
        assert_eq!(parse_content_range("bytes 10-19/*"), Some(ContentRange { start: 10, end: 19, total: None }));
        assert_eq!(parse_content_range("bytes 20-10/100"), None);
        assert_eq!(parse_content_range("bytes 0-100/100"), None, "end beyond total");
        assert_eq!(parse_content_range("items 0-1/2"), None);
        assert_eq!(parse_content_range("bytes x-1/2"), None);
        assert_eq!(parse_unsatisfied_total("bytes */0"), Some(0));
    }
}
