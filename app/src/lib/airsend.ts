import { invoke } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";
import { onCoreEvent } from "./store";

/** A KuDownloader nearby with KuAirSend on. */
export interface AirPeer {
  fingerprint: string;
  alias: string;
  avatar: string;
  os: string;
  version: string;
  ip: string;
  port: number;
  trusted: boolean;
}

export interface AirFile {
  name: string;
  size: number;
  done: boolean;
}

export type AirState = "waiting" | "transferring" | "done" | "declined" | "pin" | "failed" | "cancelled";

export interface AirTransfer {
  id: string;
  direction: "send" | "receive";
  peer: string;
  peerFingerprint: string;
  peerAvatar: string;
  state: AirState;
  files: AirFile[];
  fileCount: number;
  total: number;
  done: number;
  speed: number;
  started: number;
  finished?: number | null;
  error?: string | null;
  folder?: string | null;
  text?: string | null;
}

/** Someone wants to send files: Accept / Decline. */
export interface AirRequest {
  id: string;
  peer: string;
  peerFingerprint: string;
  peerAvatar: string;
  peerOs: string;
  files: AirFile[];
  fileCount: number;
  total: number;
}

export interface AirMessage {
  id: string;
  peer: string;
  peerFingerprint: string;
  peerAvatar: string;
  text: string;
}

export interface AirStatus {
  running: boolean;
  alias: string;
  avatar: string;
  fingerprint: string;
  port: number;
  addresses: string[];
  folder: string;
  discoveryError?: string | null;
}

export const AVATARS = ["cat", "fox", "frog", "panda", "bunny", "penguin", "pig", "chick"] as const;

export const airApi = {
  status: () => invoke<AirStatus>("airsend_status"),
  setEnabled: (enabled: boolean) => invoke<AirStatus>("airsend_set_enabled", { enabled }),
  peers: () => invoke<AirPeer[]>("airsend_peers"),
  transfers: () => invoke<AirTransfer[]>("airsend_transfers"),
  send: (fingerprint: string, paths: string[], text?: string | null, pin?: string | null) =>
    invoke<string>("airsend_send", { fingerprint, paths, text: text ?? null, pin: pin ?? null }),
  cancel: (id: string) => invoke<void>("airsend_cancel", { id }),
  decide: (id: string, accept: boolean, trust: boolean) => invoke<void>("airsend_decide", { id, accept, trust }),
  refresh: () => invoke<void>("airsend_refresh"),
  add: (address: string) => invoke<AirPeer>("airsend_add", { address }),
  trust: (fingerprint: string, trusted: boolean) => invoke<void>("airsend_trust", { fingerprint, trusted }),
  clearHistory: () => invoke<void>("airsend_clear_history"),
};

export const isFinished = (t: AirTransfer) => ["done", "declined", "pin", "failed", "cancelled"].includes(t.state);

/**
 * Live KuAirSend state for every screen: nearby devices, transfers, and the
 * incoming offers / messages waiting for an answer (shown wherever you are).
 */
interface Snapshot {
  peers: AirPeer[];
  transfers: AirTransfer[];
  requests: AirRequest[];
  messages: AirMessage[];
}

let snap: Snapshot = { peers: [], transfers: [], requests: [], messages: [] };
const listeners = new Set<() => void>();

function set(p: Partial<Snapshot>) {
  snap = { ...snap, ...p };
  listeners.forEach((l) => l());
}

onCoreEvent((e) => {
  switch (e.type) {
    case "airSendPeers":
      set({ peers: e.peers });
      break;
    case "airSendTransfer": {
      const t = e.transfer;
      const rest = snap.transfers.filter((x) => x.id !== t.id);
      const transfers = snap.transfers.some((x) => x.id === t.id) ? snap.transfers.map((x) => (x.id === t.id ? t : x)) : [t, ...rest].slice(0, 150);
      // An offer that was answered, withdrawn or timed out closes its prompt.
      const requests = t.state === "waiting" ? snap.requests : snap.requests.filter((r) => r.id !== t.id);
      set({ transfers, requests });
      break;
    }
    case "airSendRequest":
      set({ requests: [...snap.requests.filter((r) => r.id !== e.request.id), e.request] });
      break;
    case "airSendMessage":
      set({ messages: [...snap.messages, e.message].slice(-20) });
      break;
  }
});

let loaded = false;
/** First load (later changes arrive as events). */
export function loadAir() {
  if (loaded) return;
  loaded = true;
  void airApi.peers().then((peers) => set({ peers })).catch(() => {});
  void airApi.transfers().then((transfers) => set({ transfers })).catch(() => {});
}

export function reloadTransfers() {
  void airApi.transfers().then((transfers) => set({ transfers })).catch(() => {});
}

export function dismissRequest(id: string) {
  set({ requests: snap.requests.filter((r) => r.id !== id) });
}

export function dismissMessage(id: string) {
  set({ messages: snap.messages.filter((m) => m.id !== id) });
}

export function useAir(): Snapshot {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => snap,
  );
}

/** Stable per-device pick (0..n-1) from its fingerprint. */
export function hashPick(fingerprint: string, n: number): number {
  let h = 2166136261;
  for (let i = 0; i < fingerprint.length; i++) h = Math.imul(h ^ fingerprint.charCodeAt(i), 16777619);
  return (h >>> 0) % n;
}
