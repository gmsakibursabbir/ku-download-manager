import { t, tf } from "../lib/i18n";
import { useEffect, useRef } from "react";
import { Download as DownloadIcon } from "lucide-react";
import { installTools, jobLabel, useToolJobs, type ToolName } from "../lib/tools";
import type { EngineInfo } from "../lib/types";
import * as fmt from "../lib/format";
import { Button, Icon, Notice } from "../ui/primitives";

export type { ToolName };

/** Tool installs, backed by the app-wide store (they survive switching screens). */
export function useToolInstaller(onDone?: () => void) {
  const jobs = useToolJobs();
  // Refresh when any tool download ends, including ones started on another screen.
  const count = useRef(jobs.length);
  useEffect(() => {
    if (jobs.length < count.current) onDone?.();
    count.current = jobs.length;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jobs.length]);
  const busy = (jobs.find((j) => j.state === "downloading")?.tool ?? jobs[0]?.tool ?? null) as ToolName | null;
  const install = async (tools: ToolName[]) => {
    return installTools(tools);
  };
  const label = (t: ToolName) => {
    const j = jobs.find((x) => x.tool === t);
    return j ? jobLabel(j, fmt) : null;
  };
  return { busy, install, label };
}

/** Floating panel on every screen while yt-dlp / FFmpeg download. */
export function ToolDownloadsPanel() {
  const jobs = useToolJobs();
  if (!jobs.length) return null;
  return (
    <div className="tool-panel" role="status" aria-live="polite">
      {jobs.map((j) => (
        <div key={j.tool} className="tool-panel-row">
          <Icon icon={DownloadIcon} size={14} />
          <span className="tool-panel-name">{j.tool === "ffmpeg" ? "FFmpeg" : "yt-dlp"}</span>
          <span className="tool-panel-bar">
            <span style={{ width: `${j.total ? Math.min(100, (j.done / j.total) * 100) : 0}%` }} data-indeterminate={!j.total || undefined} />
          </span>
          <span className="tool-panel-text num">{jobLabel(j, fmt)}</span>
        </div>
      ))}
    </div>
  );
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
  // macOS has no official FFmpeg build to fetch (Homebrew instead).
  if (!info.ffmpeg.path && !navigator.userAgent.includes("Mac")) missing.push("ffmpeg");
  if (!missing.length) return null;
  const needsYt = missing.includes("yt-dlp");
  return (
    <Notice
      level={needsYt ? "warning" : "info"}
      icon={DownloadIcon}
      title={needsYt ? t("Media tools needed") : t("FFmpeg recommended")}
      action={
        <Button size="sm" variant="primary" busy={!!busy} onClick={() => void install(missing)}>
          {busy ? label(busy) : tf("Download {tools}", { tools: missing.map((m) => (m === "ffmpeg" ? "FFmpeg" : m)).join(" + ") })}
        </Button>
      }
    >
      {needsYt && missing.includes("ffmpeg")
        ? t("Video and audio downloads use yt-dlp (about 17 MB) and FFmpeg (about 80 MB).")
        : needsYt
          ? t("Video and audio downloads use yt-dlp (about 17 MB).")
          : t("Merging high-quality video with audio uses FFmpeg (about 80 MB).")}{" "}
      {t("They are downloaded once from their official releases and checked against the published checksums.")}
    </Notice>
  );
}
