import { useMemo, useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import type { CoreEvent, Download, Queue, Schedule, Settings } from "./types";

/**
 * Download store with two subscription levels:
 *  - structure (membership / status / order) → list re-computes its ids
 *  - per row (progress) → only that row re-renders
 * Progress arrives batched once per second from KuCore.
 */
const rows = new Map<string, Download>();
const rowListeners = new Map<string, Set<() => void>>();
const structureListeners = new Set<() => void>();
let structureVersion = 0;

const speedListeners = new Set<() => void>();
let speed = { down: 0, up: 0, history: [] as number[] };

function bumpStructure() {
  structureVersion++;
  structureListeners.forEach((l) => l());
}

function notifyRow(id: string) {
  rowListeners.get(id)?.forEach((l) => l());
}

function put(d: Download) {
  const prev = rows.get(d.id);
  rows.set(d.id, d);
  notifyRow(d.id);
  if (!prev || prev.status !== d.status || prev.queueId !== d.queueId || prev.position !== d.position || prev.name !== d.name || prev.kind !== d.kind) {
    bumpStructure();
  }
}

export function allDownloads(): Download[] {
  return [...rows.values()];
}

export function getDownload(id: string): Download | undefined {
  return rows.get(id);
}

type EventHandler = (e: CoreEvent) => void;
const eventHandlers = new Set<EventHandler>();

export function onCoreEvent(h: EventHandler): () => void {
  eventHandlers.add(h);
  return () => eventHandlers.delete(h);
}

export function applyEvent(e: CoreEvent) {
  switch (e.type) {
    case "upsert":
      put(e.download);
      break;
    case "removed":
      for (const id of e.ids) rows.delete(id);
      bumpStructure();
      break;
    case "progress": {
      let structural = false;
      for (const p of e.items) {
        const d = rows.get(p.id);
        if (!d) continue;
        if (d.status !== p.status) structural = true;
        rows.set(p.id, { ...d, status: p.status, done: p.done, total: p.total, speed: p.speed, uploadSpeed: p.uploadSpeed, activeConnections: p.activeConnections });
        notifyRow(p.id);
      }
      if (structural) bumpStructure();
      const history = [...speed.history, e.downloadSpeed].slice(-60);
      speed = { down: e.downloadSpeed, up: e.uploadSpeed, history };
      speedListeners.forEach((l) => l());
      break;
    }
  }
  eventHandlers.forEach((h) => h(e));
}

let started = false;
/** Load everything once and subscribe to KuCore events. */
export async function startStore(): Promise<CoreEvent[]> {
  if (started) return [];
  started = true;
  await listen<CoreEvent>("ku", (msg) => applyEvent(msg.payload));
  const list = await api.list();
  for (const d of list) rows.set(d.id, d);
  bumpStructure();
  // Speeds decay to zero when nothing is transferring (no more progress events).
  setInterval(() => {
    const active = [...rows.values()].some((d) => d.status === "downloading" || d.status === "seeding" || d.status === "processing");
    if (!active && (speed.down !== 0 || speed.up !== 0)) {
      speed = { down: 0, up: 0, history: [...speed.history, 0].slice(-60) };
      speedListeners.forEach((l) => l());
    }
  }, 2000);
  return api.appReady();
}

function subscribeStructure(l: () => void) {
  structureListeners.add(l);
  return () => structureListeners.delete(l);
}

export function useStructureVersion(): number {
  return useSyncExternalStore(subscribeStructure, () => structureVersion);
}

/** Ids matching a predicate, sorted; re-computed only on structural change. */
export function useDownloadIds(filter: (d: Download) => boolean, sort: (a: Download, b: Download) => number, deps: unknown[]): string[] {
  const v = useStructureVersion();
  return useMemo(() => {
    const list = [...rows.values()].filter(filter);
    list.sort(sort);
    return list.map((d) => d.id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [v, ...deps]);
}

export function useDownload(id: string | null | undefined): Download | undefined {
  return useSyncExternalStore(
    (l) => {
      if (!id) return () => {};
      let set = rowListeners.get(id);
      if (!set) rowListeners.set(id, (set = new Set()));
      set.add(l);
      return () => {
        set!.delete(l);
        if (set!.size === 0) rowListeners.delete(id);
      };
    },
    () => (id ? rows.get(id) : undefined),
  );
}

export function useSpeed() {
  return useSyncExternalStore(
    (l) => {
      speedListeners.add(l);
      return () => speedListeners.delete(l);
    },
    () => speed,
  );
}

/** Small observable for settings / queues / schedules. */
function resource<T>(load: () => Promise<T>, initial: T, eventType: CoreEvent["type"]) {
  let value = initial;
  let loaded = false;
  const listeners = new Set<() => void>();
  const refresh = async () => {
    value = await load();
    loaded = true;
    listeners.forEach((l) => l());
  };
  onCoreEvent((e) => {
    if (e.type === eventType) void refresh();
  });
  return {
    refresh,
    get: () => value,
    set(v: T) {
      value = v;
      listeners.forEach((l) => l());
    },
    use(): T {
      return useSyncExternalStore(
        (l) => {
          listeners.add(l);
          if (!loaded) void refresh();
          return () => listeners.delete(l);
        },
        () => value,
      );
    },
  };
}

export const settingsStore = resource<Settings | null>(() => api.getSettings(), null, "settingsChanged");
export const queuesStore = resource<Queue[]>(() => api.queues(), [], "queuesChanged");
export const schedulesStore = resource<Schedule[]>(() => api.schedules(), [], "schedulesChanged");

export async function updateSettings(patch: Partial<Settings>): Promise<Settings> {
  const current = settingsStore.get() ?? (await api.getSettings());
  const next = await api.saveSettings({ ...current, ...patch });
  settingsStore.set(next);
  return next;
}
