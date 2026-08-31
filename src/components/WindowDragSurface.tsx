import { getCurrentWindow } from "@tauri-apps/api/window";
import styles from "./WindowDragSurface.module.css";

export function WindowDragSurface() {
  return (
    <div
      aria-hidden="true"
      className={styles.surface}
      onMouseDown={() => void getCurrentWindow().startDragging()}
    />
  );
}
