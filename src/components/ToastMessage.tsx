import * as Toast from "@radix-ui/react-toast";
import { useEffect } from "react";
import { CheckCircle2, CircleAlert, LoaderCircle } from "lucide-react";
import styles from "./ToastMessage.module.css";

type ToastStatus = "loading" | "success" | "error";

export function ToastMessage({ notice, onOpenChange, status = "success" }: { notice?: string; onOpenChange: (open: boolean) => void; status?: ToastStatus }) {
  const Icon = status === "loading" ? LoaderCircle : status === "error" ? CircleAlert : CheckCircle2;
  useEffect(() => {
    if (!notice || status === "loading") return;
    const timer = window.setTimeout(() => onOpenChange(false), 3500);
    return () => window.clearTimeout(timer);
  }, [notice, status, onOpenChange]);
  return <Toast.Root className={`${styles.toast} ${styles[status]}`} duration={status === "loading" ? Infinity : 3500} key={`${status}-${notice}`} onOpenChange={onOpenChange} open={Boolean(notice)}><Icon aria-hidden="true" className={status === "loading" ? styles.spinning : undefined} size={17} /><Toast.Description>{notice}</Toast.Description></Toast.Root>;
}

export { Toast };
