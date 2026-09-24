# KuHTTP

KuDownloader's native HTTP/HTTPS engine. Implemented in
`crates/kucore/src/kuhttp`, integrated with KuCore behind
**Settings › Advanced › HTTP engine** (default for new installs: KuHTTP).

## Lifecycle

`Queued → Probing → Downloading → Verifying → Finalizing → Completed`, with
`Paused`, `Cancelled`, `Failed`, `Recoverable` (transient errors exhausted —
safe to resume) and `FailedVerification`.

A download becomes **Completed** only when every range of the segment map is
written, the map is internally consistent, all writes are fsynced, the
on-disk size equals the expected size, the optional checksum (user supplied,
or announced by the server via `Digest` / `X-Checksum-*`) matches, and the
atomic rename from `name.kudownload` to `name` succeeded. Otherwise no file
with the final name is ever produced.

## Modules

| File | Responsibility |
|---|---|
| `client.rs` | One pooled `reqwest` client per engine (keep-alive, HTTP/2 via ALPN, no transparent decompression); origin-scoped credentials; manual redirects with loop detection; real `bytes=0-0` probe |
| `response.rs` | Strict `206` / `Content-Range` / `Content-Length` validation; `200` to a range request is never written |
| `segment.rs` | Live segment map `[0,total)`: claim pending work, otherwise steal half of the largest remaining active segment (never below `min_segment_size`) |
| `scheduler.rs` | Adaptive connection count: conservative start (1–4 by size), grow one at a time while throughput improves ≥10 %, undo a connection that did not help, halve on 429/503, shrink on repeated connection errors |
| `worker.rs` | Reusable workers: claim → request (with `If-Range`) → validate → stream → buffered positional writes; retry budget counts only failures *without progress*; shared refresh of expired redirect targets |
| `storage.rs` | Preallocated temp file, positional writes through a small handle pool, separate sync handle, sparse temp files on Windows (avoids NTFS zero-fill), path-traversal-safe names, symlink refusal, atomic finalize |
| `resume.rs` / `recovery.rs` | `name.kudownload.json` state written atomically after fsync; on resume: size, ETag, Last-Modified must match; without validators, sampled byte ranges are re-downloaded and compared; anything suspicious restarts from zero |
| `integrity.rs` | Size check; MD5 / SHA-1 / SHA-256 / SHA-512 / BLAKE3; server-announced digests |
| `retry.rs` | Exponential backoff with jitter, `Retry-After` (seconds and HTTP-date) |
| `limiter.rs` | Token-bucket limiter, global + per download |
| `events.rs` | Coalesced progress events (per-segment detail included), lifecycle events |
| `testing.rs` | Fault-injecting test server (feature `testing`) |

Fallback chain: segmented → (range misbehaviour or `200` to a range) re-probe
→ resource changed? restart : single stream → clear error.

## Failure-injection suite (`crates/kucore/tests/kuhttp.rs`)

Every test asserts that a COMPLETED download is byte-identical to the source
and that non-completed downloads never produce the final file.

| # | Scenario | Test |
|---|---|---|
| 1 | Normal 200 (no ranges) | `t01_plain_200` |
| 2 | 206 ranges, parallel segments | `t02_segmented` |
| 3 | Server without range support / lies with `Accept-Ranges` | `t03_accept_ranges_liar`, `t01` |
| 4 | Slow connection | `t04_slow_connection` |
| 5 | One slow segment (work stealing) | `t05_one_slow_segment` |
| 6 | Random connection failures | `t06_random_failures` |
| 7 | Timeout (stalled body) | `t07_stall_timeout` |
| 8 | 429 + Retry-After | `t08_429` |
| 9 | 503 / connection limit | `t09_503_connection_limit` |
| 10 | Incorrect Content-Range | `t10_bad_content_range` |
| 11 | Truncated responses | `t11_truncated` |
| 12 | Changed ETag (between pause/resume and mid-download) | `t12_changed_etag`, `t12b_changed_during_download` |
| 13 | Changed Last-Modified; no validators (sampled check) | `t13_changed_last_modified`, `t13b_no_validators_spot_check` |
| 14 | Redirect chain, loop, cross-origin credential stripping | `t14_redirects` |
| 15 | Expired (signed) resource | `t15_expired_signed_url` |
| 16 | Disk full → paused, then resumed | `t16_disk_full` |
| 17 | Application crash | `t17_crash_recovery` |
| 18 | OS restart / power loss (stale state + garbage in unsynced regions) | `t18_power_loss` |
| 19 | Pause / resume | `t19_pause_resume` |
| 20 | 1 GB+ file | `t20_large_file` (1.1 GB) |
| 21 | Unknown content length (and truncated chunked body) | `t21_unknown_length`, `t21b_unknown_length_truncated` |
| 22 | Multiple simultaneous downloads | `t22_simultaneous` |
| — | Checksums (user, mismatch, server Digest), small-file single connection, bandwidth limit, path traversal, header injection, 401/404 handling, zero-length, adaptive growth, randomized faults | remaining tests |

## Benchmarks

See `docs/benchmarks/`. Run with
`cargo run --release -p kucore --example kuhttp_bench -- [--quick] [--reps N] [--out FILE]`.

### Results (Windows 10, SATA SSD, loopback, 2 runs per case, median)

Full table: [`benchmarks/kuhttp-vs-aria2.md`](benchmarks/kuhttp-vs-aria2.md).
"Durable" = data flushed to disk (KuHTTP reports COMPLETED only after
fsync; aria2's output is fsynced afterwards for a fair comparison). Every
file was verified byte-for-byte. Numbers below are after the adaptive
controller rework (v0.2: ~50 % growth steps, 2.5 s evaluation, partial-gain
trimming).

| Scenario | KuHTTP | aria2 | Reading |
|---|---|---|---|
| 1 GB, 1 connection | 12.2 s (84 MB/s) | 12.7 s (80 MB/s) | on par, KuHTTP slightly ahead |
| 1 GB, 8 connections | 13.5 s | 23.9 s | KuHTTP ahead (disk-bound; sparse positional writes) |
| 50 MB … 10 GB, adaptive vs aria2 16 conns | 0.35 / 6.2 / 13.6 / 98.6 / 216 s | 0.53 / 11.2 / 29.6 / 257 / 526 s | KuHTTP 1.5–2.6× faster |
| 500 MB, server caps 10 MB/s per connection, **adaptive** | 8.4 s (grew to 9) | 9.0 s (8 conns), 6.2 s (16 conns) | adaptive now beats aria2 at IDM's usual 8 connections |
| 500 MB, 0.2 % chunks drop the connection | 4.9 s, 17 retries | 12.0 s | both correct; KuHTTP recovers faster |
| CPU (1 GB, 1 conn) | 1.7 s | 5.1 s | KuHTTP uses ~⅓–½ of aria2's CPU |
| Peak RSS | 2–19 MB | 11–27 MB | |

Storage experiment (1 GB, 4 connections): sparse temp files on NTFS
roughly halve the time — kept on by default.

### Verdict

KuHTTP is correct under all 32 injected failures and at least as fast as
aria2 in every benchmarked scenario except a server that caps each
connection when aria2 is forced to 16 connections. It is therefore the
**default HTTP/HTTPS engine for new installs** (existing settings are kept).
aria2 remains available (Settings › Advanced › HTTP engine) and still
handles FTP, SFTP, BitTorrent, magnet and Metalink.

Caveats: results come from one Windows machine over loopback; WAN, HTTP/2
and Linux have not been benchmarked (CI runs the correctness suite on
Linux, not the benchmark).
tuning (streams are treated like connections), no Metalink/mirror support
(aria2 handles those), per-segment checksums are not stored (resume trusts
fsynced positions plus validators / sampled checks).
