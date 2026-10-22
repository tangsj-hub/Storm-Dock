import { useEffect, useRef, type ReactNode } from "react";
import { getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import appIcon from "../assets/app-icon.png";
import { consumePopupButtonPress } from "../lib/popupButton";
import styles from "./NativePopup.module.css";

// Platform class drives the look: macOS = native vibrancy (no CSS card),
// Windows = solid Windows 11 toast card.
document.documentElement.classList.add(/Mac/i.test(navigator.userAgent) ? "mac" : "win");

const SWIPE_THRESHOLD = 60;

export type NativePopupAction = { label: string; onClick: () => void; primary?: boolean; disabled?: boolean };

/** Shared shell for every native banner popup (switcher reminder / preview, import progress). */
export function NativePopup({ actions, ariaLabel, body, detail, icon, onDismiss, onHover, preview, resetKey, role = "status", time, title }: {
  actions: NativePopupAction[];
  ariaLabel?: string;
  body: ReactNode;
  detail?: ReactNode;
  /** Defaults to the Storm Dock app icon. */
  icon?: string;
  /** Two-finger swipe right (trackpad) dismisses, like a macOS notification banner. */
  onDismiss?: () => void;
  /** Pointer entered / left the popup (pauses auto-hide). */
  onHover?: (hovering: boolean) => void;
  /** Text of the subtle preview tag; omitted when not a preview. */
  preview?: string;
  /** Popup generation: the window is reused, so swipe state resets on each new show. */
  resetKey?: number;
  role?: "alertdialog" | "status";
  time?: ReactNode;
  title: ReactNode;
}) {
  const dismissRef = useRef(onDismiss);
  dismissRef.current = onDismiss;

  // Two-finger swipe moves the whole native window (blur + shadow included).
  useEffect(() => {
    const win = getCurrentWindow();
    let base: { x: number; y: number } | null = null;
    let baseReady: Promise<void> | null = null;
    let total = 0;
    let reset = 0;
    let raf = 0;
    let gone = false;
    let dismissed = false;
    // Capture the dismiss callback for THIS show only. A late rAF from a swipe
    // must not call a newer generation's onDismiss after resetKey changes.
    const dismissThis = dismissRef.current;
    const move = (dx: number) => { if (base) void win.setPosition(new LogicalPosition(base.x + dx, base.y)); };
    const animate = (from: number, to: number, ms: number, done?: () => void) => {
      const start = performance.now();
      const step = (now: number) => {
        if (dismissed) return;
        const k = Math.min(1, (now - start) / ms);
        const eased = to > from ? k * k : 1 - (1 - k) * (1 - k);
        move(from + (to - from) * eased);
        if (k < 1) raf = requestAnimationFrame(step); else done?.();
      };
      raf = requestAnimationFrame(step);
    };
    const finishDismiss = () => {
      if (dismissed) return;
      dismissed = true;
      gone = true;
      dismissThis?.();
    };
    const onWheel = (event: WheelEvent) => {
      if (gone || dismissed || !dismissThis || Math.abs(event.deltaX) <= Math.abs(event.deltaY)) return;
      baseReady ??= Promise.all([win.outerPosition(), win.scaleFactor()]).then(([pos, scale]) => { base = { x: pos.x / scale, y: pos.y / scale }; });
      // Fingers moving right report negative deltaX on macOS trackpads.
      total = Math.min(0, total + event.deltaX);
      window.clearTimeout(reset);
      const offset = -total;
      void baseReady.then(() => {
        if (gone || dismissed) return;
        if (offset >= SWIPE_THRESHOLD) {
          gone = true;
          animate(offset * 0.6, 460, 200, finishDismiss);
          return;
        }
        move(offset * 0.6);
      });
      reset = window.setTimeout(() => {
        if (gone || dismissed) return;
        const from = -total * 0.6;
        total = 0;
        if (base) animate(from, 0, 160, () => { base = null; baseReady = null; });
      }, 160);
    };
    // Debug / tests: same path as a completed two-finger swipe (instant dismiss, no slide-back).
    const onSwipeDismiss = () => finishDismiss();
    window.addEventListener("wheel", onWheel, { passive: true });
    window.addEventListener("storm-dock-swipe-dismiss", onSwipeDismiss);
    return () => {
      dismissed = true;
      window.removeEventListener("wheel", onWheel);
      window.removeEventListener("storm-dock-swipe-dismiss", onSwipeDismiss);
      window.clearTimeout(reset);
      cancelAnimationFrame(raf);
    };
  }, [resetKey]);

  return <div aria-label={ariaLabel} aria-live={role === "alertdialog" ? "assertive" : "polite"} className={styles.popup} onMouseEnter={() => onHover?.(true)} onMouseLeave={() => onHover?.(false)} role={role}>
    <img alt="" className={styles.icon} draggable={false} src={icon ?? appIcon} />
    <div className={styles.content}>
      <div className={styles.head}>
        <span className={styles.title}>{title}</span>
        {preview && <span className={styles.previewTag}>{preview}</span>}
        {time != null && <span className={styles.time}>{time}</span>}
      </div>
      <div className={styles.message}>{body}{detail ? <span className={styles.detail}>{detail}</span> : null}</div>
      <div className={styles.actions}>
        {actions.map((action) => <PopupButton action={action} key={action.label} resetKey={resetKey} />)}
      </div>
    </div>
  </div>;
}

function PopupButton({ action, resetKey }: { action: NativePopupAction; resetKey?: number }) {
  const armed = useRef(false);
  useEffect(() => { armed.current = false; }, [resetKey]);
  const press = (type: "pointerdown" | "click", button?: number) => {
    const next = consumePopupButtonPress({ type, button }, armed.current);
    armed.current = next.armed;
    if (next.run) action.onClick();
  };
  return <button
    className={`${styles.button} ${action.primary ? styles.primary : ""}`}
    disabled={action.disabled}
    onClick={(event) => press("click", event.button)}
    onPointerDown={(event) => press("pointerdown", event.button)}
    type="button"
  >{action.label}</button>;
}
