import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ArrowDownToLine,
  ChevronDown,
  ChevronRight,
  CircleCheck,
  CircleDashed,
  Clapperboard,
  File,
  ListPlus,
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


function WindowControls() {
  const win = getCurrentWindow();
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
  return (
    <div className="window-controls">
      <button type="button" aria-label="Minimize" onClick={() => void win.minimize()}>
        <Icon icon={Minus} size={14} />
      </button>
      <button type="button" aria-label={maximized ? "Restore" : "Maximize"} onClick={() => void win.toggleMaximize()}>
        <Icon icon={maximized ? Copy : Square} size={12} />
      </button>
      <button type="button" aria-label="Close" className="close" onClick={() => void win.close()}>
        <Icon icon={X} size={15} />
      </button>
    </div>
  );
}


export function TitleBar() {
  const app = useApp();
  const s = settingsStore.use();
  const menus: Record<string, () => MenuItem[]> = {
    Tasks: () => [
      { label: "Add URL…", shortcut: "Ctrl N", onSelect: () => app.openAdd() },
      { label: "Add from clipboard", shortcut: "Ctrl V", onSelect: () => void pasteLink(app.openAdd) },
      { label: "Add batch download…", onSelect: () => app.navigate("batch") },
      "sep",
      { label: "Video downloader…", onSelect: () => app.openMedia() },
      { label: "Grab links from a page…", onSelect: () => app.navigate("grabber") },
      "sep",
      { label: "Scheduler…", onSelect: () => app.navigate("scheduled") },
    ],
    File: () => [
      { label: "Open .torrent file…", onSelect: () => void pickTorrent(app.openAdd) },
      { label: "Open download folder", onSelect: () => void run(openDownloadDir(), "Could not open the folder") },
      "sep",
      { label: "Quit KuDownloader", onSelect: () => void invoke("quit_app") },
    ],
    Downloads: () => [
      { label: "Resume all", onSelect: () => void run(api.resumeAll(), "Could not resume") },
      { label: "Stop all", onSelect: () => void run(api.pauseAll(), "Could not stop") },
      "sep",
      { label: "Delete all completed", disabled: !allDownloads().some((d) => d.status === "completed"), onSelect: () => void run(api.clearFinished(), "Could not delete") },
      "sep",
      { label: "Options…", shortcut: "Ctrl ,", onSelect: () => app.openSettings("downloads") },
    ],
    View: () => [
      { heading: "Theme" },
      ...(["light", "dark", "system"] as const).map((t) => ({ label: t[0].toUpperCase() + t.slice(1), checked: s?.theme === t, onSelect: () => void updateSettings({ theme: t }) })),
      "sep",
      { label: "Compact rows", checked: !!s?.compact, onSelect: () => void updateSettings({ compact: !s?.compact }) },
      { label: "Details panel", shortcut: "Ctrl I", checked: app.inspectorOpen, onSelect: () => app.setInspectorOpen(!app.inspectorOpen) },
      "sep",
      { label: "Browser integration", onSelect: () => app.navigate("browser") },
      { label: "Media detection", onSelect: () => app.navigate("media") },
    ],
    Help: () => [
      { label: "Browser integration setup", onSelect: () => app.navigate("browser") },
      { label: "Keyboard shortcuts", onSelect: () => app.openSettings("general") },
      { label: "Check for updates…", onSelect: () => app.openSettings("general") },
      "sep",
      { label: "About KuDownloader", onSelect: () => app.openSettings("advanced") },
    ],
  };
  return (
    <header className="titlebar" data-tauri-drag-region>
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
            {name}
          </button>
        ))}
      </nav>
      <div className="titlebar-fill" data-tauri-drag-region />
      <WindowControls />
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
  const active = useDownloadIds((d) => d.status === "downloading" || d.status === "processing", () => 0, []).length;
  const w = 180;
  const h = 56;
  const values = s.history.length ? s.history : [0];
  const max = Math.max(...values, 1);
  const step = values.length > 1 ? w / (values.length - 1) : w;
  const pts = values.map((v, i) => [i * step, h - 2 - (v / max) * (h - 8)]);
  const line = pts.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
  const area = `0,${h} ${line} ${((values.length - 1) * step).toFixed(1)},${h}`;
  return (
    <section className="speed-monitor" aria-label="Network speed">
      <div className="speed-monitor-head">
        <span>Network</span>
        <span className="faint">{active ? `${active} active` : "Idle"}</span>
      </div>
      <div className="speed-monitor-value num">
        <Icon icon={ArrowDownToLine} size={14} />
        {fmt.speed(s.down)}
      </div>
      <svg className="speed-monitor-graph" viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" aria-hidden="true">
        <polygon points={area} className="area" />
        <polyline points={line} className="line" />
      </svg>
      <div className="speed-monitor-foot num faint">
        <span>↑ {fmt.speed(s.up)}</span>
        <span>peak {fmt.speed(max === 1 ? 0 : max)}</span>
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
        <span className="truncate">{label}</span>
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
      <TreeItem key={scope + c.id} depth={1} icon={CATEGORY_ICON[c.id] ?? File} label={c.name} active={is({ scope, category: c.id })} onClick={() => showList({ scope, category: c.id })} />
    ));

  if (collapsed) {
    const items: [LucideIcon, string, ListFilter["scope"]][] = [
      [ArrowDownToLine, "All Downloads", "all"],
      [CircleDashed, "Unfinished", "unfinished"],
      [CircleCheck, "Finished", "finished"],
      [ListOrdered, "Queues", "queue"],
    ];
    return (
      <nav className="sidebar" data-collapsed aria-label="Categories">
        <div className="sidebar-scroll">
          {items.map(([icon, label, scope]) => (
            <button key={label} type="button" className="nav-item" title={label} aria-current={is({ scope, category: "", queueId: scope === "queue" ? "main" : undefined }) ? "page" : undefined} onClick={() => showList({ scope, category: "", queueId: scope === "queue" ? "main" : undefined })}>
              <Icon icon={icon} />
            </button>
          ))}
          <button type="button" className="nav-item" title="Grabber Projects" aria-current={view === "grabber" ? "page" : undefined} onClick={() => navigate("grabber")}>
            <Icon icon={Hand} />
          </button>
          <button type="button" className="nav-item" title="Video Downloader" aria-current={view === "video" ? "page" : undefined} onClick={() => navigate("video")}>
            <Icon icon={Clapperboard} />
          </button>
          <button type="button" className="nav-item" title="Batch Downloads" aria-current={view === "batch" ? "page" : undefined} onClick={() => navigate("batch")}>
            <Icon icon={ListPlus} />
          </button>
        </div>
      </nav>
    );
  }

  return (
    <nav className="sidebar" aria-label="Categories">
      <div className="sidebar-scroll">
        <div className="nav-title">Categories</div>
        <TreeItem icon={ArrowDownToLine} label="All Downloads" active={is({ scope: "all" })} expanded={open.all} onToggle={() => toggle("all")} onClick={() => showList({ scope: "all", category: "" })} />
        {open.all && catChildren("all")}
        <TreeItem icon={CircleDashed} label="Unfinished" count={unfinished} active={is({ scope: "unfinished" })} expanded={open.unfinished} onToggle={() => toggle("unfinished")} onClick={() => showList({ scope: "unfinished", category: "" })} />
        {open.unfinished && catChildren("unfinished")}
        <TreeItem icon={CircleCheck} label="Finished" count={finished} active={is({ scope: "finished" })} expanded={open.finished} onToggle={() => toggle("finished")} onClick={() => showList({ scope: "finished", category: "" })} />
        {open.finished && catChildren("finished")}
        <TreeItem icon={ListOrdered} label="Queues" count={queued} active={isList && filter.scope === "queue"} expanded={open.queues} onToggle={() => toggle("queues")} onClick={() => showList({ scope: "queue", category: "", queueId: queues[0]?.id ?? "main" })} />
        {open.queues &&
          queues.map((q) => (
            <TreeItem key={q.id} depth={1} icon={ListOrdered} label={q.name + (q.running ? " · running" : "")} active={is({ scope: "queue", queueId: q.id })} onClick={() => showList({ scope: "queue", category: "", queueId: q.id })} />
          ))}
        <TreeItem icon={Hand} label="Grabber Projects" active={view === "grabber"} onClick={() => navigate("grabber")} />
        <TreeItem icon={Clapperboard} label="Video Downloader" active={view === "video"} onClick={() => navigate("video")} />
        <TreeItem icon={ListPlus} label="Batch Downloads" active={view === "batch"} onClick={() => navigate("batch")} />
      </div>
      <SpeedMonitor />
    </nav>
  );
}
