// Builds dist/chrome and dist/firefox from src/ + manifest.base.json.
// No bundler: the extension is plain JavaScript by design (small, auditable).
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const base = JSON.parse(readFileSync(join(here, "manifest.base.json"), "utf8"));
const key = readFileSync(join(here, "chrome-public-key.txt"), "utf8").trim();

const targets = {
  chrome: {
    ...base,
    key, // fixes the extension id so the native host allow-list matches
    background: { service_worker: "background.js" },
    minimum_chrome_version: "110",
  },
  firefox: {
    ...base,
    background: { scripts: ["background.js"] },
    browser_specific_settings: { gecko: { id: "kudownloader@kuduy.digital", strict_min_version: "115.0" } },
  },
};

for (const [name, manifest] of Object.entries(targets)) {
  const out = join(here, "dist", name);
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  cpSync(join(here, "src"), out, { recursive: true });
  cpSync(join(here, "icons"), join(out, "icons"), { recursive: true });
  writeFileSync(join(out, "manifest.json"), JSON.stringify(manifest, null, 2));
  console.log(`built ${out}`);
}
