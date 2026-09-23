//! Smart connection controller.
//!
//! Picks an initial connection count from what the probe learned (range
//! support, size) and then reacts to what the server actually allows:
//! if the server holds us to fewer connections, we stop asking for more; if
//! throughput scales, we carefully add connections. Every change in aria2
//! briefly restarts the transfer (resuming from the control file), so
//! adjustments are rate-limited and capped.

use ku_proto::Download;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const MB: i64 = 1024 * 1024;
const MAX_ADJUSTMENTS: u32 = 3;
const SETTLE: Duration = Duration::from_secs(20);

pub fn initial_connections(d: &Download) -> u32 {
    if d.meta.resumable == Some(false) {
        return 1;
    }
    if d.kind.is_bittorrent() {
        return 16;
    }
    match d.total {
        0 => 8,
        t if t < 2 * MB => 1,
        t if t < 16 * MB => 4,
        t if t < 128 * MB => 8,
        _ => 16,
    }
}

pub enum SmartAction {
    None,
    Set { connections: u32, note: String },
}

pub struct Smart {
    pub target: u32,
    last_change: Instant,
    samples: VecDeque<(i64, u32)>,
    low_conn_ticks: u32,
    adjustments: u32,
    speed_before_increase: Option<i64>,
    plateau: bool,
}

impl Smart {
    pub fn new(target: u32) -> Smart {
        Smart {
            target,
            last_change: Instant::now(),
            samples: VecDeque::with_capacity(16),
            low_conn_ticks: 0,
            adjustments: 0,
            speed_before_increase: None,
            plateau: target <= 1,
        }
    }

    fn avg_speed(&self) -> i64 {
        if self.samples.is_empty() {
            return 0;
        }
        self.samples.iter().map(|s| s.0).sum::<i64>() / self.samples.len() as i64
    }

    fn max_conns(&self) -> u32 {
        self.samples.iter().map(|s| s.1).max().unwrap_or(0)
    }

    /// Called once per second with the latest live values.
    pub fn observe(&mut self, d: &Download, now: Instant) -> SmartAction {
        self.samples.push_back((d.speed, d.active_connections));
        if self.samples.len() > 10 {
            self.samples.pop_front();
        }
        let remaining = if d.total > 0 { d.total - d.done } else { i64::MAX };
        if remaining < 8 * MB || now.duration_since(self.last_change) < SETTLE || self.samples.len() < 10 {
            return SmartAction::None;
        }
        // 1) Server caps connections: it keeps us well below the target.
        let observed = self.max_conns();
        if self.target > 2 && observed > 0 && observed * 2 <= self.target {
            self.low_conn_ticks += 1;
            if self.low_conn_ticks >= 10 {
                self.low_conn_ticks = 0;
                self.plateau = true;
                return self.set(observed.max(1), now, format!(
                    "Server allows only {observed} connection{}; KuDownloader is using {observed}.",
                    if observed == 1 { "" } else { "s" }
                ));
            }
            return SmartAction::None;
        }
        self.low_conn_ticks = 0;
        let speed = self.avg_speed();
        // 2) Evaluate the last increase.
        if let Some(before) = self.speed_before_increase.take() {
            if speed < before + before / 10 {
                // More connections did not help: settle here.
                self.plateau = true;
            }
        }
        // 3) Scale up while it helps and the transfer is long enough.
        if !self.plateau && self.adjustments < MAX_ADJUSTMENTS && self.target < 16 && remaining > 64 * MB && speed > 0 && observed >= self.target {
            let next = (self.target * 2).min(16);
            self.speed_before_increase = Some(speed);
            return self.set(next, now, format!("Throughput is scaling; increased to {next} connections."));
        }
        SmartAction::None
    }

    fn set(&mut self, n: u32, now: Instant, note: String) -> SmartAction {
        if n == self.target {
            return SmartAction::None;
        }
        self.target = n;
        self.last_change = now;
        self.adjustments += 1;
        self.samples.clear();
        SmartAction::Set { connections: n, note }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_proto::*;

    fn dl(total: i64, done: i64, speed: i64, conns: u32) -> Download {
        Download {
            id: "x".into(), engine: Engine::Aria2, kind: Kind::Http, url: String::new(), mirrors: vec![],
            name: String::new(), dir: String::new(), file_path: None, status: Status::Downloading, total, done,
            uploaded: 0, speed, upload_speed: 0, connections: 0, active_connections: conns, category: String::new(),
            queue_id: None, position: 0, created_at: 0, completed_at: None, error: None, gid: None,
            source: String::new(), options: DownloadOptions::default(), meta: DownloadMeta::default(),
        }
    }

    #[test]
    fn initial_choice() {
        assert_eq!(initial_connections(&dl(MB, 0, 0, 0)), 1);
        assert_eq!(initial_connections(&dl(500 * MB, 0, 0, 0)), 16);
        let mut d = dl(500 * MB, 0, 0, 0);
        d.meta.resumable = Some(false);
        assert_eq!(initial_connections(&d), 1);
    }

    #[test]
    fn backs_off_when_server_limits() {
        let mut s = Smart::new(16);
        let start = Instant::now();
        let mut action = None;
        for i in 0..40 {
            let t = start + SETTLE + Duration::from_secs(i);
            if let SmartAction::Set { connections, .. } = s.observe(&dl(1000 * MB, 10 * MB, 5 * MB, 2), t) {
                action = Some(connections);
                break;
            }
        }
        assert_eq!(action, Some(2));
    }

    #[test]
    fn scales_up_then_plateaus() {
        let mut s = Smart::new(4);
        let start = Instant::now();
        let mut t = start + SETTLE;
        let mut first = None;
        for _ in 0..20 {
            t += Duration::from_secs(1);
            if let SmartAction::Set { connections, .. } = s.observe(&dl(1000 * MB, 0, 4 * MB, 4), t) {
                first = Some(connections);
                break;
            }
        }
        assert_eq!(first, Some(8));
        // Speed does not improve after the increase → no further changes.
        let mut later = None;
        for _ in 0..60 {
            t += Duration::from_secs(1);
            if let SmartAction::Set { connections, .. } = s.observe(&dl(1000 * MB, 0, 4 * MB, 8), t) {
                later = Some(connections);
            }
        }
        assert_eq!(later, None);
    }
}
