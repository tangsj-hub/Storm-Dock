import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import {
  confirmCloseAction,
  setCloseBehavior,
  syncCloseBehaviorToRust,
  type CloseBehavior
} from "../lib/closeBehavior";
import styles from "./CloseAskDialog.module.css";

export function CloseAskDialog() {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [remember, setRemember] = useState(true);
  const [busy, setBusy] = useState(false);
  const finishing = useRef(false);

  useEffect(() => {
    syncCloseBehaviorToRust();
    let unlisten = () => {};
    void listen("close-requested-ask", () => {
      setRemember(true);
      setOpen(true);
    }).then((stop) => {
      unlisten = stop;
    });
    void listen<CloseBehavior>("close-behavior-changed", ({ payload }) => {
      if (payload === "ask" || payload === "tray" || payload === "quit") {
        localStorage.setItem("closeBehavior", payload);
        localStorage.setItem("closeToTray", payload === "quit" ? "false" : "true");
        window.dispatchEvent(new CustomEvent("close-behavior-changed", { detail: payload }));
      }
    }).then((stop) => {
      const prev = unlisten;
      unlisten = () => {
        prev();
        stop();
      };
    });
    return () => unlisten();
  }, []);

  const finish = async (action: "tray" | "quit" | "cancel") => {
    if (busy || finishing.current) return;
    finishing.current = true;
    setBusy(true);
    try {
      if ((action === "tray" || action === "quit") && remember) {
        setCloseBehavior(action);
        window.dispatchEvent(new CustomEvent("close-behavior-changed", { detail: action }));
      }
      setOpen(false);
      await confirmCloseAction(action);
    } catch {
      setOpen(false);
      await confirmCloseAction("cancel").catch(() => undefined);
    } finally {
      setBusy(false);
      finishing.current = false;
    }
  };

  return (
    <AlertDialog.Root
      onOpenChange={(next) => {
        if (!next && open && !finishing.current) void finish("cancel");
      }}
      open={open}
    >
      <AlertDialog.Portal>
        <AlertDialog.Overlay className={styles.dialogOverlay} />
        <AlertDialog.Content className={styles.dialogContent}>
          <AlertDialog.Title>{t("closeAskTitle")}</AlertDialog.Title>
          <AlertDialog.Description>{t("closeAskDescription")}</AlertDialog.Description>
          <div className={styles.remember}>
            <input
              checked={remember}
              id="close-ask-remember"
              onChange={(event) => setRemember(event.target.checked)}
              type="checkbox"
            />
            <label htmlFor="close-ask-remember">{t("closeAskRemember")}</label>
          </div>
          <div className={styles.dialogActions}>
            <button className={styles.cancel} disabled={busy} onClick={() => void finish("cancel")} type="button">
              {t("cancel")}
            </button>
            <button className={styles.secondary} disabled={busy} onClick={() => void finish("quit")} type="button">
              {t("closeAskQuit")}
            </button>
            <button
              autoFocus
              className={styles.primary}
              disabled={busy}
              onClick={() => void finish("tray")}
              type="button"
            >
              {t("closeAskTray")}
            </button>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}

let mounted = false;

export function mountCloseAskDialog() {
  if (mounted || typeof document === "undefined") return;
  mounted = true;
  const host = document.createElement("div");
  host.id = "close-ask-dialog-root";
  document.body.appendChild(host);
  createRoot(host).render(<CloseAskDialog />);
}
