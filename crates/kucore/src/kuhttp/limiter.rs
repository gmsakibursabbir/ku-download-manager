//! Token-bucket bandwidth limiter. One global instance plus one per download;
//! a worker acquires from both. Unlimited limiters cost one atomic load.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct RateLimiter {
    rate: AtomicU64,
    state: Mutex<(f64, Instant)>,
}

impl RateLimiter {
    pub fn new(bytes_per_sec: u64) -> RateLimiter {
        RateLimiter { rate: AtomicU64::new(bytes_per_sec), state: Mutex::new((0.0, Instant::now())) }
    }

    pub fn set_rate(&self, bytes_per_sec: u64) {
        self.rate.store(bytes_per_sec, Ordering::Relaxed);
        *self.state.lock().unwrap() = (0.0, Instant::now());
    }

    pub fn rate(&self) -> u64 {
        self.rate.load(Ordering::Relaxed)
    }

    /// Take `n` bytes of budget, sleeping if the bucket is in debt.
    pub async fn acquire(&self, n: usize) {
        let rate = self.rate.load(Ordering::Relaxed);
        if rate == 0 {
            return;
        }
        let wait = {
            let mut st = self.state.lock().unwrap();
            let now = Instant::now();
            // Allow a burst of 100 ms worth of data (at least 16 KiB).
            let burst = (rate as f64 * 0.1).max(16.0 * 1024.0);
            let elapsed = now.duration_since(st.1).as_secs_f64();
            st.0 = (st.0 + elapsed * rate as f64).min(burst);
            st.1 = now;
            st.0 -= n as f64;
            if st.0 < 0.0 {
                Duration::from_secs_f64(-st.0 / rate as f64)
            } else {
                Duration::ZERO
            }
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn limits_throughput() {
        let l = RateLimiter::new(1_000_000);
        let start = Instant::now();
        for _ in 0..30 {
            l.acquire(100_000).await;
        }
        // 3 MB at 1 MB/s ≈ 3 s (minus the initial burst allowance).
        let e = start.elapsed().as_secs_f64();
        assert!(e > 2.6 && e < 3.6, "elapsed {e}");
    }

    #[tokio::test]
    async fn unlimited_is_free() {
        let l = RateLimiter::new(0);
        let start = Instant::now();
        for _ in 0..10_000 {
            l.acquire(1 << 20).await;
        }
        assert!(start.elapsed() < Duration::from_millis(50));
    }
}
