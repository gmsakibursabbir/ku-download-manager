import { useEffect, useMemo, useState } from "react";
import { t } from "../lib/i18n";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  ChevronDown,
  ChevronRight,
  CircleCheck,
  CircleDashed,
  Clapperboard,
  File,
  ListPlus,
  Radar,
  Hand,
  ListOrdered,
  Minus,
  Square,
  Copy,
  X,
  type LucideIcon,
} from "lucide-react";
import { Icon } from "../ui/primitives";
import { showMenuAt, type MenuItem } from "../ui/overlays";
import { useApp, type ListFilter } from "./context";
import { allDownloads, queuesStore, settingsStore, updateSettings, useDownloadIds, useSpeed } from "../lib/store";
import { api } from "../lib/api";
import * as fmt from "../lib/format";
import { run } from "./downloads/actions";
import { CATEGORY_ICON } from "./downloads/FileGlyph";
import { invoke } from "@tauri-apps/api/core";
import { pasteLink, pickTorrent } from "./downloads/DownloadsView";
import { useAir } from "../lib/airsend";


export interface PlatformInfo {
  os: "windows" | "macos" | "linux" | string;
  desktop: string;
  tiling: boolean;
  left: string[];
  right: string[];
}

let platformCache: Promise<PlatformInfo> | null = null;
/** OS / desktop / window-button layout (asked once; also sets data-os etc. on <html>). */
export function loadPlatform(): Promise<PlatformInfo> {
  platformCache ??= invoke<PlatformInfo>("platform_info")
    .catch((): PlatformInfo => ({ os: navigator.userAgent.includes("Mac") ? "macos" : navigator.userAgent.includes("Windows") ? "windows" : "linux", desktop: "", tiling: false, left: [], right: ["minimize", "maximize", "close"] }))
    .then((p) => {
      const root = document.documentElement;
      root.dataset.os = p.os;
      root.dataset.desktop = p.desktop || "";
      root.dataset.tiling = String(p.tiling);
      return p;
    });
  return platformCache;
}

function usePlatform(): PlatformInfo | null {
  const [p, setP] = useState<PlatformInfo | null>(null);
  useEffect(() => void loadPlatform().then(setP), []);
  return p;
}

function WindowControls({ buttons }: { buttons: string[] }) {
  const win = useMemo(() => getCurrentWindow(), []);
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    const sync = (m: boolean) => {
      setMaximized(m);
      // No window edge while maximised (see .app outline).
      document.documentElement.dataset.maximized = String(m);
    };
    void win.isMaximized().then(sync);
    const un = win.onResized(() => void win.isMaximized().then(sync));
    return () => {
      void un.then((f) => f());
    };
  }, [win]);
  if (!buttons.length) return null;
  return (
    <div className="window-controls">
      {buttons.map((b) =>
        b === "minimize" ? (
          <button key={b} type="button" aria-label={t("Minimize")} onClick={() => void win.minimize()}>
            <Icon icon={Minus} size={14} />
          </button>
        ) : b === "maximize" ? (
          <button key={b} type="button" aria-label={maximized ? "Restore" : "Maximize"} onClick={() => void win.toggleMaximize()}>
            <Icon icon={maximized ? Copy : Square} size={12} />
          </button>
        ) : (
          <button key={b} type="button" aria-label={t("Close")} className="close" onClick={() => void win.close()}>
            <Icon icon={X} size={15} />
          </button>
        ),
      )}
    </div>
  );
}


export function TitleBar() {
  const app = useApp();
  const s = settingsStore.use();
  const platform = usePlatform();
  const left = platform?.left ?? [];
  const right = platform ? platform.right : [];
  // macOS shows ⌘ instead of Ctrl (the handlers accept both).
  const kb = (k: string) => (platform?.os === "macos" ? k.replace("Ctrl ", "⌘") : k);
  const menus: Record<string, () => MenuItem[]> = {
    Tasks: () => [
      { label: t("Add URL…"), shortcut: kb("Ctrl N"), onSelect: () => app.openAdd() },
      { label: t("Add from clipboard"), shortcut: kb("Ctrl V"), onSelect: () => void pasteLink(app.openAdd) },
      { label: t("Add batch download…"), onSelect: () => app.navigate("batch") },
      "sep",
      { label: t("Video downloader…"), onSelect: () => app.openMedia() },
      { label: t("Grab links from a page…"), onSelect: () => app.navigate("grabber") },
      "sep",
      { label: t("Scheduler…"), onSelect: () => app.navigate("scheduled") },
    ],
    File: () => [
      { label: t("Open .torrent file…"), onSelect: () => void pickTorrent(app.openAdd) },
      { label: t("Open download folder"), onSelect: () => void run(openDownloadDir(), "Could not open the folder") },
      "sep",
      { label: t("Quit KuDownloader"), onSelect: () => void invoke("quit_app") },
    ],
    Downloads: () => [
      { label: t("Resume all"), onSelect: () => void run(api.resumeAll(), "Could not resume") },
      { label: t("Stop all"), onSelect: () => void run(api.pauseAll(), "Could not stop") },
      "sep",
      { label: t("Delete all completed"), disabled: !allDownloads().some((d) => d.status === "completed"), onSelect: () => void run(api.clearFinished(), "Could not delete") },
      "sep",
      { label: t("Options…"), shortcut: kb("Ctrl ,"), onSelect: () => app.openSettings("downloads") },
    ],
    View: () => [
      { heading: t("Theme") },
      ...(["light", "dark", "system"] as const).map((th) => ({ label: t(th[0].toUpperCase() + th.slice(1)), checked: s?.theme === th, onSelect: () => void updateSettings({ theme: th }) })),
      "sep",
      { label: t("Compact rows"), checked: !!s?.compact, onSelect: () => void updateSettings({ compact: !s?.compact }) },
      { label: t("Details panel"), shortcut: kb("Ctrl I"), checked: app.inspectorOpen, onSelect: () => app.setInspectorOpen(!app.inspectorOpen) },
      "sep",
      { label: t("Browser integration"), onSelect: () => app.navigate("browser") },
      { label: t("Media detection"), onSelect: () => app.navigate("media") },
    ],
    Help: () => [
      { label: t("Getting started…"), onSelect: () => window.dispatchEvent(new Event("ku:welcome")) },
      { label: t("Browser integration setup"), onSelect: () => app.navigate("browser") },
      { label: t("Keyboard shortcuts"), onSelect: () => app.openSettings("general") },
      { label: t("Check for updates…"), onSelect: () => app.openSettings("general") },
      "sep",
      { label: t("About KuDownloader"), onSelect: () => app.openSettings("advanced") },
    ],
  };
  return (
    <header className="titlebar" data-tauri-drag-region>
      {platform?.os === "macos" && <div className="traffic-light-inset" data-tauri-drag-region />}
      {left.length > 0 && <WindowControls buttons={left} />}
      <div className="brand" data-tauri-drag-region>
        <svg width="18" height="18" viewBox="0 0 1024 1024" aria-hidden="true">
          <rect x="64" y="64" width="896" height="896" rx="208" fill="#2563EB" />
          <path d="M512 248v396" stroke="#fff" strokeWidth="88" strokeLinecap="round" />
          <path d="M340 480l172 172 172-172" fill="none" stroke="#fff" strokeWidth="88" strokeLinecap="round" strokeLinejoin="round" />
          <rect x="308" y="728" width="408" height="72" rx="36" fill="#D4FF00" />
        </svg>
        <span className="wordmark" data-tauri-drag-region>
          KuDownloader
        </span>
      </div>
      <nav className="menubar" aria-label="Menu">
        {Object.keys(menus).map((name) => (
          <button key={name} type="button" className="menubar-item" onClick={(e) => showMenuAt(e.currentTarget, menus[name]())}>
            {t(name)}
          </button>
        ))}
      </nav>
      <div className="titlebar-fill" data-tauri-drag-region />
      {right.length > 0 && <WindowControls buttons={right} />}
    </header>
  );
}

async function openDownloadDir() {
  const s = settingsStore.get() ?? (await api.getSettings());
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("reveal_path", { path: s.downloadDir });
}

function SpeedMonitor() {
  const s = useSpeed();
  const downloading = useDownloadIds((d) => d.status === "downloading" || d.status === "processing", () => 0, []).length;
  const seeding = useDownloadIds((d) => d.status === "seeding", () => 0, []).length;
  const w = 180;
  const h = 56;
  // Download and upload share one scale so their sizes compare honestly.
  const peak = Math.max(0, ...s.history, ...s.upHistory);
  const max = Math.max(peak, 1);
  const n = Math.max(s.history.length, s.upHistory.length, 2);
  const step = w / (n - 1);
  const points = (values: number[]) => {
    // Idle (no samples): a flat line along the bottom.
    const vs = values.length ? values : [0, 0];
    const offset = n - vs.length;
    return vs.map((v, i) => `${((i + offset) * step).toFixed(1)},${(h - 2 - (v / max) * (h - 8)).toFixed(1)}`).join(" ");
  };
  const down = points(s.history);
  const up = points(s.upHistory);
  const first = down.split(",")[0];
  const detail = [
    downloading ? `${downloading} ${t("Downloading").toLowerCase()}` : "",
    seeding ? `${seeding} ${t("Seeding").toLowerCase()}` : "",
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <section className="speed-monitor" aria-label="Network speed">
      <div className="speed-monitor-head">
        <span>{t("Network")}</span>
        <span className="faint" title={detail || undefined}>
          {downloading + seeding ? `${downloading + seeding} active` : t("Idle")}
        </span>
      </div>
      <div className="speed-monitor-value num">
        <Icon icon={ArrowDownToLine} size={14} />
        {fmt.speed(s.down)}
      </div>
      <svg className="speed-monitor-graph" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" aria-hidden="true">
        <polygon points={`${first},${h} ${down} ${w},${h}`} className="area" />
        <polyline points={up} className="line up" />
        <polyline points={down} className="line" />
      </svg>
      <div className="speed-monitor-foot num faint">
        {/* Green like the upload line, so it doubles as the legend. */}
        <span className="speed-up" title={t("Upload")}>
          <Icon icon={ArrowUpFromLine} size={11} />
          {fmt.speed(s.up)}
        </span>
        <span>peak {fmt.speed(peak)}</span>
      </div>
    </section>
  );
}

function TreeItem({
  icon,
  label,
  count,
  active,
  expanded,
  onToggle,
  onClick,
  depth = 0,
}: {
  icon: LucideIcon;
  label: string;
  count?: number;
  active: boolean;
  expanded?: boolean;
  onToggle?: () => void;
  onClick: () => void;
  depth?: number;
}) {
  return (
    <div className="tree-row" style={{ paddingLeft: 4 + depth * 22 }}>
      <button type="button" className="nav-item" aria-current={active ? "page" : undefined} onClick={onClick}>
        <Icon icon={icon} />
        <span className="truncate">{t(label)}</span>
        {!!count && <span className="count">{count}</span>}
      </button>
      {onToggle && (
        <button type="button" className="tree-toggle" aria-label={expanded ? `Collapse ${label}` : `Expand ${label}`} aria-expanded={expanded} onClick={onToggle}>
          <Icon icon={expanded ? ChevronDown : ChevronRight} size={14} />
        </button>
      )}
    </div>
  );
}

export function Sidebar({ collapsed }: { collapsed: boolean; onToggle?: () => void }) {
  const air = useAir();
  const { view, filter, showList, navigate } = useApp();
  const settings = settingsStore.use();
  const queues = queuesStore.use();
  const [open, setOpen] = useState<Record<string, boolean>>({ all: true, unfinished: false, finished: false, queues: false });
  const toggle = (k: string) => setOpen((o) => ({ ...o, [k]: !o[k] }));
  const unfinished = useDownloadIds((d) => d.status !== "completed" && d.status !== "seeding", () => 0, []).length;
  const finished = useDownloadIds((d) => d.status === "completed" || d.status === "seeding", () => 0, []).length;
  const queued = useDownloadIds((d) => !!d.queueId && d.status !== "completed", () => 0, []).length;
  const isList = view === "downloads";
  const is = (f: Partial<ListFilter>) => isList && filter.scope === f.scope && (f.category ?? "") === filter.category && (f.queueId ?? undefined) === (filter.queueId ?? undefined);
  const cats = settings?.categories ?? [];
  const catChildren = (scope: ListFilter["scope"]) =>
    cats.map((c) => (
      <TreeItem key={scope + c.id} depth={1} icon={CATEGORY_ICON[c.id] ?? File} label={t(c.name)} active={is({ scope, category: c.id })} onClick={() => showList({ scope, category: c.id })} />
    ));

  if (collapsed) {
    const items: [LucideIcon, string, ListFilter["scope"]][] = [
      [ArrowDownToLine, "All Downloads", "all"],
      [CircleDashed, "Unfinished", "unfinished"],
      [CircleCheck, "Finished", "finished"],
      [ListOrdered, "Queues", "queue"],
    ];
    return (
      <nav className="sidebar" data-collapsed aria-label={t("Categories")}>
        <div className="sidebar-scroll">
          {items.map(([icon, label, scope]) => (
            <button key={label} type="button" className="nav-item" title={t(label)} aria-current={is({ scope, category: "", queueId: scope === "queue" ? "main" : undefined }) ? "page" : undefined} onClick={() => showList({ scope, category: "", queueId: scope === "queue" ? "main" : undefined })}>
              <Icon icon={icon} />
            </button>
          ))}
          <button type="button" className="nav-item" title={t("Fetch Projects")} aria-current={view === "grabber" ? "page" : undefined} onClick={() => navigate("grabber")}>
            <Icon icon={Hand} />
          </button>
          <button type="button" className="nav-item" title={t("Video Downloader")} aria-current={view === "video" ? "page" : undefined} onClick={() => navigate("video")}>
            <Icon icon={Clapperboard} />
          </button>
          <button type="button" className="nav-item" title={t("Batch Downloads")} aria-current={view === "batch" ? "page" : undefined} onClick={() => navigate("batch")}>
            <Icon icon={ListPlus} />
          </button>
          <button type="button" className="nav-item" title="KuAirSend" aria-current={view === "airsend" ? "page" : undefined} onClick={() => navigate("airsend")}>
            <Icon icon={Radar} />
          </button>
        </div>
      </nav>
    );
  }

  return (
    <nav className="sidebar" aria-label={t("Categories")}>
      <div className="sidebar-scroll">
        <div className="nav-title">{t("Categories")}</div>
        <TreeItem icon={ArrowDownToLine} label={t("All Downloads")} active={is({ scope: "all" })} expanded={open.all} onToggle={() => toggle("all")} onClick={() => showList({ scope: "all", category: "" })} />
        {open.all && catChildren("all")}
        <TreeItem icon={CircleDashed} label={t("Unfinished")} count={unfinished} active={is({ scope: "unfinished" })} expanded={open.unfinished} onToggle={() => toggle("unfinished")} onClick={() => showList({ scope: "unfinished", category: "" })} />
        {open.unfinished && catChildren("unfinished")}
        <TreeItem icon={CircleCheck} label={t("Finished")} count={finished} active={is({ scope: "finished" })} expanded={open.finished} onToggle={() => toggle("finished")} onClick={() => showList({ scope: "finished", category: "" })} />
        {open.finished && catChildren("finished")}
        <TreeItem icon={ListOrdered} label={t("Queues")} count={queued} active={isList && filter.scope === "queue"} expanded={open.queues} onToggle={() => toggle("queues")} onClick={() => showList({ scope: "queue", category: "", queueId: queues[0]?.id ?? "main" })} />
        {open.queues &&
          queues.map((q) => (
            <TreeItem key={q.id} depth={1} icon={ListOrdered} label={q.name + (q.running ? " · running" : "")} active={is({ scope: "queue", queueId: q.id })} onClick={() => showList({ scope: "queue", category: "", queueId: q.id })} />
          ))}
        <TreeItem icon={Hand} label={t("Fetch Projects")} active={view === "grabber"} onClick={() => navigate("grabber")} />
        <TreeItem icon={Clapperboard} label={t("Video Downloader")} active={view === "video"} onClick={() => navigate("video")} />
        <TreeItem icon={ListPlus} label={t("Batch Downloads")} active={view === "batch"} onClick={() => navigate("batch")} />
        <TreeItem icon={Radar} label="KuAirSend" count={air.peers.length || undefined} active={view === "airsend"} onClick={() => navigate("airsend")} />
      </div>
      <SpeedMonitor />
    </nav>
  );
}
