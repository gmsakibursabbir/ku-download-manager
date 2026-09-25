import { invoke } from "@tauri-apps/api/core";
import { t, tf } from "../../lib/i18n";
import { memo, useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ChevronDown, ChevronUp, ListOrdered } from "lucide-react";
import { useDownload, getDownload, settingsStore, queuesStore, queueName } from "../../lib/store";
import * as fmt from "../../lib/format";
import type { Download } from "../../lib/types";
import { Icon } from "../../ui/primitives";
import { showMenu } from "../../ui/overlays";
import { FileGlyph } from "./FileGlyph";
import { contextMenu, openDownload, canPause, canResume, run } from "./actions";
import { api } from "../../lib/api";
import { te, tx } from "../../lib/engineText";
import { useApp } from "../context";

export type SortKey = "added" | "name" | "size" | "progress" | "speed" | "status" | "queue" | "eta";
export interface SortState {
  key: SortKey;
  dir: 1 | -1;
}

const lastTry = (d: Download) => d.completedAt ?? d.createdAt;

export function compareBy(sort: SortState) {
  return (a: Download, b: Download) => {
    let r = 0;
    switch (sort.key) {
      case "name":
        r = a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
        break;
      case "size":
        r = a.total - b.total;
        break;
      case "progress":
      case "status":
        r = fmt.percent(a.done, a.total) - fmt.percent(b.done, b.total) || a.status.localeCompare(b.status);
        break;
      case "speed":
        r = a.speed - b.speed;
        break;
      case "eta":
        r = (fmt.eta(a.done, a.total, a.speed) ?? Infinity) - (fmt.eta(b.done, b.total, b.speed) ?? Infinity);
        break;
      case "queue":
        r = a.position - b.position || a.createdAt - b.createdAt;
        break;
      default:
        r = lastTry(a) - lastTry(b);
    }
    return r * sort.dir || b.createdAt - a.createdAt;
  };
}

function dateLabel(ms: number) {
  const d = new Date(ms);
  const time = d.toLocaleTimeString(document.documentElement.lang || undefined, { hour: "2-digit", minute: "2-digit", hour12: false });
  const date = d.toLocaleDateString(document.documentElement.lang || undefined, { month: "short", day: "numeric", year: "numeric" });
  return `${time}, ${date}`;
}

function statusCell(d: Download): { text: string; tone?: "accent" | "success" | "danger" | "muted"; title?: string } {
  const pct = d.total > 0 ? `${fmt.percent(d.done, d.total).toFixed(2)}%` : null;
  switch (d.status) {
    case "downloading":
      if (d.kind === "magnet" && d.total === 0) return { text: "Getting metadata…", tone: "accent" };
      return { text: pct ?? fmt.bytes(d.done), tone: "accent" };
    case "processing":
      return { text: "Merging…", tone: "accent" };
    case "seeding":
      return { text: "Seeding", tone: "success" };
    case "completed":
      return { text: "Complete", tone: "success" };
    case "error":
      return { text: "Error", tone: "danger", title: d.error ?? undefined };
    case "paused":
    case "queued":
      if (d.done > 0 && pct) return { text: pct, tone: "muted", title: d.status === "queued" ? "Waiting to continue" : "Paused" };
      return { text: d.status === "queued" ? "Queued" : "Not started", tone: "muted" };
  }
}

const Row = memo(function Row({
  id,
  index,
  top,
  selected,
  cursor,
  onMouseDown,
  onCheck,
  onDoubleClick,
  onContextMenu,
}: {
  id: string;
  index: number;
  top: number;
  selected: boolean;
  cursor: boolean;
  onMouseDown: (e: React.MouseEvent, id: string, index: number) => void;
  onCheck: (id: string, index: number) => void;
  onDoubleClick: (id: string) => void;
  onContextMenu: (e: React.MouseEvent, id: string, index: number) => void;
}) {
  const d = useDownload(id);
  const queues = queuesStore.use();
  if (!d) return null;
  const st = statusCell(d);
  const active = d.status === "downloading";
  const eta = active ? fmt.eta(d.done, d.total, d.speed) : null;
  const queue = d.queueId ? queues.find((q) => q.id === d.queueId) : null;
  const pct = fmt.percent(d.done, d.total);
  return (
    <div
      className="row list-grid"
      role="row"
      aria-selected={selected}
      data-cursor={cursor || undefined}
      data-status={d.status}
      style={{ transform: `translateY(${top}px)` }}
      onMouseDown={(e) => onMouseDown(e, id, index)}
      onDoubleClick={() => onDoubleClick(id)}
      onContextMenu={(e) => onContextMenu(e, id, index)}
    >
      <label className="row-check" onMouseDown={(e) => e.stopPropagation()}>
        <input type="checkbox" checked={selected} onChange={() => onCheck(id, index)} aria-label={tf("Select {name}", { name: d.name })} />
      </label>
      <div className="row-name" title={d.error ? `${d.name}\n${te(d.error)}` : d.name}>
        <FileGlyph d={d} size={14} />
        <span className="row-title">{d.name}</span>
      </div>
      <div className="cell-q" title={queue ? tf("In {queue}", { queue: queueName(queue) }) : undefined}>
        {queue && <Icon icon={ListOrdered} size={14} />}
      </div>
      <div className="cell num">{d.total > 0 ? fmt.bytes(d.total, 2) : d.done > 0 ? fmt.bytes(d.done) : ""}</div>
      <div className="cell cell-status num" data-tone={st.tone} title={st.title && tx(st.title)}>
        <span>{t(st.text)}</span>
        {(active || d.status === "paused" || d.status === "queued") && d.total > 0 && d.done > 0 && (
          <span className="status-bar" aria-hidden="true">
            <span style={{ width: `${pct}%` }} />
          </span>
        )}
      </div>
      <div className="cell num">{active ? (eta != null ? fmt.duration(eta) : "—") : ""}</div>
      <div className="cell num">{active ? fmt.speed(d.speed) : d.status === "seeding" ? `↑ ${fmt.speed(d.uploadSpeed)}` : ""}</div>
      <div className="cell num faint">{d.status === "queued" && d.done === 0 ? "" : dateLabel(lastTry(d))}</div>
    </div>
  );
});

function HeaderCell({ label, k, sort, onSort }: { label: string; k?: SortKey; sort: SortState; onSort?: (s: SortState) => void }) {
  if (!k || !onSort) return <div className="hcell">{t(label)}</div>;
  const on = sort.key === k;
  return (
    <div className="hcell">
      <button type="button" onClick={() => onSort({ key: k, dir: on ? ((sort.dir * -1) as 1 | -1) : k === "name" ? 1 : -1 })} aria-sort={on ? (sort.dir === 1 ? "ascending" : "descending") : undefined}>
        {t(label)}
        {on && <Icon icon={sort.dir === 1 ? ChevronUp : ChevronDown} size={12} />}
      </button>
    </div>
  );
}

export function DownloadList({
  ids,
  empty,
  sort,
  onSort,
  onVerify,
  onDropFiles,
  reorderable,
}: {
  ids: string[];
  empty: ReactNode;
  sort: SortState;
  onSort?: (s: SortState) => void;
  onVerify: (id: string) => void;
  onDropFiles: (dt: DataTransfer) => void;
  reorderable?: boolean;
}) {
  const app = useApp();
  const { selection, setSelection, setInspectorOpen, confirmRemove } = app;
  const scrollRef = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ top: 0, height: 600 });
  const [cursor, setCursor] = useState<number>(-1);
  const [anchor, setAnchor] = useState<number>(-1);
  const [dragOver, setDragOver] = useState(false);
  const compact = settingsStore.use()?.compact ?? false;
  const rowH = compact ? 28 : 34;

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewport({ top: el.scrollTop, height: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    const valid = new Set(ids);
    const next = new Set([...selection].filter((id) => valid.has(id)));
    if (next.size !== selection.size) setSelection(next);
    if (cursor >= ids.length) setCursor(ids.length - 1);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ids]);

  const ensureVisible = (i: number) => {
    const el = scrollRef.current;
    if (!el) return;
    const top = i * rowH;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + rowH > el.scrollTop + el.clientHeight) el.scrollTop = top + rowH - el.clientHeight;
  };

  const selectRange = (from: number, to: number, additive: boolean) => {
    const [a, b] = from < to ? [from, to] : [to, from];
    const next = additive ? new Set(selection) : new Set<string>();
    for (let i = a; i <= b; i++) next.add(ids[i]);
    setSelection(next);
  };

  const onMouseDown = useCallback(
    (e: React.MouseEvent, id: string, index: number) => {
      if (e.button !== 0) return;
      scrollRef.current?.focus({ preventScroll: true });
      if (e.shiftKey && anchor >= 0) {
        selectRange(anchor, index, e.ctrlKey || e.metaKey);
      } else if (e.ctrlKey || e.metaKey) {
        const next = new Set(selection);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        setSelection(next);
        setAnchor(index);
      } else {
        setSelection(new Set([id]));
        setAnchor(index);
      }
      setCursor(index);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [anchor, selection, ids],
  );

  const onCheck = useCallback(
    (id: string, index: number) => {
      const next = new Set(selection);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      setSelection(next);
      setAnchor(index);
      setCursor(index);
    },
    [selection, setSelection],
  );

  const onDoubleClick = useCallback(
    (id: string) => {
      const d = getDownload(id);
      if (!d) return;
      if (d.status === "completed" || d.status === "seeding") openDownload(d);
      else void invoke("open_progress_window", { id }).catch(() => setInspectorOpen(true));
    },
    [setInspectorOpen],
  );

  const onContextMenu = useCallback(
    (e: React.MouseEvent, id: string, index: number) => {
      e.preventDefault();
      let ids2 = [...selection];
      if (!selection.has(id)) {
        setSelection(new Set([id]));
        setAnchor(index);
        setCursor(index);
        ids2 = [id];
      }
      showMenu(e.clientX, e.clientY, contextMenu(ids2, { confirmRemove, onVerify }));
    },
    [selection, setSelection, confirmRemove, onVerify],
  );

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!ids.length) return;
    const sel = [...selection];
    const move = (i: number) => {
      const n = Math.max(0, Math.min(ids.length - 1, i));
      setCursor(n);
      ensureVisible(n);
      if (e.shiftKey) selectRange(anchor < 0 ? n : anchor, n, false);
      else {
        setSelection(new Set([ids[n]]));
        setAnchor(n);
      }
    };
    const page = Math.max(1, Math.floor(viewport.height / rowH) - 1);
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown") && reorderable && sel.length === 1) {
      e.preventDefault();
      void run(api.reorder(sel[0], e.key === "ArrowUp" ? "up" : "down"), "Could not reorder");
      return;
    }
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        move(cursor + 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        move(cursor < 0 ? 0 : cursor - 1);
        break;
      case "PageDown":
        e.preventDefault();
        move(cursor + page);
        break;
      case "PageUp":
        e.preventDefault();
        move(cursor - page);
        break;
      case "Home":
        e.preventDefault();
        move(0);
        break;
      case "End":
        e.preventDefault();
        move(ids.length - 1);
        break;
      case "a":
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          setSelection(new Set(ids));
        }
        break;
      case " ": {
        e.preventDefault();
        const list = sel.map(getDownload).filter((d): d is Download => !!d);
        if (list.some(canPause)) void run(api.pause(sel), "Could not stop");
        else if (list.some(canResume)) void run(api.resume(sel), "Could not resume");
        break;
      }
      case "Enter": {
        const d = sel.length === 1 ? getDownload(sel[0]) : undefined;
        if (d) onDoubleClick(d.id);
        break;
      }
      case "Delete":
        if (sel.length) confirmRemove(sel);
        break;
      case "Escape":
        setSelection(new Set());
        break;
      case "ContextMenu": {
        if (!sel.length) break;
        const r = scrollRef.current!.getBoundingClientRect();
        showMenu(r.left + 80, r.top + (Math.max(cursor, 0) * rowH - scrollRef.current!.scrollTop) + rowH, contextMenu(sel, { confirmRemove, onVerify }));
        break;
      }
    }
  };

  const overscan = 8;
  const first = Math.max(0, Math.floor(viewport.top / rowH) - overscan);
  const last = Math.min(ids.length, Math.ceil((viewport.top + viewport.height) / rowH) + overscan);
  const visible = [];
  for (let i = first; i < last; i++) {
    const id = ids[i];
    visible.push(
      <Row
        key={id}
        id={id}
        index={i}
        top={i * rowH}
        selected={selection.has(id)}
        cursor={cursor === i}
        onMouseDown={onMouseDown}
        onCheck={onCheck}
        onDoubleClick={onDoubleClick}
        onContextMenu={onContextMenu}
      />,
    );
  }
  const allSelected = ids.length > 0 && ids.every((id) => selection.has(id));

  return (
    <div
      className="list card"
      style={{ ["--row-h" as string]: `${rowH}px` }}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes("Files") || e.dataTransfer.types.includes("text/uri-list") || e.dataTransfer.types.includes("text/plain")) {
          e.preventDefault();
          setDragOver(true);
        }
      }}
      onDragLeave={(e) => {
        if (!(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) setDragOver(false);
      }}
      onDrop={(e) => {
        e.preventDefault();
        setDragOver(false);
        onDropFiles(e.dataTransfer);
      }}
    >
      <div className="list-header list-grid" role="row">
        <label className="row-check">
          <input
            type="checkbox"
            checked={allSelected}
            ref={(el) => {
              if (el) el.indeterminate = !allSelected && selection.size > 0;
            }}
            onChange={() => setSelection(allSelected ? new Set() : new Set(ids))}
            aria-label={t("Select all")}
          />
        </label>
        <HeaderCell label={t("File name")} k="name" sort={sort} onSort={onSort} />
        <HeaderCell label="Q" k="queue" sort={sort} onSort={onSort} />
        <HeaderCell label={t("Size")} k="size" sort={sort} onSort={onSort} />
        <HeaderCell label={t("Status")} k="status" sort={sort} onSort={onSort} />
        <HeaderCell label={t("Time Left")} k="eta" sort={sort} onSort={onSort} />
        <HeaderCell label={t("Transfer rate")} k="speed" sort={sort} onSort={onSort} />
        <HeaderCell label={t("Last try date")} k="added" sort={sort} onSort={onSort} />
      </div>
      <div
        ref={scrollRef}
        className="list-scroll"
        role="grid"
        aria-multiselectable="true"
        tabIndex={0}
        onKeyDown={onKeyDown}
        onScroll={(e) => setViewport({ top: e.currentTarget.scrollTop, height: e.currentTarget.clientHeight })}
        onMouseDown={(e) => {
          if (e.target === e.currentTarget) setSelection(new Set());
        }}
      >
        {ids.length === 0 ? empty : <div style={{ height: ids.length * rowH, position: "relative" }}>{visible}</div>}
      </div>
      {dragOver && <div className="drop-hint">{t("Drop links or .torrent files to download")}</div>}
    </div>
  );
}
