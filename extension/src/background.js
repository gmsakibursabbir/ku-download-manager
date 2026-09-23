// KuDownloader browser extension — background (MV3 service worker on
// Chromium, event page on Firefox). Talks only to the KuDownloader native
// messaging host; never executes anything it receives.

const api = globalThis.browser ?? globalThis.chrome;
const HOST = "com.kuduy.kudownloader";
const IS_FIREFOX = typeof api.runtime.getBrowserInfo === "function";

const DEFAULT_CONFIG = {
  intercept: true,
  confirm: true,
  minSize: 0,
  extensions: [],
  skipDomains: [],
  hoverButton: true,
  mediaDetection: true,
  adapters: ["youtube", "vimeo", "dailymotion", "generic"],
  running: false,
};

// ───────────────────────── native messaging ─────────────────────────

let seq = 0;
async function native(type, payload = {}) {
  const msg = { id: ++seq, type, payload };
  let reply;
  try {
    reply = await api.runtime.sendNativeMessage(HOST, msg);
  } catch (e) {
    const text = String(e?.message ?? e);
    const missing = /not found|not registered|Specified native messaging host|No such native application/i.test(text);
    throw new Error(missing ? "KuDownloader is not installed or its browser connection is not registered." : text);
  }
  if (!reply?.ok) throw new Error(reply?.error ?? "No response from KuDownloader");
  return reply.data;
}

// ───────────────────────── configuration ─────────────────────────

let config = { ...DEFAULT_CONFIG };
let configAt = 0;

async function loadConfig(force = false) {
  if (!force && Date.now() - configAt < 60_000) return config;
  try {
    const c = await native("config");
    if (c.running) {
      config = { ...DEFAULT_CONFIG, ...c };
      await api.storage.local.set({ config });
    } else {
      const stored = (await api.storage.local.get("config")).config;
      config = { ...DEFAULT_CONFIG, ...(stored ?? {}), running: false };
    }
  } catch {
    const stored = (await api.storage.local.get("config")).config;
    config = { ...DEFAULT_CONFIG, ...(stored ?? {}), running: false };
  }
  configAt = Date.now();
  return config;
}

async function localPrefs() {
  const { interceptPaused = false } = await api.storage.local.get("interceptPaused");
  return { interceptPaused };
}

// ───────────────────────── helpers ─────────────────────────

async function cookiesFor(url) {
  try {
    const opts = { url };
    if (IS_FIREFOX) opts.firstPartyDomain = null;
    const list = await api.cookies.getAll(opts);
    return list.map((c) => ({
      name: c.name,
      value: c.value,
      domain: c.domain,
      path: c.path,
      secure: c.secure,
      httpOnly: c.httpOnly,
      hostOnly: c.hostOnly,
      expirationDate: c.expirationDate ?? null,
    }));
  } catch {
    return [];
  }
}

function basename(p) {
  if (!p) return null;
  const s = p.split(/[\\/]/).pop();
  return s || null;
}

function hostOf(url) {
  try {
    return new URL(url).hostname.toLowerCase();
  } catch {
    return "";
  }
}

function extOf(name) {
  const m = /\.([a-z0-9]{1,10})$/i.exec(name || "");
  return m ? m[1].toLowerCase() : "";
}

const OK_SCHEMES = /^(https?|ftp):\/\//i;

async function sendLink({ url, filename, referer, sizeHint, mime, prompt, engine }) {
  const cookies = await cookiesFor(url);
  return native("add", {
    url,
    filename: filename ?? null,
    sizeHint: sizeHint ?? null,
    engine: engine ?? null,
    source: "browser",
    prompt: prompt ?? config.confirm,
    options: { cookies, referer: referer ?? null, userAgent: navigator.userAgent, headers: [] },
    mime,
  });
}

async function notify(title, message) {
  try {
    await api.notifications?.create?.({ type: "basic", iconUrl: api.runtime.getURL("icons/128x128.png"), title, message });
  } catch {
    /* notifications are optional */
  }
}

// ───────────────────────── download interception ─────────────────────────

// URLs we hand back to the browser (fallback) must not be intercepted again.
const passThrough = new Map();

function shouldIntercept(item, cfg) {
  const url = item.finalUrl || item.url;
  if (!OK_SCHEMES.test(url)) return false; // blob:, data:, file:
  if (item.byExtensionId && item.byExtensionId === api.runtime.id) return false;
  if (passThrough.has(url)) return false;
  if (item.incognito) return false;
  const host = hostOf(url);
  if (cfg.skipDomains.some((d) => host === d || host.endsWith("." + d))) return false;
  const name = basename(item.filename) || basename(new URL(url).pathname);
  if (cfg.extensions.length && !cfg.extensions.includes(extOf(name))) return false;
  const size = item.totalBytes > 0 ? item.totalBytes : item.fileSize > 0 ? item.fileSize : -1;
  if (cfg.minSize > 0 && size >= 0 && size < cfg.minSize) return false;
  return true;
}

api.downloads.onCreated.addListener(async (item) => {
  if (item.state && item.state !== "in_progress") return;
  const [cfg, local] = await Promise.all([loadConfig(), localPrefs()]);
  if (!cfg.intercept || local.interceptPaused || !shouldIntercept(item, cfg)) return;
  const url = item.finalUrl || item.url;
  // Stop the browser first so the file is not downloaded twice.
  try {
    await api.downloads.cancel(item.id);
  } catch {
    return; // already finished — nothing to take over
  }
  try {
    await sendLink({
      url,
      filename: basename(item.filename),
      referer: item.referrer || null,
      sizeHint: item.totalBytes > 0 ? item.totalBytes : null,
      mime: item.mime,
      engine: "aria2",
    });
    try {
      await api.downloads.erase({ id: item.id });
    } catch {
      /* ignore */
    }
  } catch (e) {
    // KuDownloader unavailable: give the download back to the browser.
    passThrough.set(url, Date.now());
    setTimeout(() => passThrough.delete(url), 60_000);
    try {
      await api.downloads.erase({ id: item.id });
    } catch {
      /* ignore */
    }
    await api.downloads.download({ url, filename: basename(item.filename) ?? undefined, saveAs: false });
    await notify("KuDownloader", `${e.message} The browser downloaded the file instead.`);
  }
});

// ───────────────────────── context menus ─────────────────────────

function createMenus() {
  api.contextMenus.removeAll(() => {
    api.contextMenus.create({ id: "ku-link", title: "Download with KuDownloader", contexts: ["link"] });
    api.contextMenus.create({ id: "ku-media", title: "Download with KuDownloader", contexts: ["video", "audio", "image"] });
    api.contextMenus.create({ id: "ku-selection", title: "Download selected links with KuDownloader", contexts: ["selection"] });
    api.contextMenus.create({ id: "ku-page-media", title: "Download media from this page…", contexts: ["page"] });
    api.contextMenus.create({ id: "ku-all", title: "Download all links with KuDownloader…", contexts: ["page"] });
  });
}

api.runtime.onInstalled.addListener(() => {
  createMenus();
  void loadConfig(true);
});
api.runtime.onStartup?.addListener(() => {
  createMenus();
  void loadConfig(true);
});

function isMediaPage(url) {
  return /(youtube\.com\/(watch|shorts|playlist)|youtu\.be\/|vimeo\.com\/\d|dailymotion\.com\/video)/i.test(url);
}

async function collectLinks(tabId, selectionOnly) {
  const [res] = await api.scripting.executeScript({
    target: { tabId },
    args: [selectionOnly],
    func: (onlySelection) => {
      const out = [];
      const seen = new Set();
      const add = (url, text, kind) => {
        if (!url || seen.has(url) || !/^(https?|ftp):|^magnet:/i.test(url)) return;
        seen.add(url);
        out.push({ url, text: (text || "").trim().slice(0, 200) || null, kind: kind || null });
      };
      let root = document;
      let range = null;
      if (onlySelection) {
        const sel = window.getSelection();
        if (!sel || sel.rangeCount === 0) return out;
        range = sel.getRangeAt(0);
      }
      const inSel = (el) => !range || range.intersectsNode(el);
      for (const a of root.querySelectorAll("a[href]")) if (inSel(a)) add(a.href, a.textContent);
      if (!onlySelection) {
        for (const m of root.querySelectorAll("video[src], audio[src], source[src], img[src]")) add(m.currentSrc || m.src, m.alt || "", m.tagName.toLowerCase());
      }
      return out.slice(0, 20000);
    },
  });
  return res?.result ?? [];
}

api.contextMenus.onClicked.addListener(async (info, tab) => {
  try {
    await loadConfig();
    switch (info.menuItemId) {
      case "ku-link": {
        const url = info.linkUrl;
        if (isMediaPage(url)) await native("show", { mediaUrl: url, cookies: await cookiesFor(url) });
        else await sendLink({ url, referer: info.pageUrl, prompt: true });
        break;
      }
      case "ku-media": {
        const url = info.srcUrl;
        if (!OK_SCHEMES.test(url)) {
          // blob:/MSE streams: let yt-dlp read the page instead.
          await native("show", { mediaUrl: info.pageUrl, cookies: await cookiesFor(info.pageUrl) });
        } else {
          await sendLink({ url, referer: info.pageUrl, prompt: true });
        }
        break;
      }
      case "ku-page-media":
        await native("show", { mediaUrl: info.pageUrl, cookies: await cookiesFor(info.pageUrl) });
        break;
      case "ku-selection":
      case "ku-all": {
        const links = await collectLinks(tab.id, info.menuItemId === "ku-selection");
        if (!links.length) {
          await notify("KuDownloader", "No links found.");
          return;
        }
        await native("grab", { pageUrl: info.pageUrl, referer: info.pageUrl, links, cookies: await cookiesFor(info.pageUrl), userAgent: navigator.userAgent });
        break;
      }
    }
  } catch (e) {
    await notify("KuDownloader", e.message);
  }
});

// ───────────────────────── media detection ─────────────────────────

const MEDIA_TYPES = /^(video\/|audio\/|application\/(vnd\.apple\.mpegurl|x-mpegurl|dash\+xml))/i;
const detected = new Map(); // tabId → Map(url → item)

async function saveDetected(tabId) {
  const items = [...(detected.get(tabId)?.values() ?? [])];
  try {
    await api.storage.session?.set?.({ ["media:" + tabId]: items });
  } catch {
    /* session storage unavailable */
  }
  const n = items.length;
  try {
    await api.action.setBadgeText({ tabId, text: n ? String(n) : "" });
    await api.action.setBadgeBackgroundColor({ tabId, color: "#2563EB" });
  } catch {
    /* tab closed */
  }
}

async function loadDetected(tabId) {
  if (detected.has(tabId)) return [...detected.get(tabId).values()];
  try {
    const v = (await api.storage.session?.get?.("media:" + tabId))?.["media:" + tabId] ?? [];
    detected.set(tabId, new Map(v.map((i) => [i.url, i])));
    return v;
  } catch {
    return [];
  }
}

api.webRequest.onHeadersReceived.addListener(
  (d) => {
    if (d.tabId < 0 || !config.mediaDetection) return;
    const h = Object.fromEntries((d.responseHeaders ?? []).map((x) => [x.name.toLowerCase(), x.value ?? ""]));
    const type = (h["content-type"] || "").split(";")[0].trim();
    if (!MEDIA_TYPES.test(type)) return;
    const len = Number(h["content-length"] || 0);
    const path = new URL(d.url).pathname.toLowerCase();
    const manifest = /mpegurl|dash\+xml/i.test(type) || /\.(m3u8|mpd)$/.test(path);
    // Skip tiny stream fragments; keep manifests and real files.
    if (!manifest && (len && len < 512 * 1024)) return;
    if (/\.(ts|m4s|aac)$/.test(path) && !manifest) return;
    let m = detected.get(d.tabId);
    if (!m) detected.set(d.tabId, (m = new Map()));
    if (m.size > 50 || m.has(d.url)) return;
    const range = h["content-range"];
    const total = range ? Number(range.split("/")[1]) || len : len;
    m.set(d.url, { url: d.url, type, size: total || null, manifest, name: basename(path) || type, at: Date.now() });
    void saveDetected(d.tabId);
  },
  { urls: ["<all_urls>"], types: ["media", "xmlhttprequest", "other", "object"] },
  ["responseHeaders"],
);

api.tabs.onUpdated.addListener((tabId, change) => {
  if (change.status === "loading" && change.url) {
    detected.delete(tabId);
    void saveDetected(tabId);
  }
});
api.tabs.onRemoved.addListener((tabId) => {
  detected.delete(tabId);
  void api.storage.session?.remove?.("media:" + tabId);
});

// ───────────────────────── messages from content scripts / popup ─────────────────────────

api.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  // Only our own extension pages and content scripts can send messages here.
  if (sender.id !== api.runtime.id) return false;
  (async () => {
    switch (msg?.type) {
      case "config":
        return loadConfig(msg.force);
      case "status": {
        const ping = await native("ping").catch((e) => ({ error: e.message }));
        return { ...ping, ...(await localPrefs()) };
      }
      case "setInterceptPaused":
        await api.storage.local.set({ interceptPaused: !!msg.value });
        return { ok: true };
      case "analyze": {
        const url = String(msg.url || "");
        if (!/^https?:\/\//i.test(url)) throw new Error("Unsupported address");
        return native("analyze", { url, playlist: !!msg.playlist, cookies: await cookiesFor(url), referer: sender.tab?.url ?? null });
      }
      case "mediaDownload": {
        const req = msg.request || {};
        if (!/^https?:\/\//i.test(req.url || "")) throw new Error("Unsupported address");
        return native("mediaDownload", { ...req, cookies: await cookiesFor(req.url), userAgent: navigator.userAgent, source: "browser" });
      }
      case "openInApp":
        return native("show", { mediaUrl: msg.url, cookies: await cookiesFor(msg.url) });
      case "addDetected": {
        const item = msg.item;
        if (!/^https?:\/\//i.test(item?.url || "")) throw new Error("Unsupported address");
        return sendLink({ url: item.url, referer: msg.pageUrl, engine: item.manifest ? "ytdlp" : "aria2", prompt: !item.manifest });
      }
      case "detected":
        return loadDetected(msg.tabId);
      case "grabTab": {
        const links = await collectLinks(msg.tabId, !!msg.selection);
        if (!links.length) throw new Error(msg.selection ? "Select some links on the page first." : "No links found on this page.");
        return native("grab", { pageUrl: msg.pageUrl, referer: msg.pageUrl, links, cookies: await cookiesFor(msg.pageUrl), userAgent: navigator.userAgent });
      }
      case "showApp":
        return native("show", {});
      default:
        throw new Error("Unknown request");
    }
  })().then(
    (data) => sendResponse({ ok: true, data }),
    (e) => sendResponse({ ok: false, error: String(e?.message ?? e) }),
  );
  return true;
});

createMenus();
void loadConfig(true);
