import type { ReactNode } from "react";
import appIcon from "../assets/app-icon.png";
import styles from "./NativePopup.module.css";

// Platform class drives the look: macOS = native vibrancy (no CSS card),
// Windows = solid Windows 11 toast card.
document.documentElement.classList.add(/Mac/i.test(navigator.userAgent) ? "mac" : "win");

export type NativePopupAction = { label: string; onClick: () => void; primary?: boolean; disabled?: boolean };

/** Shared shell for every native banner popup (switcher reminder / preview, import progress). */
export function NativePopup({ actions, ariaLabel, body, detail, icon, preview, role = "status", time, title }: {
  actions: NativePopupAction[];
  ariaLabel?: string;
  body: ReactNode;
  detail?: ReactNode;
  /** Defaults to the Storm Dock app icon. */
  icon?: string;
  /** Text of the subtle preview tag; omitted when not a preview. */
  preview?: string;
  role?: "alertdialog" | "status";
  time?: ReactNode;
  title: ReactNode;
}) {
  return <div aria-label={ariaLabel} aria-live={role === "alertdialog" ? "assertive" : "polite"} className={styles.popup} role={role}>
    <img alt="" className={styles.icon} draggable={false} src={icon ?? appIcon} />
    <div className={styles.content}>
      <div className={styles.head}>
        <span className={styles.title}>{title}</span>
        {preview && <span className={styles.previewTag}>{preview}</span>}
        {time != null && <span className={styles.time}>{time}</span>}
      </div>
      <div className={styles.message}>{body}{detail ? <span className={styles.detail}>{detail}</span> : null}</div>
      <div className={styles.actions}>
        {actions.map((action) => <button className={`${styles.button} ${action.primary ? styles.primary : ""}`} disabled={action.disabled} key={action.label} onClick={action.onClick} type="button">{action.label}</button>)}
      </div>
    </div>
  </div>;
}
