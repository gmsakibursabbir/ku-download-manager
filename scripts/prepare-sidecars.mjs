// Stage engine binaries for `tauri build`: Tauri expects
// src-tauri/binaries/<name>-<target-triple>[.exe].
//
//   node scripts/prepare-sidecars.mjs            (uses aria2c/yt-dlp/ffmpeg from PATH or KU_*_PATH)
//
// ku-native-host and ku are built from this workspace in release mode first.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { delimiter, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(import.meta.url), "..", "..");
const win = process.platform === "win32";
const exe = win ? ".exe" : "";
const triple = execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(/host: (\S+)/)[1];
const out = join(root, "app", "src-tauri", "binaries");
mkdirSync(out, { recursive: true });

function which(name) {
  const env = process.env[`KU_${name.toUpperCase().replace(/-/g, "_")}_PATH`];
  if (env && existsSync(env)) return env;
  for (const dir of (process.env.PATH ?? "").split(delimiter)) {
    const p = join(dir, name + exe);
    if (dir && existsSync(p)) return p;
  }
  // winget portable packages (Windows): %LOCALAPPDATA%\Microsoft\WinGet\Packages\*\**\name.exe
  if (win && process.env.LOCALAPPDATA) {
    const pk = join(process.env.LOCALAPPDATA, "Microsoft", "WinGet", "Packages");
    if (existsSync(pk)) {
      const walk = (d, depth) => {
        for (const e of readdirSync(d)) {
          const p = join(d, e);
          if (e.toLowerCase() === (name + exe).toLowerCase()) return p;
          if (depth < 3 && statSync(p).isDirectory()) {
            const r = walk(p, depth + 1);
            if (r) return r;
          }
        }
        return null;
      };
      const r = walk(pk, 0);
      if (r) return r;
    }
  }
  return null;
}

console.log("building ku-native-host and ku (release)…");
execFileSync("cargo", ["build", "--release", "-p", "ku-native-host", "-p", "ku-cli"], { cwd: root, stdio: "inherit" });

const items = {
  aria2c: which("aria2c"),
  "yt-dlp": which("yt-dlp"),
  ffmpeg: which("ffmpeg"),
  "ku-native-host": join(root, "target", "release", "ku-native-host" + exe),
  ku: join(root, "target", "release", "ku" + exe),
};
let missing = false;
for (const [name, src] of Object.entries(items)) {
  if (!src || !existsSync(src)) {
    console.error(`missing: ${name} (install it or set KU_${name.toUpperCase().replace(/-/g, "_")}_PATH)`);
    missing = true;
    continue;
  }
  const dst = join(out, `${name}-${triple}${exe}`);
  copyFileSync(src, dst);
  console.log(`${name.padEnd(15)} ← ${src}`);
}
process.exit(missing ? 1 : 0);
