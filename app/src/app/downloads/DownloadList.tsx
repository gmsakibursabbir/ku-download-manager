import { memo, useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { useDownload, getDownload, settingsStore, queuesStore } from "../../lib/store";
import * as fmt from "../../lib/format";
import type { Download } from "../../lib/types";
import { Icon, IconButton, Progress, StatusBadge } from "../../ui/primitives";
import { showMenu } from "../../ui/overlays";
import { FileGlyph } from "./FileGlyph";
import { contextMenu, openDownload, primaryAction, canPause, canResume, run } from "./actions";
import { api } from "../../lib/api";
import { useApp } from "../context";

export type SortKey = "added" | "name" | "size" | "progress" | "speed" | "status" | "queue";
export interface SortState {
  key: SortKey;
  dir: 1 | -1;
}

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
        r = fmt.percent(a.done, a.total) - fmt.percent(b.done, b.total);
        break;
      case "speed":
        r = a.speed - b.speed;
        break;
      case "status":
        r = a.status.localeCompare(b.status);
        break;
      case "queue":
        r = a.position - b.position || a.createdAt - b.createdAt;
        break;
      default:
        r = (a.completedAt ?? a.createdAt) - (b.completedAt ?? b.createdAt);
    }
    return r * sort.dir || b.createdAt - a.createdAt;
  };
}

function subline(d: Download): { text: string; error?: boolean } {
  const h = fmt.host(d.url);
  const sizes = d.total > 0 ? `${fmt.bytes(d.done)} of ${fmt.bytes(d.total)}` : d.done > 0 ? fmt.bytes(d.done) : "Size unknown";
  switch (d.status) {
    case "downloading":
      if (d.kind === "magnet" && d.total === 0) return { text: "Fetching torrent metadata…" };
      return { text: [h, sizes, d.meta.playlistIndex ? `item ${d.meta.playlistIndex}` : ""].filter(Boolean).join(" · ") };
    case "processing":
      return { text: "Merging and converting…" };
    case "seeding":
      return { text: `Seeding · ${fmt.bytes(d.uploaded)} uploaded` };
    case "queued": {
      if (d.meta.retries > 0) return { text: `Retrying soon (attempt ${d.meta.retries}) · ${sizes}` };
      const q = d.queueId ? queuesStore.get().find((x) => x.id === d.queueId) : null;
      if (q) return { text: `${q.running ? "Waiting in" : "In"} ${q.name}${d.done > 0 ? ` · ${sizes}` : ""}` };
      return { text: d.done > 0 ? `Waiting for a free slot · ${sizes}` : "Waiting for a free slot" };
    }
    case "paused":
      return { text: `Paused · ${sizes}` };
    case "error":
      return { text: d.error ?? "Failed", error: true };
    case "completed":
      return { text: [h, d.dir].filter(Boolean).join(" · ") };
  }
}

const Row = memo(function Row({
  id,
  index,
  top,
  selected,
  cursor,
  onMouseDown,
  onDoubleClick,
  onContextMenu,
}: {
  id: string;
  index: number;
  top: number;
  selected: boolean;
  cursor: boolean;
  onMouseDown: (e: React.MouseEvent, id: string, index: number) => void;
  onDoubleClick: (id: string) => void;
  onContextMenu: (e: React.MouseEvent, id: string, index: number) => void;
}) {
  const d = useDownload(id);
  queuesStore.use(); // queue names appear in the subline
  if (!d) return null;
  const pct = fmt.percent(d.done, d.total);
  const active = d.status === "downloading" || d.status === "processing";
  const sub = subline(d);
  const action = primaryAction(d);
  const eta = d.status === "downloading" ? fmt.eta(d.done, d.total, d.speed) : null;
  const indeterminate = (active && d.total === 0) || d.status === "processing";
  return (
    <div
      className="row list-grid"
      role="row"
      aria-selected={selected}
      data-cursor={cursor || undefined}
      style={{ transform: `translateY(${top}px)` }}
      onMouseDown={(e) => onMouseDown(e, id, index)}
      onDoubleClick={() => onDoubleClick(id)}
      onContextMenu={(e) => onContextMenu(e, id, index)}
    >
      <FileGlyph d={d} />
      <div className="row-name">
        <div className="row-title" title={d.name}>
          {d.name}
        </div>
        <div className={`row-sub ${sub.error ? "is-error" : ""}`} title={sub.text}>
          {sub.text}
        </div>
      </div>
      {d.status === "completed" ? (
        <div className="cell-num" style={{ textAlign: "left" }}>
          {fmt.relativeDate(d.completedAt)}
        </div>
      ) : (
        <div className="row-progress num">
          <Progress value={d.status === "seeding" ? 100 : pct} state={d.status} indeterminate={indeterminate} />
          <span className="pct">{d.total > 0 && (d.done > 0 || active) && d.status !== "processing" ? `${pct < 10 ? pct.toFixed(1) : Math.floor(pct)}%` : ""}</span>
        </div>
      )}
      <div className={`cell-num num ${active && d.status !== "processing" ? "strong" : ""}`}>
        {d.status === "downloading" ? fmt.speed(d.speed) : d.status === "seeding" ? `↑ ${fmt.speed(d.uploadSpeed)}` : d.status === "completed" ? fmt.bytes(d.total) : ""}
      </div>
      <div className="cell-num num">{d.status === "downloading" ? (eta != null ? fmt.duration(eta) : "—") : ""}</div>
      <div>
        <StatusBadge status={d.status} />
      </div>
      <div className="right">
        {action && (
          <IconButton
            icon={action.icon}
            label={action.label}
            size="sm"
            className="row-action"
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              action.run();
            }}
          />
        )}
      </div>
    </div>
  );
});

function HeaderCell({ label, k, sort, onSort, right }: { label: string; k?: SortKey; sort: SortState; onSort?: (s: SortState) => void; right?: boolean }) {
  if (!k || !onSort) return <div className={right ? "right" : undefined}>{label}</div>;
  const on = sort.key === k;
  return (
    <div className={right ? "right" : undefined}>
      <button type="button" onClick={() => onSort({ key: k, dir: on ? ((sort.dir * -1) as 1 | -1) : k === "name" ? 1 : -1 })} aria-sort={on ? (sort.dir === 1 ? "ascending" : "descending") : undefined}>
        {label}
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
  footer,
  reorderable,
  finished,
}: {
  ids: string[];
  empty: ReactNode;
  sort: SortState;
  onSort?: (s: SortState) => void;
  onVerify: (id: string) => void;
  onDropFiles: (dt: DataTransfer) => void;
  footer?: ReactNode;
  reorderable?: boolean;
  finished?: boolean;
}) {
  const app = useApp();
  const { selection, setSelection, setInspectorOpen, confirmRemove } = app;
  const scrollRef = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ top: 0, height: 600 });
  const [cursor, setCursor] = useState<number>(-1);
  const [anchor, setAnchor] = useState<number>(-1);
  const [dragOver, setDragOver] = useState(false);
  const compact = settingsStore.use()?.compact ?? false;
  const rowH = compact ? 36 : 48;

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewport({ top: el.scrollTop, height: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Keep selection limited to visible ids when the list changes.
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

  const onDoubleClick = useCallback(
    (id: string) => {
      const d = getDownload(id);
      if (!d) return;
      if (d.status === "completed" || d.status === "seeding") openDownload(d);
      else setInspectorOpen(true);
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
        if (list.some(canPause)) void run(api.pause(sel), "Could not pause");
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

  const overscan = 6;
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
        onDoubleClick={onDoubleClick}
        onContextMenu={onContextMenu}
      />,
    );
  }

  return (
    <div
      className="list"
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
      style={{ position: "relative" }}
    >
      <div className="list-header list-grid" role="row">
        <div />
        <HeaderCell label="Name" k="name" sort={sort} onSort={onSort} />
        {finished ? <HeaderCell label="Completed" k="added" sort={sort} onSort={onSort} /> : <HeaderCell label="Progress" k="progress" sort={sort} onSort={onSort} />}
        {finished ? <HeaderCell label="Size" k="size" sort={sort} onSort={onSort} right /> : <HeaderCell label="Speed" k="speed" sort={sort} onSort={onSort} right />}
        <HeaderCell label={finished ? "" : "ETA"} right sort={sort} />
        <HeaderCell label="Status" k="status" sort={sort} onSort={onSort} />
        <div />
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
      {footer}
      {dragOver && <div className="drop-hint">Drop links or .torrent files to download</div>}
    </div>
  );
}
