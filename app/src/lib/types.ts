// Mirrors of the Rust types in ku-proto / kucore (serde camelCase).

export type Engine = "aria2" | "ytdlp" | "kuhttp";
export type Kind = "http" | "ftp" | "sftp" | "torrent" | "magnet" | "metalink" | "media";
export type Status = "queued" | "downloading" | "processing" | "paused" | "seeding" | "completed" | "error";

export interface BrowserCookie {
  name: string;
  value: string;
  domain: string;
  path: string;
  secure: boolean;
  httpOnly: boolean;
  hostOnly: boolean;
  expirationDate?: number | null;
}

export interface MediaOptions {
  mode: "video" | "audio" | string;
  height?: number | null;
  audioBitrate?: number | null;
  container?: string | null;
  formatId?: string | null;
  subtitles: boolean;
  subLangs?: string | null;
  embedSubtitles: boolean;
  writeThumbnail: boolean;
  embedThumbnail: boolean;
  playlist: boolean;
  playlistItems?: string | null;
}

export interface DownloadOptions {
  headers: string[];
  cookies: BrowserCookie[];
  referer?: string | null;
  userAgent?: string | null;
  username?: string | null;
  password?: string | null;
  checksum?: string | null;
  proxy?: string | null;
  speedLimit?: number | null;
  media?: MediaOptions | null;
  torrentData?: string | null;
  metalinkData?: string | null;
  selectFiles?: string | null;
}

export interface FileEntry {
  path: string;
  length: number;
  completed: number;
  selected: boolean;
}

export interface DownloadMeta {
  mime?: string | null;
  resumable?: boolean | null;
  thumbnail?: string | null;
  mediaTitle?: string | null;
  uploader?: string | null;
  duration?: number | null;
  infoHash?: string | null;
  numSeeders?: number | null;
  files: FileEntry[];
  smartNote?: string | null;
  verified?: string | null;
  retries: number;
  playlistIndex?: string | null;
}

export interface Download {
  id: string;
  engine: Engine;
  kind: Kind;
  url: string;
  mirrors: string[];
  name: string;
  dir: string;
  filePath?: string | null;
  status: Status;
  total: number;
  done: number;
  uploaded: number;
  speed: number;
  uploadSpeed: number;
  connections: number;
  activeConnections: number;
  category: string;
  queueId?: string | null;
  position: number;
  createdAt: number;
  completedAt?: number | null;
  error?: string | null;
  gid?: string | null;
  source: string;
  options: DownloadOptions;
  meta: DownloadMeta;
}

export interface AddRequest {
  url: string;
  mirrors?: string[];
  dir?: string | null;
  filename?: string | null;
  connections?: number | null;
  category?: string | null;
  queueId?: string | null;
  start?: boolean | null;
  engine?: Engine | null;
  options?: Partial<DownloadOptions>;
  sizeHint?: number | null;
  source?: string | null;
  prompt?: boolean;
}

export interface ProbeInfo {
  url: string;
  finalUrl: string;
  filename?: string | null;
  size?: number | null;
  mime?: string | null;
  resumable?: boolean | null;
  status?: number | null;
  engine?: Engine | null;
  kind?: Kind | null;
  category?: string | null;
  error?: string | null;
}

export interface VideoQuality {
  height: number;
  label: string;
  fps?: number | null;
  ext: string;
  vcodec: string;
  size?: number | null;
  hdr: boolean;
}

export interface AudioQuality {
  bitrate: number;
  ext: string;
  acodec: string;
  size?: number | null;
}

export interface PlaylistEntry {
  index: number;
  id: string;
  title: string;
  url: string;
  duration?: number | null;
}

export interface MediaInfo {
  url: string;
  webpageUrl: string;
  extractor: string;
  title: string;
  uploader?: string | null;
  duration?: number | null;
  viewCount?: number | null;
  uploadDate?: string | null;
  thumbnail?: string | null;
  isLive: boolean;
  isPlaylist: boolean;
  playlistCount?: number | null;
  entries: PlaylistEntry[];
  video: VideoQuality[];
  audio: AudioQuality[];
  subtitles: string[];
  autoSubtitles: string[];
  ffmpegAvailable: boolean;
}

export interface MediaRequest {
  url: string;
  dir?: string | null;
  media: MediaOptions;
  cookies?: BrowserCookie[];
  referer?: string | null;
  userAgent?: string | null;
  queueId?: string | null;
  title?: string | null;
  thumbnail?: string | null;
  sizeHint?: number | null;
  source?: string | null;
}

export interface GrabLink {
  url: string;
  text?: string | null;
  kind?: string | null;
}

export interface GrabRequest {
  pageUrl?: string | null;
  links: GrabLink[];
  referer?: string | null;
  cookies: BrowserCookie[];
  userAgent?: string | null;
}

export interface Queue {
  id: string;
  name: string;
  maxConcurrent: number;
  position: number;
  running: boolean;
  after: string;
  /** Synchronization: re-check finished files every N minutes (0 = off). */
  syncMinutes: number;
  lastSync: number;
}

export interface Schedule {
  id: string;
  name: string;
  enabled: boolean;
  queueId: string;
  start: string;
  stop?: string | null;
  days: number[];
  date?: string | null;
  profile?: string | null;
  after: string;
  lastStart?: string | null;
  lastStop?: string | null;
}

export interface Category {
  id: string;
  name: string;
  folder: string;
  extensions: string[];
}

export interface BandwidthProfile {
  id: string;
  name: string;
  download: number;
  upload: number;
}

export interface Settings {
  theme: "light" | "dark" | "system";
  accent: string;
  darkPalette: string;
  language: string;
  compact: boolean;
  translucent: boolean;
  startWithOs: boolean;
  minimizeToTray: boolean;
  clipboardMonitor: boolean;
  checkUpdates: boolean;
  notifyComplete: boolean;
  notifyError: boolean;
  notifyQueueDone: boolean;
  showProgressWindow: boolean;
  onboarded: boolean;
  downloadDir: string;
  useCategories: boolean;
  categories: Category[];
  maxConcurrent: number;
  defaultConnections: number;
  fileExists: "rename" | "overwrite" | "skip";
  virusScan: "off" | "defender" | "custom";
  virusScanner: string;
  virusScannerArgs: string;
  cookiesFile: string;
  cookiesFromBrowser: string;
  proxy: string;
  proxyUser: string;
  proxyPass: string;
  noProxy: string;
  userAgent: string;
  maxTries: number;
  retryWait: number;
  timeout: number;
  connectTimeout: number;
  autoRetry: number;
  checkCertificate: boolean;
  activeProfile: string;
  profiles: BandwidthProfile[];
  interceptDownloads: boolean;
  confirmBrowserDownloads: boolean;
  interceptMinSize: number;
  interceptExtensions: string[];
  skipDomains: string[];
  extraExtensionIds: string[];
  hoverButton: boolean;
  mediaDetection: boolean;
  adapters: string[];
  videoDir: string;
  videoHeight: number;
  videoContainer: string;
  audioFormat: string;
  audioBitrate: number;
  subtitles: boolean;
  subLangs: string;
  embedThumbnail: boolean;
  ytdlpPath: string;
  ffmpegPath: string;
  seedRatio: number;
  seedTime: number;
  btListenPort: string;
  enableDht: boolean;
  btMaxPeers: number;
  aria2Path: string;
  apiEnabled: boolean;
  updateEndpoint: string;
  httpEngine: "aria2" | "kuhttp";
}

export interface ProgressItem {
  id: string;
  status: Status;
  done: number;
  total: number;
  speed: number;
  uploadSpeed: number;
  activeConnections: number;
  eta?: number | null;
}

export interface LogLine {
  ts: number;
  level: string;
  message: string;
}

export type CoreEvent =
  | { type: "progress"; items: ProgressItem[]; downloadSpeed: number; uploadSpeed: number }
  | { type: "upsert"; download: Download }
  | { type: "toolProgress"; tool: string; done: number; total: number }
  | { type: "removed"; ids: string[] }
  | { type: "notice"; level: string; title: string; message: string; downloadId?: string | null }
  | { type: "completed"; id: string; name: string; path?: string | null }
  | { type: "queueDone"; queueId: string; name: string }
  | { type: "promptAdd"; request: AddRequest }
  | { type: "promptMedia"; request: MediaRequest }
  | { type: "grab"; request: GrabRequest }
  | { type: "clipboardUrl"; url: string }
  | { type: "powerCountdown"; action: string; seconds: number }
  | { type: "powerCancelled" }
  | { type: "show" }
  | { type: "settingsChanged" }
  | { type: "queuesChanged" }
  | { type: "schedulesChanged" };

export interface Details {
  log: LogLine[];
  smart?: number | null;
  pieces?: { count: number; length: number; bitfield?: string };
  files?: { index: string; path: string; length: string; completedLength: string; selected: string }[];
  trackers?: string[];
  comment?: string | null;
  connections?: { uri: string; currentUri: string; speed: number }[];
  peers?: { ip: string; port: string; seeder: boolean; downloadSpeed: number; uploadSpeed: number; bitfield?: string }[];
  /** KuHTTP live segment map. */
  segments?: { start: number; end: number; downloaded: number; active: boolean; done: boolean; speed: number; retries: number }[];
}

export interface TorrentInfo {
  name: string;
  infoHash: string;
  total: number;
  pieceLength: number;
  private: boolean;
  comment?: string | null;
  createdBy?: string | null;
  trackers: string[];
  files: { index: number; path: string; length: number }[];
}

export interface HostStatus {
  hostPath?: string | null;
  hostExists: boolean;
  chromeExtensionId: string;
  firefoxExtensionId: string;
  browsers: { browser: string; registered: boolean; location: string; browserId?: string | null; installed: boolean }[];
}

export interface EngineInfo {
  aria2: { path?: string | null; version?: string | null; running: boolean };
  ytdlp: { path?: string | null; version?: string | null };
  ffmpeg: { path?: string | null };
  dataDir: string;
}

export interface AppInfo {
  version: string;
  dataDir: string;
  apiPort?: number | null;
  platform: string;
  defaultDownloadDir: string;
}

export interface UpdateInfo {
  version: string;
  currentVersion: string;
  notes?: string | null;
  date?: string | null;
}
