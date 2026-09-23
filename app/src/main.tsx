import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/inter";
import "@fontsource-variable/bricolage-grotesque";
import "./design-system/tokens.css";
import "./design-system/base.css";
import "./design-system/components.css";
import "./app/app.css";
import App from "./App";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
