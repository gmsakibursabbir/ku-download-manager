import type { Settings } from "./types";

/** Theme, accent, dark palette and density on <html> (main window and popups). */
export function applyAppearance(s: Settings | null | undefined): "light" | "dark" {
  const root = document.documentElement;
  const theme = s?.theme ?? "system";
  const resolved = theme === "system" ? (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light") : theme;
  root.dataset.theme = resolved;
  root.dataset.material = "none";
  root.dataset.accent = s?.accent && s.accent !== "blue" ? s.accent : "";
  root.dataset.palette = s?.darkPalette && s.darkPalette !== "default" ? s.darkPalette : "";
  root.dataset.density = s?.compact ? "compact" : "comfortable";
  return resolved;
}
