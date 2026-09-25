// "↓ KuDownload" hover button and quality panel. One button and one panel
// per page (shadow DOM, styles isolated), positioned over the hovered media.
// Uses event delegation: nothing is scanned or decorated up front.

(() => {
  const api = globalThis.browser ?? globalThis.chrome;
  if (globalThis.__kuHover) return;
  globalThis.__kuHover = true;
  const isTop = window.top === window;
  /** Translated text (browser language); English when a message is missing. */
  const M = (id, fallback, subs) => {
    try {
      return api.i18n?.getMessage(id, subs) || fallback;
    } catch {
      return fallback;
    }
  };

  const ask = async (msg) => {
    // Promise form works on Chromium MV3 and Firefox alike.
    const r = await api.runtime.sendMessage(msg);
    if (!r?.ok) throw new Error(r?.error || M("noResponse", "No response from KuDownloader"));
    return r.data;
  };

  const fmtBytes = (n) => {
    if (!n) return "";
    const u = ["B", "KB", "MB", "GB"];
    let i = 0;
    while (n >= 1024 && i < u.length - 1) {
      n /= 1024;
      i++;
    }
    return `${n >= 100 ? n.toFixed(0) : n.toFixed(1)} ${u[i]}`;
  };

  const CSS = `
    :host { all: initial; }
    * { box-sizing: border-box; font-family: system-ui, -apple-system, "Segoe UI", sans-serif; }
    .btn {
      position: fixed; z-index: 2147483646; display: none; align-items: center; gap: 6px;
      height: 28px; padding: 0 10px; border: 0; border-radius: 6px;
      background: #2563EB; color: #fff; font-size: 12px; font-weight: 600; letter-spacing: .01em;
      box-shadow: 0 2px 8px rgba(0,0,0,.35); cursor: pointer; transition: background .12s;
    }
    .btn:hover { background: #1D4ED8; }
    .btn:focus-visible { outline: 2px solid #D4FF00; outline-offset: 2px; }
    .btn svg { width: 14px; height: 14px; }
    .panel {
      position: fixed; z-index: 2147483647; width: 280px; display: none;
      background: #1B1B1E; color: #F4F4F5; border-radius: 10px; font-size: 13px;
      box-shadow: 0 0 0 1px rgba(255,255,255,.08), 0 16px 40px -8px rgba(0,0,0,.6);
      overflow: hidden;
    }
    .head { display: flex; align-items: center; justify-content: space-between; padding: 10px 12px 6px; }
    .title { font-weight: 650; font-size: 13px; }
    .close { background: none; border: 0; color: #A1A1AA; font-size: 16px; cursor: pointer; width: 24px; height: 24px; border-radius: 4px; }
    .close:hover { background: rgba(255,255,255,.08); color: #fff; }
    .meta { padding: 0 12px 8px; color: #A1A1AA; font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .group { padding: 4px 6px; }
    .label { padding: 6px 6px 4px; font-size: 10.5px; font-weight: 650; text-transform: uppercase; letter-spacing: .06em; color: #71717A; }
    .opt { display: flex; align-items: center; gap: 8px; width: 100%; height: 30px; padding: 0 8px; border: 0; background: none;
      color: inherit; border-radius: 5px; cursor: pointer; font-size: 13px; text-align: left; }
    .opt:hover { background: rgba(255,255,255,.06); }
    .opt[aria-checked="true"] { background: rgba(37,99,235,.28); }
    .opt .size { margin-left: auto; color: #A1A1AA; font-variant-numeric: tabular-nums; font-size: 12px; }
    .dot { width: 12px; height: 12px; border-radius: 50%; border: 1.5px solid #71717A; flex: none; }
    .opt[aria-checked="true"] .dot { border-color: #3B74F0; background: radial-gradient(#3B74F0 45%, transparent 50%); }
    .foot { display: flex; gap: 8px; padding: 10px 12px; border-top: 1px solid rgba(255,255,255,.07); }
    .primary, .secondary { height: 30px; padding: 0 12px; border-radius: 6px; font-size: 12.5px; font-weight: 600; cursor: pointer; border: 0; }
    .primary { background: #2563EB; color: #fff; flex: 1; }
    .primary:hover { background: #1D4ED8; }
    .primary:disabled { opacity: .5; cursor: default; }
    .secondary { background: rgba(255,255,255,.08); color: #F4F4F5; }
    .secondary:hover { background: rgba(255,255,255,.14); }
    .state { padding: 16px 12px; color: #A1A1AA; font-size: 12.5px; line-height: 1.45; }
    .state.err { color: #F87171; }
    .state.ok { color: #4ADE80; }
    .spinner { display: inline-block; width: 12px; height: 12px; border: 2px solid rgba(255,255,255,.2); border-top-color: #3B74F0; border-radius: 50%;
      animation: spin .7s linear infinite; vertical-align: -2px; margin-right: 8px; }
    @keyframes spin { to { transform: rotate(360deg); } }
    @media (prefers-reduced-motion: reduce) { .spinner { animation: none; } }
  `;

  let host, root, btn, panel;
  let current = null; // { url, kind, anchor }
  let hideTimer = 0;
  let adapters = [];
  let generic = null; // pointer-position fallback for any <video>

  function mount() {
    host = document.createElement("ku-downloader");
    root = host.attachShadow({ mode: "closed" });
    const style = document.createElement("style");
    style.textContent = CSS;
    btn = document.createElement("button");
    btn.className = "btn";
    btn.type = "button";
    btn.setAttribute("aria-label", M("menuDownload", "Download with KuDownloader"));
    btn.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v11"/><path d="m7 11 5 5 5-5"/><path d="M5 20h14"/></svg><span>KuDownload</span>`;
    panel = document.createElement("div");
    panel.className = "panel";
    panel.setAttribute("role", "dialog");
    panel.setAttribute("aria-label", "KuDownloader");
    root.append(style, btn, panel);
    document.documentElement.appendChild(host);
    btn.addEventListener("mouseenter", () => clearTimeout(hideTimer));
    btn.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (current) openPanel(current);
    });
    document.addEventListener("keydown", (e) => e.key === "Escape" && closePanel(), true);
    document.addEventListener("mousedown", (e) => {
      if (panel.style.display === "block" && !e.composedPath().includes(host)) closePanel();
    }, true);
  }

  function place(anchor) {
    const r = anchor.getBoundingClientRect();
    if (r.width < 120 || r.height < 60 || r.bottom < 0 || r.top > innerHeight) return false;
    btn.style.top = `${Math.max(8, r.top + 8)}px`;
    btn.style.left = `${Math.min(innerWidth - 130, r.right - 124)}px`;
    return true;
  }

  function scheduleHide() {
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      if (panel.style.display !== "block") btn.style.display = "none";
    }, 350);
  }

  function show(hit) {
    if (!hit?.url || !place(hit.anchor)) return false;
    current = hit;
    clearTimeout(hideTimer);
    btn.style.display = "inline-flex";
    return true;
  }

  /** Largest content-sized <video> under the point (players often cover it with overlays). */
  function videoAt(x, y) {
    let best = null;
    let area = 0;
    for (const v of document.getElementsByTagName("video")) {
      const r = v.getBoundingClientRect();
      if (r.width < generic.minWidth || r.height < generic.minHeight) continue;
      if (x < r.left || x > r.right || y < r.top || y > r.bottom) continue;
      if (r.width * r.height > area) {
        best = v;
        area = r.width * r.height;
      }
    }
    return best;
  }

  function onOver(e) {
    if ((!adapters.length && !generic) || panel.style.display === "block") return;
    const path = e.composedPath ? e.composedPath() : [e.target];
    if (path.includes(host)) return;
    for (const a of adapters) {
      const el = e.target.closest?.(a.selector);
      if (el && show(a.resolve(el))) return;
    }
    if (generic) {
      const v = videoAt(e.clientX, e.clientY);
      if (v && (current?.anchor !== v || btn.style.display === "none")) show(generic.resolve(v));
    }
  }

  // Like IDM: a video that starts playing gets the button without hovering;
  // it fades out after a few seconds unless the pointer is on it.
  function onPlay(e) {
    const v = e.target;
    if (!(v instanceof HTMLVideoElement) || panel.style.display === "block") return;
    if (btn.style.display !== "none" && current?.anchor && current.anchor.contains?.(v)) return;
    let hit = null;
    for (const a of adapters) {
      const el = v.closest?.(a.selector);
      if (el && (hit = a.resolve(el))) break;
    }
    if (!hit && generic) hit = generic.resolve(v);
    if (!show(hit)) return;
    clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      if (!pointerInside() && panel.style.display !== "block") btn.style.display = "none";
    }, 4000);
  }

  // Keep the button visible in fullscreen players (not for a bare <video>,
  // which cannot hold children).
  function onFullscreen() {
    const fs = document.fullscreenElement;
    const parent = fs && !(fs instanceof HTMLVideoElement) ? fs : document.documentElement;
    if (host.parentNode !== parent) parent.appendChild(host);
    if (current && btn.style.display !== "none") place(current.anchor);
  }

  // Hide by pointer position rather than mouseleave: sites swap or overlay
  // the hovered element (YouTube's inline preview covers the thumbnail once
  // it plays), which fires mouseleave while the pointer never moved away.
  let px = -1;
  let py = -1;
  let moveQueued = false;
  const within = (r, pad) => r.width > 0 && px >= r.left - pad && px <= r.right + pad && py >= r.top - pad && py <= r.bottom + pad;
  const pointerInside = () => !!current && (within(btn.getBoundingClientRect(), 6) || within(current.anchor.getBoundingClientRect(), 2));

  function onMove(e) {
    px = e.clientX;
    py = e.clientY;
    if (moveQueued || btn.style.display === "none" || panel.style.display === "block") return;
    moveQueued = true;
    requestAnimationFrame(() => {
      moveQueued = false;
      if (pointerInside()) clearTimeout(hideTimer);
      else scheduleHide();
    });
  }

  function closePanel() {
    panel.style.display = "none";
    panel.replaceChildren();
    btn.style.display = "none";
  }

  function el(tag, cls, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text != null) n.textContent = text;
    return n;
  }

  function positionPanel() {
    const b = btn.getBoundingClientRect();
    panel.style.top = `${Math.min(innerHeight - 20 - panel.offsetHeight, b.bottom + 6)}px`;
    panel.style.left = `${Math.max(8, Math.min(innerWidth - 288, b.right - 280))}px`;
  }

  function header(title) {
    const head = el("div", "head");
    head.append(el("div", "title", title));
    const x = el("button", "close", "×");
    x.type = "button";
    x.setAttribute("aria-label", M("close", "Close"));
    x.addEventListener("click", closePanel);
    head.append(x);
    return head;
  }

  function state(text, cls = "") {
    const s = el("div", `state ${cls}`);
    if (cls === "loading") s.innerHTML = `<span class="spinner"></span>`;
    s.append(document.createTextNode(text));
    return s;
  }

  async function openPanel(hit) {
    panel.replaceChildren(header("KuDownloader"), state(M("readingFormats", "Reading available formats…"), "loading"));
    panel.style.display = "block";
    positionPanel();
    if (hit.kind === "file") {
      panel.replaceChildren(header("KuDownloader"), el("div", "meta", decodeURIComponent(hit.url.split("/").pop() || hit.url)));
      const foot = el("div", "foot");
      const go = el("button", "primary", M("downloadVideoFile", "Download video file"));
      go.type = "button";
      go.addEventListener("click", async () => {
        go.disabled = true;
        try {
          await ask({ type: "addDetected", item: { url: hit.url, manifest: false }, pageUrl: location.href });
          panel.replaceChildren(header("KuDownloader"), state(M("sent", "Sent to KuDownloader."), "ok"));
          setTimeout(closePanel, 1200);
        } catch (e) {
          panel.replaceChildren(header("KuDownloader"), state(e.message, "err"));
        }
      });
      foot.append(go);
      panel.append(foot);
      positionPanel();
      return;
    }
    let info;
    try {
      info = await ask({ type: "analyze", url: hit.url });
    } catch (e) {
      panel.replaceChildren(header("KuDownloader"), state(e.message, "err"));
      positionPanel();
      return;
    }
    render(hit, info);
  }

  function render(hit, info) {
    let choice = null;
    const opts = [];
    const pick = (o, btnEl) => {
      choice = o;
      for (const b of opts) b.setAttribute("aria-checked", String(b === btnEl));
      go.disabled = false;
    };
    const option = (label, size, o) => {
      const b = el("button", "opt");
      b.type = "button";
      b.setAttribute("role", "radio");
      b.setAttribute("aria-checked", "false");
      b.append(el("span", "dot"), el("span", "", label), el("span", "size", fmtBytes(size)));
      b.addEventListener("click", () => pick(o, b));
      opts.push(b);
      return b;
    };
    const go = el("button", "primary", M("download", "Download"));
    go.type = "button";
    go.disabled = true;
    const children = [header("KuDownloader"), el("div", "meta", info.title)];
    const heights = [2160, 1440, 1080, 720, 480];
    const video = info.video.filter((v) => heights.includes(v.height) || info.video.length <= 5);
    if (video.length) {
      const g = el("div", "group");
      g.append(el("div", "label", M("video", "Video")));
      for (const v of video.slice(0, 6)) g.append(option(`${v.height}p${v.hdr ? " HDR" : ""}`, v.size, { mode: "video", height: v.height }));
      children.push(g);
    }
    const audio = info.audio.filter((a) => [320, 256, 128].includes(a.bitrate) || !info.ffmpegAvailable);
    if (audio.length) {
      const g = el("div", "group");
      g.append(el("div", "label", M("audio", "Audio")));
      for (const a of audio.slice(0, 3)) g.append(option(`${a.bitrate} kbps ${a.ext.toUpperCase()}`, a.size, { mode: "audio", audioBitrate: a.bitrate }));
      children.push(g);
    }
    if (!video.length && !audio.length) children.push(state(M("noFormats", "No downloadable formats were found."), "err"));
    const foot = el("div", "foot");
    const more = el("button", "secondary", M("more", "More…"));
    more.type = "button";
    more.title = M("moreTitle", "Open in KuDownloader for subtitles, playlists and more options");
    more.addEventListener("click", async () => {
      await ask({ type: "openInApp", url: hit.url }).catch(() => {});
      closePanel();
    });
    go.addEventListener("click", async () => {
      if (!choice) return;
      go.disabled = true;
      go.textContent = M("sending", "Sending…");
      try {
        await ask({
          type: "mediaDownload",
          request: {
            url: info.webpageUrl || hit.url,
            title: info.title,
            thumbnail: info.thumbnail,
            media: { mode: choice.mode, height: choice.height ?? null, audioBitrate: choice.audioBitrate ?? null, subtitles: false, embedSubtitles: false, writeThumbnail: false, embedThumbnail: false, playlist: false },
          },
        });
        panel.replaceChildren(header("KuDownloader"), state(M("added", "Added to KuDownloader."), "ok"));
        positionPanel();
        setTimeout(closePanel, 1400);
      } catch (e) {
        panel.replaceChildren(header("KuDownloader"), state(e.message, "err"));
        positionPanel();
      }
    });
    foot.append(more, go);
    children.push(foot);
    panel.replaceChildren(...children);
    // Preselect a sensible default (1080p or the best below it).
    const def = opts.find((b, i) => i < video.length && video[i].height <= 1080) ?? opts[0];
    def?.click();
    positionPanel();
  }

  async function init() {
    let cfg;
    try {
      cfg = await ask({ type: "config" });
    } catch {
      return;
    }
    if (!cfg.hoverButton) return;
    const all = globalThis.__kuAdapters || [];
    const site = all.filter((a) => !a.generic && cfg.adapters.includes(a.id) && a.match());
    // Site adapters know the real video URL; elsewhere any <video> counts.
    generic = site.length ? null : (all.find((a) => a.generic && cfg.adapters.includes(a.id)) ?? null);
    adapters = site;
    if (!adapters.length && !generic) return;
    mount();
    document.addEventListener("mouseover", onOver, { passive: true, capture: true });
    document.addEventListener("play", onPlay, { passive: true, capture: true });
    document.addEventListener("fullscreenchange", onFullscreen);
    document.addEventListener("mousemove", onMove, { passive: true, capture: true });
    const pointerGone = () => {
      px = py = -1;
      scheduleHide();
    };
    document.documentElement.addEventListener("mouseleave", pointerGone);
    // Pointer moved into an iframe: this document stops seeing mousemove.
    document.addEventListener("mouseout", (e) => e.relatedTarget?.tagName === "IFRAME" && pointerGone(), { passive: true, capture: true });
    addEventListener("scroll", () => btn.style.display !== "none" && current && !place(current.anchor) && (btn.style.display = "none"), { passive: true });
  }

  // Top page: start now. Frames (ads, widgets, embedded players): start only
  // once the frame actually has a video, so idle frames cost nothing.
  if (isTop || document.getElementsByTagName("video").length) {
    void init();
  } else {
    const kick = (e) => {
      if (!(e.target instanceof HTMLVideoElement)) return;
      document.removeEventListener("loadedmetadata", kick, true);
      document.removeEventListener("play", kick, true);
      void init().then(() => btn && onPlay(e));
    };
    document.addEventListener("loadedmetadata", kick, true);
    document.addEventListener("play", kick, true);
  }
})();
