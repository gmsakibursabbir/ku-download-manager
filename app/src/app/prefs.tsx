import { useEffect, useState } from "react";
import { t, t as tr } from "../lib/i18n";
import { open } from "@tauri-apps/plugin-dialog";
import { Download, Folder, RefreshCw } from "lucide-react";
import { settingsStore, updateSettings } from "../lib/store";
import { api, errorText } from "../lib/api";
import type { EngineInfo, Settings } from "../lib/types";
import * as fmt from "../lib/format";
import { Button, IconButton, Input, PrefGroup, PrefRow, Select, Switch, Checkbox } from "../ui/primitives";
import { toast } from "../ui/overlays";
import { useToolInstaller } from "./MediaTools";

/** Audio formats; the fast ones are copied out of the stream, the others re-encoded. */
export const AUDIO_FORMATS = () => [
  { value: "m4a", label: t("M4A (AAC) · fast") },
  { value: "best", label: t("Original · fastest") },
  { value: "opus", label: t("Opus · fast") },
  { value: "mp3", label: t("MP3 · converts, slower") },
  { value: "flac", label: t("FLAC · converts, slower") },
];

export function useSettings(): Settings | null {
  return settingsStore.use();
}

export async function save(patch: Partial<Settings>) {
  try {
    await updateSettings(patch);
  } catch (e) {
    toast({ level: "error", title: t("Could not save the setting"), message: errorText(e) });
  }
}

/** Boolean preference with a switch. */
export function SwitchPref<K extends keyof Settings>({ k, label, desc }: { k: K; label: string; desc?: string }) {
  const s = useSettings();
  if (!s) return null;
  return (
    <PrefRow label={label} desc={desc}>
      <Switch label={label} checked={!!s[k]} onChange={(v) => void save({ [k]: v } as Partial<Settings>)} />
    </PrefRow>
  );
}

/** Text preference committed on blur / Enter. */
export function TextPref<K extends keyof Settings>({
  k,
  label,
  desc,
  placeholder,
  width = 280,
  type = "text",
  mono,
}: {
  k: K;
  label: string;
  desc?: string;
  placeholder?: string;
  width?: number;
  type?: string;
  mono?: boolean;
}) {
  const s = useSettings();
  const value = s ? String(s[k] ?? "") : "";
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  if (!s) return null;
  const commit = () => draft !== value && void save({ [k]: draft } as Partial<Settings>);
  return (
    <PrefRow label={label} desc={desc}>
      <Input
        type={type}
        value={draft}
        placeholder={placeholder}
        className={mono ? "mono" : undefined}
        style={{ width }}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && commit()}
      />
    </PrefRow>
  );
}

/** Numeric preference; `unit` is shown after the input. */
export function NumberPref<K extends keyof Settings>({ k, label, desc, min, max, unit, step = 1, scale = 1 }: { k: K; label: string; desc?: string; min: number; max: number; unit?: string; step?: number; scale?: number }) {
  const s = useSettings();
  const value = s ? Number(s[k] ?? 0) / scale : 0;
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  if (!s) return null;
  const commit = () => {
    const n = Math.max(min, Math.min(max, Number(draft)));
    if (Number.isFinite(n) && n !== value) void save({ [k]: Math.round(n * scale) } as Partial<Settings>);
    else setDraft(String(value));
  };
  return (
    <PrefRow label={label} desc={desc}>
      <Input type="number" min={min} max={max} step={step} value={draft} style={{ width: 88 }} onChange={(e) => setDraft(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />
      {unit && <span className="faint" style={{ fontSize: "var(--text-xs)", minWidth: 48 }}>{unit}</span>}
    </PrefRow>
  );
}

export function SelectPref<K extends keyof Settings>({ k, label, desc, options, width = 200 }: { k: K; label: string; desc?: string; options: { value: string | number; label: string }[]; width?: number }) {
  const s = useSettings();
  if (!s) return null;
  const numeric = typeof s[k] === "number";
  return (
    <PrefRow label={label} desc={desc}>
      <Select value={String(s[k])} style={{ width }} onChange={(e) => void save({ [k]: numeric ? Number(e.target.value) : e.target.value } as Partial<Settings>)} options={options.map((o) => ({ value: String(o.value), label: o.label }))} />
    </PrefRow>
  );
}

export function FolderPref<K extends keyof Settings>({ k, label, desc }: { k: K; label: string; desc?: string }) {
  const s = useSettings();
  if (!s) return null;
  const value = String(s[k] ?? "");
  return (
    <PrefRow label={label} desc={desc} stack>
      <div className="input-group" style={{ width: "100%" }}>
        <Input value={value} readOnly onDoubleClick={() => void pick()} />
        <IconButton icon={Folder} label={t("Choose folder")} onClick={() => void pick()} />
      </div>
    </PrefRow>
  );
  async function pick() {
    const p = await open({ directory: true, defaultPath: value || undefined });
    if (typeof p === "string") void save({ [k]: p } as Partial<Settings>);
  }
}

/** Comma separated list preference. */
export function ListPref<K extends keyof Settings>({ k, label, desc, placeholder }: { k: K; label: string; desc?: string; placeholder?: string }) {
  const s = useSettings();
  const value = s ? ((s[k] as unknown as string[]) ?? []).join(", ") : "";
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  if (!s) return null;
  const commit = () => {
    const list = draft
      .split(/[,\s]+/)
      .map((x) => x.trim())
      .filter(Boolean);
    if (list.join(", ") !== value) void save({ [k]: list } as Partial<Settings>);
  };
  return (
    <PrefRow label={label} desc={desc} stack>
      <Input value={draft} placeholder={placeholder} onChange={(e) => setDraft(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />
    </PrefRow>
  );
}

export const VIDEO_HEIGHTS = [2160, 1440, 1080, 720, 480, 360].map((h) => ({ value: h, label: `${h}p` }));

const ADAPTERS = [
  { id: "youtube", label: "YouTube", desc: "Button on thumbnails and the player." },
  { id: "vimeo", label: "Vimeo", desc: "Button on the player." },
  { id: "dailymotion", label: "Dailymotion", desc: "Button on the player." },
  { id: "generic", label: "Other sites", desc: "Button on any HTML5 video larger than 320 × 180." },
];

export function MediaPrefs() {
  const s = useSettings();
  if (!s) return null;
  const toggleAdapter = (id: string, on: boolean) => void save({ adapters: on ? [...new Set([...s.adapters, id])] : s.adapters.filter((a) => a !== id) });
  return (
    <>
      <PrefGroup title={t("In the browser")}>
        <SwitchPref k="hoverButton" label={t("Show the KuDownload button on videos")} desc={t("Adds a small download button to supported media on web pages.")} />
        <SwitchPref k="mediaDetection" label={t("Detect media on pages")} desc={t("Lists video and audio streams in the extension popup.")} />
      </PrefGroup>
      <PrefGroup title={t("Sites")}>
        {ADAPTERS.map((a) => (
          <PrefRow key={a.id} label={t(a.label)} desc={t(a.desc)}>
            <Switch label={t(a.label)} checked={s.adapters.includes(a.id)} disabled={!s.hoverButton} onChange={(v) => toggleAdapter(a.id, v)} />
          </PrefRow>
        ))}
      </PrefGroup>
      <PrefGroup title={t("Defaults")}>
        <SelectPref k="videoHeight" label={t("Video quality")} desc={t("Used when a quality isn't chosen explicitly. Lower qualities are picked when unavailable.")} options={VIDEO_HEIGHTS} width={120} />
        <SelectPref k="videoContainer" label={t("Video format")} options={[{ value: "mp4", label: "MP4" }, { value: "mkv", label: "MKV" }, { value: "webm", label: "WebM" }]} width={120} />
        <SelectPref k="audioFormat" label={t("Audio format")} options={AUDIO_FORMATS()} width={120} />
        <SelectPref k="audioBitrate" label={t("Audio bitrate")} options={[320, 256, 192, 128].map((b) => ({ value: b, label: `${b} kbps` }))} width={120} />
        <SwitchPref k="subtitles" label={t("Download subtitles")} desc={t("When available, embedded into the video.")} />
        <TextPref k="subLangs" label={t("Subtitle languages")} desc={t("Comma separated codes, e.g. en, de, fr.")} width={160} />
        <SwitchPref k="embedThumbnail" label={t("Embed thumbnail as cover art")} />
        <FolderPref k="videoDir" label={t("Save videos to")} />
      </PrefGroup>
      <PrefGroup title={t("Sign-in for media sites")}>
        <SelectPref
          k="cookiesFromBrowser"
          label={t("Use cookies from a browser")}
          desc={t("For age-restricted or members-only videos added without the extension. Firefox works best; recent Chrome versions on Windows lock their cookies while running.")}
          options={[{ value: "", label: t("Off") }, ...["firefox", "chrome", "edge", "brave", "chromium", "vivaldi", "opera"].map((b) => ({ value: b, label: b[0].toUpperCase() + b.slice(1) }))]}
          width={160}
        />
        <TextPref k="cookiesFile" label={t("Or a cookies.txt file")} desc={t("Netscape format, exported with a browser add-on. Cookies sent by the KuDownloader extension are always preferred.")} placeholder={t("None")} mono />
      </PrefGroup>
    </>
  );
}

export function BrowserPrefs() {
  return (
    <>
      <PrefGroup title={t("Downloads from the browser")}>
        <SwitchPref k="interceptDownloads" label={t("Take over browser downloads")} desc={t("Files you download in the browser are sent to KuDownloader instead.")} />
        <SwitchPref k="confirmBrowserDownloads" label={t("Ask before downloading")} desc={t("Shows the Add download dialog so you can pick the folder and name.")} />
        <NumberPref k="interceptMinSize" label={t("Only files larger than")} desc={t("Smaller files stay in the browser. 0 = all files.")} min={0} max={1024 * 1024} unit="MB" scale={1024 * 1024} step={0.5} />
        <ListPref k="interceptExtensions" label={t("Only these file types")} desc={t("Leave empty to take over every download.")} placeholder="zip, iso, exe, mp4" />
        <ListPref k="skipDomains" label={t("Never take over from these sites")} placeholder="example.com, intranet.local" />
      </PrefGroup>
    </>
  );
}

export function EngineStatus({ compact }: { compact?: boolean }) {
  const [info, setInfo] = useState<EngineInfo | null>(null);
  const [updating, setUpdating] = useState(false);
  const load = () => void api.engineInfo().then(setInfo).catch(() => {});
  useEffect(load, []);
  const tools = useToolInstaller(load);
  // FFmpeg on demand: Windows and Linux (macOS: Homebrew).
  const windows = !navigator.userAgent.includes("Mac");
  const get = (t: "yt-dlp" | "ffmpeg") =>
    compact ? undefined : (
      <Button size="sm" icon={Download} busy={tools.busy === t} disabled={!!tools.busy} onClick={() => void tools.install([t])}>
        {tools.label(t) ?? tr("Download")}
      </Button>
    );
  const update = async () => {
    setUpdating(true);
    try {
      const msg = await api.updateYtdlp();
      toast({ level: "success", title: "yt-dlp", message: msg });
      load();
    } catch (e) {
      toast({ level: "error", title: t("Could not update yt-dlp"), message: errorText(e) });
    } finally {
      setUpdating(false);
    }
  };
  const row = (label: string, found: boolean, detail: string, action?: React.ReactNode) => (
    <PrefRow label={label} desc={detail}>
      <span className="status" data-state={found ? "completed" : "error"}>
        {found ? t("Available") : t("Not found")}
      </span>
      {action}
    </PrefRow>
  );
  return (
    <PrefGroup title={t("Engines")}>
      {row("aria2", !!info?.aria2.path, info?.aria2.path ? `${info.aria2.version ? `Version ${info.aria2.version} · ` : ""}${info.aria2.path}` : "Required for file, FTP and torrent downloads.")}
      {row(
        "yt-dlp",
        !!info?.ytdlp.path,
        info?.ytdlp.path ? `Version ${info.ytdlp.version ?? "unknown"} · ${info.ytdlp.path}` : "Required for video and audio downloads.",
        info?.ytdlp.path && !compact ? (
          <Button size="sm" icon={RefreshCw} busy={updating} onClick={() => void update()}>
            {t("Update")}
          </Button>
        ) : info && !info.ytdlp.path ? (
          get("yt-dlp")
        ) : undefined,
      )}
      {row(
        "FFmpeg",
        !!info?.ffmpeg.path,
        info?.ffmpeg.path ?? (windows ? "Needed to merge high-quality video with audio and to convert audio." : "Needed to merge video with audio. Install it with Homebrew: brew install ffmpeg"),
        info && !info.ffmpeg.path && windows ? get("ffmpeg") : undefined,
      )}
    </PrefGroup>
  );
}

export function Checks({ items }: { items: { label: string; checked: boolean; onChange: (v: boolean) => void }[] }) {
  return (
    <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--space-3)" }}>
      {items.map((i) => (
        <Checkbox key={i.label} checked={i.checked} onChange={i.onChange}>
          {i.label}
        </Checkbox>
      ))}
    </div>
  );
}

export { fmt };
