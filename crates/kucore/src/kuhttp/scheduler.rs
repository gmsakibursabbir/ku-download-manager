//! Adaptive connection-count controller.
//!
//! Starts moderately, grows by about half the current count while each step
//! still pays off (at least 30 % of the ideal linear gain), then probes one
//! connection at a time, and backs off quickly when the server throttles
//! (429/503) or connections fail. Work redistribution itself happens in the
//! segment map (idle workers steal).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

const MB: u64 = 1024 * 1024;
const EVAL_EVERY: Duration = Duration::from_millis(2500);
const SETTLE: Duration = Duration::from_millis(2500);
const THROTTLE_COOLDOWN: Duration = Duration::from_secs(30);
/// A growth step is kept when it brings at least this share of the ideal
/// (linear) gain…
const GAIN_SHARE: f64 = 0.30;
/// …and never less than this (measurement noise).
const MIN_GAIN: f64 = 0.05;

#[derive(Debug)]
pub struct Controller {
    pub min: u32,
    pub max: u32,
    pub target: u32,
    fixed: bool,
    samples: VecDeque<(Instant, u64)>,
    last_eval: Instant,
    last_change: Instant,
    /// (speed, count) before the last growth step.
    before_growth: Option<(f64, u32)>,
    /// Coarse steps failed once: probe one connection at a time.
    fine: bool,
    plateau: bool,
    errors: u32,
    throttles: u32,
    cooldown_until: Option<Instant>,
}

/// Initial connection count from what the probe learned.
pub fn initial_connections(total: Option<u64>, ranges: bool, small: u64, min: u32, max: u32) -> u32 {
    let n = match (ranges, total) {
        (true, Some(t)) if t >= small => {
            if t < 50 * MB {
                2
            } else if t < 1024 * MB {
                4
            } else {
                6
            }
        }
        _ => 1,
    };
    n.clamp(min.max(1), max.max(1))
}

impl Controller {
    pub fn new(initial: u32, min: u32, max: u32, fixed: bool) -> Controller {
        let now = Instant::now();
        Controller {
            min: min.max(1),
            max: max.max(1),
            target: initial,
            fixed,
            samples: VecDeque::new(),
            last_eval: now,
            last_change: now,
            before_growth: None,
            fine: false,
            plateau: fixed,
            errors: 0,
            throttles: 0,
            cooldown_until: None,
        }
    }

    pub fn record_error(&mut self) {
        self.errors += 1;
    }

    pub fn record_throttle(&mut self) {
        self.throttles += 1;
    }

    /// Throughput over the last few seconds.
    pub fn speed(&self) -> f64 {
        match (self.samples.front(), self.samples.back()) {
            (Some(a), Some(b)) if b.0 > a.0 => (b.1 - a.1) as f64 / b.0.duration_since(a.0).as_secs_f64(),
            _ => 0.0,
        }
    }

    /// Feed progress; returns the (possibly new) target connection count.
    pub fn tick(&mut self, now: Instant, downloaded: u64, splittable: bool) -> u32 {
        self.samples.push_back((now, downloaded));
        while self.samples.front().is_some_and(|s| now.duration_since(s.0) > Duration::from_secs(4)) {
            self.samples.pop_front();
        }
        if self.fixed || now.duration_since(self.last_eval) < EVAL_EVERY {
            return self.target;
        }
        self.last_eval = now;
        let speed = self.speed();
        let (errors, throttles) = (std::mem::take(&mut self.errors), std::mem::take(&mut self.throttles));
        if throttles > 0 {
            // The server asks us to slow down: halve and stop exploring.
            self.set((self.target / 2).max(self.min), now);
            self.plateau = true;
            self.cooldown_until = Some(now + THROTTLE_COOLDOWN);
            return self.target;
        }
        if errors >= 2 && self.target > self.min {
            self.set(self.target - 1, now);
            self.plateau = true;
            return self.target;
        }
        if now.duration_since(self.last_change) < SETTLE {
            return self.target;
        }
        if let Some((before, old)) = self.before_growth.take() {
            let added = self.target.saturating_sub(old).max(1) as f64;
            let need = (GAIN_SHARE * added / old.max(1) as f64).max(MIN_GAIN);
            // Share of the ideal (linear) gain the step delivered.
            let efficiency = if before > 0.0 { (speed / before - 1.0) / (added / old.max(1) as f64) } else { 0.0 };
            if speed >= before * (1.0 + need) && efficiency < 0.7 && added > 1.0 {
                // Partly useful: keep only the share that paid off, then fine-tune.
                self.set(old + (added * efficiency).ceil() as u32, now);
                self.fine = true;
                return self.target;
            }
            if speed < before * (1.0 + need) {
                // The step did not pay off: undo it; after a coarse step try
                // single connections, after a single one settle.
                self.set(old, now);
                if self.fine {
                    self.plateau = true;
                } else {
                    self.fine = true;
                }
                return self.target;
            }
        }
        let cooled = self.cooldown_until.is_none_or(|t| now >= t);
        if !self.plateau && cooled && splittable && self.target < self.max && speed > 0.0 {
            let step = if self.fine { 1 } else { (self.target / 2).max(1) };
            self.before_growth = Some((speed, self.target));
            self.set(self.target + step, now);
        }
        self.target
    }

    fn set(&mut self, n: u32, now: Instant) {
        if n != self.target {
            self.target = n.clamp(self.min, self.max);
            self.last_change = now;
            self.samples.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(c: &mut Controller, secs: u64, speed_for: impl Fn(u32) -> u64, t0: &mut Instant, done: &mut u64) {
        for _ in 0..secs * 4 {
            *t0 += Duration::from_millis(250);
            *done += speed_for(c.target) / 4;
            c.tick(*t0, *done, true);
        }
    }

    #[test]
    fn initial_is_conservative() {
        assert_eq!(initial_connections(Some(5 * MB), true, 10 * MB, 1, 8), 1);
        assert_eq!(initial_connections(Some(40 * MB), true, 10 * MB, 1, 8), 2);
        assert_eq!(initial_connections(Some(500 * MB), true, 10 * MB, 1, 8), 4);
        assert_eq!(initial_connections(Some(5000 * MB), true, 10 * MB, 1, 8), 6);
        assert_eq!(initial_connections(Some(5000 * MB), false, 10 * MB, 1, 8), 1);
        assert_eq!(initial_connections(None, true, 10 * MB, 1, 8), 1);
    }

    #[test]
    fn grows_while_it_helps_then_settles() {
        // Each connection adds 5 MB/s up to 6 connections, then nothing.
        let mut c = Controller::new(2, 1, 8, false);
        let (mut t, mut d) = (Instant::now(), 0);
        run(&mut c, 120, |n| n.min(6) as u64 * 5 * MB, &mut t, &mut d);
        assert_eq!(c.target, 6);
    }

    #[test]
    fn ramps_quickly_when_each_connection_is_capped() {
        // Server caps every connection at 10 MB/s; the link carries 150 MB/s.
        let mut c = Controller::new(4, 1, 32, false);
        let (mut t, mut d) = (Instant::now(), 0);
        run(&mut c, 12, |n| (n as u64 * 10).min(150) * MB, &mut t, &mut d);
        assert!(c.target >= 13, "only {} connections after 12 s", c.target);
        run(&mut c, 60, |n| (n as u64 * 10).min(150) * MB, &mut t, &mut d);
        assert!((15..=17).contains(&c.target), "settled at {}", c.target);
    }

    #[test]
    fn backs_off_on_throttling() {
        let mut c = Controller::new(8, 1, 8, false);
        let (mut t, mut d) = (Instant::now(), 0);
        run(&mut c, 4, |_| 10 * MB, &mut t, &mut d);
        c.record_throttle();
        run(&mut c, 4, |_| 10 * MB, &mut t, &mut d);
        assert_eq!(c.target, 4);
    }

    #[test]
    fn fixed_mode_never_changes() {
        let mut c = Controller::new(4, 1, 8, true);
        let (mut t, mut d) = (Instant::now(), 0);
        c.record_throttle();
        run(&mut c, 30, |n| n as u64 * MB, &mut t, &mut d);
        assert_eq!(c.target, 4);
    }
}
