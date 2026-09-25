import { useCallback, useEffect, useRef, useState } from "react";
import { t, tf } from "../../lib/i18n";
import {
  Plus,
  Play,
  Pause,
  Trash2,
  Ellipsis,
  Square,
  ListPlus,
  ListStart,
  ListX,
  CalendarClock,
  Hand,
  Search,
  Settings as SettingsIcon,
  ChevronDown,
  X,
  FileUp,
  PanelRight,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { useApp } from "../context";
import { useDownloadIds, getDownload, queuesStore, allDownloads, queueName } from "../../lib/store";
import { api, errorText } from "../../lib/api";
import type { Download } from "../../lib/types";
import { Button, IconButton, EmptyState, Select, Input, Icon } from "../../ui/primitives";
import { showMenuAt, toast, Dialog, type MenuItem } from "../../ui/overlays";
import { DownloadList, compareBy, type SortState } from "./DownloadList";
import { Inspector, VerifyDialog } from "./Inspector";
import { SpeedControl } from "./SpeedPopover";
import { canPause, canResume, run } from "./actions";

function toBase64(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf);
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

export async function pasteLink(openAdd: (p?: { url: string }) => void) {
  const text = ((await api.readClipboard().catch(() => null)) ?? "").trim();
  openAdd(text && /^(https?|ftp|sftp):\/\/|^magnet:/i.test(text) ? { url: text } : undefined);
}

export async function pickTorrent(openAdd: (p: Record<string, unknown>) => void) {
  const path = await open({ multiple: false, filters: [{ name: "Torrent", extensions: ["torrent"] }] });
  if (typeof path !== "string") return;
  try {
    const t = await api.torrentInfo({ path });
    openAdd({ url: "", filename: t.info.name, options: { torrentData: t.data } });
  } catch (e) {
    toast({ level: "error", title: t("Could not open the torrent"), message: errorText(e) });
  }
}

/** Toolbar button, optionally split with a dropdown arrow. */
function TbButton({
  icon,
  label,
  onClick,
  menu,
  disabled,
  danger,
  title,
  secondary,
}: {
  icon: typeof Plus;
  label: string;
  onClick?: () => void;
  menu?: () => MenuItem[];
  disabled?: boolean;
  danger?: boolean;
  title?: string;
  /** Collapses to icon-only first when space is tight. */
  secondary?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const main = onClick ?? (() => menu && showMenuAt(ref.current!, menu()));
  return (
    <div ref={ref} className={`tb-btn ${danger ? "is-danger" : ""} ${secondary ? "is-secondary" : ""}`} data-split={!!(onClick && menu) || undefined}>
      <button type="button" className="tb-main" disabled={disabled} onClick={main} title={title ?? label}>
        <Icon icon={icon} />
        <span className="tb-label">{label}</span>
        {menu && !onClick && <Icon icon={ChevronDown} size={12} className="tb-caret" />}
      </button>
      {menu && onClick && (
        <button type="button" className="tb-arrow" aria-label={tf("{name} options", { name: label })} onClick={() => showMenuAt(ref.current!, menu())}>
          <Icon icon={ChevronDown} size={12} />
        </button>
      )}
    </div>
  );
}

function QueueBar({ queueId }: { queueId: string }) {
  const { showList } = useApp();
  const queues = queuesStore.use();
  const q = queues.find((x) => x.id === queueId) ?? queues[0];
  const [naming, setNaming] = useState<string | null>(null);
  if (!q) return null;
  const save = (patch: Partial<typeof q>) => run(api.saveQueue({ ...q, ...patch }).then(() => queuesStore.refresh()), "Could not update the queue");
  return (
    <div className="queue-bar card">
      <span className="section-title">{queueName(q)}</span>
      <span className="status" data-state={q.running ? "downloading" : "paused"}>
        {q.running ? t("Running") : t("Stopped")}
      </span>
      {q.running ? (
        <Button size="sm" icon={Square} onClick={() => void run(api.stopQueue(q.id), "Could not stop the queue")}>
          {t("Stop queue")}
        </Button>
      ) : (
        <Button size="sm" variant="primary" icon={Play} onClick={() => void run(api.startQueue(q.id), "Could not start the queue")}>
          {t("Start queue")}
        </Button>
      )}
      <span className="toolbar-sep" />
      <label className="muted" style={{ fontSize: "var(--text-xs)" }}>
        {t("At a time")}
      </label>
      <Select value={q.maxConcurrent} onChange={(e) => void save({ maxConcurrent: +e.target.value })} options={[1, 2, 3, 4, 5, 6, 8, 10].map((n) => ({ value: n, label: String(n) }))} style={{ width: 64, height: 28 }} aria-label={t("Downloads at a time")} />
      <label className="muted" style={{ fontSize: "var(--text-xs)" }} title={t("Re-check finished files on the server and download the ones that changed (IDM-style synchronization).")}>
        {t("Sync")}
      </label>
      <Select
        value={q.syncMinutes ?? 0}
        onChange={(e) => void save({ syncMinutes: +e.target.value })}
        options={[
          { value: 0, label: t("Off") },
          { value: 15, label: t("Every 15 min") },
          { value: 60, label: t("Every hour") },
          { value: 360, label: t("Every 6 hours") },
          { value: 1440, label: t("Daily") },
        ]}
        style={{ width: 130, height: 28 }}
        aria-label={t("Synchronize finished files")}
      />
      <label className="muted" style={{ fontSize: "var(--text-xs)" }}>
        {t("When done")}
      </label>
      <Select
        value={q.after}
        onChange={(e) => void save({ after: e.target.value })}
        options={[
          { value: "none", label: t("Do nothing") },
          { value: "sleep", label: t("Sleep") },
          { value: "shutdown", label: t("Shut down") },
          { value: "quit", label: t("Quit KuDownloader") },
        ]}
        style={{ width: 150, height: 28 }}
        aria-label={t("When the queue finishes")}
      />
      <span className="spacer" />
      <IconButton
        icon={Ellipsis}
        label={t("Queue options")}
        size="sm"
        onClick={(e) =>
          showMenuAt(
            e.currentTarget,
            [
              { label: t("New queue…"), icon: ListPlus, onSelect: () => setNaming("") },
              { label: t("Rename…"), onSelect: () => setNaming(q.name) },
              "sep",
              {
                label: t("Delete queue"),
                danger: true,
                disabled: q.id === "main",
                onSelect: () => void run(api.deleteQueue(q.id).then(() => (showList({ scope: "queue", queueId: "main" }), queuesStore.refresh())), "Could not delete the queue"),
              },
            ],
            "end",
          )
        }
      />
      {naming != null && (
        <Dialog
          title={naming === "" ? t("New queue") : t("Rename queue")}
          width={380}
          onClose={() => setNaming(null)}
          onSubmit={async () => {
            if (!naming.trim()) return;
            try {
              const saved = await api.saveQueue(naming === "" ? { name: naming, maxConcurrent: 2 } : { ...q, name: naming });
              await queuesStore.refresh();
              showList({ scope: "queue", queueId: saved.id });
              setNaming(null);
            } catch (e) {
              toast({ level: "error", title: t("Could not save the queue"), message: errorText(e) });
            }
          }}
          footer={
            <>
              <span className="spacer" />
              <Button onClick={() => setNaming(null)}>{t("Cancel")}</Button>
              <Button type="submit" variant="primary" disabled={!naming.trim()}>
                {t("Save")}
              </Button>
            </>
          }
        >
          <Input value={naming} onChange={(e) => setNaming(e.target.value)} placeholder={t("Queue name")} />
        </Dialog>
      )}
    </div>
  );
}

export function DownloadsView() {
  const app = useApp();
  const { filter, selection, inspectorOpen, setInspectorOpen, search, setSearch, openAdd, confirmRemove, navigate, openSettings } = app;
  const [sort, setSort] = useState<SortState>({ key: "added", dir: -1 });
  const [verifyId, setVerifyId] = useState<string | null>(null);
  const [searching, setSearching] = useState(!!search);
  const searchRef = useRef<HTMLInputElement>(null);
  const queues = queuesStore.use();
  useEffect(() => setSort(filter.scope === "queue" ? { key: "queue", dir: 1 } : { key: "added", dir: -1 }), [filter.scope]);
  useEffect(() => {
    if (searching) searchRef.current?.focus();
  }, [searching]);
  useEffect(() => {
    const onFind = () => setSearching(true);
    window.addEventListener("ku:find", onFind);
    return () => window.removeEventListener("ku:find", onFind);
  }, []);

  const q = search.trim().toLowerCase();
  const filterFn = useCallback(
    (d: Download) => {
      const finished = d.status === "completed" || d.status === "seeding";
      let ok = true;
      if (filter.scope === "unfinished") ok = !finished;
      else if (filter.scope === "finished") ok = finished;
      else if (filter.scope === "queue") ok = d.queueId === (filter.queueId ?? "main") && !finished;
      if (ok && filter.category) ok = d.category === filter.category;
      return ok && (!q || d.name.toLowerCase().includes(q) || d.url.toLowerCase().includes(q));
    },
    [filter, q],
  );
  // All Downloads: unfinished work first (IDM style), then finished.
  const cmp = compareBy(sort);
  const order = useCallback(
    (a: Download, b: Download) => {
      if (filter.scope === "all" && sort.key === "added") {
        const fa = a.status === "completed" ? 1 : 0;
        const fb = b.status === "completed" ? 1 : 0;
        if (fa !== fb) return fa - fb;
      }
      return cmp(a, b);
    },
    [filter.scope, sort, cmp],
  );
  const ids = useDownloadIds(filterFn, order, [filterFn, order]);
  const sel = [...selection].map(getDownload).filter((d): d is Download => !!d);
  const one = selection.size === 1 ? [...selection][0] : null;

  const onDrop = async (dt: DataTransfer) => {
    const files = [...dt.files].filter((f) => f.name.toLowerCase().endsWith(".torrent"));
    for (const f of files.slice(0, 1)) {
      try {
        const t = await api.torrentInfo({ data: toBase64(await f.arrayBuffer()) });
        openAdd({ url: "", filename: t.info.name, options: { torrentData: t.data } });
      } catch (e) {
        toast({ level: "error", title: tf("Could not open {name}", { name: f.name }), message: errorText(e) });
      }
      return;
    }
    const text = dt.getData("text/uri-list") || dt.getData("text/plain");
    const links = text
      .split(/\r?\n/)
      .map((l) => l.trim())
      .filter((l) => l && !l.startsWith("#"));
    if (links.length) openAdd({ url: links.join("\n"), queueId: filter.scope === "queue" ? filter.queueId : undefined });
    else if (dt.files.length) toast({ level: "warning", title: t("Unsupported file"), message: t("Drop .torrent files or links.") });
  };

  const queueItems = (action: "start" | "stop"): MenuItem[] =>
    queues.map((qq) => ({
      label: queueName(qq),
      icon: action === "start" ? ListStart : ListX,
      disabled: action === "start" ? qq.running : !qq.running,
      onSelect: () => void run(action === "start" ? api.startQueue(qq.id) : api.stopQueue(qq.id), `Could not ${action} the queue`),
    }));

  const scopeLabel = { all: "downloads", unfinished: "unfinished downloads", finished: "finished downloads", queue: "items in this queue" }[filter.scope];
  const empty = q ? (
    <EmptyState title={t("No matches")} text={`No ${scopeLabel} match “${search}”.`} />
  ) : filter.scope === "finished" ? (
    <EmptyState title={t("Nothing finished yet")} text={t("Completed downloads appear here.")} />
  ) : filter.scope === "queue" ? (
    <EmptyState title={t("This queue is empty")} text={t("Right-click a download and choose Move to queue, or use “Download Later”.")} />
  ) : (
    <EmptyState
      title={filter.category ? t("Nothing in this category") : t("No downloads yet")}
      text={t("Paste a link, drop a file here, or download from your browser.")}
      action={
        <Button variant="primary" icon={Plus} onClick={() => openAdd()}>
          {t("Add URL")}
        </Button>
      }
    />
  );

  return (
    <div className="main main-fluent">
      <div className="toolbar-card card">
        <TbButton icon={Plus} label={t("Add URL")} onClick={() => openAdd(filter.scope === "queue" ? { queueId: filter.queueId } : undefined)} title={t("Add URL (Ctrl N)")} />
        <span className="toolbar-sep" />
        <TbButton icon={Play} label={t("Resume")} disabled={!sel.some(canResume)} onClick={() => void run(api.resume([...selection]), "Could not resume")} title={t("Resume (Space)")} />
        <TbButton
          icon={Pause}
          label={t("Stop")}
          disabled={!sel.some(canPause) && !allDownloads().some(canPause)}
          onClick={() => void run(api.pause([...selection]), "Could not stop")}
          menu={() => [
            { label: t("Stop selected"), icon: Pause, disabled: !sel.some(canPause), onSelect: () => void run(api.pause([...selection]), "Could not stop") },
            { label: t("Stop all"), icon: Square, onSelect: () => void run(api.pauseAll(), "Could not stop") },
          ]}
          title={t("Stop (Space)")}
        />
        <TbButton
          icon={Trash2}
          label={t("Delete")}
          danger
          disabled={!sel.length && !allDownloads().some((d) => d.status === "completed")}
          onClick={() => sel.length && confirmRemove([...selection])}
          menu={() => [
            { label: t("Delete selected…"), icon: Trash2, disabled: !sel.length, onSelect: () => confirmRemove([...selection]) },
            { label: t("Delete all completed"), disabled: !allDownloads().some((d) => d.status === "completed"), onSelect: () => void run(api.clearFinished(), "Could not delete") },
          ]}
          title={t("Delete (Del)")}
        />
        <span className="toolbar-sep" />
        <TbButton icon={ListStart} label={t("Start Queue")} secondary menu={() => queueItems("start")} />
        <TbButton icon={ListX} label={t("Stop Queue")} secondary menu={() => queueItems("stop")} />
        <span className="toolbar-sep" />
        <TbButton icon={CalendarClock} label={t("Scheduler")} secondary onClick={() => navigate("scheduled")} />
        <TbButton icon={Hand} label={t("Fetch")} secondary onClick={() => navigate("grabber")} />
        <span className="spacer" />
        {searching ? (
          <div className="input-with-icon tb-search">
            <Icon icon={Search} size={14} />
            <input
              ref={searchRef}
              id="global-search"
              className="input"
              placeholder={t("Search downloads")}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  setSearch("");
                  setSearching(false);
                }
              }}
              onBlur={() => !search && setSearching(false)}
            />
            {search && <IconButton icon={X} label={t("Clear search")} size="sm" onMouseDown={(e) => e.preventDefault()} onClick={() => setSearch("")} />}
          </div>
        ) : (
          <IconButton icon={Search} label={t("Search (Ctrl F)")} onClick={() => setSearching(true)} />
        )}
        <SpeedControl />
        <IconButton
          icon={SettingsIcon}
          label={t("Settings")}
          onClick={(e) =>
            showMenuAt(
              e.currentTarget,
              [
                { label: t("Settings…"), icon: SettingsIcon, shortcut: "Ctrl ,", onSelect: () => openSettings("general") },
                { label: t("Details panel"), icon: PanelRight, shortcut: "Ctrl I", checked: undefined, onSelect: () => setInspectorOpen(!inspectorOpen) },
                { label: t("Open .torrent file…"), icon: FileUp, onSelect: () => void pickTorrent(openAdd) },
              ],
              "end",
            )
          }
        />
      </div>
      {filter.scope === "queue" && <QueueBar queueId={filter.queueId ?? "main"} />}
      <div className="main-split">
        <DownloadList ids={ids} empty={empty} sort={sort} onSort={setSort} onVerify={setVerifyId} onDropFiles={(dt) => void onDrop(dt)} reorderable={filter.scope === "queue"} />
        {inspectorOpen && one && <Inspector id={one} onClose={() => setInspectorOpen(false)} onVerify={setVerifyId} />}
      </div>
      {verifyId && <VerifyDialog id={verifyId} onClose={() => setVerifyId(null)} />}
    </div>
  );
}
