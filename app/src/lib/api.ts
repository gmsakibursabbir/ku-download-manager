import { invoke } from "@tauri-apps/api/core";
import type * as T from "./types";

/** Typed wrappers around the Rust IPC commands. */
export const api = {
  appReady: () => invoke<T.CoreEvent[]>("app_ready"),
  appInfo: () => invoke<T.AppInfo>("app_info"),
  list: () => invoke<T.Download[]>("list_downloads"),
  add: (req: T.AddRequest) => invoke<T.Download>("add_download", { req }),
  addBatch: (urls: string[], template: T.AddRequest) =>
    invoke<{ added: T.Download[]; failed: { url: string; error: string }[] }>("add_batch", { urls, template }),
  probe: (url: string, options?: Partial<T.DownloadOptions>) => invoke<T.ProbeInfo>("probe_url", { url, options }),
  pause: (ids: string[]) => invoke<void>("pause", { ids }),
  resume: (ids: string[]) => invoke<void>("resume", { ids }),
  remove: (ids: string[], deleteFiles: boolean) => invoke<void>("remove", { ids, deleteFiles }),
  pauseAll: () => invoke<void>("pause_all"),
  resumeAll: () => invoke<void>("resume_all"),
  clearFinished: () => invoke<string[]>("clear_finished"),
  edit: (id: string, patch: Record<string, unknown>) => invoke<T.Download>("edit_download", { id, patch }),
  moveToQueue: (ids: string[], queueId: string | null) => invoke<void>("move_to_queue", { ids, queueId }),
  reorder: (id: string, direction: "up" | "down" | "top" | "bottom") => invoke<void>("reorder", { id, direction }),
  details: (id: string) => invoke<T.Details>("get_details", { id }),
  verify: (id: string, algo: string) => invoke<string>("verify_hash", { id, algo }),
  openFile: (id: string) => invoke<void>("open_file", { id }),
  openFolder: (id: string) => invoke<void>("open_folder", { id }),
  openDataDir: () => invoke<void>("open_data_dir"),
  getSettings: () => invoke<T.Settings>("get_settings"),
  saveSettings: (settings: T.Settings) => invoke<T.Settings>("save_settings", { settings }),
  setProfile: (id: string) => invoke<void>("set_profile", { id }),
  queues: () => invoke<T.Queue[]>("list_queues"),
  saveQueue: (queue: Partial<T.Queue>) => invoke<T.Queue>("save_queue", { queue }),
  deleteQueue: (id: string) => invoke<void>("delete_queue", { id }),
  startQueue: (id: string) => invoke<void>("start_queue", { id }),
  stopQueue: (id: string) => invoke<void>("stop_queue", { id }),
  schedules: () => invoke<T.Schedule[]>("list_schedules"),
  saveSchedule: (schedule: Partial<T.Schedule>) => invoke<T.Schedule>("save_schedule", { schedule }),
  deleteSchedule: (id: string) => invoke<void>("delete_schedule", { id }),
  setAfterAll: (action: string) => invoke<void>("set_after_all", { action }),
  getAfterAll: () => invoke<string>("get_after_all"),
  cancelPower: () => invoke<void>("cancel_power"),
  analyze: (url: string, playlist: boolean) => invoke<T.MediaInfo>("media_analyze", { url, playlist }),
  mediaDownload: (req: T.MediaRequest) => invoke<T.Download>("media_download", { req }),
  torrentInfo: (arg: { path?: string; data?: string }) => invoke<{ info: T.TorrentInfo; data: string }>("torrent_info", arg),
  engineInfo: () => invoke<T.EngineInfo>("engine_info"),
  updateYtdlp: () => invoke<string>("update_ytdlp"),
  hostStatus: () => invoke<T.HostStatus>("native_host_status"),
  hostRegister: () => invoke<T.HostStatus>("native_host_register"),
  hostUnregister: () => invoke<T.HostStatus>("native_host_unregister"),
  readClipboard: () => invoke<string | null>("read_clipboard"),
  stats: () => invoke<unknown>("stats"),
  checkUpdate: () => invoke<T.UpdateInfo | null>("check_update"),
  installUpdate: () => invoke<void>("install_update"),
  grabPage: (url: string) => invoke<T.GrabLink[]>("grab_page", { url }),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
