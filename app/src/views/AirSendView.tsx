import { te } from "../lib/engineText";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowDownLeft, ArrowUpRight, Copy, Download, File as FileIcon, Folder, FolderOpen, MessageSquareText, Plus, RefreshCw, RotateCcw, ShieldCheck, Trash2, X } from "lucide-react";
import { airApi, AVATARS, hashPick, isFinished, loadAir, reloadTransfers, useAir, type AirPeer, type AirStatus, type AirTransfer } from "../lib/airsend";
import { errorText } from "../lib/api";
import * as fmt from "../lib/format";
import { t, tf } from "../lib/i18n";
import { settingsStore, updateSettings } from "../lib/store";
import { Button, Checkbox, Icon, IconButton, Input, PrefRow, Switch } from "../ui/primitives";
import { Dialog, showMenu, toast } from "../ui/overlays";
import { useApp } from "../app/context";
import { ANIMAL_NAMES, OsBadge, osName, PixelAnimal } from "../app/airsend/PixelAnimal";

/** What each send was, so "Try again" and a PIN retry can repeat it. */
const sent = new Map<string, { fingerprint: string; paths: string[]; text?: string }>();

async function send(peer: AirPeer, paths: string[], text?: string, pin?: string) {
  try {
    const id = await airApi.send(peer.fingerprint, paths, text ?? null, pin ?? null);
    sent.set(id, { fingerprint: peer.fingerprint, paths, text });
  } catch (e) {
    toast({ level: "error", title: tf("Could not send to {peer}", { peer: peer.alias }), message: errorText(e) });
  }
}

const asList = (r: string | string[] | null) => (r == null ? [] : Array.isArray(r) ? r : [r]);
const isLink = (s: string) => /^(https?|ftp|magnet):\S+$/i.test(s.trim());

function stateText(x: AirTransfer): string {
  const out = x.direction === "send";
  switch (x.state) {
    case "waiting":
      return out ? t("Waiting for {peer} to accept…").replace("{peer}", x.peer) : t("Waiting for your answer");
    case "transferring":
      return `${fmt.percent(x.done, x.total).toFixed(0)}% · ${fmt.speed(x.speed)}`;
    case "done":
      if (x.download) {
        const at = x.download.at && x.download.at > x.started + 30_000 ? fmt.dateTime(x.download.at) : null;
        return at ? tf("Download scheduled for {time}", { time: at }) : out ? tf("{peer} is downloading it", { peer: x.peer }) : t("Download added");
      }
      return x.text ? (out ? t("Message sent") : t("Message received")) : out ? t("Sent") : t("Received");
    case "declined":
      return t("Declined");
    case "pin":
      return t("PIN needed");
    case "cancelled":
      return t("Cancelled");
    default:
      return te(x.error) || t("Failed");
  }
}

function TransferRow({ x, peers }: { x: AirTransfer; peers: AirPeer[] }) {
  const app = useApp();
  const live = !isFinished(x);
  const again = sent.get(x.id);
  const peer = peers.find((p) => p.fingerprint === x.peerFingerprint);
  const title = x.text ? `“${x.text}”` : x.files[0] ? `${x.files[0].name}${x.fileCount > 1 ? `  +${x.fileCount - 1}` : ""}` : "…";
  return (
    <div className="air-row" data-state={x.state}>
      <PixelAnimal animal={x.peerAvatar} size={34} seed={hashPick(x.peerFingerprint, 97)} still />
      <div className="air-row-main">
        <div className="air-row-title">
          <Icon icon={x.direction === "send" ? ArrowUpRight : ArrowDownLeft} size={14} />
          <span className="truncate" title={x.text ?? x.files.map((f) => f.name).join("\n")}>
            {title}
          </span>
        </div>
        <div className="air-row-sub faint">
          <span>{x.peer}</span>
          {!x.text && x.total > 0 && <span className="num">{fmt.bytes(x.total)}</span>}
          <span className={x.state === "failed" ? "air-err" : undefined}>{stateText(x)}</span>
          {x.finished && !live && <span>{fmt.relativeDate(x.finished)}</span>}
        </div>
        {x.state === "transferring" && (
          <div className="air-bar">
            <span style={{ width: `${fmt.percent(x.done, x.total)}%` }} />
          </div>
        )}
      </div>
      <div className="air-row-actions">
        {live && <IconButton icon={X} className="is-danger" label={t("Cancel")} size="sm" onClick={() => void airApi.cancel(x.id)} />}
        {x.state === "done" && x.direction === "receive" && x.folder && !x.text && (
          <IconButton icon={FolderOpen} label={t("Open folder")} size="sm" onClick={() => void invoke("reveal_path", { path: x.folder }).catch(() => {})} />
        )}
        {x.text && (
          <IconButton
            icon={Copy}
            label={t("Copy")}
            size="sm"
            onClick={() => {
              void navigator.clipboard.writeText(x.text ?? "");
              toast({ level: "success", title: t("Copied") });
            }}
          />
        )}
        {x.text && x.direction === "receive" && isLink(x.text) && <IconButton icon={Download} label={t("Download")} size="sm" onClick={() => app.openAdd({ url: x.text!.trim() })} />}
        {!live && x.state !== "done" && x.direction === "send" && again && peer && (
          <IconButton icon={RotateCcw} label={t("Try again")} size="sm" onClick={() => void send(peer, again.paths, again.text)} />
        )}
      </div>
    </div>
  );
}

function TextDialog({ peer, onClose }: { peer: AirPeer; onClose: () => void }) {
  const [text, setText] = useState("");
  return (
    <Dialog
      title={t("Send text or a link to {peer}").replace("{peer}", peer.alias)}
      width={460}
      onClose={onClose}
      onSubmit={() => {
        if (!text.trim()) return;
        void send(peer, [], text.trim());
        onClose();
      }}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button type="submit" variant="primary" disabled={!text.trim()}>
            {t("Send")}
          </Button>
        </>
      }
    >
      <textarea className="textarea" rows={5} value={text} onChange={(e) => setText(e.target.value)} placeholder={t("A message, or a download link: the other PC can download it with one click.")} autoFocus />
    </Dialog>
  );
}

/** Hand a link to the other device: it downloads the file itself, now or at a set time. */
function RemoteDownloadDialog({ peer, onClose }: { peer: AirPeer; onClose: () => void }) {
  const [url, setUrl] = useState("");
  const [later, setLater] = useState(false);
  const [when, setWhen] = useState(() => {
    const d = new Date(Date.now() + 3600_000);
    d.setMinutes(0, 0, 0);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
  });
  const valid = /^(https?|ftp|sftp):\/\/\S+$|^magnet:\?\S+$/i.test(url.trim());
  return (
    <Dialog
      title={tf("Download on {peer}", { peer: peer.alias })}
      width={480}
      onClose={onClose}
      onSubmit={async () => {
        if (!valid) return;
        const at = later ? new Date(when).getTime() : null;
        try {
          await airApi.sendDownload(peer.fingerprint, { url: url.trim(), at });
          toast({ level: "success", title: tf("Sent to {peer}", { peer: peer.alias }), message: at ? tf("It downloads at {time}.", { time: fmt.dateTime(at) }) : undefined });
        } catch (e) {
          toast({ level: "error", title: tf("Could not send to {peer}", { peer: peer.alias }), message: errorText(e) });
        }
        onClose();
      }}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button type="submit" variant="primary" disabled={!valid}>
            {t("Send")}
          </Button>
        </>
      }
    >
      <p className="faint" style={{ marginTop: 0 }}>
        {t("The other device downloads the file itself, with its own connection.")}
      </p>
      <input className="input" value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://…" autoFocus style={{ width: "100%" }} />
      <div style={{ display: "flex", gap: "var(--space-3)", alignItems: "center", marginTop: "var(--space-3)" }}>
        <Checkbox checked={later} onChange={setLater}>
          {t("Start at")}
        </Checkbox>
        <input className="input" type="datetime-local" value={when} disabled={!later} onChange={(e) => setWhen(e.target.value)} />
      </div>
    </Dialog>
  );
}

function PinDialog({ peer, onSend, onClose }: { peer: string; onSend: (pin: string) => void; onClose: () => void }) {
  const [pin, setPin] = useState("");
  return (
    <Dialog
      title={t("PIN needed")}
      width={360}
      onClose={onClose}
      onSubmit={() => pin.trim() && onSend(pin.trim())}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button type="submit" variant="primary" disabled={!pin.trim()}>
            {t("Send")}
          </Button>
        </>
      }
    >
      <p className="faint" style={{ margin: 0 }}>
        {t("{peer} asks for a PIN. Enter the PIN shown in KuAirSend on that computer.").replace("{peer}", peer)}
      </p>
      <Input value={pin} onChange={(e) => setPin(e.target.value)} className="mono" autoFocus inputMode="numeric" />
    </Dialog>
  );
}

function AddDialog({ onClose }: { onClose: () => void }) {
  const [addr, setAddr] = useState("");
  const [busy, setBusy] = useState(false);
  const add = async () => {
    setBusy(true);
    try {
      const p = await airApi.add(addr);
      toast({ level: "success", title: t("{peer} added").replace("{peer}", p.alias) });
      onClose();
    } catch (e) {
      toast({ level: "error", title: t("Device not found"), message: errorText(e) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      title={t("Add a device by IP address")}
      width={400}
      onClose={onClose}
      onSubmit={() => void add()}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button type="submit" variant="primary" busy={busy} disabled={!addr.trim()}>
            {t("Add")}
          </Button>
        </>
      }
    >
      <p className="faint" style={{ margin: 0 }}>
        {t("For networks that hide devices from each other. The address is shown on the other computer's KuAirSend page.")}
      </p>
      <Input value={addr} onChange={(e) => setAddr(e.target.value)} placeholder="192.168.1.20" className="mono" autoFocus />
    </Dialog>
  );
}

/** You in the middle, nearby devices around you. */
function Radar({ status, peers, transfers, onPeer }: { status: AirStatus; peers: AirPeer[]; transfers: AirTransfer[]; onPeer: (p: AirPeer, el: HTMLElement) => void }) {
  const n = peers.length;
  return (
    <div className="air-radar" data-empty={n === 0 || undefined}>
      <div className="air-rings" aria-hidden="true">
        <span />
        <span />
        <span />
        <i />
      </div>
      <div className="air-me">
        <PixelAnimal animal={status.avatar} size={76} seed={hashPick(status.fingerprint, 97)} />
        <span className="air-name">{status.alias}</span>
        <span className="air-sub faint">{t("You")}</span>
      </div>
      {peers.map((p, i) => {
        // Evenly around the circle, starting at the top.
        const a = -Math.PI / 2 + (i * 2 * Math.PI) / Math.max(n, 1) + (n === 2 ? Math.PI / 2 : 0);
        // An ellipse that keeps names inside the card.
        const rx = n > 8 ? 40 : 36;
        const ry = n > 8 ? 33 : 30;
        const live = transfers.find((x) => x.peerFingerprint === p.fingerprint && x.state === "transferring");
        const pct = live ? fmt.percent(live.done, live.total) : 0;
        return (
          <button
            key={p.fingerprint}
            type="button"
            className="air-peer"
            style={{ left: `${50 + Math.cos(a) * rx}%`, top: `${50 + Math.sin(a) * ry}%` }}
            title={`${p.alias} · ${p.ip}`}
            onClick={(e) => onPeer(p, e.currentTarget)}
          >
            <span className="air-peer-ring" style={live ? { background: `conic-gradient(var(--accent) ${pct}%, transparent 0)` } : undefined}>
              <PixelAnimal animal={p.avatar} size={60} seed={hashPick(p.fingerprint, 97)} />
              <OsBadge os={p.os} />
            </span>
            <span className="air-name">
              {p.trusted && <Icon icon={ShieldCheck} size={12} />}
              {p.alias}
            </span>
            <span className="air-sub faint">{live ? `${pct.toFixed(0)}%` : osName(p.os)}</span>
          </button>
        );
      })}
      {n === 0 && (
        <div className="air-hint faint">
          {t("Looking for nearby devices… Turn on KuAirSend in KuDownloader on the other computer, on the same Wi-Fi or network.")}
        </div>
      )}
    </div>
  );
}

function AirSettings({ status }: { status: AirStatus }) {
  const s = settingsStore.use();
  const [name, setName] = useState(s?.airsendName ?? "");
  const [pin, setPin] = useState(s?.airsendPin ?? "");
  useEffect(() => setName(s?.airsendName ?? ""), [s?.airsendName]);
  useEffect(() => setPin(s?.airsendPin ?? ""), [s?.airsendPin]);
  if (!s) return null;
  const save = (patch: Parameters<typeof updateSettings>[0]) => void updateSettings(patch).catch((e) => toast({ level: "error", title: "KuAirSend", message: errorText(e) }));
  return (
    <div className="card air-settings">
      <PrefRow label={t("Your name")} desc={t("How nearby devices see you.")}>
        <Input value={name} placeholder={status.alias} onChange={(e) => setName(e.target.value)} onBlur={() => name !== s.airsendName && save({ airsendName: name.trim() })} style={{ width: 200 }} />
      </PrefRow>
      <PrefRow label={t("Your animal")} stack>
        <div className="air-animals" role="radiogroup">
          {AVATARS.map((a) => (
            <button key={a} type="button" role="radio" aria-checked={status.avatar === a} title={t(ANIMAL_NAMES[a])} onClick={() => save({ airsendAvatar: a })}>
              <PixelAnimal animal={a} size={40} still />
            </button>
          ))}
        </div>
      </PrefRow>
      <PrefRow label={t("Save received files to")} desc={<span className="mono">{status.folder}</span>}>
        <Button
          size="sm"
          icon={Folder}
          onClick={async () => {
            const d = await open({ directory: true, defaultPath: status.folder });
            if (typeof d === "string") save({ airsendFolder: d });
          }}
        >
          {t("Change…")}
        </Button>
      </PrefRow>
      <PrefRow label={t("Accept without asking")} desc={t("From any nearby KuDownloader. Devices you trust never need an answer.")}>
        <Switch checked={s.airsendAutoAccept} onChange={(v) => save({ airsendAutoAccept: v })} label={t("Accept without asking")} />
      </PrefRow>
      <PrefRow label={t("PIN")} desc={t("Senders must type this PIN. Leave empty for none.")}>
        <Input value={pin} onChange={(e) => setPin(e.target.value.replace(/\s/g, "").slice(0, 12))} onBlur={() => pin !== s.airsendPin && save({ airsendPin: pin })} className="mono" style={{ width: 120 }} />
      </PrefRow>
    </div>
  );
}

export function AirSendView() {
  const s = settingsStore.use();
  const { peers, transfers } = useAir();
  const [status, setStatus] = useState<AirStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [textFor, setTextFor] = useState<AirPeer | null>(null);
  const [remoteFor, setRemoteFor] = useState<AirPeer | null>(null);
  const [adding, setAdding] = useState(false);
  const [pinFor, setPinFor] = useState<AirTransfer | null>(null);
  const [error, setError] = useState<string | null>(null);
  const asked = useRef(new Set<string>());

  useEffect(() => {
    loadAir();
    reloadTransfers();
  }, []);
  // Name, animal and folder come from settings.
  useEffect(() => {
    void airApi
      .status()
      .then(setStatus)
      .catch((e) => setError(errorText(e)));
  }, [s?.airsendName, s?.airsendAvatar, s?.airsendFolder, s?.downloadDir, s?.airsendEnabled]);

  // A send that needs a PIN asks for it once.
  useEffect(() => {
    const x = transfers.find((t) => t.direction === "send" && t.state === "pin" && sent.has(t.id) && !asked.current.has(t.id));
    if (x) {
      asked.current.add(x.id);
      setPinFor(x);
    }
  }, [transfers]);

  const toggle = async (on: boolean) => {
    setBusy(true);
    try {
      setStatus(await airApi.setEnabled(on));
      setError(null);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const peerMenu = (p: AirPeer, el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    showMenu(r.left + r.width / 2 - 100, r.bottom - 8, [
      {
        label: t("Send files…"),
        icon: FileIcon,
        onSelect: async () => {
          const paths = asList(await open({ multiple: true }));
          if (paths.length) void send(p, paths);
        },
      },
      {
        label: t("Send a folder…"),
        icon: Folder,
        onSelect: async () => {
          const paths = asList(await open({ directory: true, multiple: true }));
          if (paths.length) void send(p, paths);
        },
      },
      { label: t("Send text or a link…"), icon: MessageSquareText, onSelect: () => setTextFor(p) },
      { label: tf("Download on {peer}…", { peer: p.alias }), icon: Download, onSelect: () => setRemoteFor(p) },
      "sep",
      {
        label: p.trusted ? t("Stop trusting this device") : t("Trust this device"),
        icon: ShieldCheck,
        onSelect: () => void airApi.trust(p.fingerprint, !p.trusted).catch((e) => toast({ level: "error", title: "KuAirSend", message: errorText(e) })),
      },
    ]);
  };

  const on = !!status?.running;
  const history = useMemo(() => transfers, [transfers]);

  return (
    <div className="main">
      <div className="toolbar">
        <span className="toolbar-title">KuAirSend</span>
        <span className="spacer" />
        {on && (
          <>
            <Button size="sm" icon={Plus} onClick={() => setAdding(true)}>
              {t("Add by IP")}
            </Button>
            <Button size="sm" icon={RefreshCw} onClick={() => void airApi.refresh()}>
              {t("Refresh")}
            </Button>
          </>
        )}
        {/* One control: the whole pill toggles, and says plainly whether you are visible. */}
        <button type="button" role="switch" aria-checked={on} className="air-toggle" disabled={busy || !status} onClick={() => void toggle(!on)} title={t("Send & receive")}>
          <span className="air-toggle-dot" aria-hidden="true" />
          <span className="air-toggle-label">{t("Send & receive")}</span>
          <span className="air-toggle-state">{on ? t("On") : t("Off")}</span>
          <span className="switch" aria-hidden="true" aria-checked={on} />
        </button>
      </div>
      <div className="page">
        <div className="page-inner air-page">
          {error && <div className="air-error">{error}</div>}
          {status && !on && (
            <div className="card air-off">
              <PixelAnimal animal={status.avatar} size={96} seed={hashPick(status.fingerprint, 97)} />
              <h2>{t("Share files with nearby computers")}</h2>
              <p className="faint">
                {t("Send files, folders, text and links between KuDownloader on your computers — over your own Wi-Fi or network at full speed, encrypted, with no cloud and no size limit.")}
              </p>
              <Button variant="primary" busy={busy} onClick={() => void toggle(true)}>
                {t("Turn on KuAirSend")}
              </Button>
              <span className="faint air-small">{t("Your system may ask to allow KuDownloader on private networks: allow it so other devices can find you.")}</span>
            </div>
          )}
          {status && on && (
            <>
              <Radar status={status} peers={peers} transfers={transfers} onPeer={peerMenu} />
              <div className="air-me-line faint">
                {t("Visible as")} <b>{status.alias}</b>
                {status.addresses.length > 0 && <span className="mono"> · {status.addresses.map((a) => `${a}:${status.port}`).join(", ")}</span>}
                {s?.airsendPin ? ` · ${t("PIN on")}` : ""}
              </div>
              {status.discoveryError && <div className="air-error">{status.discoveryError}</div>}
              <div className="air-section">
                <span className="section-title">{t("Transfers")}</span>
                {history.some(isFinished) && (
                  <Button size="sm" icon={Trash2} className="is-danger" onClick={() => void airApi.clearHistory().then(reloadTransfers)}>
                    {t("Clear history")}
                  </Button>
                )}
              </div>
              {history.length === 0 ? (
                <div className="faint air-empty">{t("Click a device to send it files, a folder, or a link.")}</div>
              ) : (
                <div className="card air-list">
                  {history.map((x) => (
                    <TransferRow key={x.id} x={x} peers={peers} />
                  ))}
                </div>
              )}
              <div className="air-section">
                <span className="section-title">{t("Settings")}</span>
              </div>
              <AirSettings status={status} />
            </>
          )}
        </div>
      </div>
      {textFor && <TextDialog peer={textFor} onClose={() => setTextFor(null)} />}
      {remoteFor && <RemoteDownloadDialog peer={remoteFor} onClose={() => setRemoteFor(null)} />}
      {adding && <AddDialog onClose={() => setAdding(false)} />}
      {pinFor && (
        <PinDialog
          peer={pinFor.peer}
          onClose={() => setPinFor(null)}
          onSend={(pin) => {
            const again = sent.get(pinFor.id);
            const peer = peers.find((p) => p.fingerprint === pinFor.peerFingerprint);
            if (again && peer) void send(peer, again.paths, again.text, pin);
            setPinFor(null);
          }}
        />
      )}
    </div>
  );
}
