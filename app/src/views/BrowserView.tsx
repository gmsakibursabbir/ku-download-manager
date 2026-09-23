import { useEffect, useState } from "react";
import { CircleAlert, FolderOpen, RefreshCw, Copy } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { api, errorText } from "../lib/api";
import type { HostStatus } from "../lib/types";
import { Button, IconButton, Notice, PrefGroup, PrefRow } from "../ui/primitives";
import { toast } from "../ui/overlays";
import { BrowserPrefs, EngineStatus, MediaPrefs } from "../app/prefs";

interface ExtensionDirs {
  chrome?: string | null;
  firefox?: string | null;
}

function Step({ n, children }: { n: number; children: React.ReactNode }) {
  return (
    <div style={{ display: "flex", gap: 12, fontSize: "var(--text-sm)" }}>
      <span className="chip" style={{ flex: "none" }}>
        {n}
      </span>
      <div style={{ flex: 1, minWidth: 0 }}>{children}</div>
    </div>
  );
}

export function BrowserView() {
  const [status, setStatus] = useState<HostStatus | null>(null);
  const [dirs, setDirs] = useState<ExtensionDirs>({});
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    void api.hostStatus().then(setStatus);
    void invoke<ExtensionDirs>("extension_dirs").then(setDirs).catch(() => {});
  }, []);
  const register = async () => {
    setBusy(true);
    try {
      setStatus(await api.hostRegister());
      toast({ level: "success", title: "Browser connection registered" });
    } catch (e) {
      toast({ level: "error", title: "Registration failed", message: errorText(e) });
    } finally {
      setBusy(false);
    }
  };
  const copy = (t: string) => {
    void navigator.clipboard.writeText(t);
    toast({ level: "success", title: "Copied" });
  };
  const registered = status?.browsers.filter((b) => b.registered) ?? [];
  const reveal = (p?: string | null) => p && void invoke("reveal_path", { path: p }).catch((e) => toast({ level: "error", title: "Could not open the folder", message: errorText(e) }));

  return (
    <div className="main">
      <div className="toolbar">
        <span className="toolbar-title">Browser Integration</span>
      </div>
      <div className="page">
        <div className="page-inner">
          <p className="page-lede">
            The KuDownloader extension sends downloads from Chrome, Edge, Brave, Chromium and Firefox to this app. It talks only to KuDownloader on this computer — nothing is sent anywhere else.
          </p>

          {status && !status.hostExists && (
            <Notice level="error" icon={CircleAlert} title="Connector missing">
              The browser connector (ku-native-host) was not found next to KuDownloader. Reinstall KuDownloader to restore it.
            </Notice>
          )}

          <PrefGroup title="Connection">
            {status?.browsers.map((b) => (
              <PrefRow key={b.browser} label={b.browser} desc={b.location}>
                <span className="status" data-state={b.registered ? "completed" : "paused"}>
                  {b.registered ? "Connected" : "Not registered"}
                </span>
              </PrefRow>
            ))}
            <PrefRow label="Re-register" desc="Run this if you moved KuDownloader or the extension says it cannot reach the app.">
              <Button size="sm" icon={RefreshCw} busy={busy} onClick={() => void register()}>
                Register again
              </Button>
            </PrefRow>
          </PrefGroup>

          <div className="pref-group-title">Install the extension</div>
          <div className="card" style={{ padding: "var(--space-4)", display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
            <div className="section-title">Chrome, Edge, Brave, Chromium</div>
            <Step n={1}>
              Open <span className="mono selectable">chrome://extensions</span> (Edge: <span className="mono selectable">edge://extensions</span>) and turn on <b>Developer mode</b>.
            </Step>
            <Step n={2}>
              Choose <b>Load unpacked</b> and select this folder:
              <div className="fact-link" style={{ marginTop: 6 }}>
                <span className="mono selectable truncate">{dirs.chrome ?? "Not bundled with this build"}</span>
                {dirs.chrome && <IconButton icon={Copy} label="Copy path" size="sm" onClick={() => copy(dirs.chrome!)} />}
                {dirs.chrome && <IconButton icon={FolderOpen} label="Open folder" size="sm" onClick={() => reveal(dirs.chrome)} />}
              </div>
            </Step>
            <Step n={3}>
              The extension id must be <span className="mono selectable">{status?.chromeExtensionId}</span> — it is fixed by the bundled key, so no further setup is needed.
            </Step>
            <div className="section-title" style={{ marginTop: "var(--space-2)" }}>
              Firefox
            </div>
            <Step n={1}>
              Open <span className="mono selectable">about:debugging#/runtime/this-firefox</span> and choose <b>Load Temporary Add-on…</b>
            </Step>
            <Step n={2}>
              Select <span className="mono">manifest.json</span> in:
              <div className="fact-link" style={{ marginTop: 6 }}>
                <span className="mono selectable truncate">{dirs.firefox ?? "Not bundled with this build"}</span>
                {dirs.firefox && <IconButton icon={Copy} label="Copy path" size="sm" onClick={() => copy(dirs.firefox!)} />}
                {dirs.firefox && <IconButton icon={FolderOpen} label="Open folder" size="sm" onClick={() => reveal(dirs.firefox)} />}
              </div>
            </Step>
            <div className="faint" style={{ fontSize: "var(--text-xs)" }}>
              {registered.length ? `Registered for ${registered.map((b) => b.browser).join(", ")}.` : "Not registered with any browser yet."} Restart the browser after installing.
            </div>
          </div>

          <BrowserPrefs />
        </div>
      </div>
    </div>
  );
}

export function MediaView() {
  return (
    <div className="main">
      <div className="toolbar">
        <span className="toolbar-title">Media Detection</span>
      </div>
      <div className="page">
        <div className="page-inner">
          <p className="page-lede">
            On supported pages the extension adds a small <b>KuDownload</b> button to videos. Choosing a quality sends the page to KuDownloader, which downloads it with yt-dlp. Detection runs only in your browser; site support is modular and can be switched off per site.
          </p>
          <MediaPrefs />
          <EngineStatus />
        </div>
      </div>
    </div>
  );
}
