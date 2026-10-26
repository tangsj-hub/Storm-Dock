import { describe, expect, it } from "vitest";
import { planTabOverflow, stripWidth, visibleWindow } from "./tabOverflow";

function widths(entries: Record<string, number>): Map<string, number> {
  return new Map(Object.entries(entries));
}

describe("stripWidth", () => {
  it("sums raw floats and gaps with no rounding", () => {
    expect(stripWidth(["a", "b", "c"], widths({ a: 80.25, b: 80.25, c: 80.25 }), 2)).toBe(
      80.25 * 3 + 4,
    );
  });
});

describe("visibleWindow", () => {
  const ids = ["a", "b", "c", "d", "e"] as const;

  it("takes a leading prefix when selected is already inside", () => {
    expect(visibleWindow(ids, 3, "b")).toEqual({
      visibleIds: ["a", "b", "c"],
      overflowIds: ["d", "e"],
    });
  });

  it("swaps selected into the last visible slot when it would overflow", () => {
    expect(visibleWindow(ids, 3, "e")).toEqual({
      visibleIds: ["a", "b", "e"],
      overflowIds: ["c", "d"],
    });
  });
});

describe("planTabOverflow", () => {
  const tabIds = ["a", "b", "c", "d", "e"] as const;

  it("shows every tab and no overflow when the full strip fits", () => {
    const widthById = widths({ a: 40, b: 40, c: 40, d: 40, e: 40 });
    expect(
      planTabOverflow({
        tabIds,
        widthById,
        gap: 2,
        availableInnerWidth: stripWidth(tabIds, widthById, 2),
        overflowBtnWidth: 32,
        selectedId: "c",
      }),
    ).toEqual({ visibleIds: [...tabIds], overflowIds: [] });
  });

  it("drops into overflow when available is 1px short of an exact full fit", () => {
    const widthById = widths({ a: 80, b: 80, c: 80, d: 80 });
    const ids = ["a", "b", "c", "d"] as const;
    const exact = stripWidth(ids, widthById, 2);
    expect(
      planTabOverflow({
        tabIds: ids,
        widthById,
        gap: 2,
        availableInnerWidth: exact - 1,
        overflowBtnWidth: 36,
        selectedId: "a",
      }),
    ).toEqual({
      visibleIds: ["a", "b", "c"],
      overflowIds: ["d"],
    });
  });

  it("re-validates after swapping in a wider selected tab", () => {
    // Prefix of 4×40 + more fits in 238, but swapping e=120 as 4th does not;
    // largest valid window is a,b,e (204 + 2 + 32 = 238).
    expect(
      planTabOverflow({
        tabIds,
        widthById: widths({ a: 40, b: 40, c: 40, d: 40, e: 120 }),
        gap: 2,
        availableInnerWidth: 238,
        overflowBtnWidth: 32,
        selectedId: "e",
      }),
    ).toEqual({
      visibleIds: ["a", "b", "e"],
      overflowIds: ["c", "d"],
    });
  });

  it("floors available once and keeps fractional tab widths", () => {
    const widthById = widths({ a: 50.4, b: 50.4, c: 50.4 });
    const ids = ["a", "b", "c"] as const;
    // floor(151.9)=151; strip=50.4*3+4=155.2 → overflow with 2 tabs + btn
    // 50.4*2+2 + 2 + 30 = 134.8 <= 151
    expect(
      planTabOverflow({
        tabIds: ids,
        widthById,
        gap: 2,
        availableInnerWidth: 151.9,
        overflowBtnWidth: 30,
        selectedId: "a",
      }),
    ).toEqual({
      visibleIds: ["a", "b"],
      overflowIds: ["c"],
    });
  });

  it("returns empty visible when nothing fits", () => {
    expect(
      planTabOverflow({
        tabIds: ["a", "b"],
        widthById: widths({ a: 100, b: 100 }),
        gap: 2,
        availableInnerWidth: 10,
        overflowBtnWidth: 32,
        selectedId: "a",
      }),
    ).toEqual({ visibleIds: [], overflowIds: ["a", "b"] });
  });

  it("five home-style tabs all visible when slot is wide enough (no max-4 cap)", () => {
    const tabIds = ["cursor", "codex", "grok", "workbuddy", "grokBot"] as const;
    const widthById = widths({
      cursor: 88,
      codex: 96,
      grok: 72,
      workbuddy: 110,
      grokBot: 100,
    });
    // strip ≈ 88+96+72+110+100 + 2*4 = 474; plenty of room
    expect(
      planTabOverflow({
        tabIds,
        widthById,
        gap: 2,
        availableInnerWidth: 600,
        overflowBtnWidth: 32,
        selectedId: "cursor",
      }),
    ).toEqual({ visibleIds: [...tabIds], overflowIds: [] });
  });

});
