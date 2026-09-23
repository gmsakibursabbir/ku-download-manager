//! aria2 process supervision and JSON-RPC client.
//!
//! KuCore spawns a private aria2c bound to 127.0.0.1 with a random port and
//! secret. aria2 exits automatically if KuDownloader dies (`--stop-with-process`).

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};

pub struct Aria2 {
    endpoint: String,
    secret: String,
    http: reqwest::Client,
    pub version: String,
    pub binary: PathBuf,
}

#[derive(Debug)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "aria2 error {}: {}", self.code, self.message)
    }
}
impl std::error::Error for RpcError {}

pub fn is_gid_not_found(e: &anyhow::Error) -> bool {
    e.downcast_ref::<RpcError>().is_some_and(|r| r.message.contains("is not found"))
}

fn free_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

pub struct SpawnConfig<'a> {
    pub binary: &'a Path,
    pub data_dir: &'a Path,
    pub download_dir: &'a str,
    pub listen_port: &'a str,
    pub enable_dht: bool,
    pub max_peers: u32,
}

impl Aria2 {
    pub async fn spawn(cfg: SpawnConfig<'_>) -> Result<(Aria2, Child)> {
        let port = free_port()?;
        let secret = uuid::Uuid::new_v4().simple().to_string();
        std::fs::create_dir_all(cfg.data_dir).ok();
        let dht = cfg.data_dir.join("dht.dat");
        let dht6 = cfg.data_dir.join("dht6.dat");
        let mut cmd = Command::new(cfg.binary);
        cmd.args([
            "--enable-rpc=true",
            "--rpc-listen-all=false",
            "--rpc-allow-origin-all=false",
            &format!("--rpc-listen-port={port}"),
            &format!("--rpc-secret={secret}"),
            "--rpc-max-request-size=16M",
            &format!("--stop-with-process={}", std::process::id()),
            // KuCore owns queueing; aria2 runs whatever it is given.
            "--max-concurrent-downloads=128",
            "--continue=true",
            "--auto-save-interval=5",
            "--save-not-found=false",
            "--file-allocation=none",
            "--console-log-level=warn",
            "--summary-interval=0",
            "--show-console-readout=false",
            "--content-disposition-default-utf8=true",
            "--uri-selector=adaptive",
            "--min-split-size=1M",
            "--follow-torrent=true",
            "--follow-metalink=true",
            "--bt-save-metadata=false",
            "--bt-remove-unselected-file=true",
            "--disk-cache=32M",
            &format!("--bt-max-peers={}", cfg.max_peers),
            &format!("--listen-port={}", cfg.listen_port),
            &format!("--dht-listen-port={}", cfg.listen_port),
            &format!("--enable-dht={}", cfg.enable_dht),
            &format!("--dht-file-path={}", dht.display()),
            &format!("--dht-file-path6={}", dht6.display()),
            &format!("--dir={}", cfg.download_dir),
        ]);
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).kill_on_drop(true);
        hide_window(&mut cmd);
        let child = cmd.spawn().with_context(|| format!("starting {}", cfg.binary.display()))?;
        let aria = Aria2 {
            endpoint: format!("http://127.0.0.1:{port}/jsonrpc"),
            secret,
            http: reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(20)).build()?,
            version: String::new(),
            binary: cfg.binary.to_path_buf(),
        };
        let mut aria = aria;
        let mut last_err = None;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            match aria.call("aria2.getVersion", vec![]).await {
                Ok(v) => {
                    aria.version = v["version"].as_str().unwrap_or("").to_string();
                    return Ok((aria, child));
                }
                Err(e) => last_err = Some(e),
            }
        }
        Err(anyhow!("aria2 did not become ready: {}", last_err.map(|e| e.to_string()).unwrap_or_default()))
    }

    fn params(&self, mut params: Vec<Value>) -> Vec<Value> {
        params.insert(0, Value::String(format!("token:{}", self.secret)));
        params
    }

    pub async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value> {
        self.call_raw(method, self.params(params)).await
    }

    async fn call_raw(&self, method: &str, params: Vec<Value>) -> Result<Value> {
        let body = json!({"jsonrpc": "2.0", "id": "ku", "method": method, "params": params});
        let resp: Value = self.http.post(&self.endpoint).json(&body).send().await?.json().await?;
        if let Some(err) = resp.get("error") {
            return Err(RpcError {
                code: err["code"].as_i64().unwrap_or(0),
                message: err["message"].as_str().unwrap_or("").to_string(),
            }
            .into());
        }
        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Run many calls in one round trip; each entry is its own Result.
    pub async fn multicall(&self, calls: Vec<(&str, Vec<Value>)>) -> Result<Vec<Result<Value, RpcError>>> {
        if calls.is_empty() {
            return Ok(Vec::new());
        }
        let list: Vec<Value> = calls
            .into_iter()
            .map(|(m, p)| json!({"methodName": m, "params": self.params(p)}))
            .collect();
        // system.multicall itself takes no token; each inner call carries it.
        let res = self.call_raw("system.multicall", vec![Value::Array(list)]).await?;
        let arr = res.as_array().cloned().unwrap_or_default();
        Ok(arr
            .into_iter()
            .map(|item| match item {
                Value::Array(mut a) if !a.is_empty() => Ok(a.remove(0)),
                other => Err(RpcError {
                    code: other["code"].as_i64().unwrap_or(0),
                    message: other["message"].as_str().unwrap_or("unknown").to_string(),
                }),
            })
            .collect())
    }

    pub async fn add_uri(&self, uris: &[String], opts: Value) -> Result<String> {
        let v = self.call("aria2.addUri", vec![json!(uris), opts]).await?;
        v.as_str().map(str::to_string).ok_or_else(|| anyhow!("addUri returned no gid"))
    }

    pub async fn add_torrent(&self, b64: &str, opts: Value) -> Result<String> {
        let v = self.call("aria2.addTorrent", vec![json!(b64), json!([]), opts]).await?;
        v.as_str().map(str::to_string).ok_or_else(|| anyhow!("addTorrent returned no gid"))
    }

    pub async fn add_metalink(&self, b64: &str, opts: Value) -> Result<String> {
        let v = self.call("aria2.addMetalink", vec![json!(b64), opts]).await?;
        v.as_array()
            .and_then(|a| a.first())
            .and_then(|g| g.as_str())
            .map(str::to_string)
            .ok_or_else(|| anyhow!("addMetalink returned no gid"))
    }

    pub async fn tell_status(&self, gid: &str) -> Result<Value> {
        self.call("aria2.tellStatus", vec![json!(gid)]).await
    }

    pub async fn force_pause(&self, gid: &str) -> Result<()> {
        self.call("aria2.forcePause", vec![json!(gid)]).await.map(|_| ())
    }

    pub async fn unpause(&self, gid: &str) -> Result<()> {
        self.call("aria2.unpause", vec![json!(gid)]).await.map(|_| ())
    }

    /// Remove from aria2 entirely (active or stopped). Missing gids are fine.
    pub async fn purge(&self, gid: &str) {
        if self.call("aria2.forceRemove", vec![json!(gid)]).await.is_ok() {
            // forceRemove is asynchronous; give aria2 a moment to stop.
            for _ in 0..20 {
                tokio::time::sleep(Duration::from_millis(50)).await;
                match self.tell_status(gid).await {
                    Ok(s) if s["status"] == "active" || s["status"] == "paused" || s["status"] == "waiting" => continue,
                    _ => break,
                }
            }
        }
        let _ = self.call("aria2.removeDownloadResult", vec![json!(gid)]).await;
    }

    pub async fn change_option(&self, gid: &str, opts: Value) -> Result<()> {
        self.call("aria2.changeOption", vec![json!(gid), opts]).await.map(|_| ())
    }

    pub async fn change_global(&self, opts: Value) -> Result<()> {
        self.call("aria2.changeGlobalOption", vec![opts]).await.map(|_| ())
    }

    pub async fn shutdown(&self) {
        let _ = self.call("aria2.forceShutdown", vec![]).await;
    }
}

pub fn hide_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Human readable explanation of aria2 exit/error codes.
pub fn describe_error(code: i64, message: &str) -> String {
    let m = message.trim();
    let base = match code {
        2 => "The connection timed out.",
        3 => "The file was not found on the server.",
        4 => "The server reported the resource as not found too many times.",
        5 => "The download was aborted because the speed stayed below the minimum limit.",
        6 => "A network problem occurred.",
        7 => "The download was interrupted.",
        8 => "The server does not support resuming, and the download could not continue.",
        9 => "Not enough disk space.",
        10 => "The piece length differs from the control file.",
        11 => "The same file is already being downloaded.",
        12 => "The same torrent is already being downloaded.",
        13 => "A file with this name already exists.",
        14 => "Renaming the file failed.",
        15 => "Could not open the existing file.",
        16 => "Could not create or truncate the file.",
        17 => "A disk write or read error occurred.",
        18 => "Could not create the destination folder.",
        19 => "The server name could not be resolved (DNS).",
        20 => "The metalink document could not be parsed.",
        21 => "An FTP command failed.",
        22 => "The server sent an invalid HTTP response.",
        23 => "Too many redirects.",
        24 => "HTTP authentication failed.",
        25 => "The torrent file is malformed.",
        26 => "The .torrent file is corrupted or missing information.",
        27 => "The magnet link is invalid.",
        28 => "An option was invalid.",
        29 => "The server is overloaded or temporarily unavailable.",
        30 => "The request was malformed.",
        31 => "The download could not be verified.",
        32 => "Checksum verification failed: the file does not match the expected hash.",
        _ => "",
    };
    match (base.is_empty(), m.is_empty()) {
        (true, true) => format!("The download failed (aria2 code {code})."),
        (true, false) => m.to_string(),
        (false, true) => base.to_string(),
        (false, false) => format!("{base} {m}"),
    }
}

/// Transient errors worth retrying automatically.
pub fn is_retryable(code: i64) -> bool {
    matches!(code, 1 | 2 | 5 | 6 | 7 | 19 | 22 | 29)
}

pub fn parse_i64(v: &Value) -> i64 {
    match v {
        Value::String(s) => s.parse().unwrap_or(0),
        Value::Number(n) => n.as_i64().unwrap_or(0),
        _ => 0,
    }
}

pub fn new_gid() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..16].to_string()
}

pub fn ensure(cond: bool, msg: &str) -> Result<()> {
    if !cond {
        bail!("{msg}");
    }
    Ok(())
}
