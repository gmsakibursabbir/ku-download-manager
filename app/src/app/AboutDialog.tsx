import { useEffect, useState } from "react";
import { CircleCheck, Download as DownloadIcon, RefreshCw } from "lucide-react";
import { api, errorText } from "../lib/api";
import { t, tf } from "../lib/i18n";
import type { UpdateInfo } from "../lib/types";
import { Button, Icon } from "../ui/primitives";
import { Dialog } from "../ui/overlays";
import { LogoMark } from "./Shell";

/** Help › About KuDownloader: version, update status, credits. */
export function AboutDialog({ onClose }: { onClose: () => void }) {
  const [version, setVersion] = useState("");
  // undefined: checking; null: up to date.
  const [update, setUpdate] = useState<UpdateInfo | null | undefined>(undefined);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const check = () => {
    setError("");
    setUpdate(undefined);
    api.checkUpdate().then(setUpdate, (e) => {
      setUpdate(null);
      setError(errorText(e));
    });
  };
  useEffect(() => {
    void api.appInfo().then((i) => setVersion(i.version)).catch(() => {});
    check();
  }, []);

  const install = async () => {
    setBusy(true);
    try {
      await api.installUpdate();
    } catch (e) {
      setError(errorText(e));
    }
    setBusy(false);
  };

  const status =
    update === undefined ? (
      <span className="about-status">
        <span className="spinner" />
        {t("Checking for updates…")}
      </span>
    ) : error ? (
      <span className="about-status is-error">{error}</span>
    ) : update ? (
      <span className="about-status is-new">
        <Icon icon={DownloadIcon} size={14} />
        {tf("Version {version} is available.", { version: update.version })}
      </span>
    ) : (
      <span className="about-status is-ok">
        <Icon icon={CircleCheck} size={14} />
        {t("KuDownloader is up to date.")}
      </span>
    );

  return (
    <Dialog
      title={t("About KuDownloader")}
      onClose={onClose}
      width={380}
      footer={
        <>
          <span style={{ flex: 1 }} />
          {update ? (
            <>
              <Button onClick={onClose}>{t("Close")}</Button>
              <Button variant="primary" icon={DownloadIcon} busy={busy} data-autofocus onClick={() => void install()}>
                {update.signed ? t("Install and restart") : t("Download update")}
              </Button>
            </>
          ) : (
            <>
              <Button icon={RefreshCw} disabled={update === undefined} onClick={check}>
                {t("Check now")}
              </Button>
              <Button variant="primary" data-autofocus onClick={onClose}>
                {t("Close")}
              </Button>
            </>
          )}
        </>
      }
    >
      <div className="about">
        <LogoMark size={64} />
        <div className="about-name">KuDownloader</div>
        <div className="about-version">{version ? tf("Version {version}", { version }) : " "}</div>
        {status}
        <div className="about-credit">{t("Made by Kuduy")}</div>
      </div>
    </Dialog>
  );
}
