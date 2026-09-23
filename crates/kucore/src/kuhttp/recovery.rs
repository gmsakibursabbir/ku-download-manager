//! Decide whether persisted progress can be trusted after a pause, crash or
//! reboot. Old bytes are only kept when the remote resource is provably the
//! same: size and validators must match, and when the server offers no
//! validators, sampled byte ranges are re-downloaded and compared.

use super::client::{self, Probe, RequestCtx};
use super::config::KuHttpConfig;
use super::error::KuError;
use super::resume::ResumeState;
use super::segment::SegmentMap;
use super::storage::Storage;
use reqwest::Client;

pub enum Decision {
    Resume(SegmentMap),
    Restart(String),
}

const SAMPLE: u64 = 4096;

pub async fn decide(state: &ResumeState, probe: &Probe, storage_len: Option<u64>) -> Decision {
    if state.total != probe.total {
        return Decision::Restart(format!(
            "the size changed ({} → {})",
            state.total.map(|t| t.to_string()).unwrap_or("unknown".into()),
            probe.total.map(|t| t.to_string()).unwrap_or("unknown".into())
        ));
    }
    if let Some(why) = state.validators.changed(&probe.validators) {
        return Decision::Restart(why);
    }
    let Some(total) = probe.total else { return Decision::Restart("the size is unknown, so partial data cannot be resumed".into()) };
    if !probe.ranges {
        return Decision::Restart("the server no longer supports resuming".into());
    }
    let Some(len) = storage_len else { return Decision::Restart("the partial file is missing".into()) };
    if len != total {
        return Decision::Restart("the partial file has the wrong size".into());
    }
    match SegmentMap::from_persisted(total, &state.ranges) {
        Some(mut m) => {
            m.reset_active();
            Decision::Resume(m)
        }
        None => Decision::Restart("the saved progress is inconsistent".into()),
    }
}

/// Without validators, compare a few already-downloaded samples with the
/// server. Any mismatch means the content changed.
pub async fn spot_check(client: &Client, ctx: &RequestCtx, cfg: &KuHttpConfig, probe: &Probe, map: &SegmentMap, storage: &Storage) -> Result<(), String> {
    let total = map.total;
    let mut samples = Vec::new();
    for (start, end) in map.written_ranges() {
        if end - start >= SAMPLE {
            samples.push(start);
            samples.push(end - SAMPLE);
        }
    }
    samples.sort_unstable();
    samples.dedup();
    // Spread up to four samples over the written data.
    let picks: Vec<u64> = if samples.len() <= 4 { samples } else { (0..4).map(|i| samples[i * (samples.len() - 1) / 3]).collect() };
    for off in picks {
        let (resp, _, _) = client::send(client, ctx, &probe.final_url, Some((off, Some(off + SAMPLE - 1))), None, cfg.max_redirects)
            .await
            .map_err(|e| e.to_string())?;
        super::response::validate_partial(&resp, off, off + SAMPLE - 1, total).map_err(|e| e.to_string())?;
        let remote = resp.bytes().await.map_err(|e| KuError::from_reqwest(&e).to_string())?;
        let local = storage.read_at(off, SAMPLE as usize).await.map_err(|e| e.to_string())?;
        if remote.as_ref() != local.as_slice() {
            return Err(format!("content at offset {off} differs"));
        }
    }
    Ok(())
}
