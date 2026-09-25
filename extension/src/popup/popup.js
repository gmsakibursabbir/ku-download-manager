const api = globalThis.browser ?? globalThis.chrome;
/** Translated text (browser language); English when a message is missing. */
const M = (id, fallback, subs) => {
  try {
    return api.i18n?.getMessage(id, subs) || fallback;
  } catch {
    return fallback;
  }
};

// Static text and direction in the browser's language.
document.documentElement.lang = api.i18n?.getUILanguage?.() || "en";
document.documentElement.dir = M("@@bidi_dir", "ltr");
for (const el of document.querySelectorAll("[data-i18n]")) el.textContent = M(el.dataset.i18n, el.textContent);
const $ = (id) => document.getElementById(id);

const ask = async (msg) => {
  // Promise form works on Chromium MV3 and Firefox alike.
  const r = await api.runtime.sendMessage(msg);
  if (!r?.ok) throw new Error(r?.error || M("noResponse", "No response from KuDownloader"));
  return r.data;
};

function bytes(n) {
  if (!n) return "";
  const u = ["B", "KB", "MB", "GB"];
  let i = 0;
  while (n >= 1024 && i < u.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n >= 100 ? n.toFixed(0) : n.toFixed(1)} ${u[i]}`;
}

function problem(text) {
  $("problem").hidden = !text;
  $("problem").textContent = text || "";
}

async function run(fn) {
  try {
    await fn();
    window.close();
  } catch (e) {
    problem(e.message);
  }
}

const MEDIA_PAGE = /(youtube\.com\/(watch|shorts|playlist)|youtu\.be\/|vimeo\.com\/\d|dailymotion\.com\/video)/i;

(async () => {
  const [tab] = await api.tabs.query({ active: true, currentWindow: true });
  const pageUrl = tab?.url ?? "";

  // Connection status
  try {
    const s = await ask({ type: "status" });
    if (s.error) {
      $("status").dataset.state = "error";
      $("status").textContent = M("notConnected", "Not connected");
      problem(s.error);
    } else {
      $("status").dataset.state = "ok";
      $("status").textContent = s.running ? M("connected", "Connected") : M("ready", "Ready");
    }
    $("intercept").checked = !s.interceptPaused;
  } catch (e) {
    $("status").dataset.state = "error";
    $("status").textContent = M("notConnected", "Not connected");
    problem(e.message);
  }

  $("intercept").addEventListener("change", (e) => void ask({ type: "setInterceptPaused", value: !e.target.checked }));

  // Detected media
  const items = tab ? await ask({ type: "detected", tabId: tab.id }).catch(() => []) : [];
  $("count").textContent = items.length ? String(items.length) : "";
  $("nomedia").hidden = items.length > 0 || MEDIA_PAGE.test(pageUrl);
  const list = $("media");
  for (const it of items.slice(0, 30)) {
    const li = document.createElement("li");
    const name = document.createElement("span");
    name.className = "name";
    name.textContent = decodeURIComponent(it.name || "media");
    name.title = it.url;
    const sub = document.createElement("span");
    sub.className = "sub";
    sub.textContent = [it.manifest ? M("stream", "Stream") : it.type, bytes(it.size)].filter(Boolean).join(" · ");
    name.append(sub);
    const btn = document.createElement("button");
    btn.type = "button";
    btn.textContent = M("download", "Download");
    btn.addEventListener("click", () => run(() => ask({ type: "addDetected", item: it, pageUrl })));
    li.append(name, btn);
    list.append(li);
  }

  if (MEDIA_PAGE.test(pageUrl)) {
    $("pageMedia").hidden = false;
    $("pageMedia").addEventListener("click", () => run(() => ask({ type: "openInApp", url: pageUrl })));
  }

  const web = /^https?:/i.test(pageUrl);
  $("all").disabled = !web;
  $("selected").disabled = !web;
  $("all").addEventListener("click", () => run(() => ask({ type: "grabTab", tabId: tab.id, pageUrl })));
  $("selected").addEventListener("click", () => run(() => ask({ type: "grabTab", tabId: tab.id, pageUrl, selection: true })));
  $("open").addEventListener("click", () => run(() => ask({ type: "showApp" })));
})();
