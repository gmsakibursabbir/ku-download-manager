import { useEffect, useState } from "react";
import { Download as DownloadIcon } from "lucide-react";
import { api, errorText } from "../lib/api";
import { onCoreEvent } from "../lib/store";
import type { EngineInfo } from "../lib/types";
import * as fmt from "../lib/format";
import { Button, Notice } from "../ui/primitives";
import { toast } from "../ui/overlays";

export type ToolName = "yt-dlp" | "ffmpeg";
type Progress = Partial<Record<ToolName, { done: number; total: number }>>;

/** Shared state for on-demand tool installs (yt-dlp, ffmpeg) with live progress. */
export function useToolInstaller(onDone?: () => void) {
  const [busy, setBusy] = useState<ToolName | null>(null);
  const [progress, setProgress] = useState<Progress>({});
  useEffect(
    () =>
      onCoreEvent((e) => {
        if (e.type === "toolProgress") setProgress((p) => ({ ...p, [e.tool]: { done: e.done, total: e.total } }));
      }),
    [],
  );
  const install = async (tools: ToolName[]) => {
    for (const t of tools) {
      setBusy(t);
      try {
        await api.installTool(t);
      } catch (e) {
        toast({ level: "error", title: `Could not download ${t === "ffmpeg" ? "FFmpeg" : "yt-dlp"}`, message: errorText(e) });
        setBusy(null);
        onDone?.();
        return false;
      }
    }
    setBusy(null);
    setProgress({});
    toast({ level: "success", title: tools.length > 1 ? "Media tools installed" : `${tools[0] === "ffmpeg" ? "FFmpeg" : "yt-dlp"} installed` });
    onDone?.();
    return true;
  };
  const label = (t: ToolName) => {
    const p = progress[t];
    if (busy !== t) return null;
    if (!p) return "Starting…";
    return p.total ? `${fmt.bytes(p.done)} of ${fmt.bytes(p.total)}` : fmt.bytes(p.done);
  };
  return { busy, install, label };
}

/**
 * Shown where media downloads need yt-dlp / FFmpeg and one is missing: one
 * click downloads the official builds (checksum-verified) into KuDownloader.
 */
export function MediaToolsNotice({ info, onInstalled }: { info: EngineInfo | null; onInstalled: () => void }) {
  const { busy, install, label } = useToolInstaller(onInstalled);
  if (!info) return null;
  const missing: ToolName[] = [];
  if (!info.ytdlp.path) missing.push("yt-dlp");
  // Linux packages depend on the distribution's ffmpeg instead.
  if (!info.ffmpeg.path && navigator.userAgent.includes("Windows")) missing.push("ffmpeg");
  if (!missing.length) return null;
  const needsYt = missing.includes("yt-dlp");
  return (
    <Notice
      level={needsYt ? "warning" : "info"}
      icon={DownloadIcon}
      title={needsYt ? "Media tools needed" : "FFmpeg recommended"}
      action={
        <Button size="sm" variant="primary" busy={!!busy} onClick={() => void install(missing)}>
          {busy ? label(busy) : `Download ${missing.map((m) => (m === "ffmpeg" ? "FFmpeg" : m)).join(" + ")}`}
        </Button>
      }
    >
      {needsYt
        ? "Video and audio downloads use yt-dlp (about 17 MB)"
        : "Merging high-quality video with audio uses FFmpeg"}
      {missing.includes("ffmpeg") && needsYt ? " and FFmpeg (about 80 MB)" : missing.includes("ffmpeg") ? " (about 80 MB)" : ""}. They are downloaded once from their official releases and checked against the published checksums.
    </Notice>
  );
}
