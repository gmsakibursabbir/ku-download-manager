//! KuHTTP failure-injection suite. The central property checked everywhere:
//! a download reported COMPLETED is byte-identical to the source; anything
//! else never produces the final file.

use kucore::kuhttp::testing::{file_matches, Resource, TestServer};
use kucore::kuhttp::{DownloadRequest, KuEvent, KuHttpConfig, KuHttpEngine, State, Status};
use sha2::Digest;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MB: u64 = 1024 * 1024;

fn cfg() -> KuHttpConfig {
    KuHttpConfig {
        small_file_threshold: MB,
        min_segment_size: 512 * 1024,
        write_buffer: 64 * 1024,
        max_retries: 6,
        initial_backoff: Duration::from_millis(50),
        max_backoff: Duration::from_millis(400),
        read_timeout: Duration::from_secs(2),
        connect_timeout: Duration::from_secs(5),
        progress_interval: Duration::from_millis(100),
        persist_interval: Duration::from_millis(250),
        ..KuHttpConfig::default()
    }
}

fn req(id: &str, url: String, dir: &Path) -> DownloadRequest {
    DownloadRequest { id: id.into(), url, dest_dir: dir.to_path_buf(), ..Default::default() }
}

async fn run(engine: &KuHttpEngine, r: DownloadRequest) -> Status {
    let id = r.id.clone();
    engine.download(r).unwrap();
    tokio::time::timeout(Duration::from_secs(180), engine.wait(&id)).await.expect("download timed out").unwrap()
}

fn final_path(st: &Status) -> PathBuf {
    PathBuf::from(st.final_path.clone().expect("final path"))
}

fn assert_complete(st: &Status, seed: u64, len: u64) {
    assert_eq!(st.state, State::Completed, "error: {:?} message: {:?}", st.error, st.message);
    assert!(file_matches(&final_path(st), seed, len), "final file differs from the source");
}

/// Never a final file unless completed.
fn assert_no_final(dir: &Path, name: &str) {
    assert!(!dir.join(name).exists(), "{name} must not exist when not completed");
}

async fn wait_progress(engine: &KuHttpEngine, id: &str, min: u64) {
    let start = Instant::now();
    while engine.status(id).map(|s| s.downloaded).unwrap_or(0) < min {
        assert!(start.elapsed() < Duration::from_secs(60), "no progress: {:?}", engine.status(id));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn setup() -> (TestServer, tempfile::TempDir, KuHttpEngine) {
    (TestServer::start().await, tempfile::tempdir().unwrap(), KuHttpEngine::new(cfg()).unwrap())
}

// 1. Normal 200 response (server without ranges at all).
#[tokio::test(flavor = "multi_thread")]
async fn t01_plain_200() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(3 * MB + 17, 1);
    r.ranges = false;
    r.advertise_ranges = false;
    srv.state.set("a.bin", r);
    let st = run(&e, req("1", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 1, 3 * MB + 17);
    assert!(!st.segmented);
}

// 2. Range 206 → parallel segments.
#[tokio::test(flavor = "multi_thread")]
async fn t02_segmented() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(32 * MB, 2);
    r.per_conn_rate = 16 * MB;
    srv.state.set("a.bin", r);
    let st = run(&e, req("2", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 2, 32 * MB);
    assert!(st.segmented);
    assert!(srv.state.peak_active.load(Ordering::SeqCst) >= 2, "used several connections");
    assert!(!dir.path().join("a.bin.kudownload").exists() && !dir.path().join("a.bin.kudownload.json").exists());
}

// 3. Server that advertises ranges but ignores them.
#[tokio::test(flavor = "multi_thread")]
async fn t03_accept_ranges_liar() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(12 * MB, 3);
    r.ranges = false;
    r.advertise_ranges = true;
    srv.state.set("a.bin", r);
    let st = run(&e, req("3", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 3, 12 * MB);
    assert!(!st.segmented, "the real probe must detect missing range support");
}

// 4. Slow connection.
#[tokio::test(flavor = "multi_thread")]
async fn t04_slow_connection() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(4 * MB, 4);
    r.per_conn_rate = 2 * MB;
    srv.state.set("a.bin", r);
    let st = run(&e, req("4", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 4, 4 * MB);
}

// 5. One slow segment: idle workers steal its work.
#[tokio::test(flavor = "multi_thread")]
async fn t05_one_slow_segment() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(48 * MB, 5);
    r.slow_start = Some((12 * MB, 256 * 1024)); // would take 48 s alone
    srv.state.set("a.bin", r);
    let t = Instant::now();
    let st = run(&e, DownloadRequest { connections: Some(4), ..req("5", srv.url("/r/a.bin"), dir.path()) }).await;
    assert_complete(&st, 5, 48 * MB);
    assert!(t.elapsed() < Duration::from_secs(12), "slow segment not mitigated: {:?}", t.elapsed());
    let steals = srv.state.log.lock().unwrap().iter().filter(|l| l.range.as_deref().is_some_and(|r| {
        let a: u64 = r.trim_start_matches("bytes=").split('-').next().unwrap().parse().unwrap();
        a > 12 * MB && a < 24 * MB
    })).count();
    assert!(steals >= 1, "work was redistributed");
}

// 6. Random connection failures.
#[tokio::test(flavor = "multi_thread")]
async fn t06_random_failures() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(24 * MB, 6);
    r.fail_prob = 0.03; // ~11 dropped connections expected
    srv.state.set("a.bin", r);
    let st = run(&e, req("6", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 6, 24 * MB);
    assert!(st.retries > 0, "failures were injected and retried");
}

// 7. Timeout: the server stalls mid-body.
#[tokio::test(flavor = "multi_thread")]
async fn t07_stall_timeout() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(6 * MB, 7);
    r.stall_next = 2;
    r.stall_bytes = MB;
    srv.state.set("a.bin", r);
    let st = run(&e, req("7", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 7, 6 * MB);
}

// 8. 429 with Retry-After.
#[tokio::test(flavor = "multi_thread")]
async fn t08_429() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(8 * MB, 8);
    r.statuses = [429, 429, 200, 429].into_iter().filter(|s| *s != 200).collect();
    srv.state.set("a.bin", r);
    let st = run(&e, req("8", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 8, 8 * MB);
}

// 9. 503 + connection limit (server refuses extra connections).
#[tokio::test(flavor = "multi_thread")]
async fn t09_503_connection_limit() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(32 * MB, 9);
    r.max_concurrent = 2;
    r.per_conn_rate = 8 * MB;
    srv.state.set("a.bin", r);
    let st = run(&e, DownloadRequest { connections: None, ..req("9", srv.url("/r/a.bin"), dir.path()) }).await;
    assert_complete(&st, 9, 32 * MB);
}

// 10. Incorrect Content-Range is never written; falls back to one stream.
#[tokio::test(flavor = "multi_thread")]
async fn t10_bad_content_range() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(16 * MB, 10);
    r.bad_content_range = true;
    srv.state.set("a.bin", r);
    let st = run(&e, req("10", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 10, 16 * MB);
    assert!(!st.segmented, "fell back to single-stream");
}

// 11. Truncated responses.
#[tokio::test(flavor = "multi_thread")]
async fn t11_truncated() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(16 * MB, 11);
    r.truncate_next = 4;
    r.truncate_bytes = 700 * 1024;
    srv.state.set("a.bin", r);
    let st = run(&e, req("11", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 11, 16 * MB);
}

async fn pause_then(srv: &TestServer, e: &KuHttpEngine, id: &str, r: DownloadRequest, change: impl FnOnce()) -> Status {
    e.download(r.clone()).unwrap();
    wait_progress(e, id, 2 * MB).await;
    e.pause(id);
    let st = e.wait(id).await.unwrap();
    assert_eq!(st.state, State::Paused);
    change();
    srv.state.update("a.bin", |r| r.per_conn_rate = 0);
    run(e, r).await
}

// 12. Changed ETag between pause and resume → restart, new content.
#[tokio::test(flavor = "multi_thread")]
async fn t12_changed_etag() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(16 * MB, 12);
    r.per_conn_rate = 2 * MB;
    srv.state.set("a.bin", r);
    let st = pause_then(&srv, &e, "12", req("12", srv.url("/r/a.bin"), dir.path()), || {
        srv.state.update("a.bin", |r| {
            r.seed = 1200;
            r.etag = Some("\"v2\"".into());
        })
    })
    .await;
    assert_complete(&st, 1200, 16 * MB);
    assert!(st.message.unwrap_or_default().contains("Started over"));
}

// 12b. ETag changes *during* the download (If-Range → 200).
#[tokio::test(flavor = "multi_thread")]
async fn t12b_changed_during_download() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(24 * MB, 121);
    r.per_conn_rate = 3 * MB;
    srv.state.set("a.bin", r);
    e.download(req("12b", srv.url("/r/a.bin"), dir.path())).unwrap();
    wait_progress(&e, "12b", 3 * MB).await;
    srv.state.update("a.bin", |r| {
        r.seed = 1210;
        r.etag = Some("\"v2\"".into());
        r.per_conn_rate = 0;
    });
    srv.state.drop_connections();
    let st = tokio::time::timeout(Duration::from_secs(60), e.wait("12b")).await.unwrap().unwrap();
    assert_complete(&st, 1210, 24 * MB);
}

// 13. Changed Last-Modified (no ETag).
#[tokio::test(flavor = "multi_thread")]
async fn t13_changed_last_modified() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(16 * MB, 13);
    r.etag = None;
    r.per_conn_rate = 2 * MB;
    srv.state.set("a.bin", r);
    let st = pause_then(&srv, &e, "13", req("13", srv.url("/r/a.bin"), dir.path()), || {
        srv.state.update("a.bin", |r| {
            r.seed = 1300;
            r.last_modified = Some("Thu, 02 Jan 2025 00:00:00 GMT".into());
        })
    })
    .await;
    assert_complete(&st, 1300, 16 * MB);
}

// 13b. No validators at all: sampled bytes reveal the change.
#[tokio::test(flavor = "multi_thread")]
async fn t13b_no_validators_spot_check() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(16 * MB, 131);
    r.etag = None;
    r.last_modified = None;
    r.per_conn_rate = 2 * MB;
    srv.state.set("a.bin", r);
    let st = pause_then(&srv, &e, "13b", req("13b", srv.url("/r/a.bin"), dir.path()), || srv.state.update("a.bin", |r| r.seed = 1310)).await;
    assert_complete(&st, 1310, 16 * MB);
}

// 14. Redirects: chain, loop, and no credential leaks across origins.
#[tokio::test(flavor = "multi_thread")]
async fn t14_redirects() {
    let (srv, dir, e) = setup().await;
    srv.state.set("a.bin", Resource::new(4 * MB, 14));
    let st = run(&e, req("14", srv.url("/redir/3/a.bin"), dir.path())).await;
    assert_complete(&st, 14, 4 * MB);

    let st = run(&e, req("14loop", srv.url("/loop/0"), dir.path())).await;
    assert_eq!(st.state, State::Failed);
    assert!(st.error.unwrap().to_lowercase().contains("redirect"));

    let other = TestServer::start().await;
    other.state.set("b.bin", Resource::new(2 * MB, 141));
    let r = DownloadRequest {
        headers: vec![("X-Api-Key".into(), "secret-key".into())],
        cookies: Some("session=abc".into()),
        basic_auth: Some(("user".into(), "pass".into())),
        ..req("14x", srv.url(&format!("/xredir/{}/b.bin", other.port)), dir.path())
    };
    let st = run(&e, r).await;
    assert_complete(&st, 141, 2 * MB);
    let origin_log = srv.state.log.lock().unwrap().clone();
    assert!(origin_log.iter().any(|l| l.authorization.is_some() && l.cookie.is_some() && l.custom.is_some()), "origin got credentials");
    let foreign = other.state.log.lock().unwrap().clone();
    assert!(!foreign.is_empty());
    assert!(foreign.iter().all(|l| l.authorization.is_none() && l.cookie.is_none() && l.custom.is_none()), "credentials leaked: {foreign:?}");
}

// 15. Expired resource: signed redirect target stops working mid-download.
#[tokio::test(flavor = "multi_thread")]
async fn t15_expired_signed_url() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(16 * MB, 15);
    r.per_conn_rate = 3 * MB;
    srv.state.set("a.bin", r);
    e.download(req("15", srv.url("/sign/a.bin"), dir.path())).unwrap();
    wait_progress(&e, "15", 2 * MB).await;
    srv.state.rotate_signature();
    srv.state.drop_connections();
    let st = tokio::time::timeout(Duration::from_secs(60), e.wait("15")).await.unwrap().unwrap();
    assert_complete(&st, 15, 16 * MB);
    assert!(srv.state.log.lock().unwrap().iter().any(|l| l.status == 403), "the old signature was rejected");
}

// 16. Disk full → paused (not corrupted), then resumes.
#[tokio::test(flavor = "multi_thread")]
async fn t16_disk_full() {
    let srv = TestServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let full = Arc::new(AtomicBool::new(true));
    let f2 = full.clone();
    let mut c = cfg();
    c.write_fault = Some(Arc::new(move |off, _| (f2.load(Ordering::SeqCst) && off >= 6 * MB).then(|| std::io::Error::from(std::io::ErrorKind::StorageFull))));
    let e = KuHttpEngine::new(c).unwrap();
    srv.state.set("a.bin", Resource::new(16 * MB, 16));
    let st = run(&e, req("16", srv.url("/r/a.bin"), dir.path())).await;
    assert_eq!(st.state, State::Paused);
    assert!(st.error.unwrap().contains("disk space"));
    assert_no_final(dir.path(), "a.bin");
    assert!(dir.path().join("a.bin.kudownload").exists());
    full.store(false, Ordering::SeqCst);
    let st = run(&e, req("16", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 16, 16 * MB);
}

// 17. Application crash (task killed without cleanup) → resume.
#[tokio::test(flavor = "multi_thread")]
async fn t17_crash_recovery() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(24 * MB, 17);
    r.per_conn_rate = 3 * MB;
    srv.state.set("a.bin", r);
    e.download(req("17", srv.url("/r/a.bin"), dir.path())).unwrap();
    wait_progress(&e, "17", 6 * MB).await;
    tokio::time::sleep(Duration::from_millis(400)).await; // a persist happened
    e.crash_for_test("17");
    drop(e);
    assert!(dir.path().join("a.bin.kudownload.json").exists());
    srv.state.update("a.bin", |r| r.per_conn_rate = 0);
    let e2 = KuHttpEngine::new(cfg()).unwrap();
    let mut events = e2.subscribe();
    let st = run(&e2, req("17", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 17, 24 * MB);
    let mut resumed = 0;
    while let Ok(ev) = events.try_recv() {
        if let KuEvent::Resumed { downloaded, .. } = ev {
            resumed = downloaded;
        }
    }
    assert!(resumed > 0, "progress survived the crash");
}

// 18. Power loss / OS restart: stale state + garbage in unsynced regions.
#[tokio::test(flavor = "multi_thread")]
async fn t18_power_loss() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(24 * MB, 18);
    r.per_conn_rate = 3 * MB;
    srv.state.set("a.bin", r);
    let state_path = dir.path().join("a.bin.kudownload.json");
    e.download(req("18", srv.url("/r/a.bin"), dir.path())).unwrap();
    wait_progress(&e, "18", 3 * MB).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let old_state = std::fs::read(&state_path).unwrap();
    wait_progress(&e, "18", 9 * MB).await;
    e.crash_for_test("18");
    drop(e);
    // Let blocking I/O of the killed process drain: a real power loss cannot
    // reorder our fsync-data-then-write-state sequence.
    tokio::time::sleep(Duration::from_millis(500)).await;
    // The disk "lost" recent state: restore the older snapshot and scribble
    // over every byte the old state does not vouch for.
    std::fs::write(&state_path, &old_state).unwrap();
    let st: serde_json::Value = serde_json::from_slice(&old_state).unwrap();
    let temp = dir.path().join("a.bin.kudownload");
    {
        use std::io::{Seek, SeekFrom, Write};
        let mut f = std::fs::OpenOptions::new().write(true).open(&temp).unwrap();
        for r in st["ranges"].as_array().unwrap() {
            let (pos, end) = (r[2].as_u64().unwrap(), r[1].as_u64().unwrap());
            f.seek(SeekFrom::Start(pos)).unwrap();
            f.write_all(&vec![0xAA; (end - pos) as usize]).unwrap();
        }
    }
    srv.state.update("a.bin", |r| r.per_conn_rate = 0);
    let e2 = KuHttpEngine::new(cfg()).unwrap();
    let st = run(&e2, req("18", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 18, 24 * MB);
}

// 19. Pause really pauses; resume completes.
#[tokio::test(flavor = "multi_thread")]
async fn t19_pause_resume() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(16 * MB, 19);
    r.per_conn_rate = 2 * MB;
    srv.state.set("a.bin", r);
    e.download(req("19", srv.url("/r/a.bin"), dir.path())).unwrap();
    wait_progress(&e, "19", 2 * MB).await;
    e.pause("19");
    let st = e.wait("19").await.unwrap();
    assert_eq!(st.state, State::Paused);
    let before = std::fs::read(dir.path().join("a.bin.kudownload.json")).unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(before, std::fs::read(dir.path().join("a.bin.kudownload.json")).unwrap(), "nothing written while paused");
    assert_eq!(srv.state.active.load(Ordering::SeqCst), 0, "no open transfers while paused");
    assert_no_final(dir.path(), "a.bin");
    srv.state.update("a.bin", |r| r.per_conn_rate = 0);
    let st = run(&e, req("19", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 19, 16 * MB);
}

// 20. Large file (1 GB+).
#[tokio::test(flavor = "multi_thread")]
async fn t20_large_file() {
    let (srv, dir, e) = setup().await;
    let len = 1100 * MB;
    srv.state.set("big.bin", Resource::new(len, 20));
    let st = run(&e, req("20", srv.url("/r/big.bin"), dir.path())).await;
    assert_complete(&st, 20, len);
}

// 21. Unknown content length (chunked, no ranges).
#[tokio::test(flavor = "multi_thread")]
async fn t21_unknown_length() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(6 * MB + 5, 21);
    r.ranges = false;
    r.advertise_ranges = false;
    r.chunked = true;
    // Slow enough (≈6 s) that the mid-transfer check can't race completion
    // on a loaded CI runner.
    r.per_conn_rate = MB;
    srv.state.set("a.bin", r);
    e.download(req("21", srv.url("/r/a.bin"), dir.path())).unwrap();
    wait_progress(&e, "21", MB).await;
    let mid = e.status("21").unwrap();
    assert_eq!(mid.total, None);
    assert_eq!(mid.percent, None, "no fake percentage");
    let st = e.wait("21").await.unwrap();
    assert_complete(&st, 21, 6 * MB + 5);
}

// 21b. Unknown length + truncated chunked body is never "completed".
#[tokio::test(flavor = "multi_thread")]
async fn t21b_unknown_length_truncated() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(4 * MB, 211);
    r.ranges = false;
    r.advertise_ranges = false;
    r.chunked = true;
    r.truncate_next = 2;
    r.truncate_bytes = MB;
    srv.state.set("a.bin", r);
    let st = run(&e, req("21b", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 211, 4 * MB);
}

// 22. Several downloads at once with a global limit.
#[tokio::test(flavor = "multi_thread")]
async fn t22_simultaneous() {
    let (srv, dir, e) = setup().await;
    for i in 0..5u64 {
        srv.state.set(&format!("f{i}.bin"), Resource::new(8 * MB + i, 220 + i));
        e.download(req(&format!("22-{i}"), srv.url(&format!("/r/f{i}.bin")), dir.path())).unwrap();
    }
    for i in 0..5u64 {
        let st = e.wait(&format!("22-{i}")).await.unwrap();
        assert_complete(&st, 220 + i, 8 * MB + i);
    }
}

// Integrity: user checksum OK / mismatch, server Digest mismatch.
#[tokio::test(flavor = "multi_thread")]
async fn integrity_checksums() {
    let (srv, dir, e) = setup().await;
    let len = 3 * MB;
    srv.state.set("ok.bin", Resource::new(len, 30));
    let hex: String = sha2::Sha256::digest(kucore::kuhttp::testing::content(30, len)).iter().map(|b| format!("{b:02x}")).collect();
    let st = run(&e, DownloadRequest { checksum: Some(format!("sha-256={hex}")), ..req("c1", srv.url("/r/ok.bin"), dir.path()) }).await;
    assert_complete(&st, 30, len);

    srv.state.set("bad.bin", Resource::new(len, 31));
    let st = run(&e, DownloadRequest { checksum: Some(format!("sha-256={hex}")), ..req("c2", srv.url("/r/bad.bin"), dir.path()) }).await;
    assert_eq!(st.state, State::FailedVerification);
    assert_no_final(dir.path(), "bad.bin");

    let mut r = Resource::new(len, 32);
    r.digest = Some("SHA-256=ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=".into());
    srv.state.set("dig.bin", r);
    let st = run(&e, req("c3", srv.url("/r/dig.bin"), dir.path())).await;
    assert_eq!(st.state, State::FailedVerification, "server Digest is checked");
    assert_no_final(dir.path(), "dig.bin");
}

// Small files use one connection.
#[tokio::test(flavor = "multi_thread")]
async fn small_file_single_connection() {
    let srv = TestServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let e = KuHttpEngine::new(KuHttpConfig { small_file_threshold: 10 * MB, ..cfg() }).unwrap();
    let mut r = Resource::new(5 * MB, 40);
    r.per_conn_rate = 10 * MB;
    srv.state.set("a.bin", r);
    let st = run(&e, req("s", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 40, 5 * MB);
    assert!(!st.segmented);
    let ranged = srv.state.log.lock().unwrap().iter().filter(|l| l.range.as_deref() != Some("bytes=0-0")).count();
    assert_eq!(ranged, 1, "exactly one transfer request");
}

// Bandwidth limit (per download).
#[tokio::test(flavor = "multi_thread")]
async fn bandwidth_limit() {
    let (srv, dir, e) = setup().await;
    srv.state.set("a.bin", Resource::new(6 * MB, 50));
    let t = Instant::now();
    let st = run(&e, DownloadRequest { speed_limit: 2 * MB, ..req("bw", srv.url("/r/a.bin"), dir.path()) }).await;
    assert_complete(&st, 50, 6 * MB);
    let secs = t.elapsed().as_secs_f64();
    assert!(secs > 2.5, "limit not applied: {secs}s");
}

// Security: malicious Content-Disposition, header injection, auth failures.
#[tokio::test(flavor = "multi_thread")]
async fn security_and_permanent_errors() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(MB, 60);
    r.content_disposition = Some("attachment; filename=\"../../evil.bin\"".into());
    srv.state.set("a.bin", r);
    let st = run(&e, req("sec1", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 60, MB);
    assert_eq!(final_path(&st), dir.path().join("evil.bin"));

    let st = run(&e, DownloadRequest { headers: vec![("X-A".into(), "v\r\nInjected: 1".into())], ..req("sec2", srv.url("/r/a.bin"), dir.path()) }).await;
    assert_eq!(st.state, State::Failed);

    let mut r = Resource::new(MB, 61);
    r.require_auth = Some("Basic dTpw".into()); // u:p
    srv.state.set("auth.bin", r);
    let t = Instant::now();
    let st = run(&e, req("sec3", srv.url("/r/auth.bin"), dir.path())).await;
    assert_eq!(st.state, State::Failed, "401 is permanent");
    assert!(t.elapsed() < Duration::from_secs(2), "401 must not be retried endlessly");
    let st = run(&e, DownloadRequest { basic_auth: Some(("u".into(), "p".into())), ..req("sec4", srv.url("/r/auth.bin"), dir.path()) }).await;
    assert_complete(&st, 61, MB);

    let st = run(&e, req("sec5", srv.url("/r/missing.bin"), dir.path())).await;
    assert_eq!(st.state, State::Failed);
    assert!(st.error.unwrap().contains("404"));
}

// Zero-length files.
#[tokio::test(flavor = "multi_thread")]
async fn zero_length() {
    let (srv, dir, e) = setup().await;
    srv.state.set("empty.bin", Resource::new(0, 70));
    let st = run(&e, req("z", srv.url("/r/empty.bin"), dir.path())).await;
    assert_complete(&st, 70, 0);
}

// Adaptive growth when every connection is capped by the server.
#[tokio::test(flavor = "multi_thread")]
async fn adaptive_growth() {
    let (srv, dir, e) = setup().await;
    let mut r = Resource::new(160 * MB, 80);
    r.per_conn_rate = 4 * MB;
    srv.state.set("a.bin", r);
    let st = run(&e, req("g", srv.url("/r/a.bin"), dir.path())).await;
    assert_complete(&st, 80, 160 * MB);
    let peak = srv.state.peak_active.load(Ordering::SeqCst);
    assert!(peak > 3, "connections grew beyond the initial 3 (peak {peak})");
}

// Randomized faults: COMPLETED ⇒ identical; otherwise no final file.
#[tokio::test(flavor = "multi_thread")]
async fn randomized_never_corrupt() {
    let (srv, dir, e) = setup().await;
    let mut completed = 0;
    for i in 0..14u64 {
        let name = format!("r{i}.bin");
        let len = (3 + i % 5) * MB + i * 1237;
        let mut r = Resource::new(len, 900 + i);
        r.fail_prob = [0.0, 0.005, 0.02][(i % 3) as usize];
        r.truncate_next = (i % 4) as u32;
        r.truncate_bytes = 300 * 1024;
        r.bad_content_range = i % 7 == 3;
        r.ranges = i % 5 != 4;
        r.chunked = i % 5 == 4 && i % 2 == 0;
        if i % 6 == 1 {
            r.statuses = [503, 429].into_iter().collect();
        }
        srv.state.set(&name, r);
        let st = run(&e, req(&format!("rnd{i}"), srv.url(&format!("/r/{name}")), dir.path())).await;
        if st.state == State::Completed {
            completed += 1;
            assert!(file_matches(&final_path(&st), 900 + i, len), "scenario {i}: COMPLETED with wrong content");
        } else {
            assert_no_final(dir.path(), &name);
        }
    }
    assert!(completed >= 12, "only {completed}/14 completed");
}
