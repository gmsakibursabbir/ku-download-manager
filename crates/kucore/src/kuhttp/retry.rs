//! Exponential backoff with jitter and Retry-After support.

use reqwest::header::{HeaderMap, RETRY_AFTER};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial: Duration,
    pub max: Duration,
}

static SEED: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);

/// Small, fast PRNG for jitter (not cryptographic).
fn next_random() -> u64 {
    let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let mut x = SEED.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed) ^ t;
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

impl RetryPolicy {
    /// Delay before attempt `attempt` (1-based), "equal jitter": half fixed,
    /// half random, so retries from many workers spread out.
    pub fn backoff(&self, attempt: u32) -> Duration {
        let exp = self.initial.saturating_mul(1u32 << attempt.saturating_sub(1).min(16));
        let capped = exp.min(self.max);
        let half = capped / 2;
        let jitter_ms = if half.as_millis() > 0 { next_random() % (half.as_millis() as u64 + 1) } else { 0 };
        half + Duration::from_millis(jitter_ms)
    }

    /// Delay honouring a server supplied Retry-After (capped).
    pub fn delay(&self, attempt: u32, retry_after: Option<Duration>) -> Duration {
        match retry_after {
            Some(r) => r.min(self.max.max(Duration::from_secs(120))),
            None => self.backoff(attempt),
        }
    }
}

pub fn parse_retry_after(h: &HeaderMap) -> Option<Duration> {
    let v = h.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(secs) = v.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let when = httpdate::parse_http_date(v).ok()?;
    Some(when.duration_since(SystemTime::now()).unwrap_or(Duration::ZERO))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_is_capped() {
        let p = RetryPolicy { max_retries: 5, initial: Duration::from_millis(100), max: Duration::from_secs(2) };
        let d1 = p.backoff(1);
        assert!(d1 >= Duration::from_millis(50) && d1 <= Duration::from_millis(100));
        let d5 = p.backoff(5);
        assert!(d5 >= Duration::from_millis(800) && d5 <= Duration::from_millis(1600));
        let d20 = p.backoff(20);
        assert!(d20 <= Duration::from_secs(2));
    }

    #[test]
    fn retry_after_forms() {
        let mut h = HeaderMap::new();
        h.insert(RETRY_AFTER, "7".parse().unwrap());
        assert_eq!(parse_retry_after(&h), Some(Duration::from_secs(7)));
        let future = SystemTime::now() + Duration::from_secs(30);
        h.insert(RETRY_AFTER, httpdate::fmt_http_date(future).parse().unwrap());
        let d = parse_retry_after(&h).unwrap();
        assert!(d <= Duration::from_secs(30) && d >= Duration::from_secs(28));
    }
}
