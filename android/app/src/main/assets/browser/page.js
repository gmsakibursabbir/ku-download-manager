// KuDownloader's page script for the built-in browser (injected into every
// page). It draws the KuDownload button over videos, applies the ad blocker's
// element-hiding rules and collects links for Fetch Projects. It talks only to
// the app, through `KuBridge`, with a per-tab secret.
(function () {
  "use strict";
  if (window.__kuPage) return;
  window.__kuPage = true;
  var TOKEN = "__KU_TOKEN__";
  var PILL = __KU_PILL__;
  var LABEL = __KU_LABEL__;
  var bridge = window.KuBridge;
  if (!bridge) return;
  // Hide the bridge from the page where the browser allows it (it is
  // non-configurable on some WebViews; deleting it then throws in strict mode).
  try {
    delete window.KuBridge;
  } catch (e) {}

  // ───────── element hiding ─────────
  function style(id, css) {
    if (!css) return;
    var el = document.getElementById(id);
    if (!el) {
      el = document.createElement("style");
      el.id = id;
      (document.head || document.documentElement).appendChild(el);
    }
    el.textContent = css;
  }

  var generic = false;
  var exceptions = "[]";
  var seenClass = {};
  var seenId = {};
  var genericCss = "";

  function scanGeneric() {
    if (!generic) return;
    var classes = [];
    var ids = [];
    var all = document.querySelectorAll("[class],[id]");
    for (var i = 0; i < all.length && i < 6000; i++) {
      var el = all[i];
      if (el.id && !seenId[el.id]) {
        seenId[el.id] = 1;
        ids.push(el.id);
      }
      var cl = el.classList;
      if (cl) {
        for (var j = 0; j < cl.length; j++) {
          if (!seenClass[cl[j]]) {
            seenClass[cl[j]] = 1;
            classes.push(cl[j]);
          }
        }
      }
    }
    if (!classes.length && !ids.length) return;
    var css = bridge.hidden(TOKEN, JSON.stringify({ classes: classes, ids: ids, exceptions: JSON.parse(exceptions) }));
    if (css) {
      genericCss += css + "\n";
      style("__ku_generic", genericCss);
    }
  }

  window.__kuCosmetic = function (css, isGeneric, ex) {
    style("__ku_hide", css);
    generic = isGeneric;
    exceptions = ex || "[]";
    scanGeneric();
  };

  // ───────── links (Fetch Projects) ─────────
  window.__kuLinks = function () {
    var out = [];
    var seen = {};
    var push = function (u, text) {
      if (!u || seen[u] || !/^(https?|ftp|magnet):/i.test(u)) return;
      seen[u] = 1;
      out.push({ url: u, text: (text || "").trim().slice(0, 200) });
    };
    var a = document.querySelectorAll("a[href]");
    for (var i = 0; i < a.length; i++) push(a[i].href, a[i].textContent);
    var m = document.querySelectorAll("video[src],audio[src],source[src],img[src]");
    for (var k = 0; k < m.length; k++) push(m[k].src, m[k].alt || m[k].title);
    bridge.links(TOKEN, JSON.stringify(out.slice(0, 5000)));
  };

  // ───────── KuDownload button on videos ─────────
  var host = null;
  var btn = null;
  var target = null;

  function ensureButton() {
    if (btn) return;
    host = document.createElement("ku-download");
    host.style.cssText = "all:initial;position:fixed;z-index:2147483646;top:0;left:0;width:0;height:0;";
    var root = host.attachShadow ? host.attachShadow({ mode: "closed" }) : host;
    var css = document.createElement("style");
    css.textContent =
      ".b{position:fixed;display:none;align-items:center;gap:6px;height:32px;padding:0 12px;border:0;border-radius:16px;" +
      "background:#2563EB;color:#fff;font:600 13px system-ui,-apple-system,Roboto,sans-serif;letter-spacing:.01em;" +
      "box-shadow:0 2px 10px rgba(0,0,0,.4);touch-action:manipulation;-webkit-tap-highlight-color:transparent}" +
      ".b:active{background:#1D4ED8}.b svg{width:16px;height:16px}";
    btn = document.createElement("button");
    btn.className = "b";
    btn.setAttribute("aria-label", LABEL);
    // Built node by node: sites with Trusted Types (YouTube) reject innerHTML.
    var NS = "http://www.w3.org/2000/svg";
    var svg = document.createElementNS(NS, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "2.6");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    [["M12 4v11"], ["M7 11l5 5 5-5"], ["M6 20h12", "#D4FF00"]].forEach(function (d) {
      var path = document.createElementNS(NS, "path");
      path.setAttribute("d", d[0]);
      if (d[1]) path.setAttribute("stroke", d[1]);
      svg.appendChild(path);
    });
    var label = document.createElement("span");
    label.textContent = LABEL;
    btn.appendChild(svg);
    btn.appendChild(label);
    btn.addEventListener(
      "click",
      function (e) {
        e.preventDefault();
        e.stopPropagation();
        var v = target;
        var src = v ? v.currentSrc || v.src || "" : "";
        if (src && src.indexOf("blob:") === 0) src = "";
        bridge.media(TOKEN, JSON.stringify({ src: src, page: location.href, title: document.title }));
      },
      true,
    );
    root.appendChild(css);
    root.appendChild(btn);
    (document.body || document.documentElement).appendChild(host);
  }

  function visible(v) {
    var r = v.getBoundingClientRect();
    if (r.width < 160 || r.height < 90) return null;
    if (r.bottom < 40 || r.top > innerHeight - 40 || r.right < 40 || r.left > innerWidth - 40) return null;
    return r;
  }

  function pick() {
    var vids = document.getElementsByTagName("video");
    var best = null;
    var bestArea = 0;
    for (var i = 0; i < vids.length; i++) {
      var r = visible(vids[i]);
      if (!r) continue;
      var area = r.width * r.height;
      if (area > bestArea) {
        best = vids[i];
        bestArea = area;
      }
    }
    return best;
  }

  var queued = false;
  var reported = null;
  function place() {
    queued = false;
    var v = pick();
    var has = !!v || document.getElementsByTagName("video").length > 0;
    if (has !== reported) {
      reported = has;
      try {
        bridge.video(TOKEN, has);
      } catch (e) {}
    }
    if (!PILL) return;
    target = v;
    if (!v) {
      if (btn) btn.style.display = "none";
      return;
    }
    ensureButton();
    if (!host.isConnected) (document.body || document.documentElement).appendChild(host);
    var r = v.getBoundingClientRect();
    btn.style.display = "inline-flex";
    var w = btn.offsetWidth || 120;
    btn.style.top = Math.max(8, Math.min(innerHeight - 44, r.top + 10)) + "px";
    btn.style.left = Math.max(8, Math.min(innerWidth - w - 8, r.right - w - 10)) + "px";
  }
  function schedule() {
    if (queued) return;
    queued = true;
    requestAnimationFrame(place);
  }

  addEventListener("scroll", schedule, { passive: true, capture: true });
  addEventListener("resize", schedule, { passive: true });
  document.addEventListener("fullscreenchange", schedule);
  document.addEventListener("loadedmetadata", schedule, true);
  document.addEventListener("play", schedule, true);

  var lastScan = 0;
  var mo = new MutationObserver(function () {
    schedule();
    var now = Date.now();
    if (generic && now - lastScan > 1500) {
      lastScan = now;
      setTimeout(scanGeneric, 300);
    }
  });
  mo.observe(document.documentElement, { childList: true, subtree: true });
  schedule();
})();
