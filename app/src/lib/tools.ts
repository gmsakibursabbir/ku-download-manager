import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api, errorText } from "./api";
import { onCoreEvent } from "./store";
import { toast } from "../ui/overlays";

/**
 * On-demand tool downloads (yt-dlp, FFmpeg) tracked for the whole app, not a
 * screen: switching views keeps them running and visible, and the backend
 * allows only one download per tool.
 */
export type ToolName = "yt-dlp" | "ffmpeg";
export interface ToolJob {
  tool: ToolName;
  state: "waiting" | "downloading";
  done: number;
  total: number;
  /** Bytes per second (smoothed). */
  speed: number;
}

const label = (t: string) => (t === "ffmpeg" ? "FFmpeg" : "yt-dlp");
let jobs: ToolJob[] = [];
const listeners = new Set<() => void>();
const changed = () => {
  jobs = [...jobs];
  listeners.forEach((l) => l());
};
const doneWaiters = new Map<string, (ok: boolean) => void>();
let last: Record<string, { at: number; done: number }> = {};

onCoreEvent((e) => {
  if (e.type === "toolProgress") {
    let j = jobs.find((x) => x.tool === e.tool);
    if (!j) {
      j = { tool: e.tool as ToolName, state: "downloading", done: 0, total: 0, speed: 0 };
      jobs.push(j);
    }
    const now = performance.now();
    const prev = last[e.tool];
    if (prev && now > prev.at && e.done >= prev.done) {
      const inst = ((e.done - prev.done) * 1000) / (now - prev.at);
      j.speed = j.speed ? j.speed * 0.7 + inst * 0.3 : inst;
    }
    last[e.tool] = { at: now, done: e.done };
    j.state = "downloading";
    j.done = e.done;
    j.total = e.total;
    changed();
  } else if (e.type === "toolDone") {
    jobs = jobs.filter((x) => x.tool !== e.tool);
    delete last[e.tool];
    changed();
    toast(e.ok ? { level: "success", title: `${label(e.tool)} installed` } : { level: "error", title: `Could not download ${label(e.tool)}`, message: e.message });
    doneWaiters.get(e.tool)?.(e.ok);
    doneWaiters.delete(e.tool);
  }
});

/** Downloads already running when this window opened. */
void invoke<string[]>("tool_jobs")
  .then((running) => {
    for (const t of running) if (!jobs.some((j) => j.tool === t)) jobs.push({ tool: t as ToolName, state: "downloading", done: 0, total: 0, speed: 0 });
    if (running.length) changed();
  })
  .catch(() => {});

/** Install tools one after another; keeps going whatever screen is open. */
export async function installTools(tools: ToolName[]): Promise<boolean> {
  const todo = tools.filter((t) => !jobs.some((j) => j.tool === t));
  if (!todo.length) return true;
  for (const t of todo) jobs.push({ tool: t, state: "waiting", done: 0, total: 0, speed: 0 });
  changed();
  for (const t of todo) {
    const finished = new Promise<boolean>((r) => doneWaiters.set(t, r));
    try {
      await api.installTool(t);
    } catch (e) {
      // "already being downloaded" etc.: the toolDone event may not come.
      if (doneWaiters.has(t)) {
        doneWaiters.delete(t);
        jobs = jobs.filter((x) => x.tool !== t);
        changed();
        toast({ level: "error", title: `Could not download ${label(t)}`, message: errorText(e) });
      }
      jobs = jobs.filter((x) => !todo.includes(x.tool) || x.state !== "waiting");
      changed();
      return false;
    }
    if (!(await finished)) return false;
  }
  return true;
}

export function useToolJobs(): ToolJob[] {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => jobs,
  );
}

export function jobLabel(j: ToolJob, fmt: { bytes: (n: number) => string; duration: (s: number) => string }): string {
  if (j.state === "waiting") return "Waiting…";
  if (!j.total) return j.done ? fmt.bytes(j.done) : "Starting…";
  const left = j.speed > 0 ? ` · ${fmt.duration((j.total - j.done) / j.speed)} left` : "";
  return `${fmt.bytes(j.done)} of ${fmt.bytes(j.total)}${left}`;
}
