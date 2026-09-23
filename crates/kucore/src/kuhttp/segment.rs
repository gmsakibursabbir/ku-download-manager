//! Live segment/work map for dynamic segmentation.
//!
//! The map always covers `[0, total)` with contiguous, non-overlapping
//! segments. Idle workers take pending work first, then *steal* half of the
//! largest remaining active segment (the active worker's range shrinks; it
//! notices on its next write). Splits never create segments smaller than
//! `min_split`, so slow workers get relieved without HTTP overhead exploding.

use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Split points are aligned so segments stay page/cluster friendly.
const ALIGN: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegState {
    Pending,
    Active,
    Done,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub id: u32,
    pub start: u64,
    /// Exclusive. May shrink while active (work stealing).
    pub end: u64,
    /// Next byte to write; `pos - start` bytes are written and validated.
    pub pos: u64,
    pub state: SegState,
    pub worker: Option<u32>,
    pub retries: u32,
    /// Exponential moving average, bytes/s.
    pub speed: f64,
    pub last_activity: Instant,
    last_sample: (Instant, u64),
    /// Position when the current worker claimed it (progress detection).
    claim_pos: u64,
}

impl Segment {
    fn new(id: u32, start: u64, end: u64, pos: u64) -> Segment {
        let now = Instant::now();
        Segment {
            id,
            start,
            end,
            pos,
            state: if pos >= end { SegState::Done } else { SegState::Pending },
            worker: None,
            retries: 0,
            speed: 0.0,
            last_activity: now,
            last_sample: (now, pos),
            claim_pos: pos,
        }
    }

    pub fn remaining(&self) -> u64 {
        self.end - self.pos
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claim {
    pub seg: u32,
    pub start: u64,
    pub end: u64,
}

#[derive(Debug)]
pub struct SegmentMap {
    pub total: u64,
    segs: Vec<Segment>,
    next_id: u32,
}

/// Persisted form: (start, end, pos) of every segment.
pub type Persisted = Vec<[u64; 3]>;

impl SegmentMap {
    /// Split `[0,total)` into `n` initial segments.
    pub fn new(total: u64, n: u32, min_split: u64) -> SegmentMap {
        let n = n.max(1) as u64;
        let n = if total == 0 { 1 } else { n.min((total / min_split.max(1)).max(1)) };
        let mut segs = Vec::new();
        let mut start = 0;
        for i in 0..n {
            let end = if i == n - 1 { total } else { align_up(total * (i + 1) / n).min(total) };
            if end > start || (total == 0 && i == 0) {
                segs.push(Segment::new(i as u32, start, end, start));
            }
            start = end;
        }
        let next_id = segs.len() as u32;
        SegmentMap { total, segs, next_id }
    }

    /// Rebuild from persisted ranges; fails if they do not tile `[0,total)`.
    pub fn from_persisted(total: u64, p: &Persisted) -> Option<SegmentMap> {
        let mut segs: Vec<Segment> = p
            .iter()
            .enumerate()
            .map(|(i, r)| Segment::new(i as u32, r[0], r[1], r[2]))
            .collect();
        segs.sort_by_key(|s| s.start);
        let m = SegmentMap { total, next_id: segs.len() as u32, segs };
        m.check().ok().map(|_| m)
    }

    pub fn persisted(&self) -> Persisted {
        self.segs.iter().map(|s| [s.start, s.end, s.pos]).collect()
    }

    /// Invariants: contiguous cover of [0,total), pos within bounds.
    pub fn check(&self) -> Result<(), String> {
        let mut expect = 0;
        for s in &self.segs {
            if s.start != expect {
                return Err(format!("gap or overlap at {expect} (segment starts at {})", s.start));
            }
            if s.end < s.start || s.pos < s.start || s.pos > s.end {
                return Err(format!("segment {}-{} has position {}", s.start, s.end, s.pos));
            }
            expect = s.end;
        }
        if expect != self.total {
            return Err(format!("map ends at {expect}, file is {} bytes", self.total));
        }
        Ok(())
    }

    pub fn downloaded(&self) -> u64 {
        self.segs.iter().map(|s| s.pos - s.start).sum()
    }

    pub fn is_complete(&self) -> bool {
        self.segs.iter().all(|s| s.pos == s.end) && self.check().is_ok()
    }

    pub fn active_count(&self) -> u32 {
        self.segs.iter().filter(|s| s.state == SegState::Active).count() as u32
    }

    pub fn done_count(&self) -> u32 {
        self.segs.iter().filter(|s| s.state == SegState::Done).count() as u32
    }

    pub fn failed_count(&self) -> u32 {
        self.segs.iter().filter(|s| s.retries > 0 && s.state != SegState::Done).count() as u32
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segs
    }

    /// Whether an idle worker could get useful work right now.
    pub fn has_work(&self, min_split: u64) -> bool {
        self.segs.iter().any(|s| s.state == SegState::Pending && s.pos < s.end)
            || self.segs.iter().any(|s| s.state == SegState::Active && s.remaining() >= 2 * min_split)
    }

    fn get(&mut self, id: u32) -> Option<&mut Segment> {
        self.segs.iter_mut().find(|s| s.id == id)
    }

    /// Give `worker` something to do: pending work first, otherwise split the
    /// largest remaining active segment.
    pub fn claim(&mut self, worker: u32, min_split: u64) -> Option<Claim> {
        let now = Instant::now();
        if let Some(s) = self.segs.iter_mut().filter(|s| s.state == SegState::Pending && s.pos < s.end).min_by_key(|s| s.pos) {
            s.state = SegState::Active;
            s.worker = Some(worker);
            s.last_activity = now;
            s.last_sample = (now, s.pos);
            s.claim_pos = s.pos;
            return Some(Claim { seg: s.id, start: s.pos, end: s.end });
        }
        let idx = self
            .segs
            .iter()
            .enumerate()
            .filter(|(_, s)| s.state == SegState::Active && s.remaining() >= 2 * min_split)
            .max_by_key(|(_, s)| s.remaining())
            .map(|(i, _)| i)?;
        let victim = &mut self.segs[idx];
        // Give the idle worker the back half (aligned); the busy worker keeps
        // the bytes it is about to receive.
        let mid = align_up(victim.pos + victim.remaining() / 2).min(victim.end - min_split).max(victim.pos + min_split);
        if mid >= victim.end || mid <= victim.pos {
            return None;
        }
        let old_end = victim.end;
        victim.end = mid;
        let id = self.next_id;
        self.next_id += 1;
        let mut s = Segment::new(id, mid, old_end, mid);
        s.state = SegState::Active;
        s.worker = Some(worker);
        self.segs.insert(idx + 1, s);
        Some(Claim { seg: id, start: mid, end: old_end })
    }

    /// Current end of a segment (it may have been shortened by a split).
    pub fn end_of(&self, seg: u32) -> u64 {
        self.segs.iter().find(|s| s.id == seg).map(|s| s.end).unwrap_or(0)
    }

    /// Record `bytes` written at the segment's position. Returns the bytes
    /// accepted (never beyond the current end).
    pub fn advance(&mut self, seg: u32, from: u64, bytes: u64) -> u64 {
        let Some(s) = self.get(seg) else { return 0 };
        if from != s.pos {
            return 0;
        }
        let take = bytes.min(s.end - s.pos);
        s.pos += take;
        let now = Instant::now();
        s.last_activity = now;
        let dt = now.duration_since(s.last_sample.0).as_secs_f64();
        if dt >= 0.5 {
            let inst = (s.pos - s.last_sample.1) as f64 / dt;
            s.speed = if s.speed == 0.0 { inst } else { s.speed * 0.6 + inst * 0.4 };
            s.last_sample = (now, s.pos);
        }
        take
    }

    /// Worker finished (successfully or not) with a segment.
    pub fn release(&mut self, seg: u32, failed: bool) {
        if let Some(s) = self.get(seg) {
            s.worker = None;
            s.speed = 0.0;
            if s.pos >= s.end {
                s.state = SegState::Done;
            } else {
                s.state = SegState::Pending;
                if failed {
                    // Only failures without progress use up the retry budget:
                    // a flaky link that keeps advancing is not stuck.
                    s.retries = if s.pos > s.claim_pos { 1 } else { s.retries + 1 };
                }
            }
        }
        self.merge_done();
    }

    pub fn retries_of(&self, seg: u32) -> u32 {
        self.segs.iter().find(|s| s.id == seg).map(|s| s.retries).unwrap_or(0)
    }

    /// Merge adjacent completed segments to keep the map (and state file) small.
    fn merge_done(&mut self) {
        let mut out: Vec<Segment> = Vec::with_capacity(self.segs.len());
        for s in self.segs.drain(..) {
            if let Some(last) = out.last_mut() {
                if last.state == SegState::Done && s.state == SegState::Done && last.end == s.start {
                    last.end = s.end;
                    last.pos = s.end;
                    continue;
                }
            }
            out.push(s);
        }
        self.segs = out;
    }

    /// Reset everything to pending (after a crash/restart the workers are gone).
    pub fn reset_active(&mut self) {
        for s in &mut self.segs {
            if s.state == SegState::Active {
                s.state = SegState::Pending;
                s.worker = None;
            }
        }
    }

    /// Completed byte ranges (for spot-check verification).
    pub fn written_ranges(&self) -> Vec<(u64, u64)> {
        self.segs.iter().filter(|s| s.pos > s.start).map(|s| (s.start, s.pos)).collect()
    }

    /// Forget written bytes of a segment (stream must restart at its start).
    pub fn rewind(&mut self, seg: u32) {
        if let Some(s) = self.get(seg) {
            s.pos = s.start;
            s.last_sample = (Instant::now(), s.pos);
        }
    }
}

fn align_up(v: u64) -> u64 {
    v.div_ceil(ALIGN) * ALIGN
}

#[cfg(test)]
mod tests {
    use super::*;
    const MB: u64 = 1024 * 1024;

    #[test]
    fn initial_split_tiles_file() {
        let m = SegmentMap::new(100 * MB + 7, 4, 2 * MB);
        assert_eq!(m.segments().len(), 4);
        m.check().unwrap();
        let small = SegmentMap::new(3 * MB, 8, 2 * MB);
        assert_eq!(small.segments().len(), 1, "never below the minimum segment size");
        let empty = SegmentMap::new(0, 4, 2 * MB);
        assert!(empty.is_complete());
    }

    #[test]
    fn steals_largest_remaining_work() {
        let mut m = SegmentMap::new(64 * MB, 2, MB);
        let a = m.claim(1, MB).unwrap();
        let b = m.claim(2, MB).unwrap();
        assert_eq!((a.start, b.start), (0, 32 * MB));
        // Worker 2 is fast and finishes.
        assert_eq!(m.advance(b.seg, b.start, 32 * MB), 32 * MB);
        m.release(b.seg, false);
        // Worker 1 progressed a little; worker 2 steals half of what is left.
        m.advance(a.seg, 0, 4 * MB);
        let c = m.claim(2, MB).unwrap();
        assert_eq!(c.end, 32 * MB);
        assert_eq!(m.end_of(a.seg), c.start);
        assert!(c.start >= 4 * MB + MB && c.start % ALIGN == 0);
        m.check().unwrap();
        // Worker 1 cannot write past its new end.
        let accepted = m.advance(a.seg, 4 * MB, 64 * MB);
        assert_eq!(accepted, c.start - 4 * MB);
        m.release(a.seg, false);
        m.advance(c.seg, c.start, c.end - c.start);
        m.release(c.seg, false);
        assert!(m.is_complete());
        assert_eq!(m.segments().len(), 1, "done segments merge");
    }

    #[test]
    fn does_not_split_tiny_remainders() {
        let mut m = SegmentMap::new(3 * MB, 1, MB);
        let a = m.claim(1, MB).unwrap();
        m.advance(a.seg, 0, 2 * MB);
        assert!(m.claim(2, MB).is_none(), "1 MB left < 2 × min split");
    }

    #[test]
    fn persisted_roundtrip_and_validation() {
        let mut m = SegmentMap::new(10 * MB, 2, MB);
        let a = m.claim(1, MB).unwrap();
        m.advance(a.seg, 0, MB);
        let p = m.persisted();
        let r = SegmentMap::from_persisted(10 * MB, &p).unwrap();
        assert_eq!(r.downloaded(), MB);
        let mut bad = p.clone();
        bad[1][0] += 1; // gap
        assert!(SegmentMap::from_persisted(10 * MB, &bad).is_none());
        assert!(SegmentMap::from_persisted(11 * MB, &p).is_none());
    }

    #[test]
    fn stale_writes_are_rejected() {
        let mut m = SegmentMap::new(10 * MB, 1, MB);
        let a = m.claim(1, MB).unwrap();
        assert_eq!(m.advance(a.seg, 5, 10), 0, "write must start at the segment position");
    }
}
