import {
  Play,
  Pause,
  RotateCcw,
  FolderOpen,
  ExternalLink,
  Copy,
  Trash2,
  ListOrdered,
  ArrowUp,
  ArrowDown,
  ArrowUpToLine,
  ArrowDownToLine,
  ShieldCheck,
  type LucideIcon,
} from "lucide-react";
import { api, errorText } from "../../lib/api";
import { getDownload, queuesStore } from "../../lib/store";
import type { Download } from "../../lib/types";
import { toast, type MenuItem } from "../../ui/overlays";

export async function run(p: Promise<unknown>, failTitle: string) {
  try {
    await p;
  } catch (e) {
    toast({ level: "error", title: failTitle, message: errorText(e) });
  }
}

export function canPause(d: Download) {
  return d.status === "downloading" || d.status === "processing" || d.status === "queued" || d.status === "seeding";
}

export function canResume(d: Download) {
  return d.status === "paused" || d.status === "error";
}

export function primaryAction(d: Download): { icon: LucideIcon; label: string; run: () => void } | null {
  switch (d.status) {
    case "downloading":
    case "processing":
    case "queued":
      return { icon: Pause, label: "Pause", run: () => void run(api.pause([d.id]), "Could not pause") };
    case "seeding":
      return { icon: Pause, label: "Stop seeding", run: () => void run(api.pause([d.id]), "Could not stop seeding") };
    case "paused":
      return { icon: Play, label: "Resume", run: () => void run(api.resume([d.id]), "Could not resume") };
    case "error":
      return { icon: RotateCcw, label: "Retry", run: () => void run(api.resume([d.id]), "Could not retry") };
    case "completed":
      return { icon: FolderOpen, label: "Show in folder", run: () => void run(api.openFolder(d.id), "Could not open the folder") };
  }
}

export function openDownload(d: Download) {
  if (d.status === "completed" || d.status === "seeding") void run(api.openFile(d.id), "Could not open the file");
}

export function contextMenu(ids: string[], opts: { confirmRemove: (ids: string[]) => void; onVerify: (id: string) => void }): MenuItem[] {
  const list = ids.map(getDownload).filter((d): d is Download => !!d);
  if (!list.length) return [];
  const one = list.length === 1 ? list[0] : null;
  const anyPause = list.some(canPause);
  const anyResume = list.some(canResume);
  const finished = one && (one.status === "completed" || one.status === "seeding");
  const queues = queuesStore.get();
  const inQueue = one?.queueId;
  const items: MenuItem[] = [];
  if (one) {
    items.push(
      { label: "Open", icon: ExternalLink, shortcut: "Enter", disabled: !finished, onSelect: () => openDownload(one) },
      { label: "Show in folder", icon: FolderOpen, shortcut: "Ctrl Shift O", onSelect: () => void run(api.openFolder(one.id), "Could not open the folder") },
      "sep",
    );
  }
  if (anyResume) items.push({ label: list.some((d) => d.status === "error") && !list.some((d) => d.status === "paused") ? "Retry" : "Resume", icon: Play, shortcut: "Space", onSelect: () => void run(api.resume(ids), "Could not resume") });
  if (anyPause) items.push({ label: "Pause", icon: Pause, shortcut: "Space", onSelect: () => void run(api.pause(ids), "Could not pause") });
  items.push({
    label: "Move to queue",
    icon: ListOrdered,
    submenu: [
      { label: "No queue (start directly)", checked: list.every((d) => !d.queueId), onSelect: () => void run(api.moveToQueue(ids, null), "Could not move") },
      "sep",
      ...queues.map((q) => ({ label: q.name, checked: list.every((d) => d.queueId === q.id), onSelect: () => void run(api.moveToQueue(ids, q.id), "Could not move") })),
    ],
  });
  if (one && inQueue && !finished) {
    items.push(
      { label: "Move to top", icon: ArrowUpToLine, onSelect: () => void run(api.reorder(one.id, "top"), "Could not reorder") },
      { label: "Move up", icon: ArrowUp, shortcut: "Alt ↑", onSelect: () => void run(api.reorder(one.id, "up"), "Could not reorder") },
      { label: "Move down", icon: ArrowDown, shortcut: "Alt ↓", onSelect: () => void run(api.reorder(one.id, "down"), "Could not reorder") },
      { label: "Move to bottom", icon: ArrowDownToLine, onSelect: () => void run(api.reorder(one.id, "bottom"), "Could not reorder") },
    );
  }
  items.push("sep");
  if (one) {
    items.push({
      label: "Copy link",
      icon: Copy,
      disabled: one.url.startsWith("torrent:"),
      onSelect: () => {
        void navigator.clipboard.writeText(one.url);
        toast({ level: "success", title: "Link copied" });
      },
    });
    if (finished && one.engine === "aria2" && one.kind !== "torrent" && one.kind !== "magnet") {
      items.push({ label: "Verify checksum…", icon: ShieldCheck, onSelect: () => opts.onVerify(one.id) });
    }
    items.push("sep");
  }
  items.push({ label: list.length > 1 ? `Remove ${list.length} downloads…` : "Remove…", icon: Trash2, shortcut: "Del", danger: true, onSelect: () => opts.confirmRemove(ids) });
  return items;
}
