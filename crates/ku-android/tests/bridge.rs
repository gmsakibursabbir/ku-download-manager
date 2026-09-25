//! The JSON bridge as the Android app drives it, on the host.

use serde_json::{json, Value};

fn call(method: &str, args: Value) -> Value {
    serde_json::from_str(&kudroid::call(method, &args.to_string())).unwrap()
}

#[test]
fn calls_from_foreign_threads_work_like_the_phone() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let downloads = tmp.path().join("Download");
    let cfg = json!({
        "env": { "KU_DATA_DIR": data.to_string_lossy(), "KU_DOWNLOAD_DIR": downloads.to_string_lossy() },
        "deviceName": "Test phone",
    });
    kudroid::init(&cfg.to_string()).unwrap();

    // A second KuDownloader (the "PC") in its own runtime.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pc_dir = tmp.path().join("pc");
    std::fs::create_dir_all(&pc_dir).unwrap();
    let pc = rt.block_on(async {
        let core = kucore::Core::open(kucore::db::Db::open(&pc_dir.join("pc.db")).unwrap()).unwrap();
        let mut s = core.settings();
        s.airsend_name = "PC".into();
        s.airsend_auto_accept = true;
        s.download_dir = pc_dir.join("dl").to_string_lossy().into_owned();
        core.save_settings(s).await.unwrap();
        let air = kucore::airsend::AirSend::open(core.clone(), pc_dir.join("air")).unwrap();
        air.start().await.unwrap();
        air
    });
    let pc_port = pc.status().port;

    // A plain Java thread, not a runtime worker: nothing may panic.
    std::thread::spawn(move || {
        let s = call("getSettings", json!({}));
        assert_eq!(s["ok"]["httpEngine"], "kuhttp");
        assert_eq!(s["ok"]["airsendName"], "Test phone");
        assert_eq!(s["ok"]["onboarded"], true, "phone defaults are applied once");

        // KuAirSend: turning on and sending to an unknown device gives an error, not a crash.
        let on = call("airSetEnabled", json!({"enabled": true}));
        assert!(on.get("ok").is_some(), "{on}");
        let peer = call("airAdd", json!({"address": format!("127.0.0.1:{pc_port}")}));
        let fp = peer["ok"]["fingerprint"].as_str().expect("peer added").to_string();
        let r = call("airSend", json!({"fingerprint": fp, "paths": [], "text": "hi"}));
        assert!(r["ok"].is_string(), "text to the PC: {r}");
        let r = call("airSendDownload", json!({"fingerprint": fp, "download": {"url": "https://example.org/a.zip"}}));
        assert!(r["ok"].is_string(), "download on the PC: {r}");
        let r = call("airSend", json!({"fingerprint": "nope", "paths": [], "text": "hi"}));
        assert!(r["error"].as_str().is_some_and(|e| !e.contains("stopped unexpectedly")), "{r}");
        let r = call("airSendDownload", json!({"fingerprint": "nope", "download": {"url": "https://example.org/a.zip"}}));
        assert!(r["error"].as_str().is_some_and(|e| !e.contains("stopped unexpectedly")), "{r}");
        let off = call("airSetEnabled", json!({"enabled": false}));
        assert!(off.get("ok").is_some(), "{off}");

        // Queue changes spawn engine work too.
        let q = call("saveQueue", json!({"queue": {"name": "Later", "maxConcurrent": 1}}));
        assert!(q.get("ok").is_some(), "{q}");

        // "Download now" goes straight to the engine (no queue): it must not wait for a queue.
        let d = call("addDownload", json!({"req": {"url": "http://127.0.0.1:9/file.bin", "start": true, "source": "android"}}));
        assert!(d["ok"]["id"].is_string(), "{d}");
        assert!(d["ok"]["queueId"].is_null(), "{d}");
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let got = call("getDownload", json!({"id": d["ok"]["id"]}));
        assert_ne!(got["ok"]["status"], "queued", "{got}");
    })
    .join()
    .unwrap();
    drop(pc);
}
