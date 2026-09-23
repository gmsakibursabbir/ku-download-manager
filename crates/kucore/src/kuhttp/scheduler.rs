//! Adaptive connection-count controller.
//!
//! Starts conservatively, grows one connection at a time while throughput
//! keeps improving by a meaningful margin and errors stay low, and backs off
//! quickly when the server throttles (429/503) or connections fail. Work
//! redistribution itself happens in the segment map (idle workers steal).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

const MB: u64 = 1024 * 1024;
const EVAL_EVERY: Duration = Duration::from_secs(3);
const SETTLE: Duration = Duration::from_secs(4);
const THROTTLE_COOLDOWN: Duration = Duration::from_secs(30);
/// Growth must improve throughput by at least this much to be kept.
const MIN_GAIN: f64 = 0.10;

#[derive(Debug)]
pub struct Controller {
    pub min: u32,
    pub max: u32,
    pub target: u32,
    fixed: bool,
    samples: VecDeque<(Instant, u64)>,
    last_eval: Instant,
    last_change: Instant,
    speed_before_growth: Option<f64>,
    plateau: bool,
    errors: u32,
    throttles: u32,
    cooldown_until: Option<Instant>,
}

/// Initial connection count from what the probe learned.
pub fn initial_connections(total: Option<u64>, ranges: bool, small: u64, min: u32, max: u32) -> u32 {
    let n = match (ranges, total) {
        (true, Some(t)) if t >= small => {
            if t < 100 * MB {
                2
            } else if t < 1024 * MB {
                3
            } else {
                4
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
            speed_before_growth: None,
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
        if let Some(before) = self.speed_before_growth.take() {
            if speed < before * (1.0 + MIN_GAIN) {
                // The last connection did not pay off: undo it and settle.
                self.set((self.target - 1).max(self.min), now);
                self.plateau = true;
                return self.target;
            }
        }
        let cooled = self.cooldown_until.is_none_or(|t| now >= t);
        if !self.plateau && cooled && splittable && self.target < self.max && speed > 0.0 {
            self.speed_before_growth = Some(speed);
            self.set(self.target + 1, now);
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
        assert_eq!(initial_connections(Some(50 * MB), true, 10 * MB, 1, 8), 2);
        assert_eq!(initial_connections(Some(5000 * MB), true, 10 * MB, 1, 8), 4);
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
