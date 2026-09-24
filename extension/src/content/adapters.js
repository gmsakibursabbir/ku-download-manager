// Site adapters. Each adapter only answers "is this element downloadable
// media, and which page URL represents it?" — the actual extraction is done
// by yt-dlp in KuDownloader, so no site logic lives in KuCore.
//
// Adapter shape:
//   id        settings key (Settings › Media › Sites)
//   match()   true when the adapter applies to the current page
//   selector  CSS selector for hoverable media (event delegation, no scanning)
//   resolve(el) → { url, kind: "media" | "file", anchor: Element } | null

(() => {
  const adapters = (globalThis.__kuAdapters = []);

  const abs = (href) => {
    try {
      return new URL(href, location.href).href;
    } catch {
      return null;
    }
  };

  function youtubeUrl(href) {
    const u = new URL(href, location.href);
    if (u.pathname.startsWith("/shorts/")) return `https://www.youtube.com${u.pathname}`;
    const embed = u.pathname.match(/^\/embed\/([\w-]{6,})/);
    if (embed) return `https://www.youtube.com/watch?v=${embed[1]}`;
    const v = u.searchParams.get("v");
    if (v) return `https://www.youtube.com/watch?v=${encodeURIComponent(v)}`;
    if (u.pathname === "/playlist" && u.searchParams.get("list")) return `https://www.youtube.com/playlist?list=${encodeURIComponent(u.searchParams.get("list"))}`;
    return null;
  }

  adapters.push({
    id: "youtube",
    match: () => /(^|\.)youtube(-nocookie)?\.com$/.test(location.hostname),
    selector: [
      // Hover preview that YouTube lays over a thumbnail once it starts playing.
      "ytd-video-preview",
      "#video-preview",
      "a#thumbnail[href]",
      "ytd-thumbnail a[href]",
      "a.reel-item-endpoint[href]",
      "yt-lockup-view-model a[href*='/watch']",
      "a.ytp-videowall-still[href]",
      "#movie_player",
    ].join(","),
    resolve(el) {
      const preview = el.closest("ytd-video-preview, #video-preview");
      if (preview) {
        const link = preview.querySelector("a[href*='/watch'], a[href*='/shorts/']");
        const url = link && youtubeUrl(link.getAttribute("href"));
        return url ? { url, kind: "media", anchor: preview } : null;
      }
      if (el.id === "movie_player") {
        const url = youtubeUrl(location.href);
        return url ? { url, kind: "media", anchor: el } : null;
      }
      const href = el.getAttribute("href");
      const url = href && youtubeUrl(href);
      return url ? { url, kind: "media", anchor: el } : null;
    },
  });

  adapters.push({
    id: "vimeo",
    match: () => /(^|\.)vimeo\.com$/.test(location.hostname) && /^\/\d+/.test(location.pathname),
    selector: ".vp-video-wrapper, .player, [data-player]",
    resolve: (el) => ({ url: location.href.split("#")[0], kind: "media", anchor: el }),
  });

  adapters.push({
    id: "dailymotion",
    match: () => /(^|\.)dailymotion\.com$/.test(location.hostname) && location.pathname.startsWith("/video/"),
    selector: "#player, .player, [class*='Player_']",
    resolve: (el) => ({ url: location.href.split("#")[0], kind: "media", anchor: el }),
  });

  // Any other page or embedded frame: HTML5 video big enough to be content
  // (not a decoration). hover.js also finds it by pointer position, so player
  // overlays that cover the <video> element still count.
  adapters.push({
    id: "generic",
    generic: true,
    match: () => true,
    selector: "video",
    minWidth: 240,
    minHeight: 135,
    resolve(el) {
      const r = el.getBoundingClientRect();
      if (r.width < this.minWidth || r.height < this.minHeight) return null;
      const src = el.currentSrc || el.src;
      if (src && /^https?:/i.test(src) && !/\.m3u8|\.mpd/i.test(src)) {
        return { url: abs(src), kind: "file", anchor: el };
      }
      // Streams (blob:/MSE): let yt-dlp read the page.
      return { url: location.href.split("#")[0], kind: "media", anchor: el };
    },
  });
})();
