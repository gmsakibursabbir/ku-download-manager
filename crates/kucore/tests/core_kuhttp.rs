//! KuCore with the experimental KuHTTP engine enabled.

use kucore::db::Db;
use kucore::ku_proto::{AddRequest, Engine, Status};
use kucore::kuhttp::testing::{file_matches, Resource, TestServer};
use kucore::Core;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MB: u64 = 1024 * 1024;

async fn open(data: &std::path::Path, dl: &std::path::Path) -> Arc<Core> {
    let core = Core::open(Db::open(&data.join("kudownloader.db")).unwrap()).unwrap();
    let mut s = core.settings();
    s.download_dir = dl.to_string_lossy().into_owned();
    s.use_categories = false;
    s.http_engine = "kuhttp".into();
    core.save_settings(s).await.unwrap();
    core.start();
    core
}

async fn wait(core: &Arc<Core>, id: &str, secs: u64, f: impl Fn(&kucore::ku_proto::Download) -> bool) -> kucore::ku_proto::Download {
    let t = Instant::now();
    loop {
        let d = core.get(id).unwrap();
        if f(&d) {
            return d;
        }
        assert!(t.elapsed() < Duration::from_secs(secs), "timeout: {:?} {}/{} {:?}", d.status, d.done, d.total, d.error);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn kucore_with_kuhttp() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let dl = tmp.path().join("dl");
    std::env::set_var("KU_DATA_DIR", &data);
    let srv = TestServer::start().await;
    let core = open(&data, &dl).await;

    // Routing: plain HTTP goes to KuHTTP when enabled.
    srv.state.set("a.bin", Resource::new(24 * MB, 1));
    let d = core.add(AddRequest { url: srv.url("/r/a.bin"), ..Default::default() }).await.unwrap();
    assert_eq!(d.engine, Engine::Kuhttp);
    let d = wait(&core, &d.id, 60, |d| d.status == Status::Completed || d.status == Status::Error).await;
    assert_eq!(d.status, Status::Completed, "{:?}", d.error);
    assert!(file_matches(std::path::Path::new(d.file_path.as_ref().unwrap()), 1, 24 * MB));

    // Pause / resume through KuCore.
    let mut r = Resource::new(16 * MB, 2);
    r.per_conn_rate = 2 * MB;
    srv.state.set("b.bin", r);
    let b = core.add(AddRequest { url: srv.url("/r/b.bin"), ..Default::default() }).await.unwrap();
    wait(&core, &b.id, 30, |d| d.done > 2 * MB as i64).await;
    core.pause(std::slice::from_ref(&b.id)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    let p = core.get(&b.id).unwrap();
    assert_eq!(p.status, Status::Paused);
    srv.state.update("b.bin", |r| r.per_conn_rate = 0);
    core.resume(std::slice::from_ref(&b.id)).await.unwrap();
    let b2 = wait(&core, &b.id, 60, |d| d.status == Status::Completed || d.status == Status::Error).await;
    assert_eq!(b2.status, Status::Completed, "{:?}", b2.error);
    assert!(file_matches(std::path::Path::new(b2.file_path.as_ref().unwrap()), 2, 16 * MB));

    // App restart mid-download resumes from the KuHTTP sidecar state.
    let mut r = Resource::new(24 * MB, 3);
    r.per_conn_rate = 512 * 1024;
    srv.state.set("c.bin", r);
    let c = core.add(AddRequest { url: srv.url("/r/c.bin"), ..Default::default() }).await.unwrap();
    wait(&core, &c.id, 30, |d| d.done > 2 * MB as i64).await;
    assert_ne!(core.get(&c.id).unwrap().status, Status::Completed);
    core.shutdown().await;
    drop(core);
    srv.state.update("c.bin", |r| r.per_conn_rate = 0);
    let core = open(&data, &dl).await;
    let c2 = wait(&core, &c.id, 60, |d| d.status == Status::Completed || d.status == Status::Error).await;
    assert_eq!(c2.status, Status::Completed, "{:?}", c2.error);
    assert!(file_matches(std::path::Path::new(c2.file_path.as_ref().unwrap()), 3, 24 * MB));
    let log = core.logs_for(&c.id);
    assert!(log.iter().any(|l| l.message.contains("KuHTTP")), "log: {log:?} status {:?} done {}", c2.status, c2.done);

    // Remove with files.
    core.remove(std::slice::from_ref(&c.id), true).await.unwrap();
    assert!(!std::path::Path::new(c2.file_path.as_ref().unwrap()).exists());
    core.shutdown().await;
}
