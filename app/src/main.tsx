import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/inter";
import "@fontsource-variable/bricolage-grotesque";
import "./design-system/tokens.css";
import "./design-system/base.css";
import "./design-system/components.css";
import "./app/app.css";
import "./app/fluent.css";
import App from "./App";
import { initLanguage } from "./lib/i18n";
import { PromptWindow } from "./app/PromptWindow";
import { ProgressWindow } from "./app/ProgressWindow";

// Popup windows load the same page: "Download File" for a browser download,
// and the per-download progress window.
const prompt = location.hash.match(/^#prompt=(\d+)$/)?.[1];
const progress = location.hash.match(/^#progress=([\w-]+)$/)?.[1];
if (prompt) document.documentElement.dataset.window = "prompt";
if (progress) document.documentElement.dataset.window = "progress";

// The interface language is loaded before the first render.
void initLanguage().finally(() =>
  createRoot(document.getElementById("root")!).render(<StrictMode>{prompt ? <PromptWindow id={prompt} /> : progress ? <ProgressWindow id={progress} /> : <App />}</StrictMode>),
);
