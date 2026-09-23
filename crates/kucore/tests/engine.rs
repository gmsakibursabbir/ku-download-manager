//! End-to-end tests against the real aria2 engine and a local HTTP server
//! with byte-range support. Skipped when aria2c is not installed.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use kucore::db::Db;
use kucore::ku_proto::{AddRequest, Status};
use kucore::{Core, Queue};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn payload(len: usize) -> Arc<Vec<u8>> {
    let mut v = Vec::with_capacity(len);
    let mut x: u32 = 0x1234_5678;
    for _ in 0..len {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        v.push((x & 0xff) as u8);
    }
    Arc::new(v)
}

#[derive(Clone)]
struct Files {
    data: Arc<Vec<u8>>,
}

async fn serve_file(State(f): State<Files>, Path(name): Path<String>, headers: HeaderMap) -> Response {
    if name == "missing.bin" {
        return Response::builder().status(StatusCode::NOT_FOUND).body(Body::empty()).unwrap();
    }
    let data = f.data.clone();
    let len = data.len();
    let (start, end, status) = match headers.get(header::RANGE).and_then(|r| r.to_str().ok()).and_then(|r| r.strip_prefix("bytes=")) {
        Some(r) => {
            let (a, b) = r.split_once('-').unwrap();
            let a: usize = a.parse().unwrap_or(0);
            let b: usize = if b.is_empty() { len - 1 } else { b.parse::<usize>().unwrap().min(len - 1) };
            (a, b, StatusCode::PARTIAL_CONTENT)
        }
        None => (0, len - 1, StatusCode::OK),
    };
    // "slow" files are throttled to ~2 MB/s so pause/queue behaviour is observable.
    let slow = name.starts_with("slow");
    let chunk = 32 * 1024;
    let stream = futures_util::stream::unfold(start, move |pos| {
        let data = data.clone();
        async move {
            if pos > end {
                return None;
            }
            if slow {
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
            let stop = (pos + chunk).min(end + 1);
            Some((Ok::<_, std::io::Error>(axum::body::Bytes::copy_from_slice(&data[pos..stop])), stop))
        }
    });
    let mut b = Response::builder()
        .status(status)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, (end - start + 1).to_string());
    if status == StatusCode::PARTIAL_CONTENT {
        b = b.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
    }
    b.body(Body::from_stream(stream)).unwrap()
}

async fn start_server(data: Arc<Vec<u8>>) -> u16 {
    let app = Router::new().route("/f/{name}", get(serve_file)).with_state(Files { data });
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    port
}

async fn wait_for(core: &Arc<Core>, id: &str, timeout: Duration, pred: impl Fn(&kucore::ku_proto::Download) -> bool) -> kucore::ku_proto::Download {
    let start = Instant::now();
    loop {
        let d = core.get(id).expect("download exists");
        if pred(&d) {
            return d;
        }
        if start.elapsed() >= timeout { eprintln!("LOG: {:#?}", core.logs_for(id)); eprintln!("DETAILS: {}", core.details(id).await.unwrap_or_default()); }
        assert!(start.elapsed() < timeout, "timed out; last state: {:?} {} / {} err={:?}", d.status, d.done, d.total, d.error);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn file_sha(path: &std::path::Path) -> String {
    sha(&std::fs::read(path).unwrap())
}

async fn open_core(data_dir: &std::path::Path, dl_dir: &std::path::Path) -> Arc<Core> {
    let db = Db::open(&data_dir.join("kudownloader.db")).unwrap();
    let core = Core::open(db).unwrap();
    let mut s = core.settings();
    s.download_dir = dl_dir.to_string_lossy().into_owned();
    s.use_categories = false;
    s.auto_retry = 0;
    core.save_settings(s).await.unwrap();
    core.start();
    core
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_downloads_with_aria2() {
    if kucore::ku_proto::paths::find_binary("aria2c", None).is_none() {
        eprintln!("aria2c not installed; skipping");
        return;
    }
    let _ = tracing_subscriber::fmt().with_env_filter("kucore=debug").try_init();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let dl_dir = tmp.path().join("downloads");
    // Single test so the process-wide data dir override is not raced.
    std::env::set_var("KU_DATA_DIR", &data_dir);
    let data = payload(12 * 1024 * 1024);
    let expected = sha(&data);
    let port = start_server(data.clone()).await;
    let url = |n: &str| format!("http://127.0.0.1:{port}/f/{n}");

    let core = open_core(&data_dir, &dl_dir).await;

    // 1. Plain multi-connection download with checksum verification.
    let d = core
        .add(AddRequest {
            url: url("fast.bin"),
            connections: Some(8),
            options: kucore::ku_proto::DownloadOptions { checksum: Some(format!("sha-256={expected}")), ..Default::default() },
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(d.name, "fast.bin");
    assert_eq!(d.total, data.len() as i64, "probe learned the size");
    let d = wait_for(&core, &d.id, Duration::from_secs(60), |d| d.status == Status::Completed || d.status == Status::Error).await;
    assert_eq!(d.status, Status::Completed, "error: {:?}", d.error);
    let path = std::path::PathBuf::from(d.file_path.clone().unwrap());
    assert_eq!(file_sha(&path), expected);
    assert_eq!(core.verify(&d.id, "sha-256").await.unwrap(), expected);

    // 2. Same name again → renamed, not overwritten.
    let d2 = core.add(AddRequest { url: format!("{}?again=1", url("fast.bin")), ..Default::default() }).await.unwrap();
    assert_eq!(d2.name, "fast (1).bin");
    wait_for(&core, &d2.id, Duration::from_secs(60), |d| d.status == Status::Completed).await;

    // 3. Pause really pauses, resume continues from the partial file.
    let s = core.add(AddRequest { url: url("slow.bin"), connections: Some(4), ..Default::default() }).await.unwrap();
    wait_for(&core, &s.id, Duration::from_secs(30), |d| d.done > 1024 * 1024).await;
    core.pause(&[s.id.clone()]).await.unwrap();
    let paused = core.get(&s.id).unwrap();
    assert_eq!(paused.status, Status::Paused);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let later = core.get(&s.id).unwrap();
    assert_eq!(later.status, Status::Paused);
    assert!(later.done < data.len() as i64);
    core.resume(&[s.id.clone()]).await.unwrap();
    let s = wait_for(&core, &s.id, Duration::from_secs(60), |d| d.status == Status::Completed || d.status == Status::Error).await;
    assert_eq!(s.status, Status::Completed, "error: {:?}", s.error);
    assert_eq!(file_sha(std::path::Path::new(s.file_path.as_ref().unwrap())), expected);

    // 4. Queue: max 1 concurrent, ordering respected, nothing runs while stopped.
    let q = core.save_queue(Queue { id: "q1".into(), name: "Test".into(), max_concurrent: 1, ..Default::default() }).unwrap();
    let a = core.add(AddRequest { url: url("slow-a.bin"), queue_id: Some(q.id.clone()), ..Default::default() }).await.unwrap();
    let b = core.add(AddRequest { url: url("slow-b.bin"), queue_id: Some(q.id.clone()), ..Default::default() }).await.unwrap();
    core.reorder(&b.id, "top").unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(core.get(&a.id).unwrap().status, Status::Queued, "stopped queue must not run");
    core.start_queue(&q.id, None).await.unwrap();
    wait_for(&core, &b.id, Duration::from_secs(10), |d| d.status == Status::Downloading).await;
    assert_eq!(core.get(&a.id).unwrap().status, Status::Queued, "b was moved to the top and max_concurrent is 1");
    wait_for(&core, &a.id, Duration::from_secs(90), |d| d.status == Status::Completed).await;
    assert_eq!(core.get(&b.id).unwrap().status, Status::Completed);
    // Queue drained → stopped automatically.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!core.queues().iter().find(|x| x.id == q.id).unwrap().running);

    // 5. Server errors are reported clearly.
    let m = core.add(AddRequest { url: url("missing.bin"), engine: Some(kucore::ku_proto::Engine::Aria2), ..Default::default() }).await.unwrap();
    let m = wait_for(&core, &m.id, Duration::from_secs(30), |d| d.status == Status::Error).await;
    assert!(m.error.unwrap().to_lowercase().contains("not found"));

    // 6. Crash recovery: stop mid-transfer, reopen, it resumes by itself.
    let r = core.add(AddRequest { url: url("slow-r.bin"), connections: Some(2), ..Default::default() }).await.unwrap();
    wait_for(&core, &r.id, Duration::from_secs(30), |d| d.done > 2 * 1024 * 1024).await;
    tokio::time::sleep(Duration::from_secs(6)).await; // let progress + control file flush
    core.shutdown().await;
    drop(core);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let core = open_core(&data_dir, &dl_dir).await;
    let rec = core.get(&r.id).unwrap();
    assert!(rec.done > 0, "partial progress survived the restart");
    let rec = wait_for(&core, &r.id, Duration::from_secs(90), |d| d.status == Status::Completed || d.status == Status::Error).await;
    assert_eq!(rec.status, Status::Completed, "error: {:?}", rec.error);
    assert_eq!(file_sha(std::path::Path::new(rec.file_path.as_ref().unwrap())), expected);

    // 7. Remove with files deletes the file.
    core.remove(&[rec.id.clone()], true).await.unwrap();
    assert!(!std::path::Path::new(rec.file_path.as_ref().unwrap()).exists());
    assert!(core.get(&rec.id).is_none());
    core.shutdown().await;
}
