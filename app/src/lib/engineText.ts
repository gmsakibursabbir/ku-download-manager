import { t, tf } from "./i18n";

/**
 * Messages the download engine (Rust) can produce, as templates. They are
 * translated for display with `te()` here, and the same table is handed to the
 * desktop shell so tray notifications are translated too.
 * `{name}` parts are values; values that are themselves engine messages
 * (reason, error, detail) are translated recursively.
 */
export const ENGINE_TEMPLATES = [
  "A disk write or read error occurred.",
  "A file with this name already exists.",
  "A network problem occurred.",
  "A queue needs a name",
  "A {algo} checksum must be {want} hexadecimal characters",
  "An FTP command failed.",
  "An option was invalid.",
  "Can't create {path}: {error}",
  "Can't save {path}: {error}",
  "Can't write {path}: {error}",
  "Checksum mismatch ({algo}): expected {expected}, got {actual}. The file was not saved.",
  "Checksum mismatch for {name} — the download was discarded.",
  "Checksum mismatch: expected {expected}, got {actual}. The file may be corrupted — download it again.",
  "Checksum verification failed: the file does not match the expected hash.",
  "Could not connect to the server. Check your network connection or proxy.",
  "Could not create or truncate the file.",
  "Could not create the destination folder.",
  "Could not create the folder {path}: {error}",
  "Could not create the folder {path}",
  "Could not find a free file name for {name}",
  "Could not open the existing file.",
  "Could not reach the site. Check your connection. ({detail})",
  "Could not reach {name}",
  "Could not scan {name}",
  "Dates must be YYYY-MM-DD",
  "Download engine restarted",
  "Download failed: {name}",
  "Download not found",
  "ETag changed",
  "Enter an IP address like 192.168.1.20",
  "Enter an http or https address.",
  "Enter an http or https page address.",
  "Files that changed on the server are being downloaded again.",
  "HTTP authentication failed.",
  "Invalid proxy configuration: {error}",
  "KuAirSend could not open a network port",
  "KuDownloader is shutting down",
  "Last-Modified changed",
  "Media downloads need an http or https address.",
  "Microsoft Defender could not scan the file (exit code {code}).",
  "Microsoft Defender found a threat.",
  "Microsoft Defender's scanner (MpCmdRun.exe) was not found.",
  "Nearby devices can't be found automatically on this network. Add them by IP address.",
  "No KuDownloader with KuAirSend on at {address} ({error})",
  "No downloadable media was found at this address.",
  "No yt-dlp build for this system; install it with your package manager.",
  "Not enough disk space. Free some space and resume the download.",
  "Not enough disk space.",
  "Power action failed",
  "Renaming the file failed.",
  "Request failed: {error}",
  "Scheduled downloads started",
  "Sending “{name}” failed",
  "Server allows only {n} connections; KuDownloader is using {used}.",
  "Server allows only {n} connection; KuDownloader is using {used}.",
  "Server rejected additional connections. KuDownloader reduced the connection count to {n} and will retry automatically.",
  "Size check failed: expected {expected} bytes, got {actual}.",
  "Started over: {reason}.",
  "That device is no longer nearby.",
  "That is not a KuDownloader device.",
  "The .torrent file is corrupted or missing information.",
  "The aria2 engine (aria2c) was not found. Reinstall KuDownloader or set its path in Settings › Advanced.",
  "The aria2 engine keeps stopping unexpectedly. Check disk space and antivirus software, then retry.",
  "The connection ended early ({got} of {expected} bytes).",
  "The connection timed out.",
  "The connection was lost.",
  "The download could not be verified.",
  "The download engine rejected this download: {error}",
  "The download failed (aria2 code {code}).",
  "The download failed.",
  "The download was aborted because the speed stayed below the minimum limit.",
  "The download was interrupted.",
  "The file arrived incomplete.",
  "The file changed on the server ({reason}).",
  "The file is no longer available (HTTP 410).",
  "The file is not on disk anymore: {path}",
  "The file was not found on the server (HTTP 404).",
  "The file was not found on the server.",
  "The magnet link is invalid.",
  "The main queue cannot be deleted",
  "The media engine (yt-dlp) is not installed yet. Download it from the Video Downloader or Settings › Advanced.",
  "The metalink document could not be parsed.",
  "The other device stopped the transfer ({status}).",
  "The piece length differs from the control file.",
  "The publisher's checksum list has no entry for {name}",
  "The request was malformed.",
  "The same file is already being downloaded.",
  "The same torrent is already being downloaded.",
  "The scanner reported a problem (exit code {code}). {detail}",
  "The sender cancelled.",
  "The sender is no longer waiting.",
  "The sender sent more data than it announced.",
  "The sender stopped responding.",
  "The server did not respond in time.",
  "The server does not support resuming, and the download could not continue.",
  "The server ignored a byte-range request.",
  "The server is overloaded or temporarily unavailable.",
  "The server is rate limiting requests (HTTP 429).",
  "The server name could not be resolved (DNS).",
  "The server redirects in a loop.",
  "The server refused access (HTTP 403). The link may have expired or require browser cookies.",
  "The server rejected the requested byte range (HTTP 416).",
  "The server reported an internal error (HTTP {code}).",
  "The server reported the resource as not found too many times.",
  "The server requires authentication (HTTP 401). Add credentials in the download options.",
  "The server responded with HTTP {code}.",
  "The server sent an invalid HTTP response.",
  "The server sent an invalid range response ({detail}).",
  "The server stopped responding.",
  "The site is rate limiting requests (HTTP 429). Try again later.",
  "The site requires you to be signed in to access this media. Use the browser extension so your session cookies are sent. ({detail})",
  "The site took too long to respond while reading media information.",
  "The torrent file is malformed.",
  "The virus scan took longer than 15 minutes",
  "The virus scanner program was not found: {path}",
  "There is nothing to send (the folder is empty).",
  "This address is a file ({type}), not a web page. Add it as a download instead.",
  "This media is DRM-protected. KuDownloader does not bypass DRM.",
  "This media is unavailable or has been removed.",
  "Threat found in {name}",
  "Throughput is scaling; increased to {n} connections.",
  "Times must be in HH:MM format",
  "Too many redirects.",
  "Turn on KuAirSend first.",
  "Unknown bandwidth profile",
  "Unknown queue",
  "Unknown tool: {name}",
  "Unsupported address. KuDownloader accepts http, https, ftp, sftp and magnet links.",
  "Virus scanning is off",
  "Windows refused to enter sleep",
  "Writing failed: {error}",
  "aria2 stopped unexpectedly. KuDownloader restarted it and is resuming your downloads.",
  "{name} asked to wait (too many attempts). Try again in a minute.",
  "{name} does not have enough free space for this.",
  "{name} is receiving from someone else. Try again in a moment.",
  "{name} could not start the download.",
  "{name} refused the transfer ({status}).",
  "{n} updated in “{queue}”",
  "“{name}” can't be read",
  "“{name}” started its queue.",
] as const;

/** Values that are themselves messages and get translated too. */
const NESTED = new Set(["reason", "error", "detail"]);

type Part = string | { name: string };
const parsed: { key: string; parts: Part[] }[] = ENGINE_TEMPLATES.map((key) => ({
  key,
  parts: key.split(/(\{\w+\})/).map((p) => (/^\{\w+\}$/.test(p) ? { name: p.slice(1, -1) } : p)),
}))
  // Most specific first: longer literal text wins.
  .sort((a, b) => literalLength(b.parts) - literalLength(a.parts));

function literalLength(parts: Part[]) {
  return parts.reduce((n, p) => n + (typeof p === "string" ? p.length : 0), 0);
}

/** Match `s` against a template; returns the placeholder values. */
export function matchTemplate(parts: Part[], s: string): Record<string, string> | null {
  const vars: Record<string, string> = {};
  let pos = 0;
  for (let i = 0; i < parts.length; i++) {
    const p = parts[i];
    if (typeof p === "string") {
      if (!s.startsWith(p, pos)) return null;
      pos += p.length;
      continue;
    }
    const next = parts[i + 1];
    let end: number;
    if (next === undefined || next === "") end = s.length;
    else {
      const lit = next as string;
      // The last literal must end the message; others take the first match.
      end = i + 2 >= parts.length ? s.lastIndexOf(lit) : s.indexOf(lit, pos + 1);
      if (end < pos + 1 || (i + 2 >= parts.length && end + lit.length !== s.length)) return null;
    }
    vars[p.name] = s.slice(pos, end);
    pos = end;
  }
  return pos === s.length ? vars : null;
}

function one(s: string): string | null {
  for (const { key, parts } of parsed) {
    const vars = matchTemplate(parts, s);
    if (!vars) continue;
    const filled: Record<string, string> = {};
    for (const [k, v] of Object.entries(vars)) filled[k] = NESTED.has(k) ? te(v) : v;
    return Object.keys(filled).length ? tf(key, filled) : t(key);
  }
  return null;
}

/** Translate a message from the engine; unknown text is left as it is. */
export function te(msg: string | null | undefined): string {
  if (!msg) return msg ?? "";
  const whole = one(msg);
  if (whole !== null) return whole;
  // A known sentence followed by more text (aria2 appends its own detail):
  // translate the sentence, then whatever follows the same way.
  for (let i = msg.indexOf(". "); i > 0; i = msg.indexOf(". ", i + 1)) {
    const head = one(msg.slice(0, i + 1));
    if (head !== null) return `${head} ${te(msg.slice(i + 2))}`;
  }
  return msg;
}

/** Engine message or interface string, whichever it is. */
export function tx(s: string): string {
  const r = te(s);
  return r === s ? t(s) : r;
}
