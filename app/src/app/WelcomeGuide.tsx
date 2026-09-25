import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowDownToLine, CircleCheck, Clapperboard, Globe, Download as DownloadIcon } from "lucide-react";
import { api } from "../lib/api";
import { updateSettings } from "../lib/store";
import type { EngineInfo, HostStatus } from "../lib/types";
import { Button, Icon } from "../ui/primitives";
import { Dialog } from "../ui/overlays";
import { YourBrowsers, type ExtensionDirs } from "../views/BrowserView";
import { useToolInstaller, type ToolName } from "./MediaTools";
import { t } from "../lib/i18n";

const isMac = navigator.userAgent.includes("Mac");

/**
 * First-run guide: what the app does, one-click media tools, and the browser
 * extension. Shown once (settings.onboarded); reopen from Help › Getting started.
 */
export function WelcomeGuide({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState(0);
  const [engines, setEngines] = useState<EngineInfo | null>(null);
  const [status, setStatus] = useState<HostStatus | null>(null);
  const [dirs, setDirs] = useState<ExtensionDirs>({});
  const loadEngines = () => void api.engineInfo().then(setEngines).catch(() => {});
  const tools = useToolInstaller(loadEngines);

  useEffect(() => {
    loadEngines();
    void api.hostStatus().then(setStatus).catch(() => {});
    void invoke<ExtensionDirs>("extension_dirs").then(setDirs).catch(() => {});
  }, []);

  const finish = () => {
    void updateSettings({ onboarded: true }).catch(() => {});
    onClose();
  };

  const missing: ToolName[] = [];
  if (engines && !engines.ytdlp.path) missing.push("yt-dlp");
  // macOS: no official FFmpeg build to fetch — Homebrew instead.
  if (engines && !engines.ffmpeg.path && !isMac) missing.push("ffmpeg");
  const toolsReady = !!engines?.ytdlp.path && (!!engines?.ffmpeg.path || isMac);

  const steps = [
    {
      title: t("Welcome to KuDownloader"),
      body: (
        <div className="welcome-body">
          <div className="welcome-hero">
            <Icon icon={ArrowDownToLine} size={28} />
          </div>
          <p className="welcome-lede">{t("Fast downloads that resume after anything — from your browser, video sites and links.")}</p>
          <ul className="welcome-list">
            <li>{t("Paste a link or press Add URL to download with several connections at once.")}</li>
            <li>{t("Downloads you start in the browser open a small Download File window.")}</li>
            <li>{t("A KuDownload button appears on videos on web pages.")}</li>
          </ul>
          <p className="faint">{t("Two quick steps get everything ready.")}</p>
        </div>
      ),
    },
    {
      title: t("Video and audio tools"),
      body: (
        <div className="welcome-body">
          <p className="welcome-lede">{t("Video downloads use yt-dlp, and FFmpeg merges the best video with its audio. Both come from their official releases and are checked before use.")}</p>
          <div className="welcome-tools">
            <ToolRow name="yt-dlp" size="~17 MB" ok={!!engines?.ytdlp.path} progress={tools.label("yt-dlp")} />
            <ToolRow name="FFmpeg" size="~80 MB" ok={!!engines?.ffmpeg.path} progress={tools.label("ffmpeg")} note={isMac && !engines?.ffmpeg.path ? "brew install ffmpeg" : undefined} />
          </div>
          {toolsReady ? (
            <div className="welcome-done">
              <Icon icon={CircleCheck} size={16} /> {t("Everything is installed.")}
            </div>
          ) : (
            <Button variant="primary" icon={DownloadIcon} busy={!!tools.busy} disabled={!missing.length} onClick={() => void tools.install(missing)}>
              {tools.busy ? t("Installing…") : missing.length > 1 ? t("Install yt-dlp and FFmpeg") : `${t("Install")} ${missing[0] === "ffmpeg" ? "FFmpeg" : "yt-dlp"}`}
            </Button>
          )}
          <p className="faint">{t("You can skip this and install them later from the Video Downloader.")}</p>
        </div>
      ),
    },
    {
      title: t("Add the browser extension"),
      body: (
        <div className="welcome-body">
          <p className="welcome-lede">{t("The extension sends downloads and videos from your browser to KuDownloader. Choose Install next to each browser you use:")}</p>
          <ol className="welcome-steps">
            <li>
              <b>{t("Chrome, Edge, Brave, Opera, Helium…")}</b> — {t("the extensions page opens: turn on Developer mode, choose Load unpacked, paste the folder path (already copied) and press Enter.")}
            </li>
            <li>
              <b>{t("Firefox, Zen, Floorp…")}</b> — {t("about:debugging opens: choose Load Temporary Add-on and pick manifest.json in the folder that opened.")}
            </li>
          </ol>
          <div className="welcome-browsers">
            <YourBrowsers dirs={dirs} status={status} onChanged={() => void api.hostStatus().then(setStatus)} compact />
          </div>
        </div>
      ),
    },
  ];
  const last = step === steps.length - 1;

  return (
    <Dialog
      title={
        <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
          <Icon icon={step === 1 ? Clapperboard : step === 2 ? Globe : ArrowDownToLine} />
          {steps[step].title}
        </span>
      }
      onClose={finish}
      width={640}
      onSubmit={() => (last ? finish() : setStep(step + 1))}
    >
      {steps[step].body}
      <div className="welcome-footer">
        <div className="welcome-dots" aria-label={`Step ${step + 1} of ${steps.length}`}>
          {steps.map((_, i) => (
            <span key={i} data-active={i === step} />
          ))}
        </div>
        <span style={{ flex: 1 }} />
        {step > 0 ? <Button onClick={() => setStep(step - 1)}>{t("Back")}</Button> : <Button onClick={finish}>{t("Skip")}</Button>}
        <Button type="submit" variant="primary">
          {last ? t("Finish") : t("Next")}
        </Button>
      </div>
    </Dialog>
  );
}

function ToolRow({ name, size, ok, progress, note }: { name: string; size: string; ok: boolean; progress: string | null; note?: string }) {
  return (
    <div className="welcome-tool">
      <span style={{ fontWeight: 600 }}>{name}</span>
      <span className="faint num">{size}</span>
      <span style={{ flex: 1 }} />
      {ok ? (
        <span className="status" data-state="completed">
          {t("Installed")}
        </span>
      ) : progress ? (
        <span className="num faint">{progress}</span>
      ) : note ? (
        <code className="mono faint">{note}</code>
      ) : (
        <span className="status" data-state="paused">
          {t("Not installed")}
        </span>
      )}
    </div>
  );
}
