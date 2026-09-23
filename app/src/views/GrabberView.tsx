import { useEffect, useMemo, useRef, useState } from "react";
import { CircleAlert, Link2, Search, Folder } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, errorText } from "../lib/api";
import { settingsStore, queuesStore } from "../lib/store";
import * as fmt from "../lib/format";
import type { GrabLink } from "../lib/types";
import { Button, Checkbox, EmptyState, Icon, IconButton, Input, Notice } from "../ui/primitives";
import { showMenuAt, toast } from "../ui/overlays";
import { useApp } from "../app/context";

const PAGE_EXT = new Set(["", "html", "htm", "php", "asp", "aspx", "jsp", "cgi", "shtml"]);

function nameOf(l: GrabLink) {
  try {
    const u = new URL(l.url);
    const seg = decodeURIComponent(u.pathname.split("/").filter(Boolean).pop() ?? "");
    return seg || u.host;
  } catch {
    return l.url;
  }
}

export function GrabberView() {
  const { grabPrefill, navigate } = useApp();
  const settings = settingsStore.use();
  const queues = queuesStore.use();
  const [pageUrl, setPageUrl] = useState(grabPrefill?.pageUrl ?? "");
  const [links, setLinks] = useState<GrabLink[]>(grabPrefill?.links ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [filesOnly, setFilesOnly] = useState(true);
  const [types, setTypes] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [dir, setDir] = useState("");
  const [busy, setBusy] = useState(false);
  const queueBtn = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (grabPrefill) {
      setPageUrl(grabPrefill.pageUrl ?? "");
      setLinks(grabPrefill.links);
      setSelected(new Set());
      setTypes(new Set());
    }
  }, [grabPrefill]);

  const fetchPage = async () => {
    setLoading(true);
    setError(null);
    try {
      const l = await api.grabPage(pageUrl.trim());
      setLinks(l);
      setSelected(new Set());
      setTypes(new Set());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  };

  const extOf = (l: GrabLink) => (l.kind ?? "").toLowerCase();
  const allTypes = useMemo(() => {
    const counts = new Map<string, number>();
    for (const l of links) {
      const e = extOf(l);
      if (!PAGE_EXT.has(e)) counts.set(e, (counts.get(e) ?? 0) + 1);
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 16);
  }, [links]);

  const visible = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return links.filter((l) => {
      const e = extOf(l);
      if (filesOnly && PAGE_EXT.has(e) && !l.url.startsWith("magnet:")) return false;
      if (types.size && !types.has(e)) return false;
      return !q || l.url.toLowerCase().includes(q) || (l.text ?? "").toLowerCase().includes(q);
    });
  }, [links, filter, filesOnly, types]);

  const allSelected = visible.length > 0 && visible.every((l) => selected.has(l.url));
  const chosen = visible.filter((l) => selected.has(l.url)).map((l) => l.url);

  const add = async (queueId: string | null) => {
    setBusy(true);
    try {
      const r = await api.addBatch(chosen, {
        url: "",
        dir: dir.trim() || null,
        queueId,
        source: grabPrefill ? "browser" : "grabber",
        options: { referer: pageUrl || grabPrefill?.referer || null, cookies: grabPrefill?.cookies ?? [], userAgent: grabPrefill?.userAgent ?? null, headers: [] },
      });
      toast({
        level: r.failed.length ? "warning" : "success",
        title: `${r.added.length} added${r.failed.length ? `, ${r.failed.length} skipped` : ""}`,
        message: r.failed[0] ? `${fmt.host(r.failed[0].url)}: ${r.failed[0].error}` : undefined,
        actions: [{ label: "View", onClick: () => navigate(queueId ? "queue" : "downloads") }],
      });
      setSelected(new Set());
    } catch (e) {
      toast({ level: "error", title: "Could not add the downloads", message: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="main">
      <div className="toolbar">
        <span className="toolbar-title">URL Grabber</span>
      </div>
      <div className="page" style={{ display: "flex", flexDirection: "column" }}>
        <div className="page-inner wide" style={{ flex: 1, width: "100%" }}>
          <form
            className="input-group"
            onSubmit={(e) => {
              e.preventDefault();
              void fetchPage();
            }}
          >
            <div className="input-with-icon" style={{ flex: 1 }}>
              <Icon icon={Link2} size={14} />
              <Input value={pageUrl} onChange={(e) => setPageUrl(e.target.value)} placeholder="Page address — or use “Download all links” in the browser extension" />
            </div>
            <Button type="submit" variant="primary" busy={loading} disabled={!/^https?:\/\//i.test(pageUrl.trim())}>
              Find links
            </Button>
          </form>
          {error && (
            <Notice level="error" icon={CircleAlert}>
              {error}
            </Notice>
          )}
          {links.length > 0 && (
            <>
              <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
                <div className="input-with-icon" style={{ width: 240 }}>
                  <Icon icon={Search} size={14} />
                  <Input value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filter" style={{ height: 28 }} />
                </div>
                <Checkbox checked={filesOnly} onChange={setFilesOnly}>
                  Files only
                </Checkbox>
                <span className="toolbar-sep" />
                {allTypes.map(([t, n]) => (
                  <button
                    key={t}
                    type="button"
                    className={`chip ${types.has(t) ? "chip-accent" : ""}`}
                    style={{ border: "none", height: 22 }}
                    onClick={() => {
                      const s = new Set(types);
                      if (s.has(t)) s.delete(t);
                      else s.add(t);
                      setTypes(s);
                    }}
                  >
                    {t.toUpperCase()} <span className="faint">{n}</span>
                  </button>
                ))}
              </div>
              <div className="card" style={{ flex: 1, minHeight: 240, maxHeight: "calc(100vh - 330px)", overflow: "auto" }}>
                {visible.length ? (
                  <table className="table">
                    <thead>
                      <tr>
                        <th style={{ width: 36 }}>
                          <Checkbox
                            checked={allSelected}
                            indeterminate={!allSelected && chosen.length > 0}
                            onChange={(v) => {
                              const s = new Set(selected);
                              for (const l of visible) v ? s.add(l.url) : s.delete(l.url);
                              setSelected(s);
                            }}
                          />
                        </th>
                        <th>Name</th>
                        <th style={{ width: 70 }}>Type</th>
                        <th style={{ width: 200 }}>Host</th>
                      </tr>
                    </thead>
                    <tbody>
                      {visible.slice(0, 2000).map((l) => (
                        <tr
                          key={l.url}
                          aria-selected={selected.has(l.url)}
                          onClick={() => {
                            const s = new Set(selected);
                            if (s.has(l.url)) s.delete(l.url);
                            else s.add(l.url);
                            setSelected(s);
                          }}
                        >
                          <td>
                            <Checkbox checked={selected.has(l.url)} onChange={() => {}} />
                          </td>
                          <td title={l.url}>
                            <div className="truncate" style={{ maxWidth: 560 }}>
                              {nameOf(l)}
                            </div>
                            {l.text && l.text !== nameOf(l) && (
                              <div className="faint truncate" style={{ fontSize: "var(--text-xs)", maxWidth: 560 }}>
                                {l.text}
                              </div>
                            )}
                          </td>
                          <td className="muted">{extOf(l).toUpperCase()}</td>
                          <td className="faint truncate" style={{ maxWidth: 200 }}>
                            {fmt.host(l.url)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                ) : (
                  <EmptyState title="No links match" text="Change the filter or show pages too." />
                )}
              </div>
              <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                <div className="input-group" style={{ width: 420 }}>
                  <Input value={dir} onChange={(e) => setDir(e.target.value)} placeholder={`Automatic (${settings?.downloadDir ?? "Downloads"})`} />
                  <IconButton
                    icon={Folder}
                    label="Choose folder"
                    onClick={async () => {
                      const p = await open({ directory: true });
                      if (typeof p === "string") setDir(p);
                    }}
                  />
                </div>
                <span className="spacer" style={{ flex: 1 }} />
                <span className="faint num" style={{ fontSize: "var(--text-sm)" }}>
                  {chosen.length} of {visible.length} selected
                </span>
                <Button ref={queueBtn} disabled={!chosen.length || busy} onClick={() => showMenuAt(queueBtn.current!, queues.map((q) => ({ label: q.name, onSelect: () => void add(q.id) })))}>
                  Add to queue
                </Button>
                <Button variant="primary" busy={busy} disabled={!chosen.length} onClick={() => void add(null)}>
                  Download {chosen.length || ""}
                </Button>
              </div>
            </>
          )}
          {!links.length && !loading && !error && (
            <div className="faint" style={{ fontSize: "var(--text-sm)" }}>
              KuDownloader reads the page once and lists the files it links to. Nothing is downloaded until you choose.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
