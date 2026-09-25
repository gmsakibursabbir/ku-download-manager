// KuDownloader site: small, dependency-free interactions.
(() => {
  "use strict";
  const REPO = "kuduyDigital/ku-download-manager";
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const dark = () => matchMedia("(prefers-color-scheme: dark)").matches;
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];

  // ───────── Navigation shadow once the page moves ─────────
  const nav = $("[data-nav]");
  const onScrollNav = () => nav.classList.toggle("scrolled", scrollY > 8);
  addEventListener("scroll", onScrollNav, { passive: true });
  onScrollNav();

  // ───────── Reveal on scroll ─────────
  const io = new IntersectionObserver(
    (entries) => entries.forEach((e) => {
      if (e.isIntersecting) {
        e.target.classList.add("in");
        io.unobserve(e.target);
      }
    }),
    { rootMargin: "0px 0px -8% 0px" },
  );
  $$(".reveal").forEach((el, i) => {
    el.style.transitionDelay = `${Math.min(i % 4, 3) * 70}ms`;
    io.observe(el);
  });

  // ───────── Dash field: small coloured dashes that swirl around the pointer ─────────
  const COLORS = ["#2563eb", "#4f8cff", "#d4ff00", "#16a34a", "#f97316", "#ec4899", "#7c3aed", "#06b6d4", "#facc15"];
  function field(canvas) {
    const ctx = canvas.getContext("2d");
    const dense = canvas.hasAttribute("data-dense");
    let w = 0, h = 0, dpr = 1, dots = [], raf = 0, visible = true, t0 = performance.now();
    const pointer = { x: 0, y: 0, tx: 0, ty: 0, active: false };

    function build() {
      const r = canvas.getBoundingClientRect();
      dpr = Math.min(devicePixelRatio || 1, 2);
      w = r.width;
      h = r.height;
      canvas.width = w * dpr;
      canvas.height = h * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      const count = Math.round((w * h) / (dense ? 1900 : 2300));
      dots = Array.from({ length: Math.min(count, 1100) }, () => ({
        hx: Math.random() * w,
        hy: Math.random() * h,
        x: 0,
        y: 0,
        len: 3 + Math.random() * 6,
        phase: Math.random() * Math.PI * 2,
        speed: 0.15 + Math.random() * 0.35,
        orbit: 4 + Math.random() * 14,
        color: COLORS[(Math.random() * COLORS.length) | 0],
        a: 0,
      }));
      if (!pointer.active) {
        pointer.x = pointer.tx = w / 2;
        pointer.y = pointer.ty = h * 0.55;
      }
    }

    function frame(now) {
      const t = (now - t0) / 1000;
      // The focus point eases toward the pointer, or wanders on its own.
      if (!pointer.active) {
        pointer.tx = w / 2 + Math.cos(t * 0.35) * w * 0.22;
        pointer.ty = h * 0.55 + Math.sin(t * 0.5) * h * 0.16;
      }
      pointer.x += (pointer.tx - pointer.x) * 0.08;
      pointer.y += (pointer.ty - pointer.y) * 0.08;
      ctx.clearRect(0, 0, w, h);
      const reach = Math.max(w, h) * 0.42;
      for (const d of dots) {
        const ox = Math.cos(t * d.speed + d.phase) * d.orbit;
        const oy = Math.sin(t * d.speed * 1.3 + d.phase) * d.orbit;
        let x = d.hx + ox;
        let y = d.hy + oy;
        const dx = x - pointer.x;
        const dy = y - pointer.y;
        const dist = Math.hypot(dx, dy) || 1;
        // Pushed outward near the focus, like a ripple around it.
        const push = Math.max(0, 1 - dist / 170) * 38;
        x += (dx / dist) * push;
        y += (dy / dist) * push;
        // Brightest in a ring around the focus, fading with distance.
        const ring = Math.exp(-(((dist - 150) / 120) ** 2));
        const near = Math.max(0, 1 - dist / reach);
        const target = 0.08 + ring * 0.85 + near * 0.25;
        d.a += (Math.min(target, 1) - d.a) * 0.1;
        // Dashes point along the swirl around the focus.
        const ang = Math.atan2(dy, dx) + Math.PI / 2 + Math.sin(t + d.phase) * 0.25;
        const lx = Math.cos(ang) * d.len;
        const ly = Math.sin(ang) * d.len;
        ctx.globalAlpha = d.a;
        ctx.strokeStyle = d.color;
        ctx.lineWidth = 2;
        ctx.lineCap = "round";
        ctx.beginPath();
        ctx.moveTo(x - lx / 2, y - ly / 2);
        ctx.lineTo(x + lx / 2, y + ly / 2);
        ctx.stroke();
      }
      ctx.globalAlpha = 1;
      if (visible && !reduced) raf = requestAnimationFrame(frame);
    }

    const host = canvas.parentElement;
    host.addEventListener("pointermove", (e) => {
      const r = canvas.getBoundingClientRect();
      pointer.tx = e.clientX - r.left;
      pointer.ty = e.clientY - r.top;
      pointer.active = true;
    });
    host.addEventListener("pointerleave", () => (pointer.active = false));
    new IntersectionObserver(([e]) => {
      visible = e.isIntersecting;
      cancelAnimationFrame(raf);
      if (visible) raf = requestAnimationFrame(frame);
    }).observe(canvas);
    new ResizeObserver(() => {
      build();
      if (reduced) frame(performance.now());
    }).observe(canvas);
    build();
    raf = requestAnimationFrame(frame);
  }
  $$("[data-field]").forEach(field);

  // ───────── The product frame stands up as it scrolls into view ─────────
  const frameEl = $("[data-tilt]");
  if (frameEl && !reduced) {
    const tilt = () => {
      const r = frameEl.getBoundingClientRect();
      const p = Math.min(1, Math.max(0, (innerHeight - r.top) / (innerHeight * 0.9)));
      frameEl.style.setProperty("--tilt", `${(1 - p) * 20}deg`);
      frameEl.style.setProperty("--scale", `${0.9 + p * 0.1}`);
    };
    addEventListener("scroll", tilt, { passive: true });
    tilt();
  }

  // ───────── Statement: words light up with scroll ─────────
  const words = $("[data-words]");
  if (words) {
    words.innerHTML = words.textContent.trim().split(/\s+/).map((w) => `<span class="w">${w}</span>`).join(" ");
    const spans = $$(".w", words);
    const light = () => {
      const r = words.getBoundingClientRect();
      const p = Math.min(1, Math.max(0, (innerHeight * 0.85 - r.top) / (r.height + innerHeight * 0.35)));
      const n = Math.round(p * spans.length);
      spans.forEach((s, i) => s.classList.toggle("on", i < n));
    };
    if (reduced) spans.forEach((s) => s.classList.add("on"));
    else {
      addEventListener("scroll", light, { passive: true });
      light();
    }
  }

  // ───────── Cards: spotlight follows the pointer ─────────
  $$("[data-glow]").forEach((card) =>
    card.addEventListener("pointermove", (e) => {
      const r = card.getBoundingClientRect();
      card.style.setProperty("--mx", `${e.clientX - r.left}px`);
      card.style.setProperty("--my", `${e.clientY - r.top}px`);
    }),
  );

  // ───────── Features: pick one, the picture follows (and cycles on its own) ─────────
  const list = $("[data-features]");
  const shot = $("[data-feature-img]");
  if (list && shot) {
    const items = $$("li", list);
    let current = 0, timer = 0, userPicked = false;
    const src = (li) => (dark() && li.dataset.shotDark) || li.dataset.shot;
    const show = (i) => {
      current = i;
      items.forEach((li, j) => li.classList.toggle("active", j === i));
      const next = src(items[i]);
      if (shot.getAttribute("src") === next) return;
      shot.classList.add("swap");
      setTimeout(() => {
        shot.src = next;
        shot.onload = () => shot.classList.remove("swap");
      }, 220);
    };
    items.forEach((li, i) => {
      li.tabIndex = 0;
      li.addEventListener("click", () => { userPicked = true; show(i); });
      li.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); userPicked = true; show(i); } });
    });
    shot.src = src(items[0]);
    // Advance while the section is on screen, until the user chooses.
    new IntersectionObserver(([e]) => {
      clearInterval(timer);
      if (e.isIntersecting && !reduced) timer = setInterval(() => { if (!userPicked) show((current + 1) % items.length); }, 4200);
    }, { threshold: 0.4 }).observe(list);
  }

  // ───────── People rail arrows ─────────
  const rail = $("[data-rail]");
  $$("[data-scroll]").forEach((b) =>
    b.addEventListener("click", () => rail.scrollBy({ left: Number(b.dataset.scroll) * (rail.clientWidth * 0.8), behavior: reduced ? "auto" : "smooth" })),
  );

  // ───────── KuAirSend radar: the app's own pixel animals ─────────
  const SPRITES = {
    cat: { half: ["o.....", "oo....", "obo...", "oaoooo", "oaaaaa", "oakaaa", "oakaaa", "obaaan", "oaaaaa", ".oaaaa", "..oooo"], pal: { o: "#3b2314", a: "#f39c38", b: "#ffd9a8", k: "#1b1b1b", n: "#ff8fab" }, bg: "#fff0dc" },
    penguin: { half: ["..oooo", ".oaaaa", "oaaaaa", "oaawww", "oawkww", "oawwwn", "oawwww", "oawwww", ".oawww", "..oooo"], pal: { o: "#10131c", a: "#2c3a5a", w: "#f5f7fb", k: "#10131c", n: "#ffa726" }, bg: "#e1eafb" },
    fox: { half: ["o.....", "oo....", "obo...", "obbo..", "oaaooo", "oaaaaa", "oakaaa", "owaaaa", "owwaaa", ".owwwd", "..owww", "...ooo"], pal: { o: "#3a1f10", a: "#f06a24", b: "#ffc9a0", w: "#fff4e6", d: "#1b1b1b", k: "#1b1b1b" }, bg: "#ffe6d8" },
    frog: { half: [".ooo..", "owwwo.", "owkwoo", "owwwoa", "oaaaaa", "oaaaaa", "onaaaa", "oaoooo", ".oaaaa", "..oooo"], pal: { o: "#1d3b17", a: "#5cc15a", w: "#ffffff", k: "#1b1b1b", n: "#ff9fb5" }, bg: "#e2f6dd" },
  };
  $$("canvas[data-animal]").forEach((c) => {
    const s = SPRITES[c.dataset.animal];
    const rows = s.half.length;
    const size = 16;
    c.width = size;
    c.height = size;
    const g = c.getContext("2d");
    g.fillStyle = s.bg;
    g.fillRect(0, 0, size, size);
    const top = Math.floor((size - rows) / 2);
    s.half.forEach((row, y) => {
      [...(row + [...row].reverse().join(""))].forEach((ch, x) => {
        if (ch === ".") return;
        g.fillStyle = s.pal[ch] || "#f0f";
        g.fillRect(x + 2, y + top, 1, 1);
      });
    });
  });

  // ───────── Your system: label the buttons and link the right file ─────────
  function detectOs() {
    const ua = navigator.userAgent;
    const plat = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
    if (/android/i.test(ua)) return "android";
    if (/iphone|ipad|ipod/i.test(ua)) return "ios";
    if (/win/i.test(plat) || /windows/i.test(ua)) return "windows";
    if (/mac/i.test(plat) || /mac os/i.test(ua)) return "mac";
    if (/linux|x11/i.test(plat + ua)) return "linux";
    return "";
  }
  const os = detectOs();
  const NAMES = { windows: "Windows", mac: "macOS", linux: "Linux", android: "Android" };
  if (NAMES[os]) {
    $$("[data-os-label] span").forEach((s) => (s.textContent = `Download for ${NAMES[os]}`));
    const card = $(`[data-platform="${os}"]`);
    if (card) {
      card.classList.add("mine");
      card.parentElement.prepend(card);
    }
  }
  $$("[data-platform-jump]").forEach((a) =>
    a.addEventListener("click", () => setTimeout(() => $(`[data-platform="${a.dataset.platformJump}"]`)?.classList.add("mine"), 400)),
  );

  // Direct links from the latest release (the release page stays the fallback).
  fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } })
    .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
    .then((rel) => {
      const version = (rel.tag_name || "").replace(/^v/, "");
      const assets = rel.assets || [];
      const find = (suffix) => assets.find((a) => a.name.endsWith(suffix));
      $$("[data-asset]").forEach((a) => {
        const hit = find(a.dataset.asset);
        if (hit) a.href = hit.browser_download_url;
      });
      const primary = {
        windows: "_x64-setup.exe",
        mac: "_aarch64.dmg",
        linux: "_amd64.AppImage",
        android: "_android-arm64-v8a.apk",
      }[os];
      const hit = primary && find(primary);
      if (hit) $$("[data-os-label]").forEach((a) => (a.href = hit.browser_download_url));
      if (version) {
        const v = $("[data-version]");
        if (v) v.textContent = `Version ${version} · Windows · macOS · Linux · Android`;
        const vl = $("[data-version-long]");
        if (vl) vl.textContent = `Version ${version}. Free and open source. Pick your system.`;
      }
    })
    .catch(() => {});
})();
