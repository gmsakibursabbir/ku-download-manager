import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { Power } from "lucide-react";
import { AppContext, type AppApi, type ListFilter, type View } from "./app/context";
import { invoke } from "@tauri-apps/api/core";
import { Sidebar, TitleBar } from "./app/Shell";
import { DownloadsView, pasteLink } from "./app/downloads/DownloadsView";
import { AddDownloadDialog } from "./app/AddDownloadDialog";
import { applyEvent, getDownload, onCoreEvent, settingsStore, startStore } from "./lib/store";
import { api } from "./lib/api";
import type { AddRequest, CoreEvent, GrabRequest, MediaRequest } from "./lib/types";
import { Button, Checkbox, Icon } from "./ui/primitives";
import { ConfirmDialog, MenuHost, ToastHost, toast } from "./ui/overlays";
import { run } from "./app/downloads/actions";

// Secondary screens load on demand to keep startup light.
const VideoDownloaderView = lazy(() => import("./views/VideoDownloaderView").then((m) => ({ default: m.VideoDownloaderView })));
const GrabberView = lazy(() => import("./views/GrabberView").then((m) => ({ default: m.GrabberView })));
const BatchView = lazy(() => import("./views/BatchView").then((m) => ({ default: m.BatchView })));
const BrowserView = lazy(() => import("./views/BrowserView").then((m) => ({ default: m.BrowserView })));
const MediaView = lazy(() => import("./views/BrowserView").then((m) => ({ default: m.MediaView })));
const ScheduledView = lazy(() => import("./views/ScheduledView").then((m) => ({ default: m.ScheduledView })));
const SettingsView = lazy(() => import("./views/SettingsView").then((m) => ({ default: m.SettingsView })));

const NAV_ORDER: Partial<ListFilter>[] = [{ scope: "all" }, { scope: "unfinished" }, { scope: "finished" }, { scope: "queue", queueId: "main" }];

function useTheme(setMaterial: (m: string) => void) {
  const s = settingsStore.use();
  useEffect(() => {
    const root = document.documentElement;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const theme = s?.theme ?? "system";
      const resolved = theme === "system" ? (mq.matches ? "dark" : "light") : theme;
      root.dataset.theme = resolved;
      // Solid window surfaces; no Mica/Acrylic compositing.
      root.dataset.material = "none";
      void invoke("set_window_theme", { dark: resolved === "dark" }).catch(() => {});
      setMaterial("none");
      try {
        localStorage.setItem("ku-theme", theme);
      } catch {
        /* storage unavailable */
      }
    };
    apply();
    root.dataset.density = s?.compact ? "compact" : "comfortable";
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [s?.theme, s?.compact, setMaterial]);
}

function PowerBanner({ action, seconds, onDone }: { action: string; seconds: number; onDone: () => void }) {
  const [left, setLeft] = useState(seconds);
  useEffect(() => {
    const t = setInterval(() => setLeft((l) => Math.max(0, l - 1)), 1000);
    return () => clearInterval(t);
  }, []);
  return (
    <div className="power-banner" role="alert">
      <Icon icon={Power} />
      <span>
        Downloads finished. Your computer will {action === "sleep" ? "sleep" : "shut down"} in <b className="num">{left}s</b>.
      </span>
      <span className="spacer" />
      <Button
        size="sm"
        variant="primary"
        onClick={() => {
          void api.cancelPower();
          onDone();
        }}
      >
        Cancel
      </Button>
    </div>
  );
}

export default function App() {
  const [view, setView] = useState<View>("downloads");
  const [selection, setSelection] = useState<Set<string>>(new Set());
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [filter, setFilter] = useState<ListFilter>({ scope: "all", category: "" });
  const [material, setMaterial] = useState("none");
  const [search, setSearch] = useState("");
  const [addPrefill, setAddPrefill] = useState<Partial<AddRequest> | null>(null);
  const [mediaPrefill, setMediaPrefill] = useState<Partial<MediaRequest> | null>(null);
  const [grabPrefill, setGrabPrefill] = useState<GrabRequest | null>(null);
  const [removeIds, setRemoveIds] = useState<string[] | null>(null);
  const [deleteFiles, setDeleteFiles] = useState(false);
  const [power, setPower] = useState<{ action: string; seconds: number } | null>(null);
  const [settingsSection, setSettingsSection] = useState("general");
  const [narrow, setNarrow] = useState(() => window.innerWidth < 1100);
  const [sidebarPref, setSidebarPref] = useState<boolean | null>(null);
  useTheme(setMaterial);

  const navigate = useCallback((v: View) => {
    setView(v);
    setSelection(new Set());
  }, []);
  const showList = useCallback((f: Partial<ListFilter>) => {
    setFilter({ scope: f.scope ?? "all", category: f.category ?? "", queueId: f.queueId });
    setView("downloads");
    setSelection(new Set());
  }, []);
  const openAdd = useCallback((p?: Partial<AddRequest>) => setAddPrefill(p ?? {}), []);
  const openMedia = useCallback(
    (req?: Partial<MediaRequest>) => {
      setMediaPrefill(req ? { ...req } : null);
      navigate("video");
    },
    [navigate],
  );
  const openGrabber = useCallback(
    (req?: GrabRequest) => {
      setGrabPrefill(req ?? null);
      navigate("grabber");
    },
    [navigate],
  );

  const api_: AppApi = useMemo(
    () => ({
      filter,
      showList,
      material,
      view,
      navigate,
      openAdd,
      openMedia,
      openGrabber,
      selection,
      setSelection,
      inspectorOpen,
      setInspectorOpen,
      search,
      setSearch,
      confirmRemove: (ids: string[]) => {
        setDeleteFiles(false);
        setRemoveIds(ids);
      },
      mediaPrefill,
      grabPrefill,
      settingsSection,
      openSettings: (s?: string) => {
        setSettingsSection(s ?? "general");
        navigate("settings");
      },
    }),
    [filter, showList, material, view, navigate, openAdd, openMedia, openGrabber, selection, inspectorOpen, search, mediaPrefill, grabPrefill, settingsSection],
  );

  // Core events that need UI decisions.
  useEffect(() => {
    const handle = (e: CoreEvent) => {
      switch (e.type) {
        case "promptAdd":
          setAddPrefill(e.request);
          break;
        case "promptMedia":
          openMedia(e.request);
          break;
        case "grab":
          openGrabber(e.request);
          break;
        case "notice":
          toast({
            level: e.level === "error" ? "error" : e.level === "warning" ? "warning" : "info",
            title: e.title,
            message: e.message,
            actions: e.downloadId
              ? [
                  {
                    label: "Show",
                    onClick: () => {
                      showList({ scope: "all" });
                      setSelection(new Set([e.downloadId!]));
                      setInspectorOpen(true);
                    },
                  },
                ]
              : undefined,
          });
          break;
        case "completed":
          if (document.hasFocus())
            toast({
              level: "success",
              title: "Download complete",
              message: e.name,
              actions: [
                { label: "Open", primary: true, onClick: () => void run(api.openFile(e.id), "Could not open the file") },
                { label: "Show in folder", onClick: () => void run(api.openFolder(e.id), "Could not open the folder") },
              ],
            });
          break;
        case "queueDone":
          toast({ level: "success", title: "Queue finished", message: `All downloads in “${e.name}” are done.` });
          break;
        case "clipboardUrl":
          toast({
            level: "info",
            title: "Download the copied link?",
            message: e.url,
            actions: [{ label: "Download", primary: true, onClick: () => openAdd({ url: e.url, source: "clipboard" }) }],
          });
          break;
        case "powerCountdown":
          setPower({ action: e.action, seconds: e.seconds });
          break;
        case "powerCancelled":
          setPower(null);
          break;
      }
    };
    const off = onCoreEvent(handle);
    void startStore().then((pending) => pending.forEach(applyEvent));
    return () => {
      off();
    };
  }, [navigate, openAdd, openMedia, openGrabber]);

  useEffect(() => {
    const onResize = () => setNarrow(window.innerWidth < 1100);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  // Global keyboard shortcuts.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      const typing = t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable;
      const modal = !!document.querySelector(".overlay");
      const ctrl = e.ctrlKey || e.metaKey;
      if (modal) return;
      if (ctrl && e.key.toLowerCase() === "n") {
        e.preventDefault();
        openAdd();
      } else if (ctrl && e.key.toLowerCase() === "f") {
        e.preventDefault();
        if (view !== "downloads") showList({ scope: "all" });
        setTimeout(() => window.dispatchEvent(new Event("ku:find")), 0);
      } else if (ctrl && e.key === ",") {
        e.preventDefault();
        navigate("settings");
      } else if (ctrl && e.key.toLowerCase() === "i") {
        e.preventDefault();
        setInspectorOpen((v) => !v);
      } else if (ctrl && e.key >= "1" && e.key <= "4") {
        e.preventDefault();
        showList(NAV_ORDER[+e.key - 1]);
      } else if (ctrl && e.shiftKey && e.key.toLowerCase() === "o" && selection.size === 1) {
        e.preventDefault();
        void run(api.openFolder([...selection][0]), "Could not open the folder");
      } else if (ctrl && e.key.toLowerCase() === "v" && !typing) {
        e.preventDefault();
        void pasteLink(openAdd);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [openAdd, navigate, showList, view, selection]);

  // Block the webview's default context menu outside text fields.
  useEffect(() => {
    const onCtx = (e: MouseEvent) => {
      const t = e.target as HTMLElement;
      if (!(t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.closest(".selectable"))) e.preventDefault();
    };
    window.addEventListener("contextmenu", onCtx);
    return () => window.removeEventListener("contextmenu", onCtx);
  }, []);

  const collapsed = sidebarPref ?? narrow;
  const removeList = (removeIds ?? []).map(getDownload).filter(Boolean);
  const anyUnfinished = removeList.some((d) => d!.status !== "completed" && d!.status !== "seeding");

  let content;
  switch (view) {
    case "downloads":
      content = <DownloadsView />;
      break;
    case "finished":
    case "torrents":
    case "queue":
      content = <DownloadsView />;
      break;
    case "video":
      content = <VideoDownloaderView />;
      break;
    case "grabber":
      content = <GrabberView />;
      break;
    case "batch":
      content = <BatchView />;
      break;
    case "browser":
      content = <BrowserView />;
      break;
    case "media":
      content = <MediaView />;
      break;
    case "scheduled":
      content = <ScheduledView />;
      break;
    case "settings":
      content = <SettingsView />;
      break;
  }

  return (
    <AppContext.Provider value={api_}>
      <div className="app">
        <TitleBar />
        <div className="app-body">
          <Sidebar collapsed={collapsed} onToggle={() => setSidebarPref(!collapsed)} />
          <div style={{ display: "grid", gridTemplateRows: power ? "auto 1fr" : "1fr", minWidth: 0, minHeight: 0 }}>
            {power && <PowerBanner action={power.action} seconds={power.seconds} onDone={() => setPower(null)} />}
            <Suspense fallback={<div className="main" />}>{content}</Suspense>
          </div>
        </div>
      </div>
      {addPrefill && <AddDownloadDialog prefill={addPrefill} onClose={() => setAddPrefill(null)} />}
      {removeIds && (
        <ConfirmDialog
          title={removeIds.length === 1 ? "Remove download?" : `Remove ${removeIds.length} downloads?`}
          message={
            removeIds.length === 1 ? (
              <>
                “{removeList[0]?.name}” will be removed from the list{anyUnfinished ? " and stopped" : ""}.
              </>
            ) : (
              <>The selected downloads will be removed from the list{anyUnfinished ? " and stopped" : ""}.</>
            )
          }
          confirmLabel="Remove"
          danger={deleteFiles}
          onClose={() => setRemoveIds(null)}
          onConfirm={() => {
            const ids = removeIds;
            setSelection(new Set());
            void run(api.remove(ids, deleteFiles), "Could not remove");
          }}
        >
          <Checkbox checked={deleteFiles} onChange={setDeleteFiles}>
            Also delete {removeIds.length === 1 ? "the file" : "the files"} from disk
          </Checkbox>
        </ConfirmDialog>
      )}
      <MenuHost />
      <ToastHost />
    </AppContext.Provider>
  );
}
