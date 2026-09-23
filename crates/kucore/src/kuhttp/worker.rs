//! A worker repeatedly claims a range from the segment map, fetches it with
//! a validated request and writes it positionally. Workers are reused across
//! segments (and HTTP connections are pooled by the shared client).

use super::client;
use super::download::{Control, Job};
use super::error::KuError;
use super::events::KuEvent;
use super::response;
use super::segment::Claim;
use reqwest::StatusCode;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub enum Exit {
    /// No more work to take.
    NoWork,
    /// More workers than the controller wants.
    Retired,
    /// Pause / cancel.
    Stopped,
    Fatal(KuError),
}

pub async fn run(job: Arc<Job>, wid: u32) -> Exit {
    let mut url_refreshes = 0;
    loop {
        if job.stopping() {
            return Exit::Stopped;
        }
        if job.unknown_eof.load(Ordering::SeqCst) {
            job.stats.remove_worker();
            return Exit::NoWork;
        }
        if job.stats.try_retire(job.target.load(Ordering::SeqCst)) {
            return Exit::Retired;
        }
        let claim = {
            let mut map = job.map.lock().unwrap();
            map.claim(wid, job.min_split).map(|c| {
                if !job.ranges && c.start > 0 {
                    // No range support: an interrupted stream restarts at 0.
                    map.rewind(c.seg);
                    Claim { start: 0, ..c }
                } else {
                    c
                }
            })
        };
        let Some(c) = claim else {
            job.stats.remove_worker();
            return Exit::NoWork;
        };
        let _ = job.events.send(KuEvent::SegmentStarted { id: job.id.clone(), start: c.start, end: c.end });
        let used_url = job.url.read().unwrap().clone();
        let result = fetch(&job, c, &used_url).await;
        let retries = {
            let mut map = job.map.lock().unwrap();
            map.release(c.seg, result.is_err());
            map.retries_of(c.seg)
        };
        match result {
            Ok(()) => {
                let _ = job.events.send(KuEvent::SegmentCompleted { id: job.id.clone(), start: c.start, end: c.end });
            }
            Err(KuError::Paused | KuError::Cancelled) => {
                job.stats.remove_worker();
                return Exit::Stopped;
            }
            Err(e @ KuError::Http { status: 401 | 403 | 404 | 410, .. }) if job.redirected() && url_refreshes < 3 => {
                // A signed/expiring redirect target: resolve the original URL again.
                url_refreshes += 1;
                if job.refresh_url(&used_url).await.is_err() {
                    job.stats.remove_worker();
                    return Exit::Fatal(e);
                }
            }
            Err(e) if e.is_transient() => {
                job.stats.retries.fetch_add(1, Ordering::Relaxed);
                if e.is_throttle() {
                    job.stats.throttles.fetch_add(1, Ordering::Relaxed);
                    job.controller.lock().unwrap().record_throttle();
                } else {
                    job.stats.errors.fetch_add(1, Ordering::Relaxed);
                    job.controller.lock().unwrap().record_error();
                }
                if retries > job.retry.max_retries {
                    job.stats.remove_worker();
                    return Exit::Fatal(e);
                }
                let _ = job.events.send(KuEvent::Retrying { id: job.id.clone(), attempt: retries, reason: e.to_string() });
                let delay = job.retry.delay(retries, e.retry_after());
                if job.sleep_or_stop(delay).await {
                    job.stats.remove_worker();
                    return Exit::Stopped;
                }
            }
            Err(e) => {
                job.stats.remove_worker();
                return Exit::Fatal(e);
            }
        }
    }
}

/// Fetch one claimed range and write it. Returns Ok when the (possibly
/// shortened) segment is fully written.
async fn fetch(job: &Job, c: Claim, url: &url::Url) -> Result<(), KuError> {
    let url = url.clone();
    let total = job.total;
    let ranged = job.segmented || c.start > 0;
    if job.total.is_none() {
        // Unknown size streams always restart from the beginning.
        job.unknown_done.store(0, Ordering::SeqCst);
    }
    let if_range = job.validators.if_range();
    let range = if ranged {
        Some((c.start, total.map(|_| c.end - 1)))
    } else {
        None
    };
    let (resp, _, _) = client::send(&job.client, &job.ctx, &url, range, if_range.as_ref(), job.cfg.max_redirects).await?;
    job.note_version(resp.version());
    // Where this response's body ends (exclusive).
    let body_end = match (ranged, total) {
        (true, Some(t)) => response::validate_partial(&resp, c.start, c.end - 1, t)?.end + 1,
        (true, None) => return Err(KuError::RangeIgnored),
        (false, t) => {
            if resp.status() != StatusCode::OK {
                return Err(client::status_error(&resp));
            }
            match (t, resp.content_length()) {
                (Some(t), Some(len)) if t != len => return Err(KuError::ResourceChanged(format!("size changed from {t} to {len} bytes"))),
                (Some(t), _) => t,
                (None, Some(len)) => len,
                (None, None) => u64::MAX,
            }
        }
    };
    let mut resp = resp;
    let wb = job.cfg.write_buffer;
    let mut buf: Vec<u8> = Vec::with_capacity(wb + 64 * 1024);
    let mut pos = c.start;
    loop {
        if job.stopping() {
            flush(job, c.seg, &mut pos, &mut buf).await?;
            return Err(if matches!(*job.control.borrow(), Control::Cancel { .. }) { KuError::Cancelled } else { KuError::Paused });
        }
        let chunk = match resp.chunk().await {
            Ok(Some(b)) => b,
            Ok(None) => break,
            Err(e) => {
                // Keep what we have: it was validated and is correct.
                flush(job, c.seg, &mut pos, &mut buf).await?;
                return Err(KuError::from_reqwest(&e));
            }
        };
        job.limiter.acquire(chunk.len()).await;
        job.global.acquire(chunk.len()).await;
        let cur_end = if total.is_some() { job.map.lock().unwrap().end_of(c.seg) } else { u64::MAX };
        let room = cur_end.saturating_sub(pos + buf.len() as u64);
        let take = (chunk.len() as u64).min(room) as usize;
        buf.extend_from_slice(&chunk[..take]);
        if pos + buf.len() as u64 >= cur_end {
            // Segment complete (or shortened by a split): stop here.
            flush(job, c.seg, &mut pos, &mut buf).await?;
            return Ok(());
        }
        if buf.len() >= wb {
            flush(job, c.seg, &mut pos, &mut buf).await?;
        }
    }
    flush(job, c.seg, &mut pos, &mut buf).await?;
    let cur_end = if total.is_some() { job.map.lock().unwrap().end_of(c.seg) } else { u64::MAX };
    if total.is_none() && body_end == u64::MAX {
        // Unknown length, no Content-Length: EOF (with a well-formed chunked
        // terminator, enforced by the HTTP stack) is the end.
        job.unknown_eof.store(true, Ordering::SeqCst);
        return Ok(());
    }
    let expected = body_end.min(cur_end);
    if pos < expected {
        return Err(KuError::Truncated { expected: expected - c.start, got: pos - c.start });
    }
    if total.is_none() {
        job.unknown_eof.store(true, Ordering::SeqCst);
    }
    if pos < cur_end {
        // Server sent a shorter (valid) range; the rest stays pending.
        return Err(KuError::Truncated { expected: cur_end - c.start, got: pos - c.start });
    }
    Ok(())
}

async fn flush(job: &Job, seg: u32, pos: &mut u64, buf: &mut Vec<u8>) -> Result<(), KuError> {
    if buf.is_empty() {
        return Ok(());
    }
    let len = buf.len() as u64;
    let data = std::mem::take(buf);
    let back = job.storage.write_at(*pos, data).await?;
    *buf = back;
    buf.clear();
    if job.total.is_some() {
        // Accepted bytes can only be fewer than written if a split moved the
        // boundary inside this write; those extra bytes are the same validated
        // content at the same offsets, so the other worker rewrites identical
        // data. `min_split` > write buffer makes this practically impossible.
        job.map.lock().unwrap().advance(seg, *pos, len);
        *pos += len;
    } else {
        job.unknown_done.fetch_add(len, Ordering::SeqCst);
        *pos += len;
    }
    Ok(())
}
