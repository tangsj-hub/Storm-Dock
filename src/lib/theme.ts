export type ThemePreference = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";
export type ChromeColor = { red: number; green: number; blue: number; alpha: number };

const KEY = "theme";

export const WINDOW_CHROME_COLORS: Record<ResolvedTheme, ChromeColor> = {
  light: { red: 255, green: 255, blue: 255, alpha: 255 },
  dark: { red: 32, green: 32, blue: 32, alpha: 255 },
};

export function chromeColor(theme: ResolvedTheme) {
  return WINDOW_CHROME_COLORS[theme];
}

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
  void syncNativeTheme(resolved);
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

async function syncNativeTheme(theme: ResolvedTheme) {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const { invoke } = await import("@tauri-apps/api/core");
    const window = getCurrentWindow();
    await window.setTheme(theme);
    await window.setBackgroundColor(chromeColor(theme));
    await invoke("sync_window_chrome");
  } catch {
    /* vite preview has no native window */
  }
}
