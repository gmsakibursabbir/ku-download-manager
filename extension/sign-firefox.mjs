// Signs dist/firefox with Mozilla (self-distribution, "unlisted") so release
// Firefox installs it permanently in one click. Needs AMO API credentials:
//   https://addons.mozilla.org/developers/addon/api/key/
//   AMO_JWT_ISSUER=... AMO_JWT_SECRET=... node sign-firefox.mjs
import { execFileSync } from "node:child_process";
import { readdirSync, renameSync } from "node:fs";
import { join } from "node:path";

const { AMO_JWT_ISSUER, AMO_JWT_SECRET } = process.env;
if (!AMO_JWT_ISSUER || !AMO_JWT_SECRET) {
  console.error("Set AMO_JWT_ISSUER and AMO_JWT_SECRET (addons.mozilla.org → Developer Hub → API keys).");
  process.exit(1);
}
const out = join(import.meta.dirname, "dist", "signed");
execFileSync("npx", ["--yes", "web-ext", "sign", "--source-dir", "dist/firefox", "--channel", "unlisted", "--artifacts-dir", out, "--api-key", AMO_JWT_ISSUER, "--api-secret", AMO_JWT_SECRET], { stdio: "inherit", shell: true, cwd: import.meta.dirname });
const xpi = readdirSync(out).find((f) => f.endsWith(".xpi"));
renameSync(join(out, xpi), join(import.meta.dirname, "dist", "kudmx.signed.xpi"));
console.log("dist/kudmx.signed.xpi ready — KuDownloader installs it in Firefox with one click.");
