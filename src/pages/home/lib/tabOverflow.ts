/**
 * Pure tab-strip overflow planner.
 *
 * Widths stay as measured floats. The only integer snap is floor(availableInnerWidth).
 */

export type TabOverflowPlan<T extends string> = {
  visibleIds: T[];
  overflowIds: T[];
};

export type TabOverflowInput<T extends string> = {
  tabIds: readonly T[];
  widthById: ReadonlyMap<T, number>;
  gap: number;
  availableInnerWidth: number;
  overflowBtnWidth: number;
  selectedId: T;
};

function tabWidth<T extends string>(id: T, widthById: ReadonlyMap<T, number>): number {
  return widthById.get(id) ?? 0;
}

/** Raw strip width: sum(widths) + gap*(n-1). */
export function stripWidth<T extends string>(
  ids: readonly T[],
  widthById: ReadonlyMap<T, number>,
  gap: number,
): number {
  if (ids.length === 0) return 0;
  let total = 0;
  for (const id of ids) total += tabWidth(id, widthById);
  return total + gap * (ids.length - 1);
}

/**
 * First `count` slots, keeping `selectedId` in the window.
 * If selected would fall outside, it takes the last visible slot.
 */
export function visibleWindow<T extends string>(
  tabIds: readonly T[],
  count: number,
  selectedId: T,
): TabOverflowPlan<T> {
  if (count >= tabIds.length) {
    return { visibleIds: [...tabIds], overflowIds: [] };
  }
  if (count <= 0) {
    return { visibleIds: [], overflowIds: [...tabIds] };
  }

  const selectedIndex = tabIds.indexOf(selectedId);
  if (selectedIndex >= 0 && selectedIndex < count) {
    return {
      visibleIds: tabIds.slice(0, count) as T[],
      overflowIds: tabIds.slice(count) as T[],
    };
  }

  const visibleIds: T[] = [];
  for (const id of tabIds) {
    if (id === selectedId) continue;
    if (visibleIds.length >= count - 1) break;
    visibleIds.push(id);
  }
  if (selectedIndex >= 0) visibleIds.push(selectedId);
  while (visibleIds.length < count) {
    const next = tabIds.find((id) => !visibleIds.includes(id));
    if (!next) break;
    visibleIds.push(next);
  }

  return {
    visibleIds,
    overflowIds: tabIds.filter((id) => !visibleIds.includes(id)) as T[],
  };
}

function withOverflowButton(
  visibleIds: readonly string[],
  strip: number,
  gap: number,
  overflowBtnWidth: number,
): number {
  if (visibleIds.length === 0) return overflowBtnWidth;
  return strip + gap + overflowBtnWidth;
}

/**
 * Plan which tabs stay in the strip vs the overflow menu.
 *
 * 1. Measure widths are provided by the caller (floats).
 * 2. If every tab fits with gaps → all visible, no overflow button.
 * 3. Else pick the largest k where the selected-aware visible window of size k
 *    plus the overflow button still fits in floor(availableInnerWidth).
 */
export function planTabOverflow<T extends string>(
  input: TabOverflowInput<T>,
): TabOverflowPlan<T> {
  const { tabIds, widthById, gap, overflowBtnWidth, selectedId } = input;
  const available = Math.floor(input.availableInnerWidth);

  if (tabIds.length === 0) return { visibleIds: [], overflowIds: [] };
  if (available <= 0) return { visibleIds: [], overflowIds: [...tabIds] };

  const allWidth = stripWidth(tabIds, widthById, gap);
  if (allWidth <= available) {
    return { visibleIds: [...tabIds], overflowIds: [] };
  }

  for (let count = tabIds.length - 1; count >= 0; count -= 1) {
    const plan = visibleWindow(tabIds, count, selectedId);
    const strip = stripWidth(plan.visibleIds, widthById, gap);
    const needed = withOverflowButton(plan.visibleIds, strip, gap, overflowBtnWidth);
    if (needed <= available) return plan;
  }

  return { visibleIds: [], overflowIds: [...tabIds] };
}
