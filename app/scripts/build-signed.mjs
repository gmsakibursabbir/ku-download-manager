// Windows release build with every executable Authenticode-signed:
// kudownloader.exe, the sidecars (ku, ku-native-host, aria2c) and the installer.
//
// Certificate: KU_SIGN_THUMBPRINT, or ../.secrets/windows-signing.json
// (made by scripts/windows/new-signing-cert.ps1). Tauri runs signtool from the
// Windows SDK with SHA-256 and an RFC 3161 timestamp.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const app = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const secrets = path.resolve(app, "..", ".secrets", "windows-signing.json");

if (process.platform !== "win32") {
  console.error("build:signed signs Windows executables; run it on Windows.");
  process.exit(1);
}
let thumbprint = process.env.KU_SIGN_THUMBPRINT;
if (!thumbprint && fs.existsSync(secrets)) thumbprint = JSON.parse(fs.readFileSync(secrets, "utf8").replace(/^﻿/, "")).thumbprint;
if (!thumbprint) {
  console.error("No signing certificate. Create one with:\n  powershell -ExecutionPolicy Bypass -File scripts\\windows\\new-signing-cert.ps1 -Trust\nor set KU_SIGN_THUMBPRINT to a certificate in Cert:\\CurrentUser\\My.");
  process.exit(1);
}
thumbprint = thumbprint.replace(/\s/g, "").toUpperCase();

// Git-ignored, like the one CI writes.
const cfg = path.join(app, "src-tauri", "tauri.winsign.conf.json");
fs.writeFileSync(cfg, JSON.stringify({ bundle: { windows: { certificateThumbprint: thumbprint, digestAlgorithm: "sha256", timestampUrl: "http://timestamp.digicert.com" } } }, null, 2));

const extra = process.argv.slice(2);
const r = spawnSync("pnpm", ["tauri", "build", "--config", "src-tauri/tauri.bundle.conf.json", "--config", "src-tauri/tauri.winsign.conf.json", ...extra], {
  cwd: app,
  stdio: "inherit",
  shell: true,
});
if (r.status !== 0) process.exit(r.status ?? 1);

// Show what was signed.
const bundle = path.resolve(app, "..", "target", "release", "bundle", "nsis");
for (const f of fs.existsSync(bundle) ? fs.readdirSync(bundle).filter((n) => n.endsWith(".exe")) : []) {
  const file = path.join(bundle, f);
  const v = spawnSync("powershell", ["-NoProfile", "-Command", `(Get-AuthenticodeSignature '${file.replace(/'/g, "''")}') | Format-List Status,SignerCertificate`], { encoding: "utf8" });
  console.log(`\n${f}\n${v.stdout.trim()}`);
}
