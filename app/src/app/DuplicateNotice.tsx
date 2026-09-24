import { useEffect, useState } from "react";
import { t } from "../lib/i18n";
import { invoke } from "@tauri-apps/api/core";
import { CopyCheck } from "lucide-react";
import { api, errorText } from "../lib/api";
import type { Download } from "../lib/types";
import * as fmt from "../lib/format";
import { Button, Notice } from "../ui/primitives";
import { toast } from "../ui/overlays";

interface Dup {
  existing: Download | null;
  existingFileExists: boolean;
  fileExists: boolean;
  path: string | null;
  policy: "rename" | "overwrite" | "skip" | string;
}

/**
 * "You already have this": the same link is in the list, or a file with the
 * target name is already in the folder. Offers open / resume / show instead
 * of downloading again (the dialog's own buttons still download a copy).
 */
export function DuplicateNotice({ url, dir, filename, onHandled }: { url: string | null; dir: string; filename: string; onHandled: () => void }) {
  const [dup, setDup] = useState<Dup | null>(null);
  useEffect(() => {
    setDup(null);
    if (!url) return;
    let alive = true;
    const t = setTimeout(() => {
      void invoke<Dup>("check_duplicate", { url, dir: dir || null, filename: filename || null })
        .then((d) => alive && setDup(d))
        .catch(() => {});
    }, 250);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [url, dir, filename]);

  if (!dup || (!dup.existing && !dup.fileExists)) return null;
  const run = (p: Promise<unknown>, done = true) =>
    void p.then(() => done && onHandled()).catch((e) => toast({ level: "error", title: "That didn't work", message: errorText(e) }));

  const e = dup.existing;
  if (e) {
    const finished = e.status === "completed" || e.status === "seeding";
    if (finished && dup.existingFileExists) {
      return (
        <Notice
          level="info"
          icon={CopyCheck}
          title={t("You already downloaded this file")}
          action={
            <span style={{ display: "inline-flex", gap: 6 }}>
              <Button size="sm" onClick={() => run(api.openFile(e.id))}>
                {t("Open")}
              </Button>
              <Button size="sm" onClick={() => run(api.openFolder(e.id), false)}>
                {t("Show in folder")}
              </Button>
            </span>
          }
        >
          {e.name} · {fmt.bytes(e.total)}. Download it again only if you need a fresh copy.
        </Notice>
      );
    }
    if (!finished) {
      const pct = e.total > 0 ? ` (${Math.floor(fmt.percent(e.done, e.total))}% done)` : "";
      return (
        <Notice
          level="info"
          icon={CopyCheck}
          title={t("This link is already in your downloads")}
          action={
            <span style={{ display: "inline-flex", gap: 6 }}>
              {e.status !== "downloading" && e.status !== "queued" && (
                <Button size="sm" variant="primary" onClick={() => run(api.resume([e.id]).then(() => invoke("open_progress_window", { id: e.id })))}>
                  {t("Resume it")}
                </Button>
              )}
              <Button size="sm" onClick={() => run(invoke("open_progress_window", { id: e.id }))}>
                {t("Show progress")}
              </Button>
            </span>
          }
        >
          {e.name}
          {pct} — resuming keeps what is already downloaded.
        </Notice>
      );
    }
  }
  if (dup.fileExists) {
    const what = dup.policy === "overwrite" ? "It will be replaced." : dup.policy === "skip" ? "The download will be skipped (Settings › Downloads)." : "The new copy gets a number, like “name (1)”.";
    return (
      <Notice level="warning" icon={CopyCheck} title={t("A file with this name is already in the folder")}>
        {what}
      </Notice>
    );
  }
  return null;
}
