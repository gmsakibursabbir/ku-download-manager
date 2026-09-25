import { useEffect, useMemo, useState } from "react";
import { t, tf } from "../lib/i18n";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { Folder, ChevronDown, ChevronRight, CircleAlert, Clapperboard, Globe, FileUp, File as FileIcon, ArrowDownToLine } from "lucide-react";
import { api, errorText } from "../lib/api";
import { settingsStore, queuesStore, updateSettings } from "../lib/store";
import * as fmt from "../lib/format";
import type { AddRequest, ProbeInfo, Settings, TorrentInfo } from "../lib/types";
import { Button, Checkbox, Icon, IconButton, Input, Notice, Select, Switch } from "../ui/primitives";
import { Dialog, toast } from "../ui/overlays";
import { CATEGORY_ICON } from "./downloads/FileGlyph";
import { DuplicateNotice } from "./DuplicateNotice";
import { useApp } from "./context";

/** Translated when shown (the language loads after this module). */
const CONNECTION_CHOICES = () => [
  { value: 0, label: t("Smart") },
  { value: 4, label: "4" },
  { value: 8, label: "8" },
  { value: 16, label: "16" },
  { value: 32, label: "32" },
  { value: -1, label: t("Custom") },
];

function isUrl(s: string) {
  return /^(https?|ftp|sftp):\/\/\S+$/i.test(s) || /^magnet:\?\S+$/i.test(s);
}

export function resolveDir(s: Settings | null, category: string | null | undefined): string {
  if (!s) return "";
  const sep = s.downloadDir.includes("\\") ? "\\" : "/";
  if (s.useCategories && category) {
    const c = s.categories.find((x) => x.id === category);
    if (c?.folder) return /^([a-zA-Z]:[\\/]|\/)/.test(c.folder) ? c.folder : `${s.downloadDir.replace(/[\\/]$/, "")}${sep}${c.folder}`;
  }
  return s.downloadDir;
}

export function AddDownloadDialog({ prefill, onClose }: { prefill?: Partial<AddRequest>; onClose: () => void }) {
  const { openMedia } = useApp();
  const settings = settingsStore.use();
  const queues = queuesStore.use();
  const [text, setText] = useState(prefill?.url ?? "");
  const [filename, setFilename] = useState(prefill?.filename ?? "");
  const [nameTouched, setNameTouched] = useState(!!prefill?.filename);
  const [dir, setDir] = useState(prefill?.dir ?? "");
  const [dirTouched, setDirTouched] = useState(!!prefill?.dir);
  const [category, setCategory] = useState<string>(prefill?.category ?? "");
  const [connections, setConnections] = useState<number>(prefill?.connections ?? settings?.defaultConnections ?? 0);
  const [customConn, setCustomConn] = useState("12");
  const [advanced, setAdvanced] = useState(false);
  const [referer, setReferer] = useState(prefill?.options?.referer ?? "");
  const [userAgent, setUserAgent] = useState(prefill?.options?.userAgent ?? "");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [checksum, setChecksum] = useState("");
  const [mirrors, setMirrors] = useState("");
  const [headers, setHeaders] = useState((prefill?.options?.headers ?? []).join("\n"));
  const [limit, setLimit] = useState("");
  const [probe, setProbe] = useState<ProbeInfo | null>(null);
  const [probing, setProbing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [torrent, setTorrent] = useState<{ info: TorrentInfo; data: string } | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());

  const lines = useMemo(
    () =>
      text
        .split(/\r?\n/)
        .map((l) => l.trim())
        .filter(Boolean),
    [text],
  );
  const validLines = lines.filter(isUrl);
  const batch = validLines.length > 1;
  const single = !batch && validLines.length === 1 ? validLines[0] : null;
  const torrentData = prefill?.options?.torrentData ?? null;
  const fromBrowser = prefill?.source === "browser";

  // Parse a provided .torrent to show its contents.
  useEffect(() => {
    if (!torrentData) return;
    api
      .torrentInfo({ data: torrentData })
      .then((t) => {
        setTorrent(t);
        setSelected(new Set(t.info.files.map((f) => f.index)));
        if (!category) setCategory("torrents");
      })
      .catch((e) => setError(errorText(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [torrentData]);

  // Probe a single link (debounced) for name, size and engine.
  useEffect(() => {
    setProbe(null);
    if (!single || torrentData) return;
    let alive = true;
    const t = setTimeout(async () => {
      setProbing(true);
      try {
        const p = await api.probe(single, { referer: referer || null, cookies: prefill?.options?.cookies ?? [], userAgent: userAgent || null, headers: [] });
        if (!alive) return;
        setProbe(p);
        if (!nameTouched && p.filename) setFilename(p.filename);
        if (!category && p.category) setCategory(p.category);
      } catch (e) {
        if (alive) setProbe({ url: single, finalUrl: single, error: errorText(e) });
      } finally {
        if (alive) setProbing(false);
      }
    }, 450);
    return () => {
      alive = false;
      clearTimeout(t);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [single, torrentData]);

  useEffect(() => {
    if (!dirTouched) setDir(resolveDir(settings, category || probe?.category));
  }, [settings, category, probe?.category, dirTouched]);

  const media = probe?.engine === "ytdlp";
  const conn = connections === -1 ? Math.max(1, Math.min(32, parseInt(customConn) || 8)) : connections;
  const canSubmit = !busy && (validLines.length > 0 || !!torrent) && (!lines.length || validLines.length === lines.length || batch);

  const build = (queueId: string | null, start: boolean): AddRequest => ({
    url: single ?? "",
    filename: batch ? null : filename.trim() || null,
    dir: dir.trim() || null,
    category: category || null,
    connections: conn,
    queueId,
    start,
    source: prefill?.source ?? "ui",
    sizeHint: prefill?.sizeHint ?? null,
    engine: prefill?.engine ?? null,
    mirrors: mirrors
      .split(/\r?\n/)
      .map((m) => m.trim())
      .filter(isUrl),
    options: {
      headers: headers
        .split(/\r?\n/)
        .map((h) => h.trim())
        .filter((h) => h.includes(":")),
      cookies: prefill?.options?.cookies ?? [],
      referer: referer.trim() || null,
      userAgent: userAgent.trim() || null,
      username: username || null,
      password: password || null,
      checksum: checksum.trim() || null,
      speedLimit: limit.trim() ? fmt.parseRate(limit) : null,
      torrentData: torrent?.data ?? null,
      selectFiles: torrent && selected.size < torrent.info.files.length ? [...selected].sort((a, b) => a - b).join(",") : null,
    },
  });

  const submit = async (queueId: string | null = null, start = true) => {
    if (!canSubmit) return;
    if (torrent && selected.size === 0) {
      setError("Select at least one file from the torrent.");
      return;
    }
    if (limit.trim() && fmt.parseRate(limit) == null) {
      setError("Speed limit must look like 500 KB or 2 MB.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (batch) {
        const res = await api.addBatch(validLines, build(queueId, start));
        if (res.failed.length) {
          toast({
            level: res.added.length ? "warning" : "error",
            title: tf("{added} added, {failed} failed", { added: res.added.length, failed: res.failed.length }),
            message: res.failed
              .slice(0, 3)
              .map((f) => `${fmt.host(f.url)}: ${f.error}`)
              .join("\n"),
          });
        } else {
          toast({ level: "success", title: tf("{n} downloads added", { n: res.added.length }) });
        }
      } else {
        const d = await api.add(build(queueId, start));
        // IDM-style progress window for a download started right away.
        if (start && !queueId && settings?.showProgressWindow !== false && d?.id) void invoke("open_progress_window", { id: d.id }).catch(() => {});
      }
      onClose();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const browse = async () => {
    const picked = await open({ directory: true, defaultPath: dir || settings?.downloadDir });
    if (typeof picked === "string") {
      setDir(picked);
      setDirTouched(true);
    }
  };

  const pickTorrentFile = async () => {
    const path = await open({ multiple: false, filters: [{ name: "Torrent", extensions: ["torrent"] }] });
    if (typeof path !== "string") return;
    try {
      const t = await api.torrentInfo({ path });
      setTorrent(t);
      setSelected(new Set(t.info.files.map((f) => f.index)));
      setText("");
      setCategory("torrents");
    } catch (e) {
      setError(errorText(e));
    }
  };

  const [remember, setRemember] = useState(false);
  const go = async (later: boolean) => {
    if (remember && category && dir.trim()) {
      // Remember this folder for the category (absolute path).
      const cats = (settings?.categories ?? []).map((c) => (c.id === category ? { ...c, folder: dir.trim() } : c));
      await updateSettings({ categories: cats }).catch(() => {});
    }
    // "Download Later" = the main queue, started by the user or a schedule.
    await submit(later ? (prefill?.queueId ?? queues[0]?.id ?? "main") : (prefill?.queueId ?? null), true);
  };
  const CatIcon = CATEGORY_ICON[category || probe?.category || ""] ?? FileIcon;
  const multi = batch || text.includes("\n");
  const title = torrent ? "Add torrent" : batch ? `Download ${validLines.length} files` : "Download File";

  return (
    <Dialog
      title={
        <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
          <Icon icon={ArrowDownToLine} />
          {title}
        </span>
      }
      onClose={onClose}
      width={580}
      onSubmit={() => void go(false)}
    >
      {fromBrowser && (
        <div className="faint" style={{ fontSize: "var(--text-xs)", display: "flex", gap: 6, alignItems: "center" }}>
          <Icon icon={Globe} size={13} /> {prefill?.options?.cookies?.length ? t("Sent from your browser with your session cookies") : t("Sent from your browser")}
        </div>
      )}
      <div className="dl-card">
        {torrent ? (
          <div className="dl-row" style={{ alignItems: "start" }}>
            <label>{t("Torrent")}</label>
            <div style={{ display: "flex", flexDirection: "column", gap: 6, minWidth: 0 }}>
              <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                <span className="truncate" style={{ fontWeight: 600, flex: 1 }}>
                  {torrent.info.name}
                </span>
                <span className="faint num" style={{ fontSize: "var(--text-xs)" }}>
                  {fmt.bytes(torrent.info.files.filter((f) => selected.has(f.index)).reduce((a, f) => a + f.length, 0))} of {fmt.bytes(torrent.info.total)}
                </span>
              </div>
              {torrent.info.files.length > 1 && (
                <div style={{ maxHeight: 160, overflow: "auto", display: "flex", flexDirection: "column", gap: 4 }}>
                  <Checkbox
                    checked={selected.size === torrent.info.files.length}
                    indeterminate={selected.size > 0 && selected.size < torrent.info.files.length}
                    onChange={(v) => setSelected(v ? new Set(torrent.info.files.map((f) => f.index)) : new Set())}
                  >
                    <span className="muted">{tf("All files ({n})", { n: torrent.info.files.length })}</span>
                  </Checkbox>
                  {torrent.info.files.map((f) => (
                    <Checkbox
                      key={f.index}
                      checked={selected.has(f.index)}
                      onChange={(v) => {
                        const n = new Set(selected);
                        if (v) n.add(f.index);
                        else n.delete(f.index);
                        setSelected(n);
                      }}
                    >
                      <span className="truncate" style={{ flex: 1 }} title={f.path}>
                        {f.path.split("/").slice(1).join("/") || f.path}
                      </span>
                      <span className="faint num" style={{ fontSize: "var(--text-xs)" }}>
                        {fmt.bytes(f.length)}
                      </span>
                    </Checkbox>
                  ))}
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="dl-row" style={{ alignItems: multi ? "start" : "center" }}>
            <label htmlFor="add-url" style={{ paddingTop: multi ? 6 : 0 }}>
              {t("URL")}
            </label>
            <div className="dl-control">
              <textarea
                id="add-url"
                className="textarea"
                rows={multi ? 4 : 1}
                wrap={multi ? "soft" : "off"}
                style={{ height: multi ? undefined : "var(--control-h)", paddingTop: multi ? undefined : 7, overflow: multi ? undefined : "hidden", fontFamily: "var(--font-ui)", fontSize: "var(--text-sm)", resize: "none" }}
                placeholder="https://example.com/file.zip — or several links, one per line"
                value={text}
                onChange={(e) => setText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey && !text.includes("\n")) {
                    e.preventDefault();
                    void go(false);
                  }
                }}
                data-autofocus
                spellCheck={false}
              />
              <IconButton icon={FileUp} label={t("Open .torrent file")} onClick={() => void pickTorrentFile()} />
            </div>
          </div>
        )}
        {!media && (
          <div className="dl-row">
            <label htmlFor="add-cat">{t("Category")}</label>
            <div className="dl-control">
              <Icon icon={CatIcon} size={18} />
              <Select
                id="add-cat"
                value={category}
                onChange={(e) => {
                  setCategory(e.target.value);
                  setDirTouched(false);
                }}
                options={[{ value: "", label: t("Automatic") }, ...(settings?.categories ?? []).map((c) => ({ value: c.id, label: c.name }))]}
                style={{ width: 180 }}
              />
            </div>
          </div>
        )}
        <div className="dl-row">
          <label htmlFor="add-dir">{t("Save As")}</label>
          <div className="dl-control">
            <Input
              id="add-dir"
              value={dir}
              onChange={(e) => {
                setDir(e.target.value);
                setDirTouched(true);
              }}
            />
            <IconButton icon={Folder} label={t("Choose folder")} onClick={() => void browse()} />
          </div>
        </div>
        {!media && (
          <div className="dl-row">
            <label>{t("Remember path for this category")}</label>
            <div className="dl-control">
              <span className="muted" style={{ fontSize: "var(--text-sm)" }}>
                {remember ? t("Yes") : t("No")}
              </span>
              <Switch label={t("Remember path for this category")} checked={remember} disabled={!category} onChange={setRemember} />
            </div>
          </div>
        )}
      </div>

      {single && (
        <div className="faint" style={{ fontSize: "var(--text-xs)", display: "flex", gap: 8, alignItems: "center", minHeight: 18 }}>
          {probing ? (
            <>
              <span className="spinner" /> {t("Checking the link…")}
            </>
          ) : probe?.error ? (
            <span style={{ color: "var(--warning)" }}>{probe.error} {t("You can still try to download it.")}</span>
          ) : probe && !media ? (
            <>
              <span className="num">
                {filename ? `${filename} · ` : ""}
                {probe.size ? fmt.bytes(probe.size) : t("Size unknown")}
              </span>
              <span>· {probe.resumable ? t("Resumable") : probe.resumable === false ? t("Not resumable") : t("Resume unknown")}</span>
            </>
          ) : null}
        </div>
      )}
      {!torrent && !media && <DuplicateNotice url={single} dir={dir} filename={filename || probe?.filename || ""} onHandled={onClose} />}
      {lines.length > 0 && validLines.length < lines.length && (
        <div style={{ color: "var(--danger)", fontSize: "var(--text-xs)" }}>
          {lines.length - validLines.length === 1 ? t("1 line is not a valid link and will be skipped.") : tf("{n} lines are not valid links and will be skipped.", { n: lines.length - validLines.length })}
        </div>
      )}
      {media && single && (
        <Notice
          icon={Clapperboard}
          title={t("This is a media page")}
          action={
            <Button
              size="sm"
              onClick={() => {
                onClose();
                openMedia({ url: single, cookies: prefill?.options?.cookies ?? [] });
              }}
            >
              {t("Choose quality…")}
            </Button>
          }
        >
          {tf("It will be downloaded with yt-dlp at your default quality ({quality}).", { quality: `${settings?.videoHeight}p` })}
        </Notice>
      )}

      <button type="button" className="dl-more" onClick={() => setAdvanced(!advanced)} aria-expanded={advanced}>
        <Icon icon={advanced ? ChevronDown : ChevronRight} size={14} /> {t("More options")}
      </button>
      {advanced && (
        <div className="form-grid">
          {!batch && !torrent && !media && (
            <>
              <label htmlFor="add-name">{t("File name")}</label>
              <Input
                id="add-name"
                value={filename}
                placeholder={probing ? "…" : t("Automatic")}
                onChange={(e) => {
                  setFilename(e.target.value);
                  setNameTouched(true);
                }}
              />
            </>
          )}
          {!torrent && !media && (
            <>
              <label htmlFor="add-conn">{t("Connections")}</label>
              <div className="input-group">
                <Select id="add-conn" value={connections} onChange={(e) => setConnections(+e.target.value)} options={CONNECTION_CHOICES()} style={{ width: 120 }} />
                {connections === -1 && <Input value={customConn} onChange={(e) => setCustomConn(e.target.value.replace(/\D/g, ""))} style={{ width: 56 }} aria-label={t("Custom connections (1-32)")} />}
              </div>
            </>
          )}
          <label>{t("Referer")}</label>
          <Input value={referer} onChange={(e) => setReferer(e.target.value)} placeholder={t("Page the link came from")} />
          <label>{t("User agent")}</label>
          <Input value={userAgent} onChange={(e) => setUserAgent(e.target.value)} placeholder={t("Default")} />
          <label>{t("Sign in")}</label>
          <div className="input-group">
            <Input value={username} onChange={(e) => setUsername(e.target.value)} placeholder={t("User name")} />
            <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder={t("Password")} />
          </div>
          <label>{t("Checksum")}</label>
          <Input value={checksum} onChange={(e) => setChecksum(e.target.value)} placeholder={t("sha-256=… (verified when finished)")} className="mono" />
          <label>{t("Speed limit")}</label>
          <Input value={limit} onChange={(e) => setLimit(e.target.value)} placeholder={t("Unlimited — e.g. 2 MB")} />
          <label style={{ alignSelf: "start", paddingTop: 6 }}>{t("Mirrors")}</label>
          <textarea className="textarea" rows={2} value={mirrors} onChange={(e) => setMirrors(e.target.value)} placeholder={t("Other links to the same file, one per line")} />
          <label style={{ alignSelf: "start", paddingTop: 6 }}>{t("Headers")}</label>
          <textarea className="textarea" rows={2} value={headers} onChange={(e) => setHeaders(e.target.value)} placeholder={t("Name: value, one per line")} />
        </div>
      )}

      {error && (
        <Notice level="error" icon={CircleAlert}>
          {error}
        </Notice>
      )}

      <div className="dl-actions" style={{ padding: "var(--space-2) 0 0" }}>
        <Button disabled={!canSubmit} onClick={() => void go(true)} title={t("Add to the queue without starting")}>
          {t("Download Later")}
        </Button>
        <Button type="submit" variant="primary" disabled={!canSubmit} busy={busy}>
          {t("Download Now")}
        </Button>
        <Button onClick={onClose}>{t("Cancel")}</Button>
      </div>
    </Dialog>
  );
}
