export type DiscoverListView = "compact" | "cards" | "split";

const STORAGE_KEY = "storm-dock.discover.listView";
const VIEWS: DiscoverListView[] = ["cards", "split", "compact"];

export function isDiscoverListView(value: unknown): value is DiscoverListView {
  return value === "compact" || value === "cards" || value === "split";
}

export function readDiscoverListView(): DiscoverListView {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (isDiscoverListView(raw)) return raw;
  } catch {
    /* ignore quota / private mode */
  }
  return "compact";
}

export function writeDiscoverListView(view: DiscoverListView) {
  try {
    localStorage.setItem(STORAGE_KEY, view);
  } catch {
    /* ignore quota / private mode */
  }
}

export const DISCOVER_VIEW_ORDER = VIEWS;
