import { describe, expect, it } from "vitest";
import { creditGroupRows, credits, daysUntil, eventSpend, expirePoolLabel, formatSpend, metric, productParts, resetDatedLabel, spendCents } from "./format";

const t = (key: string, options?: Record<string, unknown>) => {
  if (key === "usageResetsTodayAt") return `今天 ${options?.time}`;
  if (key === "usageResetsTomorrowAt") return `明天 ${options?.time}`;
  if (key === "usageResetsInWithDate") return `将在 ${options?.count} 天后重置 · ${options?.date}`;
  if (key === "usageResetPassedWithDate") return `重置日期已过 · ${options?.date}`;
  return `${key}:${options?.count ?? ""}`;
};

describe("productParts", () => {
  it("joins grok product shares the way the details line shows them", () => {
    expect(productParts([
      { name: "Grok Build", percent: 96 },
      { name: "Imagine", percent: 4 },
    ])).toBe("Grok Build 96% + Imagine 4%");
  });

  it("joins WorkBuddy credit groups as absolute capacities", () => {
    expect(productParts([
      { name: "套餐基础积分", percent: 0, used: 0, limit: 500 },
      { name: "平台奖励积分", percent: 0, used: 0, limit: 1500 },
      { name: "购买积分", percent: 0, used: 0, limit: 0 },
    ])).toBe("套餐基础积分 500 + 平台奖励积分 1,500 + 购买积分 0");
  });

  it("keeps WorkBuddy fractional credit capacities (needed decimals only)", () => {
    expect(productParts([
      { name: "套餐基础积分", percent: 0, used: 0, limit: 500.5 },
      { name: "平台奖励积分", percent: 0, used: 0, limit: 1500.25 },
    ])).toBe("套餐基础积分 500.5 + 平台奖励积分 1,500.25");
  });
});

describe("creditGroupRows", () => {
  it("maps absolute WorkBuddy pools to 基础积分/奖励积分/购买积分/赠送积分 order", () => {
    expect(creditGroupRows([
      { name: "套餐基础积分", percent: 0, used: 0, limit: 500 },
      { name: "套餐赠送积分", percent: 0, used: 0, limit: 0 },
      { name: "购买积分", percent: 0, used: 0, limit: 0 },
      { name: "平台奖励积分", percent: 0, used: 0, limit: 1500, expiresAt: "2026-11-26T07:40:09Z" },
    ])).toEqual([
      { labelKey: "usagePoolBase", limit: 500, used: 0, expiresAt: undefined },
      { labelKey: "usagePoolReward", limit: 1500, used: 0, expiresAt: "2026-11-26T07:40:09Z" },
      { labelKey: "usagePoolPurchase", limit: 0, used: 0, expiresAt: undefined },
      { labelKey: "usagePoolGift", limit: 0, used: 0, expiresAt: undefined },
    ]);
  });

  it("always fills 基础/奖励/购买/赠送 with zeros when a group is missing", () => {
    expect(creditGroupRows([
      { name: "套餐基础积分", percent: 0, used: 100, limit: 500 },
    ])).toEqual([
      { labelKey: "usagePoolBase", limit: 500, used: 100, expiresAt: undefined },
      { labelKey: "usagePoolReward", limit: 0, used: 0, expiresAt: undefined },
      { labelKey: "usagePoolPurchase", limit: 0, used: 0, expiresAt: undefined },
      { labelKey: "usagePoolGift", limit: 0, used: 0, expiresAt: undefined },
    ]);
  });

  it("still shows the four buckets when products is empty", () => {
    expect(creditGroupRows([])).toEqual([
      { labelKey: "usagePoolBase", limit: 0, used: 0, expiresAt: undefined },
      { labelKey: "usagePoolReward", limit: 0, used: 0, expiresAt: undefined },
      { labelKey: "usagePoolPurchase", limit: 0, used: 0, expiresAt: undefined },
      { labelKey: "usagePoolGift", limit: 0, used: 0, expiresAt: undefined },
    ]);
    expect(creditGroupRows(undefined)).toEqual([
      { labelKey: "usagePoolBase", limit: 0, used: 0, expiresAt: undefined },
      { labelKey: "usagePoolReward", limit: 0, used: 0, expiresAt: undefined },
      { labelKey: "usagePoolPurchase", limit: 0, used: 0, expiresAt: undefined },
      { labelKey: "usagePoolGift", limit: 0, used: 0, expiresAt: undefined },
    ]);
  });

  it("skips percent-only Grok shares", () => {
    expect(creditGroupRows([
      { name: "Grok Build", percent: 96 },
    ])).toEqual([]);
  });

  it("preserves fractional WorkBuddy capacities (no integer round)", () => {
    expect(creditGroupRows([
      { name: "套餐基础积分", percent: 0, used: 12.34, limit: 500.5 },
      { name: "平台奖励积分", percent: 0, used: 0.5, limit: 1500.25 },
    ])).toEqual([
      { labelKey: "usagePoolBase", limit: 500.5, used: 12.34, expiresAt: undefined },
      { labelKey: "usagePoolReward", limit: 1500.25, used: 0.5, expiresAt: undefined },
      { labelKey: "usagePoolPurchase", limit: 0, used: 0, expiresAt: undefined },
      { labelKey: "usagePoolGift", limit: 0, used: 0, expiresAt: undefined },
    ]);
  });
});

describe("credits / metric (WorkBuddy)", () => {
  it("formats credit amounts: integers without .00, fractions up to 2 digits", () => {
    expect(credits(500)).toBe("500");
    expect(credits(0)).toBe("0");
    expect(credits(2000)).toBe("2,000");
    expect(credits(1500.5)).toBe("1,500.5");
    expect(credits(1500.25)).toBe("1,500.25");
    expect(credits(0.45)).toBe("0.45");
    expect(credits(1.5)).toBe("1.5");
  });

  it("renders metric used / limit without trailing .00 (not remaining)", () => {
    expect(metric({ kind: "credits", used: 12.345, limit: 500.5, percent: 0 })).toBe("12.35 / 500.5");
    expect(metric({ kind: "credits", used: 0, limit: 2000, percent: 0 })).toBe("0 / 2,000");
  });

  it("does not subtract remaining again (regression 2399.10/2399.55)", () => {
    // used=0.45 consumed of 2399.55 total — never display remaining 2399.10 as the left side
    expect(metric({ kind: "credits", used: 0.45, limit: 2399.55, percent: 0 })).toBe("0.45 / 2,399.55");
    expect(metric({ kind: "credits", used: 0.45, limit: 2399.55, percent: 0 })).not.toBe("2,399.10 / 2,399.55");
  });

  it("leaves Cursor currency and Grok percent unchanged", () => {
    expect(metric({ kind: "currency", used: 123, limit: 1000, percent: 12.3 })).toBe("$1.23 / $10.00");
    expect(metric({ kind: "percent", used: 0, percent: 42.7 })).toBe("43%");
  });
});

describe("daysUntil (local calendar)", () => {
  it("maps overnight reset to 1 (tomorrow), not 0 (today)", () => {
    const now = new Date(2026, 8, 23, 20, 0, 0).getTime();
    const reset = new Date(2026, 8, 24, 2, 42, 0).toISOString();
    expect(daysUntil(reset, now)).toBe(1);
  });

  it("maps same-day upcoming reset to 0 (today)", () => {
    const now = new Date(2026, 8, 24, 1, 0, 0).getTime();
    const reset = new Date(2026, 8, 24, 2, 42, 0).toISOString();
    expect(daysUntil(reset, now)).toBe(0);
  });

  it("maps month-end overnight to 1 (tomorrow)", () => {
    const now = new Date(2026, 0, 31, 22, 0, 0).getTime();
    const reset = new Date(2026, 1, 1, 2, 0, 0).toISOString();
    expect(daysUntil(reset, now)).toBe(1);
  });
});

describe("resetDatedLabel", () => {
  it("shows tomorrow with clock inline", () => {
    const now = new Date(2026, 8, 23, 20, 0, 0).getTime();
    const reset = new Date(2026, 8, 24, 2, 42, 0).toISOString();
    const label = resetDatedLabel(reset, t, now);
    expect(label.short).toBe("usageResetsTomorrow:");
    expect(label.full).toBe("明天 02:42");
    expect(label.absolute).toMatch(/02:42/);
  });

  it("shows N-days with absolute date-time", () => {
    const now = new Date(2026, 8, 23, 20, 0, 0).getTime();
    const reset = new Date(2026, 9, 12, 8, 0, 0).toISOString();
    expect(resetDatedLabel(reset, t, now).full).toBe("将在 19 天后重置 · 10/12 08:00");
  });
});

describe("expirePoolLabel", () => {
  it("formats WorkBuddy reward DeductionEndTime as 到期 absolute", () => {
    const tExpire = (key: string, options?: Record<string, unknown>) => {
      if (key === "usageExpiresAt") return `到期 ${options?.time}`;
      return key;
    };
    const label = expirePoolLabel("2026-11-26T07:40:09.000Z", tExpire, Date.UTC(2026, 9, 26, 10, 0, 0));
    expect(label?.text).toMatch(/^到期 /);
    expect(label?.absolute).toMatch(/11\/26/);
  });
});

describe("eventSpend / formatSpend", () => {
  it("formats WorkBuddy credits without a dollar sign", () => {
    expect(eventSpend({ credits: 0.45 })).toEqual({ amount: 0.45, unit: "credits" });
    expect(formatSpend({ amount: 0.45, unit: "credits" })).toBe("0.45");
    expect(formatSpend({ amount: 1.5, unit: "credits" })).toBe("1.5");
    expect(formatSpend({ amount: 500, unit: "credits" })).toBe("500");
  });

  it("keeps Cursor/Grok currency path on chargedCents", () => {
    expect(eventSpend({ chargedCents: 123 })).toEqual({ amount: 123, unit: "currency" });
    expect(formatSpend({ amount: 123, unit: "currency" })).toBe("$1.23");
    expect(spendCents({ chargedCents: 123 })).toBe(123);
  });

  it("prefers credits over chargedCents when both present", () => {
    expect(eventSpend({ credits: 0.45, chargedCents: 45 })).toEqual({ amount: 0.45, unit: "credits" });
  });
});
