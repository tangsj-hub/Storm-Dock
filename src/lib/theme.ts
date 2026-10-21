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

function applyResolved(resolved: ResolvedTheme) {
  document.documentElement.dataset.theme = resolved;
  document.documentElement.style.colorScheme = resolved;
}

function applyPlatform() {
  const ua = navigator.userAgent;
  document.documentElement.dataset.platform = ua.includes("Windows")
    ? "windows"
    : ua.includes("Macintosh") || ua.includes("Mac OS")
      ? "macos"
      : "other";
}

export function applyTheme(pref = getPreference()) {
  applyPlatform();
  applyResolved(resolveTheme(pref, window.matchMedia("(prefers-color-scheme: dark)").matches));
  void syncNativeTheme(pref === "system" ? null : pref);
  watchSystem(pref);
}

export function setPreference(pref: ThemePreference) {
  localStorage.setItem(KEY, pref);
  applyTheme(pref);
}

let media: MediaQueryList | undefined;
let onSystemChange: (() => void) | undefined;
let stopNativeWatch: (() => void) | undefined;
let watchGen = 0;

function watchSystem(pref: ThemePreference) {
  if (media && onSystemChange) {
    media.removeEventListener("change", onSystemChange);
    media = undefined;
    onSystemChange = undefined;
  }
  stopNativeWatch?.();
  stopNativeWatch = undefined;
  const gen = ++watchGen;
  if (pref !== "system") return;
  media = window.matchMedia("(prefers-color-scheme: dark)");
  onSystemChange = () => applyResolved(media!.matches ? "dark" : "light");
  media.addEventListener("change", onSystemChange);
  void import("@tauri-apps/api/window")
    .then(({ getCurrentWindow }) =>
      getCurrentWindow().onThemeChanged(({ payload }) => {
        applyResolved(payload === "dark" ? "dark" : "light");
      }),
    )
    .then((unlisten) => {
      if (gen !== watchGen) {
        unlisten();
        return;
      }
      stopNativeWatch = unlisten;
    })
    .catch(() => undefined);
}

async function syncNativeTheme(theme: ResolvedTheme | null) {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const { invoke } = await import("@tauri-apps/api/core");
    const window = getCurrentWindow();
    // The switcher popup is a transparent window; an opaque chrome color would
    // paint square corners behind its rounded card.
    if (window.label.startsWith("popup-")) return;
    await window.setTheme(theme);
    const resolved = theme ?? ((await window.theme()) === "dark" ? "dark" : "light");
    applyResolved(resolved);
    await window.setBackgroundColor(chromeColor(resolved));
    await invoke("sync_window_chrome");
  } catch {
    /* vite preview has no native window */
  }
}
