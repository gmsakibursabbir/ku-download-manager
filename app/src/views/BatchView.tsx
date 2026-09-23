import { useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Folder, Wand2 } from "lucide-react";
import { api, errorText } from "../lib/api";
import { queuesStore, settingsStore } from "../lib/store";
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
  const { navigate } = useApp();
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
        title: `${r.added.length} added${r.failed.length ? `, ${r.failed.length} failed` : ""}`,
        message: r.failed[0] ? `${fmt.host(r.failed[0].url)}: ${r.failed[0].error}` : undefined,
        actions: [{ label: "View", onClick: () => navigate(queueId ? "queue" : "downloads") }],
      });
      if (!r.failed.length) setText("");
    } catch (e) {
      toast({ level: "error", title: "Could not add the downloads", message: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="main">
      <div className="toolbar">
        <span className="toolbar-title">Batch Downloads</span>
      </div>
      <div className="page">
        <div className="page-inner">
          <p className="page-lede">Paste links one per line, or generate a numbered series from a pattern. Duplicates are skipped.</p>
          <div className="card" style={{ padding: "var(--space-3)", display: "flex", flexDirection: "column", gap: 8 }}>
            <div className="section-title">Generate from a pattern</div>
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
                Add {preview.length > 1 ? preview.length : ""} links
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
            {urls.length} link{urls.length === 1 ? "" : "s"}
            {invalid > 0 && <span style={{ color: "var(--danger)" }}> · {invalid} invalid line{invalid === 1 ? "" : "s"} will be ignored</span>}
          </div>
          <div className="form-grid">
            <label>Save to</label>
            <div className="input-group">
              <Input value={dir} onChange={(e) => setDir(e.target.value)} placeholder={`Automatic by category (${settings?.downloadDir ?? "Downloads"})`} />
              <IconButton
                icon={Folder}
                label="Choose folder"
                onClick={async () => {
                  const p = await open({ directory: true });
                  if (typeof p === "string") setDir(p);
                }}
              />
            </div>
            <label>Connections</label>
            <Select
              value={connections == null ? "" : String(connections)}
              onChange={(e) => setConnections(e.target.value === "" ? null : +e.target.value)}
              options={[{ value: "", label: "Default" }, { value: "0", label: "Smart" }, ...[1, 2, 4, 8, 16].map((n) => ({ value: String(n), label: String(n) }))]}
              style={{ width: 160 }}
            />
          </div>
          <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
            <Button ref={queueBtn} disabled={!urls.length || busy} onClick={() => showMenuAt(queueBtn.current!, queues.map((q) => ({ label: q.name, onSelect: () => void add(q.id) })))}>
              Add to queue
            </Button>
            <Button variant="primary" busy={busy} disabled={!urls.length} onClick={() => void add(null)}>
              Download {urls.length || ""}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
