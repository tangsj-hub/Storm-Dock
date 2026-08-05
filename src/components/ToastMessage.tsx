import * as Toast from "@radix-ui/react-toast";
import styles from "./ToastMessage.module.css";

export function ToastMessage({ notice, onOpenChange }: { notice?: string; onOpenChange: (open: boolean) => void }) {
  return <Toast.Root className={styles.toast} duration={3500} onOpenChange={onOpenChange} open={Boolean(notice)}><Toast.Description>{notice}</Toast.Description></Toast.Root>;
}

export { Toast };
