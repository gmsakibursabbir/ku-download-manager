# KuDownloader for Android

The same engine as the desktop app (KuCore, Rust) with a Jetpack Compose
interface. Everything the desktop does, on a phone:

| | |
|---|---|
| Downloads | multi-connection, resumable (KuHTTP), pause/resume, redownload, edit (refresh expired links, connections, speed limit, checksum), duplicate check, categories, search, speed graph with upload line, bandwidth profiles |
| Video and music | yt-dlp (bundled Python) + FFmpeg: quality list, M4A/Opus/MP3/FLAC, subtitles, cover art, playlists |
| Torrents | aria2: magnet links, `.torrent` files with file selection, seeding, peers |
| Batch / Fetch Projects | link lists and `[001-120]` patterns; every file linked on a page |
| Queues and schedules | parallel limits, synchronization, one-off and weekly schedules (exact alarms wake the app) |
| Browser | tabs, history, bookmarks, desktop mode, EasyList/EasyPrivacy ad blocking (Brave adblock-rust, network + element hiding), the KuDownload button on videos, media detection, download takeover, pop-up blocking |
| KuAirSend | files, folders, text and links to nearby KuDownloader (phone ⇄ PC), **download on another device** now or at a set time, PIN, trusted devices, radar with pixel animals |
| Phone | share-to-KuDownloader, magnet/torrent handler, copied-link offer, Quick Settings tile, notification actions, Wi-Fi only, pause in battery saver, background service with wake/Wi-Fi/multicast locks, Material You or the desktop's accent colours, 12 languages (shared with the desktop) |

## Layout

```
crates/ku-android/      JNI bridge: KuCore, KuAirSend, ad blocker → libkudroid.so
android/app/            Compose app (package digital.kuduy.kudownloader)
android/i18n/           phone-only translations (merged with app/src/lib/locales)
android/scripts/        build-native.sh, locales.mjs, sprites.cjs
```

## Build

Needs JDK 17, the Android SDK (platform 35) and NDK, Rust with the Android
targets, `cargo-ndk`, Node 20+ and Gradle 8.11 (or Android Studio).

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo install cargo-ndk
android/scripts/build-native.sh            # KuCore → app/src/main/jniLibs
cd android && gradle assembleRelease       # APKs in app/build/outputs/apk/release
```

CI (`.github/workflows/android.yml`) does the same and attaches the APKs to the
release for a `v*` tag. Signing: `android/keystore.properties`
(`storeFile`, `storePassword`, `keyAlias`, `keyPassword`) or the
`KU_ANDROID_*` secrets; without them builds use the debug key.

Translations: `node android/scripts/locales.mjs --check` lists strings the
phone uses that a language lacks. The pixel animals come from the desktop
(`node android/scripts/sprites.cjs` after changing `PixelAnimal.tsx`).

Distribution is GitHub releases (or F-Droid): Google Play does not accept
apps that download from YouTube.
