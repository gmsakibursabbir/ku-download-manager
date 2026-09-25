import { t, t as tr, tf, tj } from "../lib/i18n";
import { useEffect, useState } from "react";
import { CircleAlert, CircleCheck, FolderOpen, Globe, RefreshCw, Copy, Package, ChevronRight, ChevronDown } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { api, errorText } from "../lib/api";
import type { HostStatus } from "../lib/types";
import { Button, Icon, IconButton, Notice, PrefGroup, PrefRow } from "../ui/primitives";
import { toast } from "../ui/overlays";
import { BrowserPrefs, EngineStatus, MediaPrefs } from "../app/prefs";
import * as fmt from "../lib/format";

export interface ExtensionDirs {
  chrome?: string | null;
  firefox?: string | null;
  crx?: string | null;
  xpi?: string | null;
  xpiSigned?: boolean;
}

interface Browser {
  id: string;
  name: string;
  family: "chromium" | "firefox";
  path: string;
  extensionsUrl: string;
}

const dirOf = (p: string) => p.replace(/[\\/][^\\/]*$/, "");

/** Next step shown after the browser opened. */
function nextStep(b: Browser, r: { mode: string; copied?: boolean }): string {
  if (r.mode === "store") return tf("{browser} opened the KuDownloader page in its store — click Add. If {browser} later says a new extension was added, choose Enable.", { browser: b.name });
  if (r.mode === "xpi") return tf("{browser} is asking to add KuDownloader — choose Add.", { browser: b.name });
  if (r.mode === "temporary") return tf("In {browser}, choose “Load Temporary Add-on…” and pick manifest.json in the folder that opened. It stays until {browser} restarts; a Mozilla-signed kudmx.xpi installs permanently.", { browser: b.name });
  return r.copied
    ? tf("In {browser}: turn on Developer mode (top right), choose “Load unpacked”, paste the folder path (already copied) and press Enter. Then restart {browser}.", { browser: b.name })
    : tf("In {browser}: turn on Developer mode (top right), choose “Load unpacked” and select the KuDownloader extension folder. Then restart {browser}.", { browser: b.name });
}

export function YourBrowsers({ dirs, status, onChanged, compact }: { dirs: ExtensionDirs; status: HostStatus | null; onChanged: () => void; compact?: boolean }) {
  const [browsers, setBrowsers] = useState<Browser[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [lastSeen, setLastSeen] = useState(0);
  useEffect(() => {
    void invoke<Browser[]>("detect_browsers").then(setBrowsers).catch(() => setBrowsers([]));
    const poll = () => void invoke<number>("extension_last_seen").then(setLastSeen).catch(() => {});
    poll();
    // Only while this page is open; the extension checks in about once a minute.
    const t = setInterval(poll, 5000);
    return () => clearInterval(t);
  }, []);

  const hostFor = (b: Browser) => status?.browsers.find((x) => x.browserId === b.id)?.registered ?? false;

  const install = async (b: Browser, quiet = false) => {
    setBusy(b.id);
    try {
      const r = await invoke<{ mode: string; copied?: boolean }>("install_extension", { browser: b.id });
      if (!quiet) toast({ level: "info", title: tf("Opened {browser}", { browser: b.name }), message: nextStep(b, r), timeout: 20000 });
      onChanged();
      return true;
    } catch (e) {
      toast({ level: "error", title: tf("Could not open {name}", { name: b.name }), message: errorText(e) });
      return false;
    } finally {
      setBusy(null);
    }
  };

  const installAll = async () => {
    if (!browsers?.length) return;
    for (const b of browsers) {
      await install(b, true);
      // Give each browser a moment to come up before the next one.
      await new Promise((r) => setTimeout(r, 1200));
    }
    toast({
      level: "info",
      title: browsers.length === 1 ? t("Opened 1 browser") : tf("Opened {n} browsers", { n: browsers.length }),
      message: t("In each Chromium browser: Developer mode → “Load unpacked” → paste the copied folder path. In Firefox-based browsers: “Load Temporary Add-on…” → manifest.json. Restart each browser afterwards."),
      timeout: 20000,
    });
  };

  const seen = lastSeen > 0 && Date.now() - lastSeen < 5 * 60_000;
  return (
    <>
      <div className="pref-group-title" style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <span style={{ flex: 1 }}>{t("Your browsers")}</span>
        {seen ? (
          <span className="status" data-state="completed">
            {tf("Extension active · {when}", { when: fmt.relativeDate(lastSeen) })}
          </span>
        ) : (
          <span className="status" data-state="paused">
            {t("Extension not detected yet")}
          </span>
        )}
      </div>
      <div className="pref-group">
        {browsers == null ? (
          <PrefRow label={t("Looking for browsers…")} />
        ) : browsers.length === 0 ? (
          <PrefRow label={t("No supported browser found")} desc={t("KuDownloader supports Chrome, Edge, Brave, Vivaldi, Opera, Chromium and Firefox.")} />
        ) : (
          browsers.map((b) => (
            <PrefRow
              key={b.id}
              label={
                <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
                  <Icon icon={Globe} /> {b.name}
                </span>
              }
              desc={
                <span>
                  {hostFor(b) ? t("Connection to KuDownloader ready") : t("Connection not registered — use “Register again” below")} · <span className="mono">{dirOf(b.path)}</span>
                </span>
              }
            >
              <Button size="sm" variant="secondary" busy={busy === b.id} disabled={!!busy} onClick={() => void install(b)}>
                {b.family === "firefox" ? t("Install add-on") : t("Install extension")}
              </Button>
            </PrefRow>
          ))
        )}
        {!!browsers?.length && (
          <PrefRow label={t("All browsers")} desc={t("Opens every browser above at its extensions page, one after another.")}>
            <Button size="sm" variant="primary" disabled={!!busy} onClick={() => void installAll()}>
              {t("Install in all browsers")}
            </Button>
          </PrefRow>
        )}
      </div>
      {!compact && (dirs.crx || dirs.xpi) && (
        <div className="faint" style={{ fontSize: "var(--text-xs)", display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <Icon icon={Package} size={13} /> {t("Packages for store or policy deployment:")}
          {dirs.crx && (
            <Button size="sm" variant="ghost" onClick={() => void invoke("reveal_path", { path: dirOf(dirs.crx!) })}>
              kudmx.crx
            </Button>
          )}
          {dirs.xpi && (
            <Button size="sm" variant="ghost" onClick={() => void invoke("reveal_path", { path: dirOf(dirs.xpi!) })}>
              {dirs.xpiSigned ? "kudmx.signed.xpi" : "kudmx.xpi"}
            </Button>
          )}
          <span>
            {t("Chromium browsers on Windows reject .crx files that don't come from their web store (error CRX_REQUIRED_PROOF_MISSING), so use Install extension above instead.")}
            {!dirs.xpiSigned && ` ${t("Release Firefox installs only Mozilla-signed .xpi files permanently.")}`}
          </span>
        </div>
      )}
    </>
  );
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
  const [manual, setManual] = useState(false);
  useEffect(() => {
    void api.hostStatus().then(setStatus);
    void invoke<ExtensionDirs>("extension_dirs").then(setDirs).catch(() => {});
  }, []);
  const register = async () => {
    setBusy(true);
    try {
      setStatus(await api.hostRegister());
      toast({ level: "success", title: t("Browser connection registered") });
    } catch (e) {
      toast({ level: "error", title: t("Registration failed"), message: errorText(e) });
    } finally {
      setBusy(false);
    }
  };
  const copy = (t: string) => {
    void navigator.clipboard.writeText(t);
    toast({ level: "success", title: tr("Copied") });
  };
  const reveal = (p?: string | null) => p && void invoke("reveal_path", { path: p }).catch((e) => toast({ level: "error", title: t("Could not open the folder"), message: errorText(e) }));

  return (
    <div className="main">
      <div className="toolbar">
        <span className="toolbar-title">{t("Browser Integration")}</span>
      </div>
      <div className="page">
        <div className="page-inner">
          <p className="page-lede">
            {t("The KuDownloader extension sends downloads, videos and links from your browser to this app. It only talks to KuDownloader on this computer.")}
          </p>

          {status && !status.hostExists && (
            <Notice level="error" icon={CircleAlert} title={t("Connector missing")}>
              {t("The browser connector (ku-native-host) was not found next to KuDownloader. Reinstall KuDownloader to restore it.")}
            </Notice>
          )}

          <YourBrowsers dirs={dirs} status={status} onChanged={() => void api.hostStatus().then(setStatus)} />

          <PrefGroup title={t("Connection")}>
            {status?.browsers.filter((b) => b.installed).map((b) => (
              <PrefRow key={b.browser} label={b.browser} desc={b.location}>
                <span className="status" data-state={b.registered ? "completed" : "paused"}>
                  {b.registered ? t("Registered") : t("Not registered")}
                </span>
              </PrefRow>
            ))}
            <PrefRow label={t("Register again")} desc={t("Run this if you moved KuDownloader or the extension says it cannot reach the app.")}>
              <Button size="sm" icon={RefreshCw} busy={busy} onClick={() => void register()}>
                {t("Register again")}
              </Button>
            </PrefRow>
          </PrefGroup>

          <button type="button" className="dl-more" onClick={() => setManual(!manual)} aria-expanded={manual}>
            <Icon icon={manual ? ChevronDown : ChevronRight} size={14} /> {t("Manual setup")}
          </button>
          {manual && (
            <div className="card" style={{ padding: "var(--space-4)", display: "flex", flexDirection: "column", gap: "var(--space-3)" }}>
              <div className="section-title">{t("Chrome, Edge, Brave, Vivaldi, Opera, Chromium")}</div>
              <Step n={1}>
                {tj("Open the browser's extensions page (for example {url}) and turn on {mode}.", { url: <span className="mono selectable">chrome://extensions</span>, mode: <b>{t("Developer mode")}</b> })}
              </Step>
              <Step n={2}>
                {tj("Choose {button} and select:", { button: <b>{t("Load unpacked")}</b> })}
                <div className="fact-link" style={{ marginTop: 6 }}>
                  <span className="mono selectable truncate">{dirs.chrome ?? t("Not bundled with this build")}</span>
                  {dirs.chrome && <IconButton icon={Copy} label={t("Copy path")} size="sm" onClick={() => copy(dirs.chrome!)} />}
                  {dirs.chrome && <IconButton icon={FolderOpen} label={t("Open folder")} size="sm" onClick={() => reveal(dirs.chrome)} />}
                </div>
              </Step>
              <Step n={3}>
                {tj("The extension id is {id}, fixed by the bundled key.", { id: <span className="mono selectable">{status?.chromeExtensionId}</span> })}
              </Step>
              <div className="section-title" style={{ marginTop: "var(--space-2)" }}>
                Firefox
              </div>
              <Step n={1}>
                {tj("Open {page} → {button} and pick {file} in:", { page: <span className="mono selectable">about:debugging#/runtime/this-firefox</span>, button: <b>{t("Load Temporary Add-on…")}</b>, file: <span className="mono">manifest.json</span> })}
                <div className="fact-link" style={{ marginTop: 6 }}>
                  <span className="mono selectable truncate">{dirs.firefox ?? t("Not bundled with this build")}</span>
                  {dirs.firefox && <IconButton icon={FolderOpen} label={t("Open folder")} size="sm" onClick={() => reveal(dirs.firefox)} />}
                </div>
              </Step>
              <div className="faint" style={{ fontSize: "var(--text-xs)", display: "flex", gap: 6, alignItems: "center" }}>
                <Icon icon={CircleCheck} size={13} /> {t("Restart the browser after installing so it picks up the connection.")}
              </div>
            </div>
          )}

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
        <span className="toolbar-title">{t("Media Detection")}</span>
      </div>
      <div className="page">
        <div className="page-inner">
          <p className="page-lede">
            {tj("On supported pages the extension adds a small {button} button to videos. Choosing a quality sends the page to KuDownloader, which downloads it with yt-dlp. Detection runs only in your browser and can be switched off per site.", { button: <b>KuDownload</b> })}
          </p>
          <MediaPrefs />
          <EngineStatus />
        </div>
      </div>
    </div>
  );
}
