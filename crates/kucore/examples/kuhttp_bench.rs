//! KuHTTP vs aria2 benchmark.
//!
//!   cargo run --release -p kucore --example kuhttp_bench -- [--quick] [--out FILE]
//!
//! The HTTP server runs in a separate process (this binary with `--serve`),
//! so neither engine is charged for serving CPU. Content is generated and
//! every downloaded file is verified byte-for-byte after each run.

use kucore::aria2::{parse_i64, Aria2, SpawnConfig};
use kucore::kuhttp::testing::{file_matches, TestServer};
use kucore::kuhttp::{DownloadRequest, KuHttpConfig, KuHttpEngine, State};
use serde_json::json;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const MB: u64 = 1024 * 1024;

#[derive(Clone)]
struct Case {
    scenario: &'static str,
    size: u64,
    rate: u64,
    fail_ppm: u64,
    engine: &'static str,
    /// 0 = adaptive (KuHTTP) ; for aria2 the split/connection count.
    conns: u32,
    /// Experiment: (sparse temp file, persist interval seconds).
    variant: Option<(bool, u64)>,
}

struct Result_ {
    case: Case,
    /// All bytes received.
    transfer: f64,
    /// Data durable on disk (KuHTTP: COMPLETED; aria2: complete + fsync).
    secs: f64,
    avg: f64,
    peak: f64,
    cpu: f64,
    rss: u64,
    peak_conns: u32,
    retries: String,
    ok: bool,
    note: String,
    runs: usize,
    range: (f64, f64),
}

/// Peak throughput = most bytes transferred in any 1 s window.
#[derive(Default)]
struct Peak(std::collections::VecDeque<(Instant, u64)>, f64);
impl Peak {
    fn add(&mut self, now: Instant, bytes: u64) {
        self.0.push_back((now, bytes));
        while self.0.len() > 1 && now.duration_since(self.0[0].0) > Duration::from_millis(1000) {
            let (t, b) = self.0.pop_front().unwrap();
            let dt = now.duration_since(t).as_secs_f64();
            if dt >= 0.95 {
                self.1 = self.1.max((bytes - b) as f64 / dt);
            }
        }
    }
}

struct Sampler {
    sys: System,
    pid: Pid,
    cpu0: u64,
    rss0: u64,
    peak_rss: u64,
}

impl Sampler {
    fn new(pid: u32) -> Sampler {
        let mut sys = System::new();
        let pid = Pid::from_u32(pid);
        sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::everything());
        let (cpu0, rss0) = sys.process(pid).map(|p| (p.accumulated_cpu_time(), p.memory())).unwrap_or((0, 0));
        Sampler { sys, pid, cpu0, rss0, peak_rss: rss0 }
    }
    fn sample(&mut self) {
        self.sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[self.pid]), true, ProcessRefreshKind::everything());
        if let Some(p) = self.sys.process(self.pid) {
            self.peak_rss = self.peak_rss.max(p.memory());
        }
    }
    /// (cpu seconds used since creation, peak RSS, baseline RSS)
    fn finish(&mut self) -> (f64, u64, u64) {
        self.sample();
        let cpu = self.sys.process(self.pid).map(|p| p.accumulated_cpu_time()).unwrap_or(self.cpu0);
        ((cpu.saturating_sub(self.cpu0)) as f64 / 1000.0, self.peak_rss, self.rss0)
    }
}

fn name(c: &Case, run: u64) -> String {
    format!("dyn-{}-{}-{}-{}.bin", c.size, 7000 + run, c.rate, c.fail_ppm)
}

async fn run_kuhttp(c: &Case, base: &str, dir: &Path, run: u64) -> Result_ {
    let mut cfg = KuHttpConfig { max_connections: 16, progress_interval: Duration::from_millis(100), ..KuHttpConfig::default() };
    if let Some(v) = c.variant {
        cfg.sparse_files = v.0;
        cfg.persist_interval = Duration::from_secs(v.1);
    }
    let engine = KuHttpEngine::new(cfg).unwrap();
    let file = name(c, run);
    let req = DownloadRequest {
        id: file.clone(),
        url: format!("{base}/r/{file}"),
        dest_dir: dir.to_path_buf(),
        connections: (c.conns > 0).then_some(c.conns),
        ..Default::default()
    };
    let mut s = Sampler::new(std::process::id());
    let t = Instant::now();
    engine.download(req).unwrap();
    let (mut peak, mut peak_conns, mut transfer) = (Peak::default(), 0u32, None);
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        s.sample();
        let st = engine.status(&file).unwrap();
        peak.add(Instant::now(), st.downloaded);
        peak_conns = peak_conns.max(st.connections);
        if transfer.is_none() && st.downloaded >= c.size {
            transfer = Some(t.elapsed().as_secs_f64());
        }
        if st.state.is_terminal() {
            break;
        }
    }
    let secs = t.elapsed().as_secs_f64();
    let st = engine.status(&file).unwrap();
    let (cpu, rss, rss0) = s.finish();
    let path = dir.join(&file);
    let ok = st.state == State::Completed && file_matches(&path, 7000 + run, c.size);
    let _ = std::fs::remove_file(&path);
    Result_ {
        case: c.clone(),
        transfer: transfer.unwrap_or(secs),
        secs,
        avg: c.size as f64 / secs,
        peak: peak.1,
        cpu,
        rss: rss.saturating_sub(rss0),
        peak_conns,
        retries: st.retries.to_string(),
        ok,
        note: if st.state == State::Completed { String::new() } else { format!("{:?}: {:?}", st.state, st.error) },
        runs: 1,
        range: (secs, secs),
    }
}

async fn run_aria2(c: &Case, base: &str, dir: &Path, bin: &Path, run: u64) -> Result_ {
    let data = dir.join("aria2-data");
    let (a, child) = Aria2::spawn(SpawnConfig {
        binary: bin,
        data_dir: &data,
        download_dir: &dir.to_string_lossy(),
        listen_port: "6881-6999",
        enable_dht: false,
        max_peers: 55,
    })
    .await
    .unwrap();
    let pid = child.id().unwrap();
    let file = name(c, run);
    let n = c.conns.max(1);
    let gid = a
        .add_uri(
            &[format!("{base}/r/{file}")],
            json!({
                "dir": dir.to_string_lossy(), "out": file,
                "split": n.to_string(), "max-connection-per-server": n.min(16).to_string(),
                "min-split-size": "1M", "max-tries": "20", "retry-wait": "1",
            }),
        )
        .await
        .unwrap();
    let mut s = Sampler::new(pid);
    let t = Instant::now();
    let (mut peak, mut peak_conns, mut status);
    peak = Peak::default();
    peak_conns = 0u32;
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        s.sample();
        // aria2 can be slow to answer under heavy I/O; keep polling.
        let Ok(v) = a.tell_status(&gid).await else { continue };
        peak.add(Instant::now(), parse_i64(&v["completedLength"]) as u64);
        peak_conns = peak_conns.max(parse_i64(&v["connections"]) as u32);
        status = v["status"].as_str().unwrap_or("").to_string();
        if status == "complete" || status == "error" {
            break;
        }
    }
    let transfer = t.elapsed().as_secs_f64();
    let (cpu, rss, _) = s.finish();
    a.shutdown().await;
    drop(child);
    let path = dir.join(&file);
    // Same durability guarantee as KuHTTP: data flushed to disk.
    if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&path) {
        let _ = f.sync_all();
    }
    let secs = t.elapsed().as_secs_f64();
    let ok = status == "complete" && file_matches(&path, 7000 + run, c.size);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(dir.join(format!("{file}.aria2")));
    Result_ {
        case: c.clone(),
        transfer,
        secs,
        avg: c.size as f64 / secs,
        peak: peak.1,
        cpu,
        rss,
        peak_conns,
        retries: "n/a".into(),
        ok,
        note: if status == "complete" { String::new() } else { status },
        runs: 1,
        range: (secs, secs),
    }
}

fn median(mut runs: Vec<Result_>) -> Result_ {
    let ok = runs.iter().all(|r| r.ok);
    let (lo, hi) = runs.iter().fold((f64::MAX, 0f64), |(a, b), r| (a.min(r.secs), b.max(r.secs)));
    let n = runs.len();
    runs.sort_by(|a, b| a.secs.partial_cmp(&b.secs).unwrap());
    let rss = runs.iter().map(|r| r.rss).max().unwrap_or(0);
    let mut m = runs.swap_remove(n / 2);
    m.ok = ok;
    m.rss = rss;
    m.runs = n;
    m.range = (lo, hi);
    m
}

fn mbps(b: f64) -> String {
    format!("{:.0}", b / MB as f64)
}

fn size_label(b: u64) -> String {
    if b >= 1024 * MB {
        format!("{} GB", b / (1024 * MB))
    } else {
        format!("{} MB", b / MB)
    }
}

/// Kills the server process however the benchmark ends (including panics).
struct ServerGuard(std::process::Child);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn serve_forever() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let srv = TestServer::start().await;
        println!("PORT {}", srv.port);
        std::io::stdout().flush().unwrap();
        std::future::pending::<()>().await;
    });
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--serve") {
        return serve_forever();
    }
    let quick = args.iter().any(|a| a == "--quick");
    let out = args.windows(2).find(|w| w[0] == "--out").map(|w| PathBuf::from(&w[1]));
    let aria2 = kucore::ku_proto::paths::find_binary("aria2c", None).expect("aria2c is required for the comparison");

    // Out-of-process server.
    let mut server = Command::new(std::env::current_exe().unwrap()).arg("--serve").stdout(Stdio::piped()).spawn().unwrap();
    let mut line = String::new();
    std::io::BufReader::new(server.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let port: u16 = line.trim().trim_start_matches("PORT ").parse().unwrap();
    let _server = ServerGuard(server);
    let base = format!("http://127.0.0.1:{port}");
    let dir = std::env::temp_dir().join(format!("kuhttp-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let mut cases = Vec::new();
    if args.iter().any(|a| a == "--experiment") {
        for (sparse, persist) in [(true, 3), (false, 3), (true, 30), (false, 30)] {
            cases.push(Case { scenario: "X · storage experiment, 1 GB", size: 1024 * MB, rate: 0, fail_ppm: 0, engine: "KuHTTP", conns: 4, variant: Some((sparse, persist)) });
        }
        cases.push(Case { scenario: "X · storage experiment, 1 GB", size: 1024 * MB, rate: 0, fail_ppm: 0, engine: "aria2", conns: 4, variant: None });
    }
    let experiment = !cases.is_empty();
    let sizes: &[u64] = if experiment { &[] } else if quick { &[50 * MB, 500 * MB] } else { &[50 * MB, 500 * MB, 1024 * MB, 5 * 1024 * MB, 10 * 1024 * MB] };
    for &size in sizes {
        cases.push(Case { scenario: "A · loopback, unthrottled", size, rate: 0, fail_ppm: 0, engine: "KuHTTP", conns: 0, variant: None });
        cases.push(Case { scenario: "A · loopback, unthrottled", size, rate: 0, fail_ppm: 0, engine: "aria2", conns: 16, variant: None });
    }
    let fixed_size = if quick { 500 * MB } else { 1024 * MB };
    for n in [1, 2, 4, 8] {
        cases.push(Case { scenario: "B · fixed connections, unthrottled", size: fixed_size, rate: 0, fail_ppm: 0, engine: "KuHTTP", conns: n, variant: None });
        cases.push(Case { scenario: "B · fixed connections, unthrottled", size: fixed_size, rate: 0, fail_ppm: 0, engine: "aria2", conns: n, variant: None });
    }
    let capped = if quick { 200 * MB } else { 500 * MB };
    for n in [1, 2, 4, 8, 0] {
        cases.push(Case { scenario: "C · server caps each connection at 10 MB/s", size: capped, rate: 10 * MB, fail_ppm: 0, engine: "KuHTTP", conns: n, variant: None });
    }
    for n in [1, 2, 4, 8, 16] {
        cases.push(Case { scenario: "C · server caps each connection at 10 MB/s", size: capped, rate: 10 * MB, fail_ppm: 0, engine: "aria2", conns: n, variant: None });
    }
    let faulty = if quick { 200 * MB } else { 500 * MB };
    cases.push(Case { scenario: "D · 0.2 % of 64 KiB chunks drop the connection", size: faulty, rate: 0, fail_ppm: 2000, engine: "KuHTTP", conns: 0, variant: None });
    cases.push(Case { scenario: "D · 0.2 % of 64 KiB chunks drop the connection", size: faulty, rate: 0, fail_ppm: 2000, engine: "aria2", conns: 8, variant: None });

    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let reps: usize = args.windows(2).find(|w| w[0] == "--reps").and_then(|w| w[1].parse().ok()).unwrap_or(3);
    let mut results = Vec::new();
    for (i, c) in cases.iter().enumerate() {
        // Files above 1 GB run once; everything else is repeated (median reported).
        let n = if c.size > 1024 * MB { 1 } else { reps };
        let mut runs = Vec::new();
        for k in 0..n {
            let id = (i * 10 + k) as u64;
            runs.push(rt.block_on(async {
                if c.engine == "KuHTTP" {
                    run_kuhttp(c, &base, &dir, id).await
                } else {
                    run_aria2(c, &base, &dir, &aria2, id).await
                }
            }));
        }
        let r = median(runs);
        eprintln!(
            "{:<48} {:>6} {:<7} conns={:<8} xfer {:>6.2}s durable {:>7.2}s avg {:>5} MB/s peak {:>5} MB/s cpu {:>6.2}s rss {:>4} MB ok={} {}",
            c.scenario,
            size_label(c.size),
            c.engine,
            if c.conns == 0 { "adaptive".to_string() } else { c.conns.to_string() },
            r.transfer,
            r.secs,
            mbps(r.avg),
            mbps(r.peak),
            r.cpu,
            r.rss / MB,
            r.ok,
            r.note
        );
        results.push(r);
    }
    let _ = std::fs::remove_dir_all(&dir);

    let mut md = String::new();
    md.push_str("# KuHTTP vs aria2 benchmark\n\n");
    md.push_str(&format!(
        "Generated by `cargo run --release -p kucore --example kuhttp_bench{}` on {} ({} logical CPUs), aria2 {}.\n\n",
        if quick { " -- --quick" } else { "" },
        std::env::consts::OS,
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        Command::new(&aria2).arg("--version").output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).and_then(|s| s.lines().next().map(|l| l.replace("aria2 version ", ""))).unwrap_or_default()
    ));
    md.push_str("Local HTTP/1.1 server in a separate process over loopback; content generated on the fly; every file verified byte-for-byte. CPU = process CPU time during the transfer; RSS = peak resident memory (KuHTTP: growth of the benchmark process, aria2: its process).\n\n");
    let mut scenario = "";
    for r in &results {
        if r.case.scenario != scenario {
            scenario = r.case.scenario;
            md.push_str(&format!("\n## {scenario}\n\n| Size | Engine | Connections | Runs | Transfer (s) | Durable (s), median | Range (s) | Avg MB/s | Peak MB/s | CPU (s) | RSS (MB) | Peak conns | Retries | Verified |\n|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|\n"));
        }
        md.push_str(&format!(
            "| {} | {} | {} | {} | {:.2} | {:.2} | {:.2}–{:.2} | {} | {} | {:.2} | {} | {} | {} | {}{} |\n",
            size_label(r.case.size),
            r.case.engine,
            if r.case.conns == 0 { "adaptive".into() } else { r.case.conns.to_string() },
            r.runs,
            r.transfer,
            r.secs,
            r.range.0,
            r.range.1,
            mbps(r.avg),
            mbps(r.peak),
            r.cpu,
            r.rss / MB,
            r.peak_conns,
            r.retries,
            if r.ok { "✓" } else { "✗" },
            if r.note.is_empty() { String::new() } else { format!(" {}", r.note) }
        ));
    }
    match out {
        Some(p) => {
            std::fs::write(&p, md).unwrap();
            eprintln!("wrote {}", p.display());
        }
        None => println!("{md}"),
    }
}
