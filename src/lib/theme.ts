export type ThemePreference = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

const KEY = "theme";

export function getPreference(): ThemePreference {
  const value = localStorage.getItem(KEY);
  return value === "light" || value === "dark" ? value : "system";
}

export function resolveTheme(pref: ThemePreference, systemDark: boolean): ResolvedTheme {
  if (pref === "light" || pref === "dark") return pref;
  return systemDark ? "dark" : "light";
}

export function applyTheme(pref = getPreference()) {
  const resolved = resolveTheme(pref, window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = resolved;
  document.documentElement.style.colorScheme = resolved;
  void syncNativeTheme(pref);
  watchSystem(pref);
}

export function setPreference(pref: ThemePreference) {
  localStorage.setItem(KEY, pref);
  applyTheme(pref);
}

let media: MediaQueryList | undefined;
let onSystemChange: (() => void) | undefined;

function watchSystem(pref: ThemePreference) {
  if (media && onSystemChange) {
    media.removeEventListener("change", onSystemChange);
    media = undefined;
    onSystemChange = undefined;
  }
  if (pref !== "system") return;
  media = window.matchMedia("(prefers-color-scheme: dark)");
  onSystemChange = () => applyTheme("system");
  media.addEventListener("change", onSystemChange);
}

async function syncNativeTheme(pref: ThemePreference) {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setTheme(pref === "system" ? null : pref);
  } catch {
    /* vite preview has no native window */
  }
}
