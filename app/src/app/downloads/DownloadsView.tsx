import { useCallback, useRef, useState } from "react";
import { Plus, ClipboardPaste, Clapperboard, Play, Pause, Trash2, PanelRight, Ellipsis, Square, ListPlus, FileUp } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { useApp, type View } from "../context";
import { useDownloadIds, getDownload, queuesStore, settingsStore, updateSettings, useStructureVersion, allDownloads, useSpeed } from "../../lib/store";
import { api, errorText } from "../../lib/api";
import type { Download } from "../../lib/types";
import * as fmt from "../../lib/format";
import { Button, IconButton, EmptyState, Select, Input } from "../../ui/primitives";
import { showMenuAt, toast, Dialog, type MenuItem } from "../../ui/overlays";
import { DownloadList, compareBy, type SortState } from "./DownloadList";
import { Inspector, VerifyDialog } from "./Inspector";
import { SpeedControl } from "./SpeedPopover";
import { canPause, canResume, run } from "./actions";

type Mode = Extract<View, "downloads" | "finished" | "torrents" | "queue">;

const TITLES: Record<Mode, string> = { downloads: "Downloads", finished: "Finished", torrents: "Torrents", queue: "Queue" };

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
    toast({ level: "error", title: "Could not open the torrent", message: errorText(e) });
  }
}

function QueueBar({ queueId, setQueueId }: { queueId: string; setQueueId: (id: string) => void }) {
  const queues = queuesStore.use();
  const q = queues.find((x) => x.id === queueId) ?? queues[0];
  const [naming, setNaming] = useState<string | null>(null);
  if (!q) return null;
  const save = (patch: Partial<typeof q>) => run(api.saveQueue({ ...q, ...patch }).then(() => queuesStore.refresh()), "Could not update the queue");
  return (
    <div className="toolbar" style={{ background: "var(--bg-content)", gap: "var(--space-3)" }}>
      {queues.length > 1 ? (
        <Select value={q.id} onChange={(e) => setQueueId(e.target.value)} options={queues.map((x) => ({ value: x.id, label: x.name }))} style={{ width: 180 }} aria-label="Queue" />
      ) : (
        <span className="section-title">{q.name}</span>
      )}
      <span className="status" data-state={q.running ? "downloading" : "paused"}>
        {q.running ? "Running" : "Stopped"}
      </span>
      {q.running ? (
        <Button icon={Square} onClick={() => void run(api.stopQueue(q.id), "Could not stop the queue")}>
          Stop queue
        </Button>
      ) : (
        <Button variant="primary" icon={Play} onClick={() => void run(api.startQueue(q.id), "Could not start the queue")}>
          Start queue
        </Button>
      )}
      <span className="toolbar-sep" />
      <label className="muted" style={{ fontSize: "var(--text-xs)" }}>
        At a time
      </label>
      <Select
        value={q.maxConcurrent}
        onChange={(e) => void save({ maxConcurrent: +e.target.value })}
        options={[1, 2, 3, 4, 5, 6, 8, 10].map((n) => ({ value: n, label: String(n) }))}
        style={{ width: 64 }}
        aria-label="Downloads at a time"
      />
      <label className="muted" style={{ fontSize: "var(--text-xs)" }}>
        When done
      </label>
      <Select
        value={q.after}
        onChange={(e) => void save({ after: e.target.value })}
        options={[
          { value: "none", label: "Do nothing" },
          { value: "sleep", label: "Sleep" },
          { value: "shutdown", label: "Shut down" },
          { value: "quit", label: "Quit KuDownloader" },
        ]}
        style={{ width: 150 }}
        aria-label="When the queue finishes"
      />
      <span className="spacer" />
      <IconButton
        icon={Ellipsis}
        label="Queue options"
        onClick={(e) =>
          showMenuAt(
            e.currentTarget,
            [
              { label: "New queue…", icon: ListPlus, onSelect: () => setNaming("") },
              { label: "Rename…", onSelect: () => setNaming(q.name) },
              "sep",
              {
                label: "Delete queue",
                danger: true,
                disabled: q.id === "main",
                onSelect: () =>
                  void run(
                    api.deleteQueue(q.id).then(() => {
                      setQueueId("main");
                      return queuesStore.refresh();
                    }),
                    "Could not delete the queue",
                  ),
              },
            ],
            "end",
          )
        }
      />
      {naming != null && (
        <Dialog
          title={naming === "" ? "New queue" : "Rename queue"}
          width={380}
          onClose={() => setNaming(null)}
          onSubmit={async () => {
            if (!naming.trim()) return;
            try {
              const saved = await api.saveQueue(naming === "" || !q ? { name: naming, maxConcurrent: 2 } : { ...q, name: naming });
              await queuesStore.refresh();
              setQueueId(saved.id);
              setNaming(null);
            } catch (e) {
              toast({ level: "error", title: "Could not save the queue", message: errorText(e) });
            }
          }}
          footer={
            <>
              <span className="spacer" />
              <Button onClick={() => setNaming(null)}>Cancel</Button>
              <Button type="submit" variant="primary" disabled={!naming.trim()}>
                Save
              </Button>
            </>
          }
        >
          <Input value={naming} onChange={(e) => setNaming(e.target.value)} placeholder="Queue name" />
        </Dialog>
      )}
    </div>
  );
}

export function DownloadsView({ mode }: { mode: Mode }) {
  const app = useApp();
  const { selection, inspectorOpen, setInspectorOpen, search, openAdd, openMedia, confirmRemove } = app;
  const [sort, setSort] = useState<SortState>(mode === "queue" ? { key: "queue", dir: 1 } : { key: "added", dir: -1 });
  const [queueId, setQueueId] = useState("main");
  const [verifyId, setVerifyId] = useState<string | null>(null);
  const q = search.trim().toLowerCase();
  const filter = useCallback(
    (d: Download) => {
      const finished = d.status === "completed" || d.status === "seeding";
      let ok: boolean;
      switch (mode) {
        case "downloads":
          ok = true;
          break;
        case "finished":
          ok = finished;
          break;
        case "torrents":
          ok = d.kind === "torrent" || d.kind === "magnet";
          break;
        case "queue":
          ok = d.queueId === queueId && !finished;
          break;
      }
      return ok && (!q || d.name.toLowerCase().includes(q) || d.url.toLowerCase().includes(q));
    },
    [mode, q, queueId],
  );
  // "All downloads": unfinished work first (like IDM), then finished, each by the chosen sort.
  const cmp = compareBy(sort);
  const order = useCallback(
    (a: Download, b: Download) => {
      if (mode === "downloads" && sort.key === "added") {
        const fa = a.status === "completed" ? 1 : 0;
        const fb = b.status === "completed" ? 1 : 0;
        if (fa !== fb) return fa - fb;
      }
      return cmp(a, b);
    },
    [mode, sort, cmp],
  );
  const ids = useDownloadIds(filter, order, [filter, order]);
  useStructureVersion();
  const sel = [...selection].map(getDownload).filter((d): d is Download => !!d);
  const one = selection.size === 1 ? [...selection][0] : null;
  const settings = settingsStore.use();
  const speed = useSpeed();
  const moreRef = useRef<HTMLButtonElement>(null);

  const onDrop = async (dt: DataTransfer) => {
    const files = [...dt.files].filter((f) => f.name.toLowerCase().endsWith(".torrent"));
    for (const f of files.slice(0, 1)) {
      try {
        const t = await api.torrentInfo({ data: toBase64(await f.arrayBuffer()) });
        openAdd({ url: "", filename: t.info.name, options: { torrentData: t.data } });
      } catch (e) {
        toast({ level: "error", title: `Could not open ${f.name}`, message: errorText(e) });
      }
      return;
    }
    const text = dt.getData("text/uri-list") || dt.getData("text/plain");
    const links = text
      .split(/\r?\n/)
      .map((l) => l.trim())
      .filter((l) => l && !l.startsWith("#"));
    if (links.length) openAdd({ url: links.join("\n"), queueId: mode === "queue" ? queueId : undefined });
    else if (dt.files.length) toast({ level: "warning", title: "Unsupported file", message: "Drop .torrent files or links." });
  };

  const more: MenuItem[] = [
    { label: "Pause all", icon: Pause, onSelect: () => void run(api.pauseAll(), "Could not pause") },
    { label: "Resume all", icon: Play, onSelect: () => void run(api.resumeAll(), "Could not resume") },
    "sep",
    { label: "Open .torrent file…", icon: FileUp, onSelect: () => void pickTorrent(openAdd) },
    {
      label: "Clear finished",
      disabled: !allDownloads().some((d) => d.status === "completed"),
      onSelect: () => void run(api.clearFinished(), "Could not clear finished downloads"),
    },
    "sep",
    {
      label: "Compact rows",
      checked: !!settings?.compact,
      onSelect: () => void run(updateSettings({ compact: !settings?.compact }), "Could not change the layout"),
    },
  ];

  const empty = q ? (
    <EmptyState title="No matches" text={`Nothing in ${TITLES[mode]} matches “${search}”.`} />
  ) : mode === "downloads" ? (
    <EmptyState
      title="No downloads yet"
      text="Paste a link, drop a file here, or download from your browser."
      action={
        <Button variant="primary" icon={Plus} onClick={() => openAdd()}>
          Add download
        </Button>
      }
    />
  ) : mode === "finished" ? (
    <EmptyState title="Nothing finished yet" text="Completed downloads appear here." />
  ) : mode === "torrents" ? (
    <EmptyState
      title="No torrents"
      text="Open a .torrent file or paste a magnet link."
      action={
        <Button icon={FileUp} onClick={() => void pickTorrent(openAdd)}>
          Open .torrent file
        </Button>
      }
    />
  ) : (
    <EmptyState title="This queue is empty" text="Right-click a download and choose Move to queue, or use “Add to queue” when adding." />
  );

  const activeCount = allDownloads().filter((d) => d.status === "downloading" || d.status === "processing").length;
  const footer = (
    <div className="list-footer num">
      <span>
        {ids.length} {ids.length === 1 ? "item" : "items"}
        {selection.size > 1 ? ` · ${selection.size} selected` : ""}
      </span>
      <span className="spacer" />
      {activeCount > 0 && (
        <span>
          {activeCount} active · {fmt.speed(speed.down)}
        </span>
      )}
    </div>
  );

  return (
    <div className="main">
      <div className="toolbar">
        <span className="toolbar-title">{TITLES[mode]}</span>
        <Button variant="primary" icon={Plus} onClick={() => openAdd(mode === "queue" ? { queueId } : undefined)} title="Add download (Ctrl N)">
          Add download
        </Button>
        <Button variant="ghost" icon={ClipboardPaste} onClick={() => void pasteLink(openAdd)} title="Paste link (Ctrl V)">
          <span className="btn-label-optional">Paste link</span>
        </Button>
        <Button variant="ghost" icon={Clapperboard} onClick={() => openMedia()} title="Download video or audio">
          <span className="btn-label-optional">Media</span>
        </Button>
        <span className="toolbar-sep" />
        <IconButton icon={Play} label="Resume (Space)" disabled={!sel.some(canResume)} onClick={() => void run(api.resume([...selection]), "Could not resume")} />
        <IconButton icon={Pause} label="Pause (Space)" disabled={!sel.some(canPause)} onClick={() => void run(api.pause([...selection]), "Could not pause")} />
        <IconButton icon={Trash2} label="Remove (Del)" disabled={!sel.length} onClick={() => confirmRemove([...selection])} />
        <span className="spacer" />
        <SpeedControl />
        <IconButton icon={PanelRight} label="Details (Ctrl I)" on={inspectorOpen} onClick={() => setInspectorOpen(!inspectorOpen)} />
        <IconButton ref={moreRef} icon={Ellipsis} label="More" onClick={(e) => showMenuAt(e.currentTarget, more, "end")} />
      </div>
      <div style={{ display: "grid", gridTemplateRows: mode === "queue" ? "auto 1fr" : "1fr", minHeight: 0 }}>
        {mode === "queue" && <QueueBar queueId={queueId} setQueueId={setQueueId} />}
        <div className="main-split">
          <DownloadList
            ids={ids}
            empty={empty}
            sort={sort}
            onSort={setSort}
            onVerify={setVerifyId}
            onDropFiles={(dt) => void onDrop(dt)}
            footer={footer}
            reorderable={mode === "queue"}
            finished={mode === "finished"}
          />
          {inspectorOpen && one && <Inspector id={one} onClose={() => setInspectorOpen(false)} onVerify={setVerifyId} />}
        </div>
      </div>
      {verifyId && <VerifyDialog id={verifyId} onClose={() => setVerifyId(null)} />}
    </div>
  );
}

