const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function bytes(n: number | null | undefined, digits = 1): string {
  if (n == null || n < 0 || Number.isNaN(n)) return "—";
  if (n < 1024) return `${n} B`;
  let v = n;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(digits)} ${UNITS[i]}`;
}

export function speed(n: number): string {
  if (!n) return "0 B/s";
  return `${bytes(n)}/s`;
}

export function duration(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) return "—";
  const s = Math.round(seconds);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${String(s % 60).padStart(2, "0")}s`;
  const h = Math.floor(m / 60);
  if (h < 48) return `${h}h ${String(m % 60).padStart(2, "0")}m`;
  return `${Math.floor(h / 24)}d ${h % 24}h`;
}

export function clock(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds)) return "";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${r}` : `${m}:${r}`;
}

export function percent(done: number, total: number): number {
  if (!total || total <= 0) return 0;
  return Math.max(0, Math.min(100, (done / total) * 100));
}

export function eta(done: number, total: number, spd: number): number | null {
  if (!spd || !total || total <= done) return null;
  return (total - done) / spd;
}

export function host(url: string): string {
  try {
    const u = new URL(url);
    return u.host || u.protocol.replace(":", "");
  } catch {
    return url.startsWith("magnet:") ? "magnet link" : url.startsWith("torrent:") ? "torrent file" : "";
  }
}

export function relativeDate(ms: number | null | undefined): string {
  if (!ms) return "";
  const d = new Date(ms);
  const now = new Date();
  const sameDay = d.toDateString() === now.toDateString();
  const yesterday = new Date(now.getTime() - 86400000).toDateString() === d.toDateString();
  const time = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  if (sameDay) return `Today ${time}`;
  if (yesterday) return `Yesterday ${time}`;
  return d.toLocaleDateString(undefined, { day: "numeric", month: "short", year: d.getFullYear() === now.getFullYear() ? undefined : "numeric" }) + ` ${time}`;
}

/** Parse "2M", "500K", "1.5 MB/s" into bytes per second. */
export function parseRate(s: string): number | null {
  const m = s.trim().toUpperCase().match(/^(\d+(?:\.\d+)?)\s*([KMG]?)B?(?:\/S)?$/);
  if (!m) return null;
  const mul = { "": 1, K: 1024, M: 1024 ** 2, G: 1024 ** 3 }[m[2] as "" | "K" | "M" | "G"];
  return Math.round(parseFloat(m[1]) * mul);
}

export function extension(name: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
}
