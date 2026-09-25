import { useEffect, useState } from "react";
import { LANGUAGES, t, tf } from "../lib/i18n";
import { Plus, Trash2, RefreshCw, FolderOpen, Download as DownloadIcon } from "lucide-react";
import { api, errorText } from "../lib/api";
import { settingsStore } from "../lib/store";
import * as fmt from "../lib/format";
import type { AppInfo, BandwidthProfile, Category, UpdateInfo } from "../lib/types";
import { Button, IconButton, Input, Kbd, PrefGroup, PrefRow, Segmented, Select } from "../ui/primitives";
import { toast } from "../ui/overlays";
import { useApp } from "../app/context";
import { BrowserPrefs, EngineStatus, FolderPref, MediaPrefs, NumberPref, SelectPref, SwitchPref, TextPref, save } from "../app/prefs";

const SECTIONS = [
  ["general", "General"],
  ["downloads", "Downloads"],
  ["connection", "Connection"],
  ["speed", "Speed"],
  ["scheduler", "Scheduler"],
  ["browser", "Browser"],
  ["media", "Media"],
  ["torrent", "Torrent"],
  ["notifications", "Notifications"],
  ["appearance", "Appearance"],
  ["advanced", "Advanced"],
] as const;

type Section = (typeof SECTIONS)[number][0];

const SHORTCUTS: [string, string][] = [
  ["Add download", "Ctrl N"],
  ["Paste link from clipboard", "Ctrl V"],
  ["Search", "Ctrl F"],
  ["Pause / resume selection", "Space"],
  ["Open finished file", "Enter"],
  ["Show in folder", "Ctrl Shift O"],
  ["Remove selection", "Del"],
  ["Select all", "Ctrl A"],
  ["Move up / down in queue", "Alt ↑ / Alt ↓"],
  ["Toggle details", "Ctrl I"],
  ["Go to Downloads … Torrents", "Ctrl 1 … 5"],
  ["Settings", "Ctrl ,"],
];

function Updates() {
  const [state, setState] = useState<{ busy: boolean; info?: UpdateInfo | null; error?: string }>({ busy: false });
  const check = async () => {
    setState({ busy: true });
    try {
      setState({ busy: false, info: await api.checkUpdate() });
    } catch (e) {
      setState({ busy: false, error: errorText(e) });
    }
  };
  const install = async () => {
    setState((s) => ({ ...s, busy: true }));
    try {
      await api.installUpdate();
      setState((s) => ({ ...s, busy: false }));
    } catch (e) {
      setState({ busy: false, error: errorText(e) });
    }
  };
  const desc = state.error ?? (state.info === null ? t("KuDownloader is up to date.") : state.info ? tf("Version {version} is available.", { version: state.info.version }) : t("Checks the release feed for a newer version."));
  return (
    <PrefRow label={t("Updates")} desc={desc}>
      {state.info ? (
        <Button size="sm" variant="primary" icon={DownloadIcon} busy={state.busy} onClick={() => void install()}>
          {state.info.signed ? t("Install and restart") : t("Download update")}
        </Button>
      ) : (
        <Button size="sm" icon={RefreshCw} busy={state.busy} onClick={() => void check()}>
          {t("Check now")}
        </Button>
      )}
    </PrefRow>
  );
}

function Categories() {
  const s = settingsStore.use();
  const [draft, setDraft] = useState<Category[]>(s?.categories ?? []);
  useEffect(() => setDraft(s?.categories ?? []), [s?.categories]);
  if (!s) return null;
  const commit = (next: Category[]) => void save({ categories: next });
  const update = (i: number, p: Partial<Category>) => setDraft(draft.map((c, j) => (j === i ? { ...c, ...p } : c)));
  return (
    <PrefGroup title={t("Categories")}>
      <div style={{ padding: "var(--space-2) var(--space-4)" }}>
        <table className="table">
          <thead>
            <tr>
              <th style={{ width: 150 }}>{t("Name")}</th>
              <th style={{ width: 170 }}>{t("Folder")}</th>
              <th>{t("File types")}</th>
              <th style={{ width: 36 }} />
            </tr>
          </thead>
          <tbody>
            {draft.map((c, i) => (
              <tr key={c.id}>
                <td>
                  <Input value={c.name} style={{ height: 26 }} onChange={(e) => update(i, { name: e.target.value })} onBlur={() => commit(draft)} />
                </td>
                <td>
                  <Input value={c.folder} style={{ height: 26 }} onChange={(e) => update(i, { folder: e.target.value })} onBlur={() => commit(draft)} />
                </td>
                <td>
                  <Input
                    value={c.extensions.join(", ")}
                    style={{ height: 26 }}
                    onChange={(e) => update(i, { extensions: e.target.value.split(/[,\s]+/).map((x) => x.replace(/^\./, "").toLowerCase()).filter(Boolean) })}
                    onBlur={() => commit(draft)}
                  />
                </td>
                <td>
                  <IconButton icon={Trash2} className="is-danger" label={`Delete ${c.name}`} size="sm" disabled={c.id === "torrents" || c.id === "video" || c.id === "music"} onClick={() => commit(draft.filter((_, j) => j !== i))} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <Button size="sm" variant="ghost" icon={Plus} style={{ marginTop: 8 }} onClick={() => commit([...draft, { id: `c${Date.now().toString(36)}`, name: "New category", folder: "New category", extensions: [] }])}>
          {t("Add category")}
        </Button>
      </div>
    </PrefGroup>
  );
}

function Profiles() {
  const s = settingsStore.use();
  const [draft, setDraft] = useState<BandwidthProfile[]>(s?.profiles ?? []);
  useEffect(() => setDraft(s?.profiles ?? []), [s?.profiles]);
  if (!s) return null;
  const commit = () => void save({ profiles: draft });
  const setRate = (i: number, key: "download" | "upload", v: string) => {
    const rate = v.trim() === "" || v.trim() === "0" ? 0 : fmt.parseRate(v);
    if (rate == null) {
      toast({ level: "warning", title: t("Enter a limit like 500 KB or 2 MB") });
      return;
    }
    const next = draft.map((p, j) => (j === i ? { ...p, [key]: rate } : p));
    setDraft(next);
    void save({ profiles: next });
  };
  return (
    <PrefGroup title={t("Bandwidth profiles")}>
      <div style={{ padding: "var(--space-2) var(--space-4)" }}>
        <table className="table">
          <thead>
            <tr>
              <th>{t("Profile")}</th>
              <th style={{ width: 150 }}>{t("Download limit")}</th>
              <th style={{ width: 150 }}>{t("Upload limit")}</th>
              <th style={{ width: 80 }} />
              <th style={{ width: 36 }} />
            </tr>
          </thead>
          <tbody>
            {draft.map((p, i) => (
              <tr key={p.id}>
                <td>
                  <Input value={p.name} style={{ height: 26 }} onChange={(e) => setDraft(draft.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)))} onBlur={commit} />
                </td>
                <td>
                  <Input key={`${p.id}-d-${p.download}`} defaultValue={p.download ? fmt.bytes(p.download) : ""} placeholder={t("Unlimited")} style={{ height: 26 }} onBlur={(e) => { setRate(i, "download", e.target.value); }} onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()} />
                </td>
                <td>
                  <Input key={`${p.id}-u-${p.upload}`} defaultValue={p.upload ? fmt.bytes(p.upload) : ""} placeholder={t("Unlimited")} style={{ height: 26 }} onBlur={(e) => { setRate(i, "upload", e.target.value); }} onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()} />
                </td>
                <td>{s.activeProfile === p.id ? <span className="chip chip-accent">{t("Active")}</span> : <Button size="sm" variant="ghost" onClick={() => void api.setProfile(p.id).then(() => settingsStore.refresh())}>{t("Use")}</Button>}</td>
                <td>
                  <IconButton icon={Trash2} className="is-danger" label={`Delete ${p.name}`} size="sm" disabled={draft.length <= 1 || s.activeProfile === p.id} onClick={() => void save({ profiles: draft.filter((_, j) => j !== i) })} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
          <Button size="sm" variant="ghost" icon={Plus} onClick={() => void save({ profiles: [...draft, { id: `p${Date.now().toString(36)}`, name: "New profile", download: 5 * 1024 * 1024, upload: 1024 * 1024 }] })}>
            {t("Add profile")}
          </Button>
        </div>
        <div className="faint" style={{ fontSize: "var(--text-xs)", marginTop: 8 }}>
          {t("Enter limits like 500 KB or 2.5 MB. Leave empty for unlimited.")}
        </div>
      </div>
    </PrefGroup>
  );
}

function AfterAll() {
  const [value, setValue] = useState("none");
  useEffect(() => void api.getAfterAll().then(setValue), []);
  return (
    <PrefRow label={t("When all downloads finish")} desc={t("Applies to this session only. You get a 60-second warning before sleep or shutdown.")}>
      <Select
        value={value}
        style={{ width: 180 }}
        onChange={(e) => {
          setValue(e.target.value);
          void api.setAfterAll(e.target.value);
        }}
        options={[
          { value: "none", label: t("Do nothing") },
          { value: "sleep", label: t("Sleep") },
          { value: "shutdown", label: t("Shut down") },
          { value: "quit", label: t("Quit KuDownloader") },
        ]}
      />
    </PrefRow>
  );
}

function Advanced() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  useEffect(() => void api.appInfo().then(setInfo), []);
  return (
    <>
      <EngineStatus />
      <PrefGroup title={t("HTTP engine")}>
        <SelectPref
          k="httpEngine"
          label={t("Engine for HTTP and HTTPS downloads")}
          desc={t("KuHTTP is KuDownloader’s own adaptive engine (faster in our benchmarks, resumes after crashes). aria2 is the long-proven alternative. FTP, SFTP, BitTorrent, magnet and Metalink always use aria2.")}
          options={[
            { value: "kuhttp", label: t("KuHTTP (recommended)") },
            { value: "aria2", label: "aria2" },
          ]}
          width={200}
        />
      </PrefGroup>
      <PrefGroup title={t("Engine locations")}>
        <TextPref k="aria2Path" label="aria2c" desc={t("Leave empty to use the bundled engine.")} placeholder={t("Automatic")} width={320} mono />
        <TextPref k="ytdlpPath" label="yt-dlp" placeholder={t("Automatic")} width={320} mono />
        <TextPref k="ffmpegPath" label="FFmpeg" placeholder={t("Automatic")} width={320} mono />
      </PrefGroup>
      <PrefGroup title={t("Integration")}>
        <PrefRow label={t("Local API")} desc={info?.apiPort ? `Listening on 127.0.0.1:${info.apiPort} for the ku command-line tool and the browser connector. Access requires a per-session token.` : t("Not running.")} />
        <TextPref k="updateEndpoint" label={t("Update feed")} desc={t("Leave empty to use the official release feed.")} placeholder={t("Default")} width={320} mono />
      </PrefGroup>
      <PrefGroup title={t("Data")}>
        <PrefRow label={t("Data folder")} desc={info?.dataDir}>
          <Button size="sm" icon={FolderOpen} onClick={() => void api.openDataDir()}>
            {t("Open")}
          </Button>
        </PrefRow>
        <PrefRow label={t("Version")} desc={`KuDownloader ${info?.version ?? ""} · ${info?.platform ?? ""}`} />
      </PrefGroup>
    </>
  );
}

const ACCENTS: [string, string, string][] = [
  ["blue", "#2563EB", "Blue"],
  ["violet", "#7C3AED", "Violet"],
  ["teal", "#0D9488", "Teal"],
  ["green", "#16A34A", "Green"],
  ["orange", "#EA580C", "Orange"],
  ["pink", "#DB2777", "Pink"],
  ["red", "#DC2626", "Red"],
  ["graphite", "#52525B", "Graphite"],
];

export function SettingsView() {
  const { settingsSection, navigate } = useApp();
  const [section, setSection] = useState<Section>((SECTIONS.find(([k]) => k === settingsSection)?.[0] ?? "general") as Section);
  useEffect(() => {
    const s = SECTIONS.find(([k]) => k === settingsSection);
    if (s) setSection(s[0]);
  }, [settingsSection]);
  const settings = settingsStore.use();
  if (!settings) return null;

  return (
    <div className="main">
      <div className="toolbar">
        <span className="toolbar-title">{t("Settings")}</span>
      </div>
      <div className="settings-layout" style={{ minHeight: 0 }}>
        <nav className="settings-nav" aria-label={t("Settings sections")}>
          {SECTIONS.map(([k, label]) => (
            <button key={k} type="button" className="nav-item" aria-current={section === k ? "page" : undefined} onClick={() => setSection(k)}>
              {t(label)}
            </button>
          ))}
        </nav>
        <div className="page">
          <div className="page-inner" style={{ maxWidth: 760 }}>
            {section === "general" && (
              <>
                <PrefGroup>
                  <SwitchPref k="startWithOs" label={t("Start KuDownloader when you sign in")} desc={t("Starts minimized to the tray so scheduled downloads and the browser connection are ready.")} />
                  <SwitchPref k="minimizeToTray" label={t("Keep running in the tray when the window is closed")} desc={t("Downloads continue in the background. Quit from the tray icon.")} />
                  <SwitchPref k="clipboardMonitor" label={t("Offer to download copied links")} desc={t("When you copy a link to a file or video, KuDownloader offers to download it.")} />
                  <SwitchPref k="checkUpdates" label={t("Check for updates automatically")} />
                  <Updates />
                </PrefGroup>
                <PrefGroup title={t("Keyboard shortcuts")}>
                  <table className="table" style={{ margin: "var(--space-1) 0" }}>
                    <tbody>
                      {SHORTCUTS.map(([label, keys]) => (
                        <tr key={label}>
                          <td style={{ paddingLeft: "var(--space-4)", borderBottom: "none" }}>{t(label)}</td>
                          <td style={{ textAlign: "right", paddingRight: "var(--space-4)", borderBottom: "none" }}>
                            <span style={{ display: "inline-flex", gap: 4 }}>
                              {keys.split(" / ").map((k) => (
                                <Kbd key={k}>{k}</Kbd>
                              ))}
                            </span>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </PrefGroup>
              </>
            )}
            {section === "downloads" && (
              <>
                <PrefGroup>
                  <FolderPref k="downloadDir" label={t("Download folder")} />
                  <SwitchPref k="useCategories" label={t("Sort into category folders")} desc={t("Archives, programs, video and so on go into their own sub-folders.")} />
                  <NumberPref k="maxConcurrent" label={t("Simultaneous downloads")} desc={t("Downloads outside queues. Queues have their own limit.")} min={1} max={32} />
                  <SelectPref
                    k="defaultConnections"
                    label={t("Connections per download")}
                    desc={t("Smart adapts to what the server allows and backs off on errors.")}
                    options={[{ value: 0, label: t("Smart") }, ...[1, 2, 4, 8, 16, 32].map((n) => ({ value: n, label: String(n) }))]}
                    width={140}
                  />
                  <SelectPref
                    k="fileExists"
                    label={t("If the file already exists")}
                    options={[
                      { value: "rename", label: t("Keep both (rename)") },
                      { value: "overwrite", label: t("Overwrite") },
                      { value: "skip", label: t("Don't download") },
                    ]}
                    width={180}
                  />
                </PrefGroup>
                <PrefGroup title={t("Virus scan")}>
                  <SelectPref
                    k="virusScan"
                    label={t("Scan finished downloads")}
                    desc={t("Only problems are reported. Downloads are never opened automatically.")}
                    options={[
                      { value: "off", label: t("Off") },
                      ...(navigator.userAgent.includes("Windows") ? [{ value: "defender", label: t("Microsoft Defender") }] : []),
                      { value: "custom", label: t("Another scanner…") },
                    ]}
                    width={200}
                  />
                  {settings.virusScan === "custom" && (
                    <>
                      <TextPref k="virusScanner" label={t("Scanner program")} desc={t("Full path to the scanner's command-line program (for example clamscan).")} placeholder={t("C:\\Program Files\\…\\scanner.exe")} mono />
                      <TextPref k="virusScannerArgs" label={t("Arguments")} desc="{file} is replaced by the downloaded file." mono />
                    </>
                  )}
                </PrefGroup>
                <Categories />
              </>
            )}
            {section === "connection" && (
              <>
                <PrefGroup title={t("Proxy")}>
                  <TextPref k="proxy" label={t("Proxy server")} desc="http://host:port, https:// or socks5://. Leave empty for a direct connection." placeholder={t("None")} mono />
                  <TextPref k="proxyUser" label={t("User name")} width={200} />
                  <TextPref k="proxyPass" label={t("Password")} type="password" width={200} />
                  <TextPref k="noProxy" label={t("Bypass for")} desc={t("Comma separated hosts or domains.")} placeholder="localhost, .intranet" />
                </PrefGroup>
                <PrefGroup title={t("Requests")}>
                  <TextPref k="userAgent" label={t("User agent")} desc={t("Leave empty for a current browser user agent.")} placeholder={t("Default")} width={320} />
                  <SwitchPref k="checkCertificate" label={t("Verify TLS certificates")} desc={t("Turn off only for servers you trust with self-signed certificates.")} />
                </PrefGroup>
                <PrefGroup title={t("Retries and timeouts")}>
                  <NumberPref k="maxTries" label={t("Attempts per connection")} desc={t("0 = unlimited.")} min={0} max={100} />
                  <NumberPref k="retryWait" label={t("Wait between attempts")} min={0} max={600} unit="seconds" />
                  <NumberPref k="autoRetry" label={t("Restart failed downloads")} desc={t("For network errors and overloaded servers.")} min={0} max={20} unit="times" />
                  <NumberPref k="timeout" label={t("Read timeout")} min={5} max={600} unit="seconds" />
                  <NumberPref k="connectTimeout" label={t("Connect timeout")} min={5} max={300} unit="seconds" />
                </PrefGroup>
              </>
            )}
            {section === "speed" && <Profiles />}
            {section === "scheduler" && (
              <PrefGroup>
                <AfterAll />
                <PrefRow label={t("Schedules")} desc={t("Start queues at set times and apply speed profiles.")}>
                  <Button size="sm" onClick={() => navigate("scheduled")}>
                    {t("Open Scheduled")}
                  </Button>
                </PrefRow>
              </PrefGroup>
            )}
            {section === "browser" && (
              <>
                <BrowserPrefs />
                <PrefGroup>
                  <PrefRow label={t("Extension and connection status")}>
                    <Button size="sm" onClick={() => navigate("browser")}>
                      {t("Open Browser Integration")}
                    </Button>
                  </PrefRow>
                </PrefGroup>
              </>
            )}
            {section === "media" && <MediaPrefs />}
            {section === "torrent" && (
              <PrefGroup>
                <NumberPref k="seedRatio" label={t("Seed until ratio")} desc={t("0 = stop when the download completes.")} min={0} max={100} step={0.1} />
                <NumberPref k="seedTime" label={t("Seed for at most")} desc={t("0 = no time limit (when a ratio is set).")} min={0} max={100000} unit="minutes" />
                <TextPref k="btListenPort" label={t("Listening ports")} desc={t("Port or range, e.g. 6881-6999.")} width={140} mono />
                <SwitchPref k="enableDht" label={t("Use DHT")} desc={t("Find peers without trackers, needed for most magnet links.")} />
                <NumberPref k="btMaxPeers" label={t("Peers per torrent")} min={1} max={1000} />
              </PrefGroup>
            )}
            {section === "notifications" && (
              <PrefGroup>
                <SwitchPref k="notifyComplete" label={t("When a download completes")} />
                <SwitchPref k="notifyError" label={t("When a download fails")} desc={t("Only while the window is hidden; otherwise an in-app message appears.")} />
                <SwitchPref k="notifyQueueDone" label={t("When a queue finishes")} />
                <SwitchPref k="showProgressWindow" label={t("Show a progress window for new downloads")} desc={t("Like IDM: a small window with speed, time left and the connection map. Double-click any unfinished download to open it.")} />
              </PrefGroup>
            )}
            {section === "appearance" && (
              <PrefGroup>
                <PrefRow label={t("Theme")}>
                  <Segmented
                    label={t("Theme")}
                    value={settings.theme}
                    onChange={(v) => void save({ theme: v })}
                    options={[
                      { value: "light", label: t("Light") },
                      { value: "dark", label: t("Dark") },
                      { value: "system", label: t("System") },
                    ]}
                  />
                </PrefRow>
                <PrefRow label={t("Accent colour")}>
                  <div className="accent-swatches" role="radiogroup" aria-label={t("Accent colour")}>
                    {ACCENTS.map(([id, color, name]) => (
                      <button
                        key={id}
                        type="button"
                        role="radio"
                        aria-checked={(settings.accent || "blue") === id}
                        aria-label={t(name)}
                        title={t(name)}
                        className="accent-swatch"
                        style={{ background: color }}
                        onClick={() => void save({ accent: id })}
                      />
                    ))}
                  </div>
                </PrefRow>
                <SelectPref
                  k="darkPalette"
                  label={t("Dark mode colours")}
                  options={[
                    { value: "default", label: t("Graphite") },
                    { value: "midnight", label: t("Midnight blue") },
                    { value: "black", label: t("Pure black (OLED)") },
                  ]}
                  width={180}
                />
                <SwitchPref k="compact" label={t("Compact rows")} desc={t("Shows more downloads at once by hiding the second line.")} />
                <SelectPref
                  k="language"
                  label={t("Language")}
                  desc={t("Restarts the interface.")}
                  options={[{ value: "system", label: t("System") }, ...LANGUAGES.map((l) => ({ value: l.code, label: l.name }))]}
                  width={180}
                />
              </PrefGroup>
            )}
            {section === "advanced" && <Advanced />}
          </div>
        </div>
      </div>
    </div>
  );
}

