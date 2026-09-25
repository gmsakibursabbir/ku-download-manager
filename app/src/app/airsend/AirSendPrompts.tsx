import { useState } from "react";
import { Copy, Download } from "lucide-react";
import { airApi, dismissDownload, dismissMessage, dismissRequest, dismissTrust, useAir, type AirDownloadRequest, type AirMessage, type AirRequest, type AirTrustRequest } from "../../lib/airsend";
import { errorText } from "../../lib/api";
import * as fmt from "../../lib/format";
import { t, tf } from "../../lib/i18n";
import { Button, Checkbox } from "../../ui/primitives";
import { Dialog, toast } from "../../ui/overlays";
import { useApp } from "../context";
import { OsBadge, PixelAnimal } from "./PixelAnimal";
import { hashPick } from "../../lib/airsend";

const isLink = (s: string) => /^(https?|ftp|magnet):\S+$/i.test(s.trim());

function RequestDialog({ r }: { r: AirRequest }) {
  const [trust, setTrust] = useState(false);
  const [busy, setBusy] = useState(false);
  const answer = async (accept: boolean) => {
    setBusy(true);
    try {
      await airApi.decide(r.id, accept, accept && trust);
    } catch (e) {
      toast({ level: "error", title: "KuAirSend", message: errorText(e) });
    } finally {
      dismissRequest(r.id);
    }
  };
  const shown = r.files.slice(0, 6);
  return (
    <Dialog
      title="KuAirSend"
      width={440}
      onClose={() => void answer(false)}
      footer={
        <>
          <Button className="is-danger" disabled={busy} onClick={() => void answer(false)}>
            {t("Decline")}
          </Button>
          <Button variant="primary" busy={busy} onClick={() => void answer(true)} data-autofocus>
            {t("Accept")}
          </Button>
        </>
      }
    >
      <div className="air-offer">
        <span className="air-peer-ring">
          <PixelAnimal animal={r.peerAvatar} size={64} seed={hashPick(r.peerFingerprint, 97)} />
          <OsBadge os={r.peerOs} />
        </span>
        <div className="air-offer-text">
          <b>{r.peer}</b>
          <span className="faint">
            {t("wants to send you {files}").replace("{files}", r.fileCount === 1 ? t("1 file") : t("{n} files").replace("{n}", String(r.fileCount)))} · {fmt.bytes(r.total)}
          </span>
        </div>
      </div>
      <ul className="air-offer-files">
        {shown.map((f, i) => (
          <li key={i}>
            <span className="truncate" title={f.name}>
              {f.name}
            </span>
            <span className="faint num">{fmt.bytes(f.size)}</span>
          </li>
        ))}
        {r.fileCount > shown.length && <li className="faint">{t("+{n} more").replace("{n}", String(r.fileCount - shown.length))}</li>}
      </ul>
      <Checkbox checked={trust} onChange={setTrust}>
        {t("Always accept from this device")}
      </Checkbox>
    </Dialog>
  );
}

function DownloadDialog({ r }: { r: AirDownloadRequest }) {
  const [trust, setTrust] = useState(false);
  const [busy, setBusy] = useState(false);
  const answer = async (accept: boolean) => {
    setBusy(true);
    try {
      await airApi.decide(r.id, accept, accept && trust);
    } catch (e) {
      toast({ level: "error", title: "KuAirSend", message: errorText(e) });
    } finally {
      dismissDownload(r.id);
    }
  };
  const d = r.download;
  const later = d.at && d.at > Date.now() + 30_000;
  return (
    <Dialog
      title="KuAirSend"
      width={460}
      onClose={() => void answer(false)}
      footer={
        <>
          <Button className="is-danger" disabled={busy} onClick={() => void answer(false)}>
            {t("Decline")}
          </Button>
          <Button variant="primary" icon={Download} busy={busy} onClick={() => void answer(true)} data-autofocus>
            {later ? t("Schedule") : t("Download")}
          </Button>
        </>
      }
    >
      <div className="air-offer">
        <span className="air-peer-ring">
          <PixelAnimal animal={r.peerAvatar} size={64} seed={hashPick(r.peerFingerprint, 97)} />
          <OsBadge os={r.peerOs} />
        </span>
        <div className="air-offer-text">
          <b>{r.peer}</b>
          <span className="faint">
            {later ? tf("wants this computer to download this at {time}", { time: fmt.dateTime(d.at!) }) : t("wants this computer to download this now")}
          </span>
        </div>
      </div>
      <div className="air-message mono">{d.filename ? `${d.filename}\n${d.url}` : d.url}</div>
      <Checkbox checked={trust} onChange={setTrust}>
        {t("Always accept from this device")}
      </Checkbox>
    </Dialog>
  );
}

/** A device now trusts this computer: trust it back, once, and nothing asks again. */
function TrustDialog({ r }: { r: AirTrustRequest }) {
  const [busy, setBusy] = useState(false);
  const answer = async (yes: boolean) => {
    setBusy(true);
    try {
      if (yes) await airApi.trust(r.peerFingerprint, true);
    } catch (e) {
      toast({ level: "error", title: "KuAirSend", message: errorText(e) });
    } finally {
      dismissTrust(r.peerFingerprint);
    }
  };
  return (
    <Dialog
      title="KuAirSend"
      width={440}
      onClose={() => void answer(false)}
      footer={
        <>
          <Button disabled={busy} onClick={() => void answer(false)}>
            {t("Not now")}
          </Button>
          <Button variant="primary" busy={busy} onClick={() => void answer(true)} data-autofocus>
            {t("Trust")}
          </Button>
        </>
      }
    >
      <div className="air-offer">
        <span className="air-peer-ring">
          <PixelAnimal animal={r.peerAvatar} size={64} seed={hashPick(r.peerFingerprint, 97)} />
          <OsBadge os={r.peerOs} />
        </span>
        <div className="air-offer-text">
          <b>{tf("{peer} trusts this computer", { peer: r.peer })}</b>
          <span className="faint">{tf("Trust {peer} too? Files, links and scheduled downloads between you will then go through without asking.", { peer: r.peer })}</span>
        </div>
      </div>
    </Dialog>
  );
}

function MessageDialog({ m }: { m: AirMessage }) {
  const app = useApp();
  const link = isLink(m.text);
  const close = () => dismissMessage(m.id);
  return (
    <Dialog
      title={t("Message from {peer}").replace("{peer}", m.peer)}
      width={460}
      onClose={close}
      footer={
        <>
          <Button
            icon={Copy}
            onClick={() => {
              void navigator.clipboard.writeText(m.text);
              toast({ level: "success", title: t("Copied") });
            }}
          >
            {t("Copy")}
          </Button>
          {link ? (
            <Button
              variant="primary"
              icon={Download}
              data-autofocus
              onClick={() => {
                close();
                app.openAdd({ url: m.text.trim() });
              }}
            >
              {t("Download")}
            </Button>
          ) : (
            <Button variant="primary" onClick={close} data-autofocus>
              {t("Close")}
            </Button>
          )}
        </>
      }
    >
      <div className="air-offer">
        <PixelAnimal animal={m.peerAvatar} size={48} seed={hashPick(m.peerFingerprint, 97)} />
        <div className={`air-message ${link ? "mono" : ""}`}>{m.text}</div>
      </div>
    </Dialog>
  );
}

/** Incoming offers and messages, over whatever screen is open (one at a time). */
export function AirSendPrompts() {
  const { requests, downloads, trusts, messages } = useAir();
  if (requests[0]) return <RequestDialog key={requests[0].id} r={requests[0]} />;
  if (downloads[0]) return <DownloadDialog key={downloads[0].id} r={downloads[0]} />;
  if (trusts[0]) return <TrustDialog key={trusts[0].peerFingerprint} r={trusts[0]} />;
  if (messages[0]) return <MessageDialog key={messages[0].id} m={messages[0]} />;
  return null;
}
