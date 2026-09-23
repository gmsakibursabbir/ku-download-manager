//! Minimal blocking HTTP/1.1 client for the loopback-only local API.
//! Avoids pulling a full HTTP stack into the CLI and the native host.

use crate::{paths, ApiInfo};
use serde::{de::DeserializeOwned, Serialize};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

#[derive(Debug)]
pub enum ClientError {
    /// The app is not running (no api.json or nothing listening).
    NotRunning,
    Http(u16, String),
    Io(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::NotRunning => write!(f, "KuDownloader is not running"),
            ClientError::Http(code, msg) => write!(f, "{msg} (HTTP {code})"),
            ClientError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ClientError {}

pub fn read_api_info() -> Option<ApiInfo> {
    let text = std::fs::read_to_string(paths::api_file()).ok()?;
    serde_json::from_str(&text).ok()
}

#[derive(Clone)]
pub struct Client {
    info: ApiInfo,
    timeout: Duration,
}

impl Client {
    /// Connect using `api.json`; fails fast with `NotRunning`.
    pub fn discover() -> Result<Client, ClientError> {
        let info = read_api_info().ok_or(ClientError::NotRunning)?;
        let c = Client { info, timeout: Duration::from_secs(90) };
        c.get::<serde_json::Value>("/v1/health").map_err(|_| ClientError::NotRunning)?;
        Ok(c)
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        self.request("GET", path, None)
    }

    pub fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T, ClientError> {
        let body = serde_json::to_vec(body).map_err(|e| ClientError::Io(e.to_string()))?;
        self.request("POST", path, Some(body))
    }

    pub fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        self.request("DELETE", path, None)
    }

    fn request<T: DeserializeOwned>(&self, method: &str, path: &str, body: Option<Vec<u8>>) -> Result<T, ClientError> {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, self.info.port));
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
            .map_err(|_| ClientError::NotRunning)?;
        stream.set_read_timeout(Some(self.timeout)).ok();
        stream.set_write_timeout(Some(Duration::from_secs(10))).ok();
        let body = body.unwrap_or_default();
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\n\
             Content-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
            port = self.info.port,
            token = self.info.token,
            len = body.len()
        );
        let io = |e: std::io::Error| ClientError::Io(e.to_string());
        stream.write_all(head.as_bytes()).map_err(io)?;
        stream.write_all(&body).map_err(io)?;
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).map_err(io)?;
        let (status, body) = parse_response(&raw).ok_or_else(|| ClientError::Io("malformed response".into()))?;
        if !(200..300).contains(&status) {
            let msg = serde_json::from_slice::<crate::ApiError>(&body)
                .map(|e| e.error)
                .unwrap_or_else(|_| String::from_utf8_lossy(&body).into_owned());
            return Err(ClientError::Http(status, msg));
        }
        let body = if body.is_empty() { b"null".to_vec() } else { body };
        serde_json::from_slice(&body).map_err(|e| ClientError::Io(format!("invalid response: {e}")))
    }
}

fn parse_response(raw: &[u8]) -> Option<(u16, Vec<u8>)> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..split]).ok()?;
    let body = &raw[split + 4..];
    let mut lines = head.split("\r\n");
    let status: u16 = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let chunked = lines.any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("transfer-encoding:") && l.contains("chunked")
    });
    Some((status, if chunked { dechunk(body)? } else { body.to_vec() }))
}

fn dechunk(mut body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let eol = body.windows(2).position(|w| w == b"\r\n")?;
        let size_str = std::str::from_utf8(&body[..eol]).ok()?;
        let size = usize::from_str_radix(size_str.split(';').next()?.trim(), 16).ok()?;
        body = &body[eol + 2..];
        if size == 0 {
            return Some(out);
        }
        out.extend_from_slice(body.get(..size)?);
        body = body.get(size + 2..)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_chunked() {
        let raw = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}";
        assert_eq!(parse_response(raw), Some((200, b"{}".to_vec())));
        let raw = b"HTTP/1.1 404 Not Found\r\ntransfer-encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n";
        assert_eq!(parse_response(raw), Some((404, b"abcde".to_vec())));
    }
}
