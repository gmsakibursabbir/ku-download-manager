// Interface translations for the Android app.
//
//   node android/scripts/locales.mjs          write app/src/main/assets/i18n/<code>.json
//   node android/scripts/locales.mjs --check  also fail when a language misses a string
//
// Keys are the English text. The desktop's translations (app/src/lib/locales)
// are reused; strings only the phone shows live in android/i18n/<code>.json.
import { readFileSync, readdirSync, writeFileSync, mkdirSync, existsSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..");
const desktop = join(root, "app", "src", "lib", "locales");
const extra = join(here, "..", "i18n");
const out = join(here, "..", "app", "src", "main", "assets", "i18n");
const sources = join(here, "..", "app", "src", "main", "java");

function loadTs(file) {
  const src = readFileSync(file, "utf8")
    .replace(/^\s*const \w+\s*:\s*Record<string,\s*string>\s*=\s*/m, "return ")
    .replace(/^\s*export default \w+;\s*$/m, "");
  return Function(src)();
}

/** Every t("…") / tf("…") literal in the Kotlin sources. */
function usedStrings() {
  const found = new Set();
  const walk = (d) => {
    for (const n of readdirSync(d)) {
      const p = join(d, n);
      if (statSync(p).isDirectory()) walk(p);
      else if (p.endsWith(".kt")) {
        const s = readFileSync(p, "utf8");
        for (const m of s.matchAll(/\bt[fe]?\(\s*"((?:[^"\\]|\\.)*)"/g)) {
          if (m[0].startsWith("te(")) continue;
          if (m[1].includes("${")) continue;
          found.add(JSON.parse(`"${m[1]}"`));
        }
      }
    }
  };
  walk(sources);
  return found;
}

mkdirSync(out, { recursive: true });
const used = usedStrings();
const check = process.argv.includes("--check");
let failed = false;
const codes = readdirSync(desktop).filter((f) => f.endsWith(".ts")).map((f) => f.replace(/\.ts$/, ""));
for (const code of codes) {
  const base = loadTs(join(desktop, `${code}.ts`));
  const extraFile = join(extra, `${code}.json`);
  const phone = existsSync(extraFile) ? JSON.parse(readFileSync(extraFile, "utf8")) : {};
  const merged = { ...base, ...phone };
  writeFileSync(join(out, `${code}.json`), JSON.stringify(merged));
  const missing = [...used].filter((k) => !(k in merged));
  console.log(`${code}: ${Object.keys(merged).length} strings, ${missing.length} of ${used.size} used strings missing`);
  if (missing.length && check) {
    failed = true;
    for (const m of missing.slice(0, 20)) console.log(`  - ${m}`);
  }
}
if (process.argv.includes("--missing")) {
  const base = loadTs(join(desktop, "bn.ts"));
  const missing = [...used].filter((k) => !(k in base)).sort();
  writeFileSync(join(here, "..", "i18n", "_missing.json"), JSON.stringify(missing, null, 1));
  console.log(`${missing.length} strings not in the desktop translations → android/i18n/_missing.json`);
}
process.exit(failed ? 1 : 0);
