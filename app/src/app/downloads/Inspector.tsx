import { useEffect, useMemo, useState } from "react";
import { Copy, FolderOpen, ExternalLink, Pause, Play, RotateCcw, Trash2, X, CircleAlert, Gauge, ChevronRight, ChevronDown, ShieldCheck, Link as LinkIcon } from "lucide-react";
import { useDownload } from "../../lib/store";
import { api, errorText } from "../../lib/api";
import * as fmt from "../../lib/format";
import type { Details, Download } from "../../lib/types";
import { Button, Icon, IconButton, Notice, Progress, StatusBadge, Tabs, Input } from "../../ui/primitives";
import { toast, Dialog } from "../../ui/overlays";
import { FileGlyph } from "./FileGlyph";
import { canPause, canResume, openDownload, run } from "./actions";
import { useApp } from "../context";

function engineLabel(d: Download) {
  const kind = { http: "HTTP", ftp: "FTP", sftp: "SFTP", torrent: "BitTorrent", magnet: "Magnet", metalink: "Metalink", media: "Media" }[d.kind];
  return `${kind} · ${d.engine === "ytdlp" ? "yt-dlp" : d.engine === "kuhttp" ? "KuHTTP (experimental)" : "aria2"}`;
}

function connectionsLabel(d: Download, smartTarget?: number | null) {
  if (d.engine === "ytdlp") return "Managed by yt-dlp";
  if (d.kind === "torrent" || d.kind === "magnet") return `${d.activeConnections} peers`;
  const mode = d.connections === 0 ? `Smart${smartTarget ? ` (target ${smartTarget})` : ""}` : `${d.connections}`;
  const live = d.status === "downloading" ? ` · ${d.activeConnections} active` : "";
  return mode + live;
}

/** aria2 bitfield (hex, MSB first) → booleans. */
function pieces(bitfield: string | undefined, count: number): boolean[] {
  if (!bitfield || !count) return [];
  const out: boolean[] = [];
  for (const ch of bitfield) {
    const n = parseInt(ch, 16);
    for (let b = 3; b >= 0 && out.length < count; b--) out.push(((n >> b) & 1) === 1);
  }
  return out;
}

type AdvTab = "connections" | "pieces" | "files" | "peers" | "trackers" | "log";

function Advanced({ d }: { d: Download }) {
  const [open, setOpen] = useState(false);
  const [details, setDetails] = useState<Details | null>(null);
  const bt = d.kind === "torrent" || d.kind === "magnet";
  const tabs: { value: AdvTab; label: string }[] = useMemo(() => {
    const t: { value: AdvTab; label: string }[] = [];
    if (d.engine === "aria2" && !bt) t.push({ value: "connections", label: "Connections" });
    if (d.engine === "kuhttp") t.push({ value: "connections", label: "Segments" });
    if (d.engine === "aria2") t.push({ value: "pieces", label: "Pieces" });
    if (bt) t.push({ value: "files", label: "Files" }, { value: "peers", label: "Peers" }, { value: "trackers", label: "Trackers" });
    t.push({ value: "log", label: "Log" });
    return t;
  }, [d.engine, bt]);
  const [tab, setTab] = useState<AdvTab>(tabs[0].value);
  useEffect(() => {
    if (!tabs.some((t) => t.value === tab)) setTab(tabs[0].value);
  }, [tabs, tab]);

  const live = d.status === "downloading" || d.status === "seeding" || d.status === "processing";
  useEffect(() => {
    if (!open) return;
    let alive = true;
    const load = () =>
      api
        .details(d.id)
        .then((x) => alive && setDetails(x))
        .catch(() => {});
    void load();
    // Only poll while the panel is open and the transfer is live.
    const t = live ? setInterval(load, 1000) : undefined;
    return () => {
      alive = false;
      if (t) clearInterval(t);
    };
  }, [open, d.id, live]);

  const conns = details?.segments
    ? details.segments.map((sg) => ({ uri: "", currentUri: `${fmt.bytes(sg.start)} – ${fmt.bytes(sg.end)}${sg.retries ? ` · ${sg.retries} retries` : ""}`, speed: sg.done ? 0 : sg.speed, done: sg.done, pct: fmt.percent(sg.downloaded, sg.end - sg.start) }))
    : (details?.connections ?? []).map((c) => ({ ...c, done: false, pct: -1 }));
  const maxSpeed = Math.max(1, ...conns.map((c) => c.speed));
  const map = pieces(details?.pieces?.bitfield, details?.pieces?.count ?? 0);
  const mapShown = map.length > 600 ? map.filter((_, i) => i % Math.ceil(map.length / 600) === 0) : map;

  return (
    <div className="insp-advanced">
      <button type="button" className="insp-section-title" onClick={() => setOpen(!open)} aria-expanded={open}>
        <Icon icon={open ? ChevronDown : ChevronRight} size={14} />
        Technical details
      </button>
      {open && (
        <div style={{ marginTop: "var(--space-3)" }}>
          <Tabs value={tab} onChange={setTab} tabs={tabs} />
          {tab === "connections" &&
            (conns.length ? (
              <div className="conn-list num">
                {conns.map((c, i) => (
                  <div key={i} className="conn-row" title={c.currentUri}>
                    <span className="faint">{d.engine === "kuhttp" ? "Seg" : "Conn"} {String(i + 1).padStart(2, "0")}</span>
                    <Progress value={c.pct >= 0 ? c.pct : (c.speed / maxSpeed) * 100} state={c.done ? "completed" : "downloading"} />
                    <span style={{ textAlign: "right" }}>{c.done ? "Done" : fmt.speed(c.speed)}</span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="faint" style={{ fontSize: "var(--text-xs)" }}>
                {live ? "Connecting…" : "No active connections."}
              </div>
            ))}
          {tab === "pieces" &&
            (mapShown.length ? (
              <>
                <div className="piece-map">
                  {mapShown.map((h, i) => (
                    <span key={i} data-have={h} />
                  ))}
                </div>
                <div className="faint" style={{ fontSize: "var(--text-xs)", marginTop: 8 }}>
                  {details?.pieces?.count} pieces of {fmt.bytes(details?.pieces?.length)}
                </div>
              </>
            ) : (
              <div className="faint" style={{ fontSize: "var(--text-xs)" }}>Piece information is available while the download is active.</div>
            ))}
          {tab === "files" && (
            <div className="conn-list">
              {(details?.files?.length ? details.files.map((f) => ({ path: f.path, length: +f.length, completed: +f.completedLength, selected: f.selected === "true" })) : d.meta.files).map((f, i) => (
                <div key={i} style={{ display: "grid", gridTemplateColumns: "1fr 64px", gap: 8, opacity: f.selected ? 1 : 0.5 }}>
                  <span className="truncate" title={f.path}>
                    {f.path.split(/[\\/]/).pop()}
                  </span>
                  <span className="num faint" style={{ textAlign: "right" }}>
                    {f.length ? `${Math.floor(fmt.percent(f.completed, f.length))}%` : ""} {fmt.bytes(f.length)}
                  </span>
                </div>
              ))}
            </div>
          )}
          {tab === "peers" &&
            (details?.peers?.length ? (
              <div className="conn-list num">
                {details.peers.map((p, i) => (
                  <div key={i} style={{ display: "grid", gridTemplateColumns: "1fr 70px 70px", gap: 8 }}>
                    <span className="truncate mono">
                      {p.ip}
                      {p.seeder ? " · seed" : ""}
                    </span>
                    <span style={{ textAlign: "right" }}>↓ {fmt.speed(p.downloadSpeed)}</span>
                    <span className="faint" style={{ textAlign: "right" }}>
                      ↑ {fmt.speed(p.uploadSpeed)}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="faint" style={{ fontSize: "var(--text-xs)" }}>{live ? "Looking for peers…" : "Not connected to peers."}</div>
            ))}
          {tab === "trackers" && (
            <div className="conn-list">
              {(details?.trackers ?? []).map((t, i) => (
                <span key={i} className="mono truncate selectable" title={t}>
                  {t}
                </span>
              ))}
              {!details?.trackers?.length && <span className="faint">Trackerless (DHT) or not loaded yet.</span>}
            </div>
          )}
          {tab === "log" && (
            <div className="log-list">
              {(details?.log ?? []).map((l, i) => (
                <div key={i} className={`lvl-${l.level}`}>
                  <span className="faint">{new Date(l.ts).toLocaleTimeString()}</span> {l.message}
                </div>
              ))}
              {!details?.log?.length && <span className="faint">No log entries in this session.</span>}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function VerifyDialog({ id, onClose }: { id: string; onClose: () => void }) {
  const d = useDownload(id);
  const [algo, setAlgo] = useState(() => d?.options.checksum?.split(/[=:]/)[0] ?? "sha-256");
  const [expected, setExpected] = useState(() => d?.options.checksum?.split(/[=:]/)[1] ?? "");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ hash: string; match: boolean | null } | null>(null);
  const [error, setError] = useState<string | null>(null);
  if (!d) return null;
  const verify = async () => {
    setBusy(true);
    setError(null);
    try {
      const hash = await api.verify(id, algo);
      const exp = expected.trim().toLowerCase();
      setResult({ hash, match: exp ? exp === hash : null });
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      title="Verify checksum"
      onClose={onClose}
      width={520}
      onSubmit={verify}
      footer={
        <>
          <span className="spacer" />
          <Button onClick={onClose}>Close</Button>
          <Button type="submit" variant="primary" busy={busy}>
            Compute
          </Button>
        </>
      }
    >
      <div className="muted truncate" style={{ fontSize: "var(--text-sm)" }}>
        {d.name}
      </div>
      <div className="form-grid">
        <label>Algorithm</label>
        <select className="select" value={algo} onChange={(e) => setAlgo(e.target.value)}>
          <option value="sha-256">SHA-256</option>
          <option value="sha-512">SHA-512</option>
          <option value="sha-1">SHA-1</option>
          <option value="md5">MD5</option>
        </select>
        <label>Expected</label>
        <Input value={expected} onChange={(e) => setExpected(e.target.value)} placeholder="Optional — paste the published hash" className="mono" />
      </div>
      {error && (
        <Notice level="error" icon={CircleAlert}>
          {error}
        </Notice>
      )}
      {result && (
        <Notice level={result.match === false ? "error" : "info"} icon={result.match === false ? CircleAlert : ShieldCheck} title={result.match == null ? "Hash computed" : result.match ? "The file matches" : "The file does not match"}>
          <span className="mono selectable" style={{ overflowWrap: "anywhere" }}>
            {result.hash}
          </span>
        </Notice>
      )}
    </Dialog>
  );
}

export function Inspector({ id, onClose, onVerify }: { id: string; onClose: () => void; onVerify: (id: string) => void }) {
  const d = useDownload(id);
  const { confirmRemove } = useApp();
  const [editUrl, setEditUrl] = useState<string | null>(null);
  if (!d) return null;
  const pct = d.status === "completed" ? 100 : fmt.percent(d.done, d.total);
  const active = d.status === "downloading" || d.status === "processing";
  const eta = active ? fmt.eta(d.done, d.total, d.speed) : null;
  const finished = d.status === "completed" || d.status === "seeding";
  const path = d.filePath ?? `${d.dir}${d.dir.includes("\\") ? "\\" : "/"}${d.name}`;
  const copy = (text: string, what: string) => {
    void navigator.clipboard.writeText(text);
    toast({ level: "success", title: `${what} copied` });
  };

  return (
    <aside className="inspector" aria-label="Download details">
      <div className="toolbar" style={{ padding: "0 var(--space-2) 0 var(--space-4)" }}>
        <span className="section-title" style={{ flex: 1 }}>
          Details
        </span>
        <IconButton icon={X} label="Close details (Ctrl I)" size="sm" onClick={onClose} />
      </div>
      <div className="inspector-scroll">
        {d.meta.thumbnail && <img className="insp-thumb" src={d.meta.thumbnail} alt="" referrerPolicy="no-referrer" />}
        <div className="insp-head">
          {!d.meta.thumbnail && <FileGlyph d={d} size={18} />}
          <div style={{ minWidth: 0, flex: 1 }}>
            <div className="insp-name selectable" title={d.name}>
              {d.name}
            </div>
            <div style={{ marginTop: 4, display: "flex", gap: 8, alignItems: "center" }}>
              <StatusBadge status={d.status} />
              {d.meta.uploader && <span className="faint truncate" style={{ fontSize: "var(--text-xs)" }}>{d.meta.uploader}</span>}
            </div>
          </div>
        </div>

        {d.status === "error" && (
          <Notice
            level="error"
            icon={CircleAlert}
            title="Download failed"
            action={
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <Button size="sm" icon={RotateCcw} onClick={() => void run(api.resume([d.id]), "Could not retry")}>
                  Retry
                </Button>
                {d.engine === "aria2" && !d.url.startsWith("torrent:") && (
                  <Button size="sm" variant="ghost" icon={LinkIcon} onClick={() => setEditUrl(d.url)}>
                    New link
                  </Button>
                )}
              </div>
            }
          >
            {d.error}
          </Notice>
        )}
        {d.meta.smartNote && d.status !== "error" && d.status !== "completed" && (
          <Notice icon={Gauge}>{d.meta.smartNote}</Notice>
        )}

        {!finished && (
          <div className="insp-progress">
            <div className="insp-progress-top">
              <span className="insp-pct num">{d.total > 0 ? `${pct.toFixed(pct < 10 ? 1 : 0)}%` : "—"}</span>
              <span className="faint num" style={{ fontSize: "var(--text-xs)" }}>
                {d.total > 0 ? `${fmt.bytes(d.done)} of ${fmt.bytes(d.total)}` : `${fmt.bytes(d.done)} downloaded`}
              </span>
            </div>
            <Progress value={pct} state={d.status} indeterminate={(active && d.total === 0) || d.status === "processing"} />
            <div className="insp-stats num">
              <div className="insp-stat">
                <div className="k">Speed</div>
                <div className="v">{active ? fmt.speed(d.speed) : "—"}</div>
              </div>
              <div className="insp-stat">
                <div className="k">Time left</div>
                <div className="v">{eta != null ? fmt.duration(eta) : "—"}</div>
              </div>
              <div className="insp-stat">
                <div className="k">{d.kind === "torrent" || d.kind === "magnet" ? "Peers" : "Connections"}</div>
                <div className="v">{active ? d.activeConnections : "—"}</div>
              </div>
            </div>
          </div>
        )}

        <div className="insp-actions">
          {finished && (
            <Button variant="primary" size="sm" icon={ExternalLink} onClick={() => openDownload(d)}>
              Open
            </Button>
          )}
          <Button size="sm" icon={FolderOpen} onClick={() => void run(api.openFolder(d.id), "Could not open the folder")}>
            Show in folder
          </Button>
          {canPause(d) && (
            <Button size="sm" icon={Pause} onClick={() => void run(api.pause([d.id]), "Could not pause")}>
              {d.status === "seeding" ? "Stop seeding" : "Pause"}
            </Button>
          )}
          {canResume(d) && d.status !== "error" && (
            <Button size="sm" icon={Play} onClick={() => void run(api.resume([d.id]), "Could not resume")}>
              Resume
            </Button>
          )}
          <IconButton icon={Trash2} className="is-danger" label="Remove…" size="sm" onClick={() => confirmRemove([d.id])} />
        </div>

        <dl className="facts">
          <dt>Saved to</dt>
          <dd className="fact-link">
            <span className="selectable" title={path}>
              {path}
            </span>
          </dd>
          <dt>Link</dt>
          <dd className="fact-link">
            <span className="truncate" title={d.url}>
              {d.url.startsWith("torrent:") ? "Local .torrent file" : d.url}
            </span>
            {!d.url.startsWith("torrent:") && <IconButton icon={Copy} label="Copy link" size="sm" onClick={() => copy(d.url, "Link")} />}
          </dd>
          <dt>Size</dt>
          <dd className="num">{d.total > 0 ? `${fmt.bytes(d.total)} (${Math.round(d.total).toLocaleString()} bytes)` : "Unknown"}</dd>
          <dt>Type</dt>
          <dd>{engineLabel(d)}</dd>
          <dt>Connections</dt>
          <dd>{connectionsLabel(d)}</dd>
          {d.meta.resumable === false && (
            <>
              <dt>Resume</dt>
              <dd>Not supported by the server</dd>
            </>
          )}
          {d.meta.infoHash && (
            <>
              <dt>Info hash</dt>
              <dd className="mono selectable" style={{ fontSize: "var(--text-2xs)" }}>
                {d.meta.infoHash}
              </dd>
            </>
          )}
          {d.kind === "torrent" && d.uploaded > 0 && (
            <>
              <dt>Uploaded</dt>
              <dd className="num">
                {fmt.bytes(d.uploaded)} · ratio {(d.uploaded / Math.max(1, d.total)).toFixed(2)}
              </dd>
            </>
          )}
          {d.mirrors.length > 0 && (
            <>
              <dt>Mirrors</dt>
              <dd>{d.mirrors.length}</dd>
            </>
          )}
          {d.meta.verified && (
            <>
              <dt>Checksum</dt>
              <dd style={{ color: "var(--success)" }}>Verified ({d.meta.verified.split("=")[0]})</dd>
            </>
          )}
          <dt>Added</dt>
          <dd>{fmt.relativeDate(d.createdAt)} · {d.source}</dd>
          {d.completedAt && (
            <>
              <dt>Completed</dt>
              <dd>{fmt.relativeDate(d.completedAt)}</dd>
            </>
          )}
        </dl>

        {finished && d.engine === "aria2" && d.kind !== "torrent" && d.kind !== "magnet" && (
          <Button size="sm" variant="ghost" icon={ShieldCheck} onClick={() => onVerify(d.id)} style={{ alignSelf: "flex-start" }}>
            Verify checksum…
          </Button>
        )}

        <Advanced d={d} />
      </div>
      {editUrl != null && (
        <Dialog
          title="Refresh download link"
          width={520}
          onClose={() => setEditUrl(null)}
          onSubmit={async () => {
            try {
              await api.edit(d.id, { url: editUrl });
              await api.resume([d.id]);
              setEditUrl(null);
            } catch (e) {
              toast({ level: "error", title: "Could not update the link", message: errorText(e) });
            }
          }}
          footer={
            <>
              <span className="spacer" />
              <Button onClick={() => setEditUrl(null)}>Cancel</Button>
              <Button type="submit" variant="primary">
                Update and resume
              </Button>
            </>
          }
        >
          <div className="muted" style={{ fontSize: "var(--text-sm)" }}>
            Paste a fresh link to the same file. Already downloaded data is kept.
          </div>
          <Input value={editUrl} onChange={(e) => setEditUrl(e.target.value)} />
        </Dialog>
      )}
    </aside>
  );
}
