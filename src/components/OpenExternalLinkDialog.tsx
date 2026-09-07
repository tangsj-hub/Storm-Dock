import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { useTranslation } from "react-i18next";
import { openExternalUrl } from "../lib/api";
import styles from "./OpenExternalLinkDialog.module.css";

export type OpenExternalLinkDialogProps = {
  url?: string;
  /** Dialog title override; defaults to i18n modelOpenExternalTitle. */
  title?: string;
  onOpenChange: (open: boolean) => void;
  onError?: (message: string) => void;
};

/** Confirm before opening an http(s) URL in the system browser (Tauri has no target=_blank). */
export function OpenExternalLinkDialog({ url, title, onOpenChange, onError }: OpenExternalLinkDialogProps) {
  const { t } = useTranslation();
  const open = Boolean(url);

  const confirm = () => {
    if (!url) return;
    void openExternalUrl(url).catch((error) => {
      onError?.(error instanceof Error ? error.message : String(error));
    });
  };

  return (
    <AlertDialog.Root onOpenChange={onOpenChange} open={open}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay className={styles.overlay} />
        <AlertDialog.Content className={styles.content}>
          <AlertDialog.Title className={styles.title}>
            {title || t("modelOpenExternalTitle")}
          </AlertDialog.Title>
          <AlertDialog.Description className={styles.description}>
            {t("modelOpenExternalConfirm")}
            {url ? (
              <>
                {" "}
                <span className={styles.url}>{url}</span>
              </>
            ) : null}
          </AlertDialog.Description>
          <div className={styles.actions}>
            <AlertDialog.Cancel asChild>
              <button className={styles.cancel} type="button">{t("cancel")}</button>
            </AlertDialog.Cancel>
            <AlertDialog.Action asChild>
              <button className={styles.confirm} onClick={confirm} type="button">
                {t("modelOpenExternalAction")}
              </button>
            </AlertDialog.Action>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
