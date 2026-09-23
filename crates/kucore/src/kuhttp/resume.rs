//! Persistent resume state, stored next to the temp file as
//! `name.kudownload.json` and written atomically (write, fsync, rename).
//! Only byte positions that were fsynced are ever recorded.

use super::client::Validators;
use super::error::KuError;
use super::segment::Persisted;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

pub const STATE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResumeState {
    pub version: u32,
    pub id: String,
    /// The URL as requested (redirects are re-resolved on resume).
    pub url: String,
    pub final_name: String,
    pub total: Option<u64>,
    pub validators: Validators,
    pub segmented: bool,
    pub ranges: Persisted,
    pub created_at: u64,
    pub updated_at: u64,
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn load(path: &Path) -> Option<ResumeState> {
    let text = std::fs::read(path).ok()?;
    let s: ResumeState = serde_json::from_slice(&text).ok()?;
    (s.version == STATE_VERSION).then_some(s)
}

pub fn save(path: &Path, s: &ResumeState) -> Result<(), KuError> {
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(s).map_err(|e| KuError::Io(e.to_string()))?;
    let mut f = std::fs::File::create(&tmp).map_err(|e| KuError::from_io(&e))?;
    f.write_all(&bytes).map_err(|e| KuError::from_io(&e))?;
    f.sync_all().map_err(|e| KuError::from_io(&e))?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| KuError::from_io(&e))
}

pub fn remove(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("json.tmp"));
}
