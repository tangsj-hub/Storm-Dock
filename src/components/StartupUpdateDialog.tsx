import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { checkForAppUpdate, installUpdateAndRestart } from "../lib/updater";
import {
  readDismissedStartupUpdateVersion,
  rememberDismissedStartupUpdate,
  shouldPromptStartupUpdate
} from "../lib/startupUpdate";
import styles from "./StartupUpdateDialog.module.css";

export function StartupUpdateDialog() {
  const { t } = useTranslation();
  const [update, setUpdate] = useState<{ version: string; notes?: string }>();
  const [open, setOpen] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string>();
  const dismissed = useRef(false);
  const installButtonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const result = await checkForAppUpdate();
        if (cancelled) return;
        if (
          !shouldPromptStartupUpdate({
            result,
            dismissedVersion: readDismissedStartupUpdateVersion()
          })
        ) {
          return;
        }
        if (result.status !== "available") return;
        setUpdate({ version: result.version, notes: result.notes });
        setOpen(true);
      } catch {
        // Startup checks are optional; stay quiet if the network or updater is unavailable.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const dismiss = () => {
    if (installing) return;
    if (update && !dismissed.current) {
      dismissed.current = true;
      rememberDismissedStartupUpdate(update.version);
    }
    setOpen(false);
  };

  const install = async () => {
    if (!update || installing) return;
    setInstalling(true);
    setError(undefined);
    try {
      await installUpdateAndRestart();
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      setInstalling(false);
    }
  };

  return (
    <AlertDialog.Root
      onOpenChange={(next) => {
        if (!next) dismiss();
      }}
      open={open}
    >
      <AlertDialog.Portal>
        <AlertDialog.Overlay className={styles.dialogOverlay} />
        <AlertDialog.Content
          className={styles.dialogContent}
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            installButtonRef.current?.focus();
          }}
        >
          <AlertDialog.Title>{t("updateAvailable", { version: update?.version ?? "" })}</AlertDialog.Title>
          <AlertDialog.Description>{t("startupUpdateDescription")}</AlertDialog.Description>
          {update?.notes ? <p className={styles.notes}>{update.notes}</p> : null}
          {error ? <p className={styles.error}>{t("updateInstallFailed", { error })}</p> : null}
          <div className={styles.dialogActions}>
            <AlertDialog.Cancel asChild>
              <button className={styles.later} disabled={installing} type="button">
                {t("startupUpdateLater")}
              </button>
            </AlertDialog.Cancel>
            <button
              className={styles.install}
              disabled={installing || !update}
              onClick={() => void install()}
              ref={installButtonRef}
              type="button"
            >
              {installing
                ? t("updateInstalling")
                : t("updateDownloadInstall", { version: update?.version ?? "" })}
            </button>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}

let mounted = false;

export function mountStartupUpdateDialog() {
  if (mounted || typeof document === "undefined") return;
  mounted = true;
  const host = document.createElement("div");
  host.id = "startup-update-dialog-root";
  document.body.appendChild(host);
  createRoot(host).render(<StartupUpdateDialog />);
}
