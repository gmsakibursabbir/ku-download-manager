//! Ad and tracker blocking for the built-in browser: EasyList-syntax filter
//! lists matched by Brave's adblock engine. Lists are downloaded when the user
//! turns blocking on (or updates them) and the compiled engine is cached, so
//! later starts load in milliseconds.

use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use adblock::Engine;
use anyhow::{bail, Result};
use kucore::Settings;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::Duration;

/// Used when the interface does not name its own lists.
pub const DEFAULT_LISTS: &[&str] = &[
    "https://easylist.to/easylist/easylist.txt",
    "https://easylist.to/easylist/easyprivacy.txt",
    "https://secure.fanboy.co.nz/fanboy-annoyance.txt",
];

struct Loaded {
    engine: Engine,
    rules: usize,
    updated: i64,
}

static ENGINE: RwLock<Option<Loaded>> = RwLock::new(None);

fn dir(data: &Path) -> PathBuf {
    data.join("adblock")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Load the compiled engine saved by the last update, if any.
pub async fn load_cached(data: &Path) {
    let d = dir(data);
    let Ok(bytes) = tokio::fs::read(d.join("engine.dat")).await else { return };
    let meta: Value = tokio::fs::read_to_string(d.join("meta.json")).await.ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let loaded = tokio::task::spawn_blocking(move || {
        let mut engine = Engine::default();
        engine.deserialize(&bytes).ok()?;
        Some(engine)
    })
    .await
    .ok()
    .flatten();
    match loaded {
        Some(engine) => {
            *ENGINE.write().unwrap_or_else(|p| p.into_inner()) = Some(Loaded {
                engine,
                rules: meta["rules"].as_u64().unwrap_or(0) as usize,
                updated: meta["updated"].as_i64().unwrap_or(0),
            });
        }
        None => tracing::warn!("ad blocker: the saved filters could not be read; update them in Settings"),
    }
}

/// Download the filter lists, compile them and save the result.
pub async fn update(data: &Path, lists: &[String], s: &Settings) -> Result<Value> {
    let lists: Vec<String> = if lists.is_empty() { DEFAULT_LISTS.iter().map(|s| s.to_string()).collect() } else { lists.to_vec() };
    let mut b = reqwest::Client::builder().timeout(Duration::from_secs(60)).gzip(true).user_agent(format!("KuDownloader/{}", env!("CARGO_PKG_VERSION")));
    if !s.proxy.trim().is_empty() {
        if let Ok(p) = reqwest::Proxy::all(s.proxy.trim()) {
            b = b.proxy(p);
        }
    }
    let client = b.build()?;
    let mut texts = Vec::new();
    let mut failed = Vec::new();
    for url in &lists {
        if !url.starts_with("https://") {
            failed.push(json!({"url": url, "error": "Only https addresses are accepted."}));
            continue;
        }
        let r = async {
            let resp = client.get(url).send().await?.error_for_status()?;
            if resp.content_length().is_some_and(|l| l > 32 << 20) {
                anyhow::bail!("The list is too large.");
            }
            Ok::<String, anyhow::Error>(resp.text().await?)
        }
        .await;
        match r {
            Ok(t) => texts.push(t),
            Err(e) => failed.push(json!({"url": url, "error": format!("{e:#}")})),
        }
    }
    if texts.is_empty() {
        bail!("No filter list could be downloaded. Check the connection and try again.");
    }
    let rules: usize = texts.iter().map(|t| t.lines().filter(|l| !l.is_empty() && !l.starts_with('!')).count()).sum();
    let (engine, bytes) = tokio::task::spawn_blocking(move || {
        let mut set = FilterSet::new(false);
        for t in texts {
            set.add_filter_list(t, ParseOptions::default());
        }
        let engine = Engine::new_with_filter_set(set);
        let bytes = engine.serialize();
        (engine, bytes)
    })
    .await?;
    let d = dir(data);
    tokio::fs::create_dir_all(&d).await?;
    tokio::fs::write(d.join("engine.dat"), &bytes).await?;
    let updated = now_ms();
    tokio::fs::write(d.join("meta.json"), json!({"rules": rules, "updated": updated, "lists": lists}).to_string()).await?;
    *ENGINE.write().unwrap_or_else(|p| p.into_inner()) = Some(Loaded { engine, rules, updated });
    Ok(json!({"rules": rules, "updated": updated, "failed": failed}))
}

pub fn clear(data: &Path) {
    *ENGINE.write().unwrap_or_else(|p| p.into_inner()) = None;
    let _ = std::fs::remove_dir_all(dir(data));
}

pub fn status() -> Value {
    match ENGINE.read().unwrap_or_else(|p| p.into_inner()).as_ref() {
        Some(l) => json!({"loaded": true, "rules": l.rules, "updated": l.updated}),
        None => json!({"loaded": false, "rules": 0, "updated": null}),
    }
}

/// Should the browser refuse this request? `kind` is the resource type
/// ("script", "image", "xmlhttprequest", "sub_frame", "other", …).
pub fn should_block(url: &str, source: &str, kind: &str) -> bool {
    let guard = ENGINE.read().unwrap_or_else(|p| p.into_inner());
    let Some(l) = guard.as_ref() else { return false };
    let Ok(req) = Request::new(url, source, kind, "GET") else { return false };
    l.engine.check_network_request(&req).should_block()
}

/// Page-specific element hiding: CSS selectors, scriptlets, and whether
/// generic (class/id) rules apply.
pub fn cosmetic(url: &str) -> Value {
    let guard = ENGINE.read().unwrap_or_else(|p| p.into_inner());
    let Some(l) = guard.as_ref() else { return json!({"hide": [], "script": "", "generic": false, "exceptions": []}) };
    let r = l.engine.url_cosmetic_resources(url);
    json!({
        "hide": r.hide_selectors.into_iter().collect::<Vec<_>>(),
        "script": r.injected_script,
        "generic": !r.generichide,
        "exceptions": r.exceptions.into_iter().collect::<Vec<_>>(),
    })
}

/// Generic hiding rules that apply to the classes and ids found on a page.
pub fn hidden(classes: &[String], ids: &[String], exceptions: &[String]) -> Vec<String> {
    let guard = ENGINE.read().unwrap_or_else(|p| p.into_inner());
    let Some(l) = guard.as_ref() else { return Vec::new() };
    let ex: HashSet<String> = exceptions.iter().cloned().collect();
    l.engine.hidden_class_id_selectors(classes, ids, &ex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_listed_requests_and_hides_elements() {
        let mut set = FilterSet::new(false);
        set.add_filter_list("||ads.example.com^\n@@||ads.example.com/allowed.js\nexample.org##.banner\n##.ad-box\n".into(), ParseOptions::default());
        let engine = Engine::new_with_filter_set(set);
        *ENGINE.write().unwrap() = Some(Loaded { engine, rules: 4, updated: 0 });
        assert!(should_block("https://ads.example.com/x.js", "https://news.example.net/", "script"));
        assert!(!should_block("https://ads.example.com/allowed.js", "https://news.example.net/", "script"));
        assert!(!should_block("https://cdn.example.net/app.js", "https://news.example.net/", "script"));
        let c = cosmetic("https://example.org/page");
        assert!(c["hide"].as_array().unwrap().iter().any(|s| s == ".banner"));
        assert_eq!(hidden(&["ad-box".into()], &[], &[]), vec![".ad-box".to_string()]);
    }
}
