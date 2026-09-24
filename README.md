# KuDownloader

A lightweight native download manager for Windows and Linux: aria2 and yt-dlp
under the hood, a Rust core, a compact Tauri UI and a browser extension that
hands downloads and videos to the app.

```
Browser ── KuDownloader extension ── Native Messaging ── ku-native-host
                                                              │ (local API, token)
ku CLI ──────────────────────────────────────────────────────┤
                                                              ▼
Desktop app (Tauri) ── IPC ──▶ KuCore ──┬─▶ aria2   HTTP(S) · FTP · SFTP · BitTorrent · magnet · Metalink
                                        ├─▶ yt-dlp  media sites · HLS/DASH (FFmpeg for merging)
                                        └─▶ KuHTTP  native adaptive HTTP engine (experimental, opt-in)
                                        │
                                        └── SQLite (state, queues, schedules, settings)
```

## Repository

| Path | What |
|---|---|
| `crates/kucore` | Engine: SQLite persistence, aria2 supervisor + JSON-RPC, yt-dlp runner, queue/scheduler, smart connections, retries, crash recovery, local API, native-host registration, power actions, **KuHTTP** (`src/kuhttp`) |
| `crates/ku-proto` | Shared types, paths, dependency-free loopback API client and app launcher |
| `crates/ku-native-host` | Native messaging host (whitelisted requests only) |
| `crates/ku-cli` | `ku` command-line tool |
| `app/` | Tauri 2 shell (`src-tauri`) and React UI (`src`, design tokens in `src/design-system`) |
| `extension/` | WebExtension (Chrome, Chromium, Edge, Brave, Firefox): interception, context menus, media detection, hover button with site adapters |
| `docs/benchmarks` | KuHTTP vs aria2 results |

## Build and run

Requirements: Rust (stable), Node 22+, pnpm, and on Windows the MSVC build
tools. Engines: `aria2c`, `yt-dlp` and optionally `ffmpeg` on `PATH` (winget:
`aria2.aria2`, `yt-dlp.yt-dlp`, `Gyan.FFmpeg`; Linux: distro packages).

```bash
cd app && pnpm install && pnpm tauri dev      # desktop app (dev)
cd extension && node build.mjs                # extension → extension/dist/{chrome,firefox}
cargo build --release -p ku-cli -p ku-native-host
```

**Browser Integration** in the app lists every installed browser — anything
registered with Windows (Chrome, Edge, Brave, Helium, Opera, Vivaldi, Zen,
Floorp, …) plus known install folders — and installs the extension per browser
or in all of them: it opens the browser's extensions page and copies the
extension folder path for *Load unpacked*. The native host is registered where
each browser actually reads it (e.g. `HKCU\Software\imput\Helium\NativeMessagingHosts`),
under the current user, at startup and before each install.

Chromium browsers on Windows refuse `.crx` files that don't come from their
web store (`CRX_REQUIRED_PROOF_MISSING`), so `kudmx.crx` is for store upload or
policy deployment; *Load unpacked* is the direct path. Release Firefox installs
only Mozilla-signed add-ons permanently (`npm run sign:firefox` in `extension/`
with AMO API keys); otherwise use *Load Temporary Add-on*. The Chromium
extension id is fixed by the manifest key (`bmbpbbaapbbelemahbmnjhppdlpdgkdi`).

Downloads caught in the browser open a small always-on-top **Download File**
window (the main window can stay in the tray). On any site the extension shows
a **KuDownload** button over videos — on hover, and for a few seconds when a
video starts playing — including players inside iframes and in fullscreen.

### CLI

```bash
ku add https://example.com/file.iso -c smart
ku media "https://www.youtube.com/watch?v=…" --quality 1080
ku info "https://vimeo.com/…"
ku list · ku pause all · ku resume <id> · ku wait <id> · ku queue start main
```

The CLI starts the app in the tray if it is not running (`--no-launch` to disable).

## Tests

```bash
cargo test --workspace --release
```

* `kucore` unit tests (classification, filenames, DB, torrent parser, smart
  controller, hashing, grabber, KuHTTP components).
* `tests/engine.rs` — real aria2 against a local range server: checksum,
  rename-on-conflict, pause/resume integrity, queue ordering and concurrency,
  404 handling, crash recovery, delete-with-files.
* `tests/kuhttp.rs` — KuHTTP failure-injection suite (32 scenarios; see below).
* `tests/core_kuhttp.rs` — KuCore driving KuHTTP (routing, pause/resume,
  restart recovery).

## Packaging

```bash
cd extension && node build.mjs && node pack.mjs   # unpacked builds + dist/packages (kudmx.xpi, kudmx.crx)
node scripts/prepare-sidecars.mjs                 # Windows: aria2c, yt-dlp, ffmpeg + ku-native-host, ku
cd app && pnpm tauri build --config src-tauri/tauri.bundle.conf.json          # Windows .exe (NSIS)
cd app && pnpm tauri build --config src-tauri/tauri.bundle.linux.conf.json    # Linux .deb / .rpm
```

Windows: per-user NSIS installer with the engines bundled. Linux: `.deb`,
`.rpm` and an Arch package (`packaging/arch/PKGBUILD`, repackaged from the
`.deb`) that depend on the distribution's `aria2`, `ffmpeg` and `yt-dlp`.

CI (`.github/workflows/ci.yml`) runs the tests on Windows and Linux. Pushing a
`v*` tag builds the `.exe`, `.deb`, `.rpm` and `.pkg.tar.zst` into a **draft**
GitHub release. Optional repository secrets:

| Secret | Enables |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` (+ `_PASSWORD`) | signed updater artifacts and `latest.json` (key in `.secrets/updater.key`) |
| `KU_EXTENSION_KEY` | `kudmx.crx` in the installers (PEM text of `.secrets/extension-key.pem`) |

The update feed URL defaults to GitHub releases and can be changed in
Settings › Advanced.

## Security model

* The local API listens on 127.0.0.1 only, requires a per-run bearer token
  (stored in the per-user data folder), rejects requests carrying an `Origin`
  header and foreign `Host` headers.
* The native host accepts a fixed set of request types and only http, https,
  ftp, sftp and magnet URLs; it can start KuDownloader and nothing else.
* Downloaded files are never opened automatically. DRM-protected media is
  detected and refused.
* Credentials and cookies are sent only to the origin they belong to; KuHTTP
  strips them on cross-origin redirects.

## KuHTTP (experimental)

KuHTTP is KuDownloader's own HTTP engine (`crates/kucore/src/kuhttp`): real
range probing, a live segment map with work stealing, an adaptive connection
controller, strict `Content-Range` validation, ETag / Last-Modified / sampled
content checks on resume, fsync-before-persist crash safety, checksum
verification and atomic finalize. It is **off by default** — aria2 remains
the production HTTP engine until KuHTTP's benchmarks justify switching.
Enable it in Settings › Advanced › HTTP engine. Details and results:
[`docs/kuhttp.md`](docs/kuhttp.md).
