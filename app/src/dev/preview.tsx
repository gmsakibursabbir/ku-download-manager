/**
 * Development-only visual harness: renders the real App in a regular browser
 * with Tauri's official IPC mock so the UI can be reviewed against edge cases
 * (long names, failures, unknown sizes, hundreds of rows). Not part of the
 * production build — vite only bundles index.html.
 */
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Download, Settings } from "../lib/types";

const params = new URLSearchParams(location.search);
const count = Number(params.get("n") ?? 14);
const now = Date.now();
const MB = 1024 * 1024;

function dl(i: number, p: Partial<Download>): Download {
  return {
    id: `d${i}`,
    engine: "aria2",
    kind: "http",
    url: `https://releases.example.org/files/${p.name ?? "file"}`,
    mirrors: [],
    name: "file.bin",
    dir: "C:\\Users\\you\\Downloads",
    filePath: null,
    status: "downloading",
    total: 0,
    done: 0,
    uploaded: 0,
    speed: 0,
    uploadSpeed: 0,
    connections: 0,
    activeConnections: 0,
    category: "",
    queueId: null,
    position: i,
    createdAt: now - i * 60000,
    completedAt: null,
    error: null,
    gid: null,
    source: "ui",
    options: { headers: [], cookies: [] },
    meta: { files: [], retries: 0 },
    ...p,
  };
}

const base: Download[] = [
  dl(1, { name: "ubuntu-24.04.1-desktop-amd64.iso", category: "images-disk", total: 5.8 * 1024 * MB, done: 3.96 * 1024 * MB, speed: 24.6 * MB, activeConnections: 16, meta: { files: [], retries: 0, resumable: true, smartNote: "Throughput is scaling; increased to 16 connections." } }),
  dl(2, { name: "Windows11_24H2_EnglishInternational_x64_with_a_very_long_file_name_that_keeps_going.iso", category: "images-disk", total: 6.4 * 1024 * MB, done: 2.7 * 1024 * MB, speed: 18.2 * MB, activeConnections: 8 }),
  dl(3, { name: "DaVinci_Resolve_Studio_19.1.2_Windows.exe", category: "programs", total: 4.1 * 1024 * MB, done: 0.77 * 1024 * MB, speed: 0, activeConnections: 0 }),
  dl(4, { name: "Switzerland 4K – Breathtaking Nature Scenery.mp4", engine: "ytdlp", kind: "media", category: "video", total: 342 * MB, done: 120 * MB, speed: 8.7 * MB, meta: { files: [], retries: 0, thumbnail: "https://i.ytimg.com/vi/linlz7-Pnvw/hqdefault.jpg", uploader: "Scenic Relaxation" } }),
  dl(5, { name: "The.Last.of.Us.S01E01.1080p.mkv", kind: "torrent", category: "torrents", total: 3.4 * 1024 * MB, done: 1.9 * 1024 * MB, speed: 6.1 * MB, uploadSpeed: 400 * 1024, activeConnections: 24, url: "magnet:?xt=urn:btih:abc", meta: { files: [], retries: 0, infoHash: "c9e15763f722f23e98a29decdfae341b98d53056", numSeeders: 12 } }),
  dl(6, { name: "stream-without-length.bin", total: 0, done: 48 * MB, speed: 2.2 * MB, activeConnections: 1 }),
  dl(7, { name: "node-v22.9.0-x64.msi", category: "programs", status: "paused", total: 30 * MB, done: 12 * MB }),
  dl(8, { name: "dataset-2026-q3.tar.gz", category: "archives", status: "queued", queueId: "main", total: 12 * 1024 * MB }),
  dl(9, { name: "report-final-v3.pdf", category: "documents", status: "error", total: 2 * MB, done: 0, error: "The server refused access (HTTP 403). The link may have expired or require browser cookies." }),
  dl(10, { name: "Big Buck Bunny.mp4", engine: "ytdlp", kind: "media", category: "video", status: "processing", total: 158 * MB, done: 158 * MB }),
  dl(11, { name: "nature-4k-60fps.mp4", category: "video", status: "completed", total: 1.2 * 1024 * MB, done: 1.2 * 1024 * MB, completedAt: now - 3600_000, filePath: "C:\\Users\\you\\Downloads\\Video\\nature-4k-60fps.mp4" }),
  dl(12, { name: "debian-12.7.0-amd64-netinst.iso", kind: "torrent", category: "torrents", status: "seeding", total: 631 * MB, done: 631 * MB, uploaded: 820 * MB, uploadSpeed: 1.2 * MB, completedAt: now - 7200_000 }),
  dl(13, { name: "Album – Live at the Hall.flac", category: "music", status: "completed", total: 412 * MB, done: 412 * MB, completedAt: now - 86400_000 * 2 }),
  dl(14, { name: "tiny.txt", category: "documents", status: "completed", total: 312, done: 312, completedAt: now - 60_000 }),
];

const list: Download[] = [...base];
for (let i = base.length + 1; i <= count; i++) {
  const statuses = ["completed", "completed", "queued", "paused", "downloading", "error"] as const;
  const status = statuses[i % statuses.length];
  const total = ((i * 37) % 900) * MB + 5 * MB;
  list.push(dl(i, { name: `archive-part-${String(i).padStart(4, "0")}.zip`, category: "archives", status, total, done: status === "completed" ? total : total * ((i % 10) / 10), speed: status === "downloading" ? (i % 7) * MB : 0, completedAt: status === "completed" ? now - i * 1000 : null, error: status === "error" ? "The connection timed out." : null }));
}

const settings: Settings = {
  theme: (params.get("theme") as Settings["theme"]) ?? "dark",
  compact: params.get("compact") === "1",
  accent: params.get("accent") ?? "blue",
  darkPalette: params.get("palette") ?? "default",
  language: params.get("lang") ?? "en",
  translucent: false,
  startWithOs: false,
  minimizeToTray: true,
  clipboardMonitor: true,
  checkUpdates: true,
  notifyComplete: true,
  notifyError: true,
  notifyQueueDone: true,
  showProgressWindow: true,
  onboarded: params.get("welcome") !== "1",
  downloadDir: "C:\\Users\\you\\Downloads",
  useCategories: true,
  categories: [
    { id: "archives", name: "Archives", folder: "Archives", extensions: ["zip", "rar", "7z"] },
    { id: "images-disk", name: "Disk images", folder: "ISOs", extensions: ["iso"] },
    { id: "programs", name: "Programs", folder: "Programs", extensions: ["exe", "msi"] },
    { id: "video", name: "Video", folder: "Video", extensions: ["mp4", "mkv"] },
    { id: "music", name: "Music", folder: "Music", extensions: ["mp3", "flac"] },
    { id: "documents", name: "Documents", folder: "Documents", extensions: ["pdf"] },
    { id: "torrents", name: "Torrents", folder: "Torrents", extensions: [] },
  ],
  maxConcurrent: 4,
  defaultConnections: 0,
  fileExists: "rename",
  virusScan: "off",
  virusScanner: "",
  virusScannerArgs: '"{file}"',
  cookiesFile: "",
  cookiesFromBrowser: "",
  proxy: "",
  proxyUser: "",
  proxyPass: "",
  noProxy: "",
  userAgent: "",
  maxTries: 5,
  retryWait: 5,
  timeout: 60,
  connectTimeout: 30,
  autoRetry: 3,
  checkCertificate: true,
  activeProfile: "unlimited",
  profiles: [
    { id: "unlimited", name: "Unlimited", download: 0, upload: 0 },
    { id: "balanced", name: "Balanced", download: 10 * MB, upload: MB },
    { id: "background", name: "Background", download: 2 * MB, upload: 256 * 1024 },
  ],
  interceptDownloads: true,
  confirmBrowserDownloads: true,
  interceptMinSize: 0,
  interceptExtensions: [],
  skipDomains: [],
  extraExtensionIds: [],
  airsendEnabled: params.has("air"),
  airsendName: "",
  airsendAvatar: "",
  airsendFolder: "",
  airsendAutoAccept: false,
  airsendPin: "",
  airsendTrusted: ["b2"],
  hoverButton: true,
  mediaDetection: true,
  adapters: ["youtube", "vimeo", "generic"],
  videoDir: "C:\\Users\\you\\Downloads\\Video",
  videoHeight: 1080,
  videoContainer: "mp4",
  audioFormat: "mp3",
  audioBitrate: 320,
  subtitles: false,
  subLangs: "en",
  embedThumbnail: false,
  ytdlpPath: "",
  ffmpegPath: "",
  seedRatio: 0,
  seedTime: 0,
  btListenPort: "6881-6999",
  enableDht: true,
  btMaxPeers: 55,
  aria2Path: "",
  apiEnabled: true,
  updateEndpoint: "",
  httpEngine: "aria2",
};

const MOCK_BROWSERS = [
  { id: "google-chrome", name: "Google Chrome", family: "chromium", path: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe", extensionsUrl: "chrome://extensions/", key: "Software\\Google\\Chrome\\NativeMessagingHosts" },
  { id: "helium", name: "Helium", family: "chromium", path: "C:\\Users\\you\\AppData\\Local\\imput\\Helium\\Application\\chrome.exe", extensionsUrl: "chrome://extensions/", key: "Software\\imput\\Helium\\NativeMessagingHosts" },
  { id: "microsoft-edge", name: "Microsoft Edge", family: "chromium", path: "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe", extensionsUrl: "edge://extensions/", key: "Software\\Microsoft\\Edge\\NativeMessagingHosts" },
  { id: "zen", name: "Zen", family: "firefox", path: "C:\\Program Files\\Zen Browser\\zen.exe", extensionsUrl: "about:addons", key: "Software\\Mozilla\\NativeMessagingHosts" },
];

const airPeers = [
  { fingerprint: "b2", alias: "Living-room laptop", avatar: "panda", os: "windows", version: "0.2.3", ip: "192.168.1.20", port: 53318, trusted: true },
  { fingerprint: "c3", alias: "MacBook Air", avatar: "penguin", os: "macos", version: "0.2.3", ip: "192.168.1.31", port: 53318, trusted: false },
  { fingerprint: "d4", alias: "Arch desktop", avatar: "frog", os: "linux", version: "0.2.3", ip: "192.168.1.42", port: 53318, trusted: false },
  { fingerprint: "e5", alias: "Office PC", avatar: "cat", os: "windows", version: "0.2.3", ip: "192.168.1.51", port: 53318, trusted: false },
];
const airTransfers = [
  { id: "t1", direction: "send", peer: "MacBook Air", peerFingerprint: "c3", peerAvatar: "penguin", state: "transferring", files: [{ name: "Holiday video 4K.mp4", size: 3.2 * 1024 * MB, done: false }], fileCount: 1, total: 3.2 * 1024 * MB, done: 1.9 * 1024 * MB, speed: 86 * MB, started: now - 20000 },
  { id: "t2", direction: "receive", peer: "Living-room laptop", peerFingerprint: "b2", peerAvatar: "panda", state: "done", files: [{ name: "Photos/IMG_0001.jpg", size: 4 * MB, done: true }], fileCount: 248, total: 1.1 * 1024 * MB, done: 1.1 * 1024 * MB, speed: 0, started: now - 3600000, finished: now - 3500000, folder: "C:\Users\you\Downloads\KuAirSend" },
  { id: "t3", direction: "receive", peer: "Arch desktop", peerFingerprint: "d4", peerAvatar: "frog", state: "done", files: [], fileCount: 0, total: 0, done: 0, speed: 0, started: now - 7200000, finished: now - 7200000, text: "https://releases.ubuntu.com/24.04/ubuntu-24.04.1-desktop-amd64.iso" },
  { id: "t4", direction: "send", peer: "Office PC", peerFingerprint: "e5", peerAvatar: "cat", state: "declined", files: [{ name: "report-final-v3.pdf", size: 2 * MB, done: false }], fileCount: 1, total: 2 * MB, done: 0, speed: 0, started: now - 86400000, finished: now - 86400000 },
];

mockWindows("main");
mockIPC(
  (cmd, args) => {
    switch (cmd) {
      case "list_downloads":
        return params.get("empty") ? [] : list;
      case "app_ready":
        return [];
      case "get_settings":
        return settings;
      case "save_settings":
        Object.assign(settings, (args as { settings: Settings }).settings);
        return settings;
      case "list_queues":
        return [{ id: "main", name: "Main queue", maxConcurrent: 2, position: 0, running: false, after: "none", syncMinutes: 0, lastSync: 0 }];
      case "list_schedules":
        return [{ id: "s1", name: "Night downloads", enabled: true, queueId: "main", start: "02:00", stop: "07:00", days: [], date: null, profile: "background", after: "sleep" }];
      case "get_details":
        return {
          log: [
            { ts: now - 60000, level: "info", message: "Probe: application/x-iso9660-image · 6227702579 bytes · ranges supported" },
            { ts: now - 59000, level: "info", message: "Started with aria2 (gid 2f1c5a9e8d7b6c4a)" },
            { ts: now - 20000, level: "info", message: "Throughput is scaling; increased to 16 connections." },
          ],
          smart: 16,
          connections: Array.from({ length: 16 }, (_, i) => ({ uri: "", currentUri: "https://releases.example.org/", speed: (1.1 + ((i * 7) % 6) / 10) * MB })),
          pieces: { count: 180, length: 32 * MB, bitfield: "ffffffffffffffff" + "ff00ff".repeat(4) },
        };
      case "stats":
        return {};
      case "app_info":
        return { version: "0.1.0", dataDir: "C:\\Users\\you\\AppData\\Roaming\\KuDownloader", apiPort: 64669, platform: "windows", defaultDownloadDir: "C:\\Users\\you\\Downloads" };
      case "native_host_status":
        return {
          hostPath: "C:\\Program Files\\KuDownloader\\ku-native-host.exe",
          hostExists: true,
          chromeExtensionId: "bmbpbbaapbbelemahbmnjhppdlpdgkdi",
          firefoxExtensionId: "kudownloader@kuduy.digital",
          browsers: MOCK_BROWSERS.map((b) => ({ browser: b.name, registered: true, installed: true, browserId: b.id, location: "HKCU\\" + b.key + "\\com.kuduy.kudownloader" })),
        };
      case "detect_browsers":
        return MOCK_BROWSERS.map(({ key: _k, ...b }) => b);
      case "check_duplicate": {
        const u = (args as { url: string }).url;
        const existing = list.find((d) => d.url === u) ?? null;
        return { existing, existingFileExists: existing?.status === "completed", fileExists: false, path: null, policy: "rename" };
      }
      case "redownload":
        return null;
      case "airsend_status":
      case "airsend_set_enabled":
        if (cmd === "airsend_set_enabled") settings.airsendEnabled = (args as { enabled: boolean }).enabled;
        return { running: settings.airsendEnabled, alias: settings.airsendName || "Studio-PC", avatar: settings.airsendAvatar || "fox", fingerprint: "a1", port: 53318, addresses: ["192.168.1.14"], folder: "C:\Users\you\Downloads\KuAirSend", discoveryError: null };
      case "airsend_peers":
        return settings.airsendEnabled ? airPeers : [];
      case "airsend_transfers":
        return airTransfers;
      case "get_download":
        return list.find((d) => d.id === (args as { id: string }).id) ?? null;
      case "platform_info": {
        const os = params.get("os") ?? "windows";
        const desktop = params.get("desktop") ?? "";
        const tiling = ["niri", "hyprland", "sway", "i3"].includes(desktop);
        const layouts: Record<string, [string[], string[]]> = { gnome: [[], ["close"]], pantheon: [["close"], ["maximize"]] };
        const [left, right] = os === "macos" || tiling ? [[], []] : (layouts[desktop] ?? [[], ["minimize", "maximize", "close"]]);
        return { os, desktop, tiling, left, right };
      }
      case "install_tool": {
        // Simulated download: ~6 s of progress, then done.
        const tool = (args as { name: string }).name;
        const total = tool === "ffmpeg" ? 82 * MB : 17 * MB;
        return (async () => {
          const { emit } = await import("@tauri-apps/api/event");
          for (let i = 1; i <= 24; i++) {
            await new Promise((r) => setTimeout(r, 250));
            await emit("ku", { type: "toolProgress", tool, done: Math.round((total * i) / 24), total });
          }
          await emit("ku", { type: "toolDone", tool, ok: true, message: "" });
          return "installed";
        })();
      }
      case "tool_jobs":
        return [];
      case "get_prompt":
        return { url: "https://download.example.org/releases/KuSetup-2.4.1-x64.exe", source: "browser", sizeHint: 88 * MB, options: { headers: [], cookies: [{ name: "s", value: "1", domain: "example.org" }], referer: "https://example.org/download" } };
      case "extension_last_seen":
        return Date.now() - 60000;
      case "install_extension":
        return { mode: (args as { browser?: string } | undefined)?.browser === "zen" ? "temporary" : "unpacked", copied: true };
      case "extension_dirs":
        return {
          chrome: "C:\\Program Files\\KuDownloader\\extension\\chrome",
          firefox: "C:\\Program Files\\KuDownloader\\extension\\firefox",
          crx: "C:\\Program Files\\KuDownloader\\extension\\kudmx.crx",
          xpi: "C:\\Program Files\\KuDownloader\\extension\\kudmx.xpi",
          xpiSigned: false,
        };
      case "engine_info":
        return { aria2: { path: "C:\\Program Files\\KuDownloader\\aria2c.exe", version: "1.37.0", running: true }, ytdlp: { path: "C:\\Program Files\\KuDownloader\\yt-dlp.exe", version: "2026.08.19" }, ffmpeg: { path: null }, dataDir: "" };
      case "get_after_all":
        return "none";
      case "probe_url":
        return { url: "", finalUrl: "", filename: "ubuntu-24.04.1-desktop-amd64.iso", size: 6227702579, mime: "application/x-iso9660-image", resumable: true, engine: "aria2", kind: "http", category: "images-disk" };
      case "plugin:window|is_maximized":
        return false;
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

// Live-looking progress so the rows and speed graph can be judged in motion.
setInterval(async () => {
  const { emit } = await import("@tauri-apps/api/event");
  const items = list
    .filter((d) => d.status === "downloading" || d.status === "seeding")
    .map((d) => {
      const jitter = 0.85 + Math.random() * 0.3;
      d.speed = d.status === "seeding" ? 0 : Math.round(d.speed ? d.speed * jitter : 0);
      if (d.total) d.done = Math.min(d.total, d.done + d.speed);
      else d.done += d.speed;
      return { id: d.id, status: d.status, done: d.done, total: d.total, speed: d.speed, uploadSpeed: d.uploadSpeed, activeConnections: d.activeConnections };
    });
  await emit("ku", { type: "progress", items, downloadSpeed: items.reduce((a, b) => a + b.speed, 0), uploadSpeed: 1.6 * MB });
}, 1000);

await import("../main");
