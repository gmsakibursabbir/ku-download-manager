import { t, tf } from "../lib/i18n";
import { useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Folder, Wand2 } from "lucide-react";
import { api, errorText } from "../lib/api";
import { queuesStore, settingsStore, queueName } from "../lib/store";
import * as fmt from "../lib/format";
import { Button, IconButton, Input, Select } from "../ui/primitives";
import { showMenuAt, toast } from "../ui/overlays";
import { useApp } from "../app/context";

/** Expand `[001-120]`, `[a-z]` ranges (first one per pass, recursively). */
export function expandPattern(p: string, limit = 5000): string[] {
  const m = p.match(/\[(\d+)-(\d+)(?::(\d+))?\]|\[([a-z])-([a-z])\]/i);
  if (!m) return [p];
  const out: string[] = [];
  const [before, after] = [p.slice(0, m.index), p.slice(m.index! + m[0].length)];
  if (m[1] != null) {
    const width = m[1].length;
    const a = parseInt(m[1], 10);
    const b = parseInt(m[2], 10);
    const step = Math.max(1, parseInt(m[3] ?? "1", 10));
    for (let i = a; a <= b ? i <= b : i >= b; i += a <= b ? step : -step) {
      for (const rest of expandPattern(after, limit)) {
        out.push(before + String(i).padStart(width, "0") + rest);
        if (out.length >= limit) return out;
      }
    }
  } else {
    const a = m[4].charCodeAt(0);
    const b = m[5].charCodeAt(0);
    for (let c = Math.min(a, b); c <= Math.max(a, b); c++) {
      for (const rest of expandPattern(after, limit)) {
        out.push(before + String.fromCharCode(c) + rest);
        if (out.length >= limit) return out;
      }
    }
  }
  return out;
}

export function BatchView() {
  const { showList } = useApp();
  const settings = settingsStore.use();
  const queues = queuesStore.use();
  const [text, setText] = useState("");
  const [pattern, setPattern] = useState("");
  const [dir, setDir] = useState("");
  const [connections, setConnections] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const queueBtn = useRef<HTMLButtonElement>(null);

  const urls = useMemo(
    () => [
      ...new Set(
        text
          .split(/\r?\n/)
          .map((l) => l.trim())
          .filter((l) => /^(https?|ftp|sftp):\/\/\S+$|^magnet:\?\S+$/i.test(l)),
      ),
    ],
    [text],
  );
  const invalid = text.split(/\r?\n/).filter((l) => l.trim() && !l.trim().startsWith("#")).length - urls.length;
  const preview = pattern.trim() ? expandPattern(pattern.trim(), 5000) : [];

  const add = async (queueId: string | null) => {
    setBusy(true);
    try {
      const r = await api.addBatch(urls, { url: "", dir: dir.trim() || null, queueId, connections, source: "batch" });
      toast({
        level: r.failed.length ? "warning" : "success",
        title: r.failed.length ? tf("{added} added, {failed} failed", { added: r.added.length, failed: r.failed.length }) : tf("{n} added", { n: r.added.length }),
        message: r.failed[0] ? `${fmt.host(r.failed[0].url)}: ${r.failed[0].error}` : undefined,
        actions: [{ label: t("View"), onClick: () => showList(queueId ? { scope: "queue", queueId } : { scope: "all" }) }],
      });
      if (!r.failed.length) setText("");
    } catch (e) {
      toast({ level: "error", title: t("Could not add the downloads"), message: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="main">
      <div className="toolbar">
        <span className="toolbar-title">{t("Batch Downloads")}</span>
      </div>
      <div className="page">
        <div className="page-inner">
          <p className="page-lede">{t("Paste links one per line, or generate a numbered series from a pattern. Duplicates are skipped.")}</p>
          <div className="card" style={{ padding: "var(--space-3)", display: "flex", flexDirection: "column", gap: 8 }}>
            <div className="section-title">{t("Generate from a pattern")}</div>
            <div className="input-group">
              <Input className="mono" value={pattern} onChange={(e) => setPattern(e.target.value)} placeholder="https://example.com/photos/img[001-120].jpg   ·   [a-z] and [1-100:5] work too" />
              <Button
                icon={Wand2}
                disabled={preview.length === 0 || (preview.length === 1 && preview[0] === pattern.trim())}
                onClick={() => {
                  setText((t) => (t.trim() ? `${t.trim()}\n` : "") + preview.join("\n"));
                  setPattern("");
                }}
              >
                {preview.length > 1 ? tf("Add {n} links", { n: preview.length }) : t("Add links")}
              </Button>
            </div>
            {preview.length > 1 && (
              <div className="faint mono truncate" style={{ fontSize: "var(--text-2xs)" }}>
                {preview[0]} … {preview[preview.length - 1]}
              </div>
            )}
          </div>
          <textarea className="textarea" rows={14} value={text} onChange={(e) => setText(e.target.value)} placeholder={"https://example.com/file1.zip\nhttps://example.com/file2.zip\nmagnet:?xt=urn:btih:…"} spellCheck={false} />
          <div className="faint num" style={{ fontSize: "var(--text-xs)" }}>
            {urls.length === 1 ? t("1 link") : tf("{n} links", { n: urls.length })}
            {invalid > 0 && <span style={{ color: "var(--danger)" }}> · {invalid === 1 ? t("1 invalid line will be ignored") : tf("{n} invalid lines will be ignored", { n: invalid })}</span>}
          </div>
          <div className="form-grid">
            <label>{t("Save to")}</label>
            <div className="input-group">
              <Input value={dir} onChange={(e) => setDir(e.target.value)} placeholder={tf("Automatic by category ({folder})", { folder: settings?.downloadDir ?? "Downloads" })} />
              <IconButton
                icon={Folder}
                label={t("Choose folder")}
                onClick={async () => {
                  const p = await open({ directory: true });
                  if (typeof p === "string") setDir(p);
                }}
              />
            </div>
            <label>{t("Connections")}</label>
            <Select
              value={connections == null ? "" : String(connections)}
              onChange={(e) => setConnections(e.target.value === "" ? null : +e.target.value)}
              options={[{ value: "", label: t("Default") }, { value: "0", label: t("Smart") }, ...[1, 2, 4, 8, 16].map((n) => ({ value: String(n), label: String(n) }))]}
              style={{ width: 160 }}
            />
          </div>
          <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
            <Button ref={queueBtn} disabled={!urls.length || busy} onClick={() => showMenuAt(queueBtn.current!, queues.map((q) => ({ label: queueName(q), onSelect: () => void add(q.id) })))}>
              {t("Add to queue")}
            </Button>
            <Button variant="primary" busy={busy} disabled={!urls.length} onClick={() => void add(null)}>
              {t("Download")}{' '}{urls.length || ""}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
