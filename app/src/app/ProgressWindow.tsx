import { te } from "../lib/engineText";
import { useEffect, useLayoutEffect, useMemo, useState } from "react";
import { t } from "../lib/i18n";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { ArrowDownToLine, ChevronDown, ChevronRight, Minus, X } from "lucide-react";
import { api, errorText } from "../lib/api";
import { settingsStore } from "../lib/store";
import { applyAppearance } from "../lib/appearance";
import * as fmt from "../lib/format";
import type { CoreEvent, Details, Download } from "../lib/types";
import { Button, Icon, IconButton, Progress } from "../ui/primitives";

const ACTIVE = new Set(["downloading", "processing", "seeding"]);

/** Blocks for the map: aria2 piece bitfield or KuHTTP segment ranges → 0..1 fill per cell. */
function mapCells(details: Details | null, d: Download, cells = 120): number[] {
  if (details?.segments?.length && d.total > 0) {
    const out = new Array(cells).fill(0);
    for (const sg of details.segments) {
      const got = sg.done ? sg.end - sg.start : sg.downloaded;
      const from = sg.start;
      const to = sg.start + got;
      for (let i = 0; i < cells; i++) {
        const a = (i / cells) * d.total;
        const b = ((i + 1) / cells) * d.total;
        const overlap = Math.max(0, Math.min(b, to) - Math.max(a, from));
        out[i] = Math.min(1, out[i] + overlap / (b - a));
      }
    }
    return out;
  }
  const bf = details?.pieces?.bitfield;
  const count = details?.pieces?.count ?? 0;
  if (bf && count) {
    const bits: number[] = [];
    for (const ch of bf) {
      const n = parseInt(ch, 16);
      for (let b = 3; b >= 0 && bits.length < count; b--) bits.push((n >> b) & 1);
    }
    return Array.from({ length: Math.min(cells, count) }, (_, i) => {
      const per = count / Math.min(cells, count);
      const slice = bits.slice(Math.floor(i * per), Math.floor((i + 1) * per) || Math.floor(i * per) + 1);
      return slice.reduce((a, b) => a + b, 0) / slice.length;
    });
  }
  const pct = d.total > 0 ? d.done / d.total : 0;
  return Array.from({ length: cells }, (_, i) => (i + 1) / cells <= pct ? 1 : 0);
}

/** IDM-style window for one download: status, speed, time left, live segment map. */
export function ProgressWindow({ id }: { id: string }) {
  const win = useMemo(() => getCurrentWindow(), []);
  const settings = settingsStore.use();
  const [d, setD] = useState<Download | null>(null);
  const [details, setDetails] = useState<Details | null>(null);
  const [more, setMore] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => void applyAppearance(settings), [settings]);

  // Initial state + live updates for this download only.
  useEffect(() => {
    void invoke<Download | null>("get_download", { id }).then((x) => (x ? setD(x) : void win.close()));
    const un = listen<CoreEvent>("ku", ({ payload: e }) => {
      if (e.type === "upsert" && e.download.id === id) setD(e.download);
      else if (e.type === "removed" && e.ids.includes(id)) void win.close();
      else if (e.type === "progress") {
        const p = e.items.find((i) => i.id === id);
        if (p) setD((old) => (old ? { ...old, ...p } : old));
      }
    });
    return () => void un.then((f) => f());
  }, [id, win]);

  const live = !!d && ACTIVE.has(d.status);
  useEffect(() => {
    if (!live || !more) return;
    let alive = true;
    const load = () => void api.details(id).then((x) => alive && setDetails(x)).catch(() => {});
    load();
    const t = setInterval(load, 1000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, [id, live, more]);

  // Fit the window to its content when it opens and when the content changes
  // (details shown/hidden, finished, error). In between the user can resize
  // freely: the details area scrolls and the buttons stay pinned at the bottom.
  const [shown, setShown] = useState(false);
  const finished = d?.status === "completed" || d?.status === "seeding";
  const hasConns = (details?.segments?.length || details?.connections?.length || 0) > 0;
  useLayoutEffect(() => {
    if (!d) return;
    const el = document.querySelector<HTMLElement>(".pw");
    const body = el?.querySelector<HTMLElement>(".pw-body");
    if (!el || !body) return;
    const natural = el.offsetHeight - body.clientHeight + body.scrollHeight;
    // Grow to fit, but not past a comfortable height: the rest scrolls.
    void win.setSize(new LogicalSize(window.innerWidth, Math.min(natural + 2, 620, screen.availHeight - 80))).then(() => {
      if (!shown) {
        setShown(true);
        void win.show().then(() => win.setFocus());
      }
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [!!d, win, more, finished, !!error, hasConns]);

  if (!d) return null;
  const pct = fmt.percent(d.done, d.total);
  const complete = d.status === "completed" || d.status === "seeding";
  const eta = d.speed > 0 && d.total > 0 ? (d.total - d.done) / d.speed : null;
  const cells = mapCells(details, d);
  const conns = details?.segments
    ? details.segments.filter((s) => !s.done).map((s) => ({ label: `${fmt.bytes(s.start)} – ${fmt.bytes(s.end)}`, speed: s.speed, pct: fmt.percent(s.downloaded, s.end - s.start) }))
    : (details?.connections ?? []).map((c) => ({ label: c.currentUri || c.uri, speed: c.speed, pct: -1 }));
  const maxSpeed = Math.max(1, ...conns.map((c) => c.speed));
  const act = (p: Promise<unknown>) => void p.catch((e) => setError(errorText(e)));
  const status = complete ? "Download complete" : d.status === "error" ? "Failed" : d.status === "paused" ? "Paused" : d.status === "queued" ? "Queued" : d.status === "processing" ? "Merging…" : "Receiving data…";

  return (
    <div className="pw">
      <header className="pw-head" data-tauri-drag-region>
        <Icon icon={ArrowDownToLine} />
        <span className="pw-title truncate" data-tauri-drag-region>
          {complete ? t("Download complete") : `${d.done > 0 && d.total > 0 ? `${Math.floor(pct)}% ` : ""}${d.name}`}
        </span>
        <IconButton icon={Minus} label={t("Minimize")} size="sm" onClick={() => void win.minimize()} />
        <IconButton icon={X} label={t("Close")} size="sm" onClick={() => void win.close()} />
      </header>

      <div className="pw-body">
        <div className="pw-facts">
          <span>{t("File")}</span>
          <b className="truncate" title={d.name}>
            {d.name}
          </b>
          <span>{t("Address")}</span>
          <span className="truncate faint" title={d.url}>
            {d.url}
          </span>
          <span>{t("Status")}</span>
          <span style={{ color: d.status === "error" ? "var(--danger)" : undefined }}>{d.status === "error" ? te(d.error) || t(status) : t(status)}</span>
          <span>{t("File size")}</span>
          <span className="num">{d.total > 0 ? fmt.bytes(d.total) : t("Unknown")}</span>
          <span>{t("Downloaded")}</span>
          <span className="num">
            {fmt.bytes(d.done)}
            {d.total > 0 ? ` (${pct.toFixed(2)} %)` : ""}
          </span>
          {!complete && (
            <>
              <span>{t("Transfer rate")}</span>
              <span className="num">{live ? fmt.speed(d.speed) : "—"}</span>
              <span>{t("Time left")}</span>
              <span className="num">{eta != null ? fmt.duration(eta) : "—"}</span>
              <span>{t("Resume")}</span>
              <span>{d.meta.resumable === false ? t("No — pausing restarts the file") : d.meta.resumable ? t("Yes") : t("Unknown")}</span>
            </>
          )}
        </div>

        <Progress value={complete ? 100 : pct} state={complete ? "completed" : d.status === "error" ? "error" : d.status === "paused" ? "paused" : "downloading"} indeterminate={live && d.total <= 0} />

        {!complete && (
          <>
            <button type="button" className="dl-more" onClick={() => setMore(!more)} aria-expanded={more}>
              <Icon icon={more ? ChevronDown : ChevronRight} size={14} /> {more ? t("Hide details") : t("Show details")}
            </button>
            {more && (
              <>
                <div className="pw-map" aria-label={t("Downloaded parts of the file")} style={{ gridTemplateColumns: `repeat(${cells.length}, 1fr)` }}>
                  {cells.map((v, i) => (
                    <span key={i} style={{ opacity: v > 0 ? 0.35 + v * 0.65 : 1 }} data-have={v > 0} />
                  ))}
                </div>
                {conns.length > 0 && (
                  <div className="pw-conns num">
                    {conns.slice(0, 16).map((c, i) => (
                      <div key={i} className="conn-row" title={c.label}>
                        <span className="faint">
                          {d.engine === "kuhttp" ? t("Part") : t("Conn")} {String(i + 1).padStart(2, "0")}
                        </span>
                        <Progress value={c.pct >= 0 ? c.pct : (c.speed / maxSpeed) * 100} state="downloading" />
                        <span style={{ textAlign: "right" }}>{fmt.speed(c.speed)}</span>
                      </div>
                    ))}
                  </div>
                )}
              </>
            )}
          </>
        )}
        {error && <div style={{ color: "var(--danger)", fontSize: "var(--text-xs)" }}>{error}</div>}
      </div>

      <footer className="dl-actions pw-actions">
        {complete ? (
          <>
            <Button variant="primary" onClick={() => act(api.openFile(d.id).then(() => win.close()))}>
              {t("Open")}
            </Button>
            <Button onClick={() => act(api.openFolder(d.id))}>{t("Open folder")}</Button>
            <Button onClick={() => void win.close()}>{t("Close")}</Button>
          </>
        ) : (
          <>
            {live || d.status === "queued" ? (
              <Button onClick={() => act(api.pause([d.id]))}>{t("Pause")}</Button>
            ) : (
              <Button variant="primary" onClick={() => act(api.resume([d.id]))}>
                {d.status === "error" ? t("Retry") : t("Resume")}
              </Button>
            )}
            {/* Like IDM: Cancel stops the download and keeps it in the list (resumable); deleting is done from the list. */}
            <Button className="is-danger" title={t("Stops the download and keeps it in your list, so you can resume it later.")} onClick={() => act((live || d.status === "queued" ? api.pause([d.id]) : Promise.resolve()).then(() => win.close()))}>
              {t("Cancel download")}
            </Button>
            <Button onClick={() => void win.close()}>{t("Hide")}</Button>
          </>
        )}
      </footer>
    </div>
  );
}
