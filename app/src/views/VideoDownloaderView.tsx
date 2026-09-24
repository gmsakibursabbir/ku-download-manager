import { AUDIO_FORMATS } from "../app/prefs";
import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { CircleAlert, Folder, Search, TriangleAlert, ListVideo, Radio as RadioIcon, X } from "lucide-react";
import { api, errorText } from "../lib/api";
import { settingsStore, queuesStore } from "../lib/store";
import * as fmt from "../lib/format";
import type { EngineInfo, MediaInfo, MediaOptions } from "../lib/types";
import { MediaToolsNotice } from "../app/MediaTools";
import { Button, Checkbox, Icon, IconButton, Input, Notice, Radio, Segmented, Select } from "../ui/primitives";
import { showMenuAt, toast } from "../ui/overlays";
import { useApp } from "../app/context";

function pickDefault(info: MediaInfo, preferred: number): number | null {
  const h = info.video.map((v) => v.height);
  if (!h.length) return null;
  return h.find((x) => x <= preferred) ?? h[h.length - 1];
}

export function VideoDownloaderView() {
  const { mediaPrefill, showList } = useApp();
  const settings = settingsStore.use();
  const queues = queuesStore.use();
  const [url, setUrl] = useState(mediaPrefill?.url ?? "");
  const [playlist, setPlaylist] = useState(false);
  const [info, setInfo] = useState<MediaInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<"video" | "audio">("video");
  const [height, setHeight] = useState<number | null>(null);
  const [bitrate, setBitrate] = useState<number>(settings?.audioBitrate ?? 320);
  const [container, setContainer] = useState(settings?.videoContainer ?? "mp4");
  const [audioFormat, setAudioFormat] = useState(settings?.audioFormat ?? "m4a");
  const [subs, setSubs] = useState(settings?.subtitles ?? false);
  const [subLangs, setSubLangs] = useState<Set<string>>(new Set((settings?.subLangs ?? "en").split(/[,\s]+/).filter(Boolean)));
  const [embedThumb, setEmbedThumb] = useState(settings?.embedThumbnail ?? false);
  const [items, setItems] = useState<Set<number>>(new Set());
  const [dir, setDir] = useState("");
  const [busy, setBusy] = useState(false);
  const queueBtn = useRef<HTMLButtonElement>(null);
  const [engines, setEngines] = useState<EngineInfo | null>(null);
  const loadEngines = () => void api.engineInfo().then(setEngines).catch(() => {});
  useEffect(loadEngines, []);
  // Bumped on every analyze/cancel; stale responses are ignored.
  const reqId = useRef(0);
  const urlRef = useRef<HTMLInputElement>(null);

  /** Stop a running lookup (the result is discarded when it arrives). */
  const cancel = () => {
    reqId.current++;
    setLoading(false);
  };
  /** Clear the link and everything derived from it. */
  const clear = () => {
    cancel();
    setUrl("");
    setInfo(null);
    setError(null);
    setPlaylist(false);
    urlRef.current?.focus();
  };
  const cookies = mediaPrefill?.cookies ?? [];

  useEffect(() => {
    if (settings && !dir) setDir(settings.videoDir);
  }, [settings, dir]);

  const analyze = async (u = url, pl = playlist) => {
    const target = u.trim();
    if (!/^https?:\/\//i.test(target)) {
      setError("Enter the address of a video or playlist page (http or https).");
      return;
    }
    const id = ++reqId.current;
    setLoading(true);
    setError(null);
    setInfo(null);
    try {
      const r = await api.analyze(target, pl);
      if (id !== reqId.current) return;
      setInfo(r);
      setHeight(pickDefault(r, settings?.videoHeight ?? 1080));
      setItems(new Set(r.entries.map((e) => e.index)));
      if (!r.video.length && r.audio.length) setMode("audio");
    } catch (e) {
      if (id === reqId.current) setError(errorText(e));
    } finally {
      if (id === reqId.current) setLoading(false);
    }
  };

  // Auto-analyze links handed over from the browser or the Add dialog.
  useEffect(() => {
    if (mediaPrefill?.url) {
      setUrl(mediaPrefill.url);
      const pl = /[?&]list=/.test(mediaPrefill.url) && !/[?&]v=/.test(mediaPrefill.url);
      setPlaylist(pl);
      void analyze(mediaPrefill.url, pl);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mediaPrefill]);

  const hasListParam = /[?&]list=/.test(url);
  const allLangs = useMemo(() => {
    if (!info) return [];
    const manual = info.subtitles;
    const auto = info.autoSubtitles.filter((l) => !manual.includes(l) && l.length <= 3);
    return [...manual.map((l) => ({ l, auto: false })), ...auto.slice(0, 20).map((l) => ({ l, auto: true }))];
  }, [info]);

  const chosenSize = info
    ? mode === "video"
      ? info.video.find((v) => v.height === height)?.size
      : info.audio.find((a) => a.bitrate === bitrate)?.size ?? info.audio[0]?.size
    : null;

  const download = async (queueId: string | null) => {
    if (!info) return;
    const media: MediaOptions = {
      mode,
      height: mode === "video" ? height : null,
      audioBitrate: mode === "audio" ? bitrate : null,
      container: mode === "video" ? container : info.ffmpegAvailable ? audioFormat : null,
      subtitles: subs && mode === "video",
      subLangs: [...subLangs].join(","),
      embedSubtitles: subs,
      writeThumbnail: false,
      embedThumbnail: embedThumb,
      playlist: info.isPlaylist,
      playlistItems: info.isPlaylist && items.size < info.entries.length ? [...items].sort((a, b) => a - b).join(",") : null,
    };
    setBusy(true);
    try {
      await api.mediaDownload({
        url: info.isPlaylist ? info.webpageUrl || url : info.webpageUrl || url,
        dir: dir || null,
        media,
        cookies,
        queueId,
        title: info.title,
        thumbnail: info.thumbnail,
        sizeHint: info.isPlaylist ? null : chosenSize ?? null,
        source: mediaPrefill?.source ?? "ui",
      });
      toast({ level: "success", title: queueId ? "Added to queue" : "Download started", message: info.title, actions: [{ label: "View", onClick: () => showList(queueId ? { scope: "queue", queueId } : { scope: "all" }) }] });
    } catch (e) {
      toast({ level: "error", title: "Could not start the download", message: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="main">
      <div className="toolbar">
        <span className="toolbar-title">Video Downloader</span>
      </div>
      <div className="page">
        <div className="page-inner">
          <MediaToolsNotice info={engines} onInstalled={loadEngines} />
          <form
            className="input-group"
            onSubmit={(e) => {
              e.preventDefault();
              void analyze();
            }}
          >
            <div className="input-with-icon" style={{ flex: 1 }}>
              <Icon icon={Search} size={14} />
              <Input
                ref={urlRef}
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                onKeyDown={(e) => e.key === "Escape" && (loading ? cancel() : clear())}
                placeholder="Paste a video, channel or playlist link"
                style={{ paddingRight: 34 }}
                autoFocus
              />
              {(url || info || error) && <IconButton icon={X} label="Clear link" size="sm" onClick={clear} style={{ position: "absolute", right: 4 }} />}
            </div>
            {loading ? (
              <Button onClick={cancel}>Cancel</Button>
            ) : (
              <Button type="submit" variant="primary">
                Analyze
              </Button>
            )}
          </form>
          {hasListParam && (
            <Checkbox
              checked={playlist}
              onChange={(v) => {
                setPlaylist(v);
                if (info) void analyze(url, v);
              }}
            >
              This link belongs to a playlist — download the whole playlist
            </Checkbox>
          )}

          {loading && (
            <div className="faint" style={{ display: "flex", gap: 8, alignItems: "center", fontSize: "var(--text-sm)" }}>
              <span className="spinner" /> Reading media information…
            </div>
          )}
          {error && (
            <Notice level="error" icon={CircleAlert} title="This link can't be downloaded">
              {error}
            </Notice>
          )}
          {!info && !loading && !error && (
            <div className="faint" style={{ fontSize: "var(--text-sm)" }}>
              Works with YouTube, Vimeo, Dailymotion and many other sites supported by yt-dlp. DRM-protected media is not supported.
            </div>
          )}

          {info && (
            <>
              <div style={{ display: "grid", gridTemplateColumns: "240px 1fr", gap: "var(--space-4)" }}>
                <div style={{ position: "relative" }}>
                  {info.thumbnail ? (
                    <img className="insp-thumb" src={info.thumbnail} alt="" referrerPolicy="no-referrer" />
                  ) : (
                    <div className="insp-thumb" style={{ display: "grid", placeItems: "center" }}>
                      <Icon icon={info.isPlaylist ? ListVideo : RadioIcon} size={24} />
                    </div>
                  )}
                  {info.duration ? (
                    <span className="chip num" style={{ position: "absolute", right: 8, bottom: 8, background: "rgba(0,0,0,.7)", color: "#fff" }}>
                      {fmt.clock(info.duration)}
                    </span>
                  ) : null}
                </div>
                <div style={{ minWidth: 0, display: "flex", flexDirection: "column", gap: 6 }}>
                  <div style={{ fontSize: "var(--text-lg)", fontWeight: 600, lineHeight: 1.3 }} className="selectable">
                    {info.title}
                  </div>
                  <div className="muted" style={{ fontSize: "var(--text-sm)" }}>
                    {[info.uploader, info.viewCount != null ? `${info.viewCount.toLocaleString()} views` : null, info.uploadDate ? `${info.uploadDate.slice(0, 4)}-${info.uploadDate.slice(4, 6)}-${info.uploadDate.slice(6)}` : null]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                  <div style={{ display: "flex", gap: 6 }}>
                    <span className="chip">{info.extractor}</span>
                    {info.isPlaylist && <span className="chip chip-accent">Playlist · {info.playlistCount} items</span>}
                    {info.isLive && <span className="chip">Live</span>}
                  </div>
                </div>
              </div>

              {!info.ffmpegAvailable && (
                <Notice level="warning" icon={TriangleAlert} title="FFmpeg not found">
                  Only single-file formats are available, which are often limited to lower quality. Download FFmpeg above (or in Settings › Advanced) for full quality and audio conversion, then analyze again.
                </Notice>
              )}

              <Segmented
                label="Download type"
                value={mode}
                onChange={setMode}
                options={[
                  { value: "video", label: "Video" },
                  { value: "audio", label: "Audio only" },
                ]}
              />

              <div className="card" style={{ overflow: "hidden" }}>
                {mode === "video" ? (
                  info.video.length ? (
                    <table className="table">
                      <thead>
                        <tr>
                          <th style={{ width: 36 }} />
                          <th>Quality</th>
                          <th>Format</th>
                          <th style={{ textAlign: "right" }}>Size</th>
                        </tr>
                      </thead>
                      <tbody>
                        {info.video.map((v) => (
                          <tr key={v.height} aria-selected={height === v.height} onClick={() => setHeight(v.height)}>
                            <td>
                              <Radio name="q" checked={height === v.height} onChange={() => setHeight(v.height)} />
                            </td>
                            <td>
                              {v.label}
                              {v.hdr ? <span className="chip" style={{ marginLeft: 6 }}>HDR</span> : null}
                              {v.height === pickDefault(info, settings?.videoHeight ?? 1080) && <span className="faint"> · default</span>}
                            </td>
                            <td className="muted">{[v.ext.toUpperCase(), v.vcodec, v.fps && v.fps > 30 ? `${Math.round(v.fps)} fps` : ""].filter(Boolean).join(" · ") || "Best available"}</td>
                            <td className="num muted" style={{ textAlign: "right" }}>
                              {v.size ? `≈ ${fmt.bytes(v.size)}` : "—"}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  ) : (
                    <div className="faint" style={{ padding: "var(--space-4)", fontSize: "var(--text-sm)" }}>
                      No video formats — this looks like audio only.
                    </div>
                  )
                ) : info.audio.length ? (
                  <table className="table">
                    <thead>
                      <tr>
                        <th style={{ width: 36 }} />
                        <th>Bitrate</th>
                        <th>Format</th>
                        <th style={{ textAlign: "right" }}>Size</th>
                      </tr>
                    </thead>
                    <tbody>
                      {info.audio.map((a) => (
                        <tr key={a.bitrate} aria-selected={bitrate === a.bitrate} onClick={() => setBitrate(a.bitrate)}>
                          <td>
                            <Radio name="a" checked={bitrate === a.bitrate} onChange={() => setBitrate(a.bitrate)} />
                          </td>
                          <td className="num">{a.bitrate} kbps</td>
                          <td className="muted">{info.ffmpegAvailable && audioFormat !== "best" ? audioFormat.toUpperCase() : `${a.ext.toUpperCase()} (original)`}</td>
                          <td className="num muted" style={{ textAlign: "right" }}>
                            {a.size ? `≈ ${fmt.bytes(a.size)}` : "—"}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                ) : (
                  <div className="faint" style={{ padding: "var(--space-4)", fontSize: "var(--text-sm)" }}>
                    No audio track found.
                  </div>
                )}
              </div>

              {info.isPlaylist && info.entries.length > 0 && (
                <div className="card" style={{ maxHeight: 260, overflow: "auto" }}>
                  <table className="table">
                    <thead>
                      <tr>
                        <th style={{ width: 36 }}>
                          <Checkbox
                            checked={items.size === info.entries.length}
                            indeterminate={items.size > 0 && items.size < info.entries.length}
                            onChange={(v) => setItems(v ? new Set(info.entries.map((e) => e.index)) : new Set())}
                          />
                        </th>
                        <th>#</th>
                        <th>Title</th>
                        <th style={{ textAlign: "right" }}>Length</th>
                      </tr>
                    </thead>
                    <tbody>
                      {info.entries.map((e) => (
                        <tr key={e.index}>
                          <td>
                            <Checkbox
                              checked={items.has(e.index)}
                              onChange={(v) => {
                                const n = new Set(items);
                                if (v) n.add(e.index);
                                else n.delete(e.index);
                                setItems(n);
                              }}
                            />
                          </td>
                          <td className="faint num">{e.index}</td>
                          <td className="truncate" style={{ maxWidth: 480 }}>
                            {e.title}
                          </td>
                          <td className="faint num" style={{ textAlign: "right" }}>
                            {e.duration ? fmt.clock(e.duration) : ""}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}

              <div className="form-grid" style={{ gridTemplateColumns: "112px 1fr" }}>
                {mode === "video" ? (
                  <>
                    <label>Format</label>
                    <Select value={container} onChange={(e) => setContainer(e.target.value)} options={[{ value: "mp4", label: "MP4 (most compatible)" }, { value: "mkv", label: "MKV" }, { value: "webm", label: "WebM" }]} style={{ width: 240 }} disabled={!info.ffmpegAvailable} />
                    <label>Subtitles</label>
                    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                      <Checkbox checked={subs} onChange={setSubs}>
                        Download and embed subtitles {allLangs.length ? "" : "(none available)"}
                      </Checkbox>
                      {subs && allLangs.length > 0 && (
                        <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                          {allLangs.map(({ l, auto }) => (
                            <button
                              key={l}
                              type="button"
                              className={`chip ${subLangs.has(l) ? "chip-accent" : ""}`}
                              style={{ border: "none", height: 22 }}
                              onClick={() => {
                                const n = new Set(subLangs);
                                if (n.has(l)) n.delete(l);
                                else n.add(l);
                                setSubLangs(n);
                              }}
                              title={auto ? "Automatic captions" : "Subtitles"}
                            >
                              {l}
                              {auto ? " (auto)" : ""}
                            </button>
                          ))}
                        </div>
                      )}
                    </div>
                  </>
                ) : (
                  <>
                    <label>Format</label>
                    <Select value={audioFormat} onChange={(e) => setAudioFormat(e.target.value)} disabled={!info.ffmpegAvailable} options={AUDIO_FORMATS()} style={{ width: 240 }} />
                  </>
                )}
                <label>Cover art</label>
                <Checkbox checked={embedThumb} onChange={setEmbedThumb}>
                  Embed the thumbnail
                </Checkbox>
                <label>Save to</label>
                <div className="input-group">
                  <Input value={dir} onChange={(e) => setDir(e.target.value)} />
                  <IconButton
                    icon={Folder}
                    label="Choose folder"
                    onClick={async () => {
                      const p = await open({ directory: true, defaultPath: dir || undefined });
                      if (typeof p === "string") setDir(p);
                    }}
                  />
                </div>
              </div>

              <div style={{ display: "flex", gap: 8, justifyContent: "flex-end", alignItems: "center" }}>
                <span className="faint num" style={{ marginRight: "auto", fontSize: "var(--text-sm)" }}>
                  {info.isPlaylist ? `${items.size} of ${info.entries.length} items` : chosenSize ? `About ${fmt.bytes(chosenSize)}` : ""}
                </span>
                <Button
                  ref={queueBtn}
                  disabled={busy || (info.isPlaylist && items.size === 0)}
                  onClick={() => (queues.length === 1 ? void download(queues[0].id) : showMenuAt(queueBtn.current!, queues.map((q) => ({ label: q.name, onSelect: () => void download(q.id) }))))}
                >
                  Add to queue
                </Button>
                <Button variant="primary" busy={busy} disabled={info.isPlaylist && items.size === 0} onClick={() => void download(null)}>
                  Download
                </Button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
