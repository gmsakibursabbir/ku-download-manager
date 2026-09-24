//! Native messaging host: the only bridge between the browser extension and
//! KuCore. It speaks the WebExtension length-prefixed JSON protocol on stdio
//! and forwards a fixed whitelist of requests to the local API.
//!
//! It never executes anything the browser sends; the only process it can
//! start is the KuDownloader app itself (when a download is requested and the
//! app is not running).

use ku_proto::client::{Client, ClientError};
use ku_proto::{launcher, AddRequest, GrabRequest, MediaRequest};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, Read, Write};
use std::time::Duration;

const MAX_INCOMING: usize = 32 * 1024 * 1024;
const MAX_OUTGOING: usize = 1024 * 1024 - 64;

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    id: Value,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    payload: Value,
}

fn read_message(r: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let n = u32::from_ne_bytes(len) as usize;
    if n > MAX_INCOMING {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message too large"));
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(Some(buf))
}

fn write_message(w: &mut impl Write, v: &Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(v)?;
    if bytes.len() > MAX_OUTGOING {
        bytes = serde_json::to_vec(&json!({"id": v["id"], "ok": false, "error": "response too large"}))?;
    }
    w.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    w.write_all(&bytes)?;
    w.flush()
}

fn url_ok(u: &str) -> bool {
    let l = u.trim().to_ascii_lowercase();
    ["http://", "https://", "ftp://", "sftp://", "magnet:"].iter().any(|p| l.starts_with(p))
}

fn connect(launch: bool) -> Result<Client, String> {
    let r = if launch { launcher::ensure_running(Duration::from_secs(20)) } else { Client::discover() };
    r.map_err(|e| match e {
        ClientError::NotRunning => "KuDownloader is not running".to_string(),
        other => other.to_string(),
    })
}

/// What a browser may not decide: where files go, the proxy, local torrent
/// or metalink data. Those come from the user (settings or the confirm window).
fn from_browser(mut req: AddRequest) -> AddRequest {
    req.dir = None;
    req.options.proxy = None;
    req.options.torrent_data = None;
    req.options.metalink_data = None;
    req.options.select_files = None;
    req
}

fn handle(kind: &str, payload: Value) -> Result<Value, String> {
    let e = |e: ClientError| e.to_string();
    match kind {
        "ping" => {
            let running = Client::discover().is_ok();
            Ok(json!({"host": env!("CARGO_PKG_VERSION"), "running": running}))
        }
        "config" => match connect(false) {
            Ok(c) => c.get::<Value>("/v1/config/browser").map_err(e).map(|mut v| {
                v["running"] = json!(true);
                v
            }),
            Err(_) => Ok(json!({"running": false})),
        },
        "stats" => connect(false)?.get::<Value>("/v1/stats").map_err(e),
        "add" => {
            let req = from_browser(serde_json::from_value(payload).map_err(|e| format!("invalid request: {e}"))?);
            if !url_ok(&req.url) {
                return Err("unsupported URL scheme".into());
            }
            connect(true)?.post("/v1/downloads", &req).map_err(e)
        }
        "addBatch" => {
            let urls: Vec<String> = serde_json::from_value(payload["urls"].clone()).map_err(|e| e.to_string())?;
            let urls: Vec<String> = urls.into_iter().filter(|u| url_ok(u)).collect();
            let template = from_browser(serde_json::from_value(payload["template"].clone()).unwrap_or_default());
            connect(true)?.post("/v1/downloads/batch", &json!({"urls": urls, "template": template})).map_err(e)
        }
        "analyze" => {
            let url = payload["url"].as_str().unwrap_or("");
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err("unsupported URL scheme".into());
            }
            connect(true)?.with_timeout(Duration::from_secs(150)).post("/v1/media/analyze", &payload).map_err(e)
        }
        "mediaDownload" => {
            let mut req: MediaRequest = serde_json::from_value(payload).map_err(|e| format!("invalid request: {e}"))?;
            req.dir = None;
            if !req.url.starts_with("http://") && !req.url.starts_with("https://") {
                return Err("unsupported URL scheme".into());
            }
            connect(true)?.post("/v1/media/download", &req).map_err(e)
        }
        "grab" => {
            let mut req: GrabRequest = serde_json::from_value(payload).map_err(|e| format!("invalid request: {e}"))?;
            req.links.retain(|l| url_ok(&l.url));
            connect(true)?.post("/v1/grab", &req).map_err(e)
        }
        "show" => connect(true)?.post("/v1/ui/show", &payload).map_err(e),
        other => Err(format!("unknown request type \"{other}\"")),
    }
}

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    loop {
        let raw = match read_message(&mut input) {
            Ok(Some(r)) => r,
            Ok(None) => return,
            Err(_) => return,
        };
        let reply = match serde_json::from_slice::<Message>(&raw) {
            Ok(m) => match handle(&m.kind, m.payload) {
                Ok(data) => json!({"id": m.id, "ok": true, "data": data}),
                Err(err) => json!({"id": m.id, "ok": false, "error": err}),
            },
            Err(err) => json!({"id": null, "ok": false, "error": format!("malformed message: {err}")}),
        };
        if write_message(&mut output, &reply).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_roundtrip() {
        let mut buf = Vec::new();
        write_message(&mut buf, &json!({"a": 1})).unwrap();
        let mut r = &buf[..];
        let msg = read_message(&mut r).unwrap().unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&msg).unwrap(), json!({"a": 1}));
        assert!(read_message(&mut r).unwrap().is_none());
    }

    #[test]
    fn rejects_bad_schemes() {
        assert!(!url_ok("file:///etc/passwd"));
        assert!(!url_ok("javascript:alert(1)"));
        assert!(url_ok("magnet:?xt=urn:btih:x"));
        assert!(handle("add", json!({"url": "file:///c:/x"})).is_err());
        let req: AddRequest = serde_json::from_value(json!({"url": "https://x/a", "dir": "/etc/startup", "options": {"proxy": "http://evil:1", "torrentData": "AA=="}})).unwrap();
        let req = from_browser(req);
        assert!(req.dir.is_none() && req.options.proxy.is_none() && req.options.torrent_data.is_none());
        assert!(handle("nope", Value::Null).is_err());
    }
}
