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
import { PromptWindow } from "./app/PromptWindow";

// Popup windows ("Download File" for a browser download) load the same page.
const prompt = location.hash.match(/^#prompt=(\d+)$/)?.[1];
if (prompt) document.documentElement.dataset.window = "prompt";

createRoot(document.getElementById("root")!).render(<StrictMode>{prompt ? <PromptWindow id={prompt} /> : <App />}</StrictMode>);
