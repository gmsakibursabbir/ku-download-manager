/**
 * Interface translations. Keys are the English strings themselves, so an
 * untranslated string simply stays English. Each language lives in
 * `locales/<code>.ts` and is loaded on demand before the first render.
 * The choice is made in Settings › Appearance ("system" follows the OS) and
 * cached in localStorage so every window (main, popups) starts in it.
 */

export const LANGUAGES: { code: string; name: string }[] = [
  { code: "en", name: "English" },
  { code: "zh", name: "简体中文" },
  { code: "hi", name: "हिन्दी" },
  { code: "es", name: "Español" },
  { code: "ar", name: "العربية" },
  { code: "fr", name: "Français" },
  { code: "bn", name: "বাংলা" },
  { code: "pt", name: "Português" },
  { code: "ru", name: "Русский" },
  { code: "ja", name: "日本語" },
  { code: "de", name: "Deutsch" },
  { code: "ko", name: "한국어" },
];
const RTL = new Set(["ar"]);

const loaders = import.meta.glob<{ default: Record<string, string> }>("./locales/*.ts");

function resolve(pref: string): string {
  if (pref && pref !== "system") return pref;
  const nav = (navigator.language || "en").toLowerCase();
  return LANGUAGES.find((l) => nav === l.code || nav.startsWith(l.code + "-"))?.code ?? "en";
}

function stored(): string {
  try {
    return localStorage.getItem("ku-lang") ?? "system";
  } catch {
    return "system";
  }
}

let lang = "en";
let dict: Record<string, string> | undefined;

/** Load the chosen language before the first render. */
export async function initLanguage(): Promise<void> {
  lang = resolve(stored());
  const load = loaders[`./locales/${lang}.ts`];
  dict = load ? (await load().catch(() => ({ default: {} }))).default : undefined;
  document.documentElement.lang = lang;
  document.documentElement.dir = RTL.has(lang) ? "rtl" : "ltr";
}

/** Translate an interface string (English is the key). */
export function t(s: string): string {
  return dict?.[s] ?? s;
}

/** Remember the language setting; reloads the window when it changes. */
export function syncLanguage(setting: string | undefined) {
  if (!setting) return;
  try {
    if (stored() === setting) return;
    localStorage.setItem("ku-lang", setting);
  } catch {
    return;
  }
  if (resolve(setting) !== lang) location.reload();
}
