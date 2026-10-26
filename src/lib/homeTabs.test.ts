import { afterEach, describe, expect, it } from "vitest";
import {
  getHomeTabs,
  HOME_TABS,
  normalizeHomeTabs,
  requestedHomeTab,
  resolveHomeView,
  setHomeTabs,
  visibleHomeTabs,
} from "./homeTabs";

const memory = new Map<string, string>();
Object.defineProperty(globalThis, "localStorage", {
  value: {
    getItem: (key: string) => memory.get(key) ?? null,
    setItem: (key: string, value: string) => {
      memory.set(key, value);
    },
    removeItem: (key: string) => {
      memory.delete(key);
    },
  },
});

describe("normalizeHomeTabs", () => {
  it("defaults to all known tabs visible in order", () => {
    expect(normalizeHomeTabs(undefined)).toEqual(HOME_TABS.map((id) => ({ id, visible: true })));
  });

  it("keeps custom order, drops unknown ids, and appends missing tabs as visible", () => {
    expect(
      normalizeHomeTabs([
        { id: "grok", visible: false },
        { id: "nope", visible: true },
        { id: "cursor", visible: true },
      ]),
    ).toEqual([
      { id: "grok", visible: false },
      { id: "cursor", visible: true },
      { id: "codex", visible: true },
      { id: "workbuddy", visible: true },
      { id: "grokBot", visible: true },
    ]);
  });

  it("forces the first tab visible when every tab is hidden", () => {
    expect(
      normalizeHomeTabs(HOME_TABS.map((id) => ({ id, visible: false }))),
    ).toEqual([
      { id: "cursor", visible: true },
      { id: "codex", visible: false },
      { id: "grok", visible: false },
      { id: "workbuddy", visible: false },
      { id: "grokBot", visible: false },
    ]);
  });

  it("accepts a bare id list", () => {
    expect(normalizeHomeTabs(["grokBot", "cursor"])).toEqual([
      { id: "grokBot", visible: true },
      { id: "cursor", visible: true },
      { id: "codex", visible: true },
      { id: "grok", visible: true },
      { id: "workbuddy", visible: true },
    ]);
  });
});

describe("home tab storage and view", () => {
  afterEach(() => {
    localStorage.removeItem("homeTabs");
  });

  it("reads and writes normalized prefs", () => {
    expect(getHomeTabs()).toEqual(HOME_TABS.map((id) => ({ id, visible: true })));
    expect(setHomeTabs([{ id: "codex", visible: true }, { id: "cursor", visible: false }])).toEqual([
      { id: "codex", visible: true },
      { id: "cursor", visible: false },
      { id: "grok", visible: true },
      { id: "workbuddy", visible: true },
      { id: "grokBot", visible: true },
    ]);
    expect(visibleHomeTabs()).toEqual(["codex", "grok", "workbuddy", "grokBot"]);
  });

  it("falls back when the requested tab is hidden", () => {
    const tabs = normalizeHomeTabs([
      { id: "grok", visible: true },
      { id: "cursor", visible: false },
      { id: "codex", visible: false },
      { id: "grokBot", visible: false },
    ]);
    expect(requestedHomeTab("?kind=grokBot")).toBe("grokBot");
    expect(resolveHomeView("?kind=cursor", tabs)).toEqual({
      selected: "grok",
      grokBotMode: false,
      tab: "grok",
    });
    expect(resolveHomeView("?kind=grokBot", tabs)).toEqual({
      selected: "grok",
      grokBotMode: false,
      tab: "grok",
    });
  });

  it("keeps grok bot as cursor + grokBotMode", () => {
    expect(resolveHomeView("?kind=grokBot")).toEqual({
      selected: "cursor",
      grokBotMode: true,
      tab: "grokBot",
    });
  });
});
