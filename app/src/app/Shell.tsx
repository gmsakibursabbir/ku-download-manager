import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ArrowDownToLine,
  ListOrdered,
  CircleCheck,
  CalendarClock,
  Magnet,
  Globe,
  Radar,
  Clapperboard,
  Link2,
  ListPlus,
  Settings as SettingsIcon,
  Search,
  Minus,
  Square,
  Copy,
  X,
  PanelLeftClose,
  PanelLeftOpen,
  type LucideIcon,
} from "lucide-react";
import { Icon } from "../ui/primitives";
import { useApp, type View } from "./context";
import { useDownloadIds, useSpeed } from "../lib/store";
import * as fmt from "../lib/format";

function Sparkline({ values }: { values: number[] }) {
  const w = 72;
  const h = 18;
  if (values.length < 2) return <svg width={w} height={h} aria-hidden="true" />;
  const max = Math.max(...values, 1);
  const step = w / (values.length - 1);
  const pts = values.map((v, i) => `${(i * step).toFixed(1)},${(h - 1 - (v / max) * (h - 3)).toFixed(1)}`).join(" ");
  return (
    <svg width={w} height={h} className="sparkline" aria-hidden="true">
      <polyline points={pts} fill="none" stroke="currentColor" strokeWidth="1.25" strokeLinejoin="round" />
    </svg>
  );
}

function WindowControls() {
  const win = getCurrentWindow();
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    void win.isMaximized().then(setMaximized);
    const un = win.onResized(() => void win.isMaximized().then(setMaximized));
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
  const { search, setSearch, view, navigate } = useApp();
  const s = useSpeed();
  const searchable = view === "downloads" || view === "finished" || view === "queue" || view === "torrents";
  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region>
        <svg width="18" height="18" viewBox="0 0 1024 1024" aria-hidden="true">
          <rect x="64" y="64" width="896" height="896" rx="208" fill="#2563EB" />
          <path d="M512 248v396" stroke="#fff" strokeWidth="88" strokeLinecap="round" />
          <path d="M340 480l172 172 172-172" fill="none" stroke="#fff" strokeWidth="88" strokeLinecap="round" strokeLinejoin="round" />
          <rect x="308" y="728" width="408" height="72" rx="36" fill="#D4FF00" />
        </svg>
        <span className="wordmark">KuDownloader</span>
      </div>
      <div className="titlebar-search" data-tauri-drag-region>
        <div className="input-with-icon">
          <Icon icon={Search} size={14} />
          <input
            id="global-search"
            className="input"
            placeholder="Search downloads"
            value={search}
            onFocus={() => !searchable && navigate("downloads")}
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setSearch("");
                (e.target as HTMLInputElement).blur();
              }
            }}
          />
          <kbd className="search-kbd">Ctrl F</kbd>
        </div>
      </div>
      <div className="titlebar-speed num" data-tauri-drag-region title="Total download / upload speed">
        <span className="speed-down">
          <Icon icon={ArrowDownToLine} size={13} />
          {fmt.speed(s.down)}
        </span>
        {s.up > 0 && <span className="faint">↑ {fmt.speed(s.up)}</span>}
        <Sparkline values={s.history} />
      </div>
      <WindowControls />
    </header>
  );
}

interface NavItem {
  view: View;
  label: string;
  icon: LucideIcon;
  count?: number;
  shortcut?: string;
}

export function Sidebar({ collapsed, onToggle }: { collapsed: boolean; onToggle: () => void }) {
  const { view, navigate } = useApp();
  // Count what still needs attention; the Downloads view itself lists everything.
  const active = useDownloadIds((d) => d.status !== "completed" && d.status !== "seeding", () => 0, []).length;
  const queued = useDownloadIds((d) => !!d.queueId && d.status !== "completed" && d.status !== "seeding", () => 0, []).length;
  const finished = useDownloadIds((d) => d.status === "completed" || d.status === "seeding", () => 0, []).length;
  const torrents = useDownloadIds((d) => d.kind === "torrent" || d.kind === "magnet", () => 0, []).length;

  const groups: { title?: string; items: NavItem[] }[] = [
    {
      items: [
        { view: "downloads", label: "Downloads", icon: ArrowDownToLine, count: active, shortcut: "Ctrl 1" },
        { view: "queue", label: "Queue", icon: ListOrdered, count: queued, shortcut: "Ctrl 2" },
        { view: "finished", label: "Finished", icon: CircleCheck, count: finished, shortcut: "Ctrl 3" },
        { view: "scheduled", label: "Scheduled", icon: CalendarClock, shortcut: "Ctrl 4" },
        { view: "torrents", label: "Torrents", icon: Magnet, count: torrents, shortcut: "Ctrl 5" },
      ],
    },
    {
      title: "Browser",
      items: [
        { view: "browser", label: "Browser Integration", icon: Globe },
        { view: "media", label: "Media Detection", icon: Radar },
      ],
    },
    {
      title: "Tools",
      items: [
        { view: "video", label: "Video Downloader", icon: Clapperboard },
        { view: "grabber", label: "URL Grabber", icon: Link2 },
        { view: "batch", label: "Batch Downloads", icon: ListPlus },
      ],
    },
  ];

  const item = (it: NavItem) => (
    <button
      key={it.view}
      type="button"
      className="nav-item"
      aria-current={view === it.view ? "page" : undefined}
      onClick={() => navigate(it.view)}
      title={collapsed ? it.label : it.shortcut ? `${it.label} (${it.shortcut})` : undefined}
    >
      <Icon icon={it.icon} />
      {!collapsed && <span className="truncate">{it.label}</span>}
      {!collapsed && !!it.count && <span className="count">{it.count}</span>}
    </button>
  );

  return (
    <nav className="sidebar" data-collapsed={collapsed || undefined} aria-label="Main">
      <div className="sidebar-scroll">
        {groups.map((g, i) => (
          <div key={i} className="nav-group">
            {g.title && !collapsed && <div className="nav-title">{g.title}</div>}
            {g.title && collapsed && <div className="nav-sep" />}
            {g.items.map(item)}
          </div>
        ))}
      </div>
      <div className="sidebar-footer">
        {item({ view: "settings", label: "Settings", icon: SettingsIcon, shortcut: "Ctrl ," })}
        <button type="button" className="nav-item nav-collapse" onClick={onToggle} title={collapsed ? "Expand sidebar" : "Collapse sidebar"}>
          <Icon icon={collapsed ? PanelLeftOpen : PanelLeftClose} />
          {!collapsed && <span>Collapse</span>}
        </button>
      </div>
    </nav>
  );
}
