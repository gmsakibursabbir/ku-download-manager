import { useEffect, useLayoutEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import type { AddRequest } from "../lib/types";
import { settingsStore } from "../lib/store";
import { ToastHost } from "../ui/overlays";
import { AddDownloadDialog } from "./AddDownloadDialog";
import { AppContext, type AppApi } from "./context";

/**
 * Standalone "Download File" window (IDM-style) for a download caught in the
 * browser. Renders only the add dialog; closes itself when done.
 */
export function PromptWindow({ id }: { id: string }) {
  const [request, setRequest] = useState<AddRequest | null>(null);
  const settings = settingsStore.use();
  // One handle for the window's lifetime (getCurrentWindow() returns a new object each call).
  const win = useMemo(() => getCurrentWindow(), []);

  useEffect(() => {
    void invoke<AddRequest | null>("get_prompt", { id }).then((r) => (r ? setRequest(r) : void win.close()));
  }, [id, win]);

  // Theme only; the popup has no density or material choices.
  useEffect(() => {
    const root = document.documentElement;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const theme = settings?.theme ?? "system";
    root.dataset.theme = theme === "system" ? (mq.matches ? "dark" : "light") : theme;
    root.dataset.material = "none";
  }, [settings?.theme]);

  // Fit the window to the dialog (it grows with "More options"), then show it.
  useLayoutEffect(() => {
    if (!request) return;
    const form = document.querySelector<HTMLElement>(".dialog");
    if (!form) return;
    let shown = false;
    const fit = () => {
      const h = Math.min(Math.ceil(form.scrollHeight) + 2, window.screen.availHeight - 80);
      void win.setSize(new LogicalSize(window.innerWidth, h)).then(() => {
        if (!shown) {
          shown = true;
          void win.show().then(() => win.setFocus());
        }
      });
    };
    const ro = new ResizeObserver(fit);
    ro.observe(form);
    fit();
    return () => ro.disconnect();
  }, [request, win]);

  const close = () => void win.close();
  const api = useMemo(
    () =>
      ({
        openMedia: (req) => {
          void invoke("open_media_in_main", { url: req?.url ?? "", cookies: req?.cookies ?? [] }).finally(close);
        },
      }) as AppApi,
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  if (!request) return null;
  return (
    <AppContext.Provider value={api}>
      <AddDownloadDialog prefill={request} onClose={close} />
      <ToastHost />
    </AppContext.Provider>
  );
}
