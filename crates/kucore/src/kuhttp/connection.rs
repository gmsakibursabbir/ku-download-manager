//! Per-download connection bookkeeping (lock-free counters).

use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Default, Debug)]
pub struct ConnStats {
    pub workers: AtomicU32,
    pub peak_workers: AtomicU32,
    pub retries: AtomicU32,
    pub errors: AtomicU32,
    pub throttles: AtomicU32,
}

impl ConnStats {
    pub fn add_worker(&self) -> u32 {
        let n = self.workers.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak_workers.fetch_max(n, Ordering::Relaxed);
        n
    }

    pub fn remove_worker(&self) -> u32 {
        self.workers.fetch_sub(1, Ordering::SeqCst) - 1
    }

    pub fn workers(&self) -> u32 {
        self.workers.load(Ordering::SeqCst)
    }

    /// Atomically retire one worker if more are running than `target`.
    pub fn try_retire(&self, target: u32) -> bool {
        self.workers
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |w| (w > target).then(|| w - 1))
            .is_ok()
    }
}
