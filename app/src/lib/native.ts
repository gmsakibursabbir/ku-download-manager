import { invoke } from "@tauri-apps/api/core";
import { ENGINE_TEMPLATES } from "./engineText";
import { t } from "./i18n";

/** Text the desktop shell shows itself: tray menu and tooltip, notifications, popup titles. */
export const NATIVE_KEYS = [
  "Show KuDownloader",
  "Pause all",
  "Resume all",
  "Quit KuDownloader",
  "{n} active",
  "Download File",
  "Download progress",
  "Download complete",
  "Queue finished",
  "All downloads in “{name}” are done.",
  "Downloads finished",
  "Your computer will sleep in {n} seconds.",
  "Your computer will shut down in {n} seconds.",
  "1 file",
  "{n} files",
  "{peer} wants to send you {files}.",
  "{peer} wants this computer to download {url}",
  "{peer} trusts this computer. Trust it too?",
  "Message from {peer}",
  "Received 1 file from {peer}.",
  "Received {n} files from {peer}.",
  "Link copied",
  "Open KuDownloader to download it.",
] as const;

/** Hand the shell its strings in the current language (tray relabels at once). */
export function syncNativeStrings() {
  const strings: Record<string, string> = {};
  for (const k of [...NATIVE_KEYS, ...ENGINE_TEMPLATES]) strings[k] = t(k);
  void invoke("set_native_strings", { strings }).catch(() => {});
}
