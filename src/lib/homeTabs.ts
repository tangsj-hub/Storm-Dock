import { homePath, type ApplicationKind } from "./types";

export const HOME_TABS = ["cursor", "codex", "grok", "workbuddy", "grokBot"] as const;
export type HomeTabId = (typeof HOME_TABS)[number];
export type HomeTabPref = { id: HomeTabId; visible: boolean };

const KEY = "homeTabs";
const DEFAULT_HOME_TABS: HomeTabPref[] = HOME_TABS.map((id) => ({ id, visible: true }));

export function isHomeTabId(value: unknown): value is HomeTabId {
  return HOME_TABS.includes(value as HomeTabId);
}

export function normalizeHomeTabs(raw: unknown): HomeTabPref[] {
  const seen = new Set<HomeTabId>();
  const next: HomeTabPref[] = [];
  if (Array.isArray(raw)) {
    for (const item of raw) {
      const id = item && typeof item === "object" && "id" in item ? item.id : item;
      if (!isHomeTabId(id) || seen.has(id)) continue;
      seen.add(id);
      next.push({
        id,
        visible: typeof item === "object" && item && "visible" in item ? item.visible !== false : true,
      });
    }
  }
  for (const id of HOME_TABS) {
    if (seen.has(id)) continue;
    next.push({ id, visible: true });
  }
  if (!next.some((tab) => tab.visible)) next[0].visible = true;
  return next;
}

export function getHomeTabs(): HomeTabPref[] {
  try {
    const stored = localStorage.getItem(KEY);
    return normalizeHomeTabs(stored ? JSON.parse(stored) : undefined);
  } catch {
    return normalizeHomeTabs(undefined);
  }
}

export function setHomeTabs(tabs: HomeTabPref[]) {
  const next = normalizeHomeTabs(tabs);
  localStorage.setItem(KEY, JSON.stringify(next));
  return next;
}

export function visibleHomeTabs(tabs = getHomeTabs()): HomeTabId[] {
  return tabs.filter((tab) => tab.visible).map((tab) => tab.id);
}

export function requestedHomeTab(search = window.location.search): HomeTabId {
  const kind = new URLSearchParams(search).get("kind");
  return isHomeTabId(kind) ? kind : "cursor";
}

export function resolveHomeView(search = window.location.search, tabs = getHomeTabs()) {
  const visible = visibleHomeTabs(tabs);
  const requested = requestedHomeTab(search);
  const tab = visible.includes(requested) ? requested : visible[0];
  return tab === "grokBot"
    ? { selected: "cursor" as ApplicationKind, grokBotMode: true, tab }
    : { selected: tab, grokBotMode: false, tab };
}

export function resolvedHomePath(search = window.location.search, notice?: string) {
  return homePath(resolveHomeView(search).tab, notice);
}

/** Brand marks that keep native colors — do not apply dark-theme invert (`ink`). */
export function isColorBrandTab(kind: HomeTabId): boolean {
  return kind === "workbuddy" || kind === "grokBot";
}

/** Class for tab/app icons: monochrome SVGs get `ink`; brand PNGs (WorkBuddy, Grok Bot) do not. */
export function iconInkClass(kind: HomeTabId, extra = ""): string {
  const parts = [extra, isColorBrandTab(kind) ? "" : "ink"].filter(Boolean);
  return parts.join(" ");
}
