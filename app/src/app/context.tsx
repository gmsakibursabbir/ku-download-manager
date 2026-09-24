import { createContext, useContext } from "react";
import type { AddRequest, GrabRequest, MediaRequest } from "../lib/types";

export type View =
  | "downloads"
  | "queue"
  | "finished"
  | "scheduled"
  | "torrents"
  | "browser"
  | "media"
  | "video"
  | "grabber"
  | "batch"
  | "airsend"
  | "settings";

/** What the download list shows (the Categories tree). */
export interface ListFilter {
  scope: "all" | "unfinished" | "finished" | "queue";
  /** Category id; "" = every category. */
  category: string;
  queueId?: string;
}

export interface AppApi {
  filter: ListFilter;
  showList: (f: Partial<ListFilter>) => void;
  material: string;
  view: View;
  navigate: (v: View) => void;
  openAdd: (prefill?: Partial<AddRequest>) => void;
  openMedia: (req?: Partial<MediaRequest>) => void;
  openGrabber: (req?: GrabRequest) => void;
  selection: Set<string>;
  setSelection: (s: Set<string>) => void;
  inspectorOpen: boolean;
  setInspectorOpen: (v: boolean) => void;
  search: string;
  setSearch: (s: string) => void;
  confirmRemove: (ids: string[]) => void;
  /** Payload handed to a view when navigating (media URL, grab links). */
  mediaPrefill: Partial<MediaRequest> | null;
  grabPrefill: GrabRequest | null;
  settingsSection: string;
  openSettings: (section?: string) => void;
}

export const AppContext = createContext<AppApi | null>(null);

export function useApp(): AppApi {
  const v = useContext(AppContext);
  if (!v) throw new Error("AppContext missing");
  return v;
}
