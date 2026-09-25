//! KuAirSend between two devices in one process, over real mutual TLS on loopback.

use kucore::airsend::{AirSend, AirTransfer};
use kucore::db::Db;
use kucore::{Core, CoreEvent};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

async fn device(root: &Path, name: &str, auto_accept: bool) -> (Arc<Core>, Arc<AirSend>) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let core = Core::open(Db::open(&dir.join("kudownloader.db")).unwrap()).unwrap();
    let mut s = core.settings();
    s.download_dir = dir.join("downloads").to_string_lossy().into_owned();
    s.airsend_name = name.into();
    s.airsend_auto_accept = auto_accept;
    core.save_settings(s).await.unwrap();
    let air = AirSend::open(core.clone(), dir.join("airsend")).unwrap();
    air.start().await.unwrap();
    (core, air)
}

async fn finished(air: &AirSend, id: &str, secs: u64) -> AirTransfer {
    let t = Instant::now();
    loop {
        if let Some(x) = air.transfers().into_iter().find(|x| x.id == id && x.finished.is_some()) {
            return x;
        }
        assert!(t.elapsed() < Duration::from_secs(secs), "transfer {id} did not finish");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sends_files_folders_and_text_between_devices() {
    let tmp = tempfile::tempdir().unwrap();
    let (_ca, alice) = device(tmp.path(), "alice", false).await;
    let (cb, bob) = device(tmp.path(), "bob", true).await;
    let port = bob.status().port;
    let peer = alice.add_address(&format!("127.0.0.1:{port}")).await.unwrap();
    assert_eq!(peer.alias, "bob");
    assert_eq!(peer.fingerprint, bob.status().fingerprint);

    // A file and a folder (tree kept on the other side).
    let src = tmp.path().join("src");
    std::fs::create_dir_all(src.join("album/disc 2")).unwrap();
    let big: Vec<u8> = (0..64u32 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
    std::fs::write(src.join("big.bin"), &big).unwrap();
    std::fs::write(src.join("album/cover.jpg"), b"jpg").unwrap();
    std::fs::write(src.join("album/disc 2/track.flac"), b"flac").unwrap();
    let paths = vec![src.join("big.bin").to_string_lossy().into_owned(), src.join("album").to_string_lossy().into_owned()];
    let t0 = Instant::now();
    let id = alice.send(&peer.fingerprint, paths, None, None).unwrap();
    let sent = finished(&alice, &id, 60).await;
    assert_eq!(sent.state, "done", "{:?}", sent.error);
    assert_eq!(sent.file_count, 3);
    let secs = t0.elapsed().as_secs_f64();
    println!("64 MiB + folder in {secs:.2}s ({:.0} MB/s)", 64.0 * 1.048576 / secs);

    let inbox = Path::new(&cb.settings().download_dir).join("KuAirSend");
    assert_eq!(std::fs::read(inbox.join("big.bin")).unwrap(), big);
    assert_eq!(std::fs::read(inbox.join("album/disc 2/track.flac")).unwrap(), b"flac");
    let got = bob.transfers().into_iter().find(|t| t.direction == "receive" && t.file_count == 3).unwrap();
    assert_eq!(got.state, "done");
    assert_eq!(got.done, got.total);

    // Same names again: kept side by side, never overwritten.
    let id = alice.send(&peer.fingerprint, vec![src.join("album/cover.jpg").to_string_lossy().into_owned()], None, None).unwrap();
    assert_eq!(finished(&alice, &id, 20).await.state, "done");
    assert!(inbox.join("cover.jpg").exists());

    // A link arrives as a message.
    let mut rx = cb.subscribe();
    let id = alice.send(&peer.fingerprint, vec![], Some("https://example.org/file.iso".into()), None).unwrap();
    assert_eq!(finished(&alice, &id, 20).await.state, "done");
    let msg = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(CoreEvent::AirSendMessage { message }) = rx.recv().await {
                return message;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(msg.text, "https://example.org/file.iso");
    assert_eq!(msg.peer, "alice");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn receiver_decides_and_pin_is_enforced() {
    let tmp = tempfile::tempdir().unwrap();
    let (_ca, alice) = device(tmp.path(), "alice", false).await;
    let (cb, bob) = device(tmp.path(), "bob", false).await;
    let peer = alice.add_address(&format!("127.0.0.1:{}", bob.status().port)).await.unwrap();
    let file = tmp.path().join("note.txt");
    std::fs::write(&file, b"hello").unwrap();
    let paths = vec![file.to_string_lossy().into_owned()];

    // Declined: nothing is written.
    let mut rx = cb.subscribe();
    let id = alice.send(&peer.fingerprint, paths.clone(), None, None).unwrap();
    let req = loop {
        if let Ok(CoreEvent::AirSendRequest { request }) = rx.recv().await {
            break request;
        }
    };
    assert_eq!(req.peer, "alice");
    bob.decide(&req.id, false, false).await.unwrap();
    assert_eq!(finished(&alice, &id, 20).await.state, "declined");
    let inbox = Path::new(&cb.settings().download_dir).join("KuAirSend");
    assert!(!inbox.join("note.txt").exists());

    // Accepted and trusted: the next one needs no answer.
    let id = alice.send(&peer.fingerprint, paths.clone(), None, None).unwrap();
    let req = loop {
        if let Ok(CoreEvent::AirSendRequest { request }) = rx.recv().await {
            break request;
        }
    };
    bob.decide(&req.id, true, true).await.unwrap();
    assert_eq!(finished(&alice, &id, 20).await.state, "done");
    let id = alice.send(&peer.fingerprint, paths.clone(), None, None).unwrap();
    assert_eq!(finished(&alice, &id, 20).await.state, "done");
    assert!(inbox.join("note (2).txt").exists());

    // PIN.
    let mut s = cb.settings();
    s.airsend_pin = "4321".into();
    cb.save_settings(s).await.unwrap();
    let id = alice.send(&peer.fingerprint, paths.clone(), None, None).unwrap();
    assert_eq!(finished(&alice, &id, 20).await.state, "pin");
    let id = alice.send(&peer.fingerprint, paths.clone(), None, Some("4321".into())).unwrap();
    assert_eq!(finished(&alice, &id, 20).await.state, "done");

    // Guessing the PIN locks the sender out for a while, even with the right PIN.
    for _ in 0..5 {
        let id = alice.send(&peer.fingerprint, paths.clone(), None, Some("0000".into())).unwrap();
        assert_eq!(finished(&alice, &id, 20).await.state, "pin");
    }
    let id = alice.send(&peer.fingerprint, paths, None, Some("4321".into())).unwrap();
    let t = finished(&alice, &id, 20).await;
    assert_eq!(t.state, "failed");
    assert!(t.error.unwrap_or_default().contains("too many attempts"));
}

/// Two devices find each other through multicast alone. Ignored in CI (runners
/// often have no multicast route); run with `cargo test -p kucore --test airsend -- --ignored`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn devices_discover_each_other() {
    let tmp = tempfile::tempdir().unwrap();
    let (_ca, alice) = device(tmp.path(), "alice", false).await;
    let (_cb, bob) = device(tmp.path(), "bob", false).await;
    let t = Instant::now();
    while !(alice.peers().iter().any(|p| p.alias == "bob") && bob.peers().iter().any(|p| p.alias == "alice")) {
        assert!(t.elapsed() < Duration::from_secs(10), "not discovered: alice sees {:?}, bob sees {:?}", alice.peers(), bob.peers());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!("discovered in {:?}", t.elapsed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hands_a_download_to_another_device_now_or_later() {
    let tmp = tempfile::tempdir().unwrap();
    let (_ca, phone) = device(tmp.path(), "phone", false).await;
    let (pc_core, pc) = device(tmp.path(), "pc", false).await;
    let peer = phone.add_address(&format!("127.0.0.1:{}", pc.status().port)).await.unwrap();
    let mut rx = pc_core.subscribe();

    // Not trusted: the PC is asked first.
    let dl = kucore::airsend::RemoteDownload { url: "https://example.org/files/big.iso".into(), ..Default::default() };
    let id = phone.send_download(&peer.fingerprint, dl, None).unwrap();
    let req = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(CoreEvent::AirSendDownload { request }) = rx.recv().await {
                return request;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(req.download.url, "https://example.org/files/big.iso");
    pc.decide(&req.id, true, true).await.unwrap();
    assert_eq!(finished(&phone, &id, 20).await.state, "done");
    let d = pc_core.list().into_iter().find(|d| d.url == "https://example.org/files/big.iso").expect("download added on the PC");
    assert_eq!(d.source, "airsend");

    // Trusted now, and for later: its own queue with a one-off schedule.
    let at = chrono_ms() + 3 * 3600 * 1000;
    let dl = kucore::airsend::RemoteDownload { url: "https://example.org/files/night.zip".into(), at: Some(at), ..Default::default() };
    let id = phone.send_download(&peer.fingerprint, dl, None).unwrap();
    assert_eq!(finished(&phone, &id, 20).await.state, "done");
    let d = pc_core.list().into_iter().find(|d| d.url.ends_with("night.zip")).expect("scheduled download added");
    let q = d.queue_id.clone().expect("in a queue");
    assert!(q.starts_with("airsend-"), "{q}");
    assert_eq!(d.status, kucore::ku_proto::Status::Queued);
    let s = pc_core.schedules().into_iter().find(|s| s.queue_id == q).expect("schedule for the queue");
    assert!(s.enabled && s.date.is_some());

    // Addresses the engine would refuse never reach the other device.
    let bad = kucore::airsend::RemoteDownload { url: "file:///etc/passwd".into(), ..Default::default() };
    assert!(phone.send_download(&peer.fingerprint, bad, None).is_err());
}

fn chrono_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn trusting_a_device_asks_it_once_then_nothing_prompts() {
    let tmp = tempfile::tempdir().unwrap();
    let (phone_core, phone) = device(tmp.path(), "phone", false).await;
    let (pc_core, pc) = device(tmp.path(), "pc", false).await;
    let pc_peer = phone.add_address(&format!("127.0.0.1:{}", pc.status().port)).await.unwrap();
    let phone_peer = pc.add_address(&format!("127.0.0.1:{}", phone.status().port)).await.unwrap();
    let mut pc_rx = pc_core.subscribe();
    let mut phone_rx = phone_core.subscribe();

    // The phone trusts the PC: the PC is asked (once) to trust back.
    phone.set_trusted(&pc_peer.fingerprint, true).await.unwrap();
    let ask = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(CoreEvent::AirSendTrust { request }) = pc_rx.recv().await {
                return request;
            }
        }
    })
    .await
    .expect("the PC is asked to trust back");
    assert_eq!(ask.peer, "phone");

    // It does; the phone already trusts the PC, so it is not asked again.
    pc.set_trusted(&phone_peer.fingerprint, true).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    while let Ok(e) = phone_rx.try_recv() {
        assert!(!matches!(e, CoreEvent::AirSendTrust { .. }), "a device that already trusts is not asked");
    }

    // Now links go through without a prompt, both ways.
    let dl = kucore::airsend::RemoteDownload { url: "https://example.org/files/movie.mkv".into(), ..Default::default() };
    let id = phone.send_download(&pc_peer.fingerprint, dl, None).unwrap();
    assert_eq!(finished(&phone, &id, 10).await.state, "done");
    while let Ok(e) = pc_rx.try_recv() {
        assert!(!matches!(e, CoreEvent::AirSendDownload { .. }), "a trusted device is not asked");
    }
    assert!(pc_core.list().iter().any(|d| d.url.ends_with("movie.mkv")));
    let dl = kucore::airsend::RemoteDownload { url: "https://example.org/files/back.zip".into(), ..Default::default() };
    let id = pc.send_download(&phone_peer.fingerprint, dl, None).unwrap();
    assert_eq!(finished(&pc, &id, 10).await.state, "done");
    assert!(phone_core.list().iter().any(|d| d.url.ends_with("back.zip")));
}
