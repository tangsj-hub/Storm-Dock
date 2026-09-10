import { invoke } from "@tauri-apps/api/core";

export type CloseBehavior = "ask" | "tray" | "quit";

const KEY = "closeBehavior";
const LEGACY_KEY = "closeToTray";

export function getCloseBehavior(): CloseBehavior {
  const stored = localStorage.getItem(KEY);
  if (stored === "ask" || stored === "tray" || stored === "quit") return stored;
  // First upgrade: ask once so users see the chooser. Explicit false → quit.
  if (localStorage.getItem(LEGACY_KEY) === "false") return "quit";
  return "ask";
}

export function setCloseBehavior(behavior: CloseBehavior) {
  localStorage.setItem(KEY, behavior);
  localStorage.setItem(LEGACY_KEY, behavior === "quit" ? "false" : "true");
  void invoke("set_close_behavior", { behavior }).catch(() => undefined);
}

export function syncCloseBehaviorToRust(behavior = getCloseBehavior()) {
  // Persist migrated value so settings and Rust stay aligned after upgrade.
  if (!localStorage.getItem(KEY)) localStorage.setItem(KEY, behavior);
  void invoke("set_close_behavior", { behavior }).catch(() => undefined);
}

export async function confirmCloseAction(action: "tray" | "quit" | "cancel") {
  await invoke("confirm_close_action", { action });
}
