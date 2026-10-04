import { describe, expect, it } from "vitest";
import type { Account } from "../../../lib/types";
import {
  subscriptionLabel,
  subscriptionPlanBadge,
  usageLabel,
  endpointHost,
  accountKindKey,
  grokBotUsageLabel,
  grokBotResetLabel,
  isGrokBotListEligible,
  subscriptionDatedLabel,
} from "./accountPresentation";

const t = (key: string, options?: Record<string, unknown>) => {
  if (key === "subscriptionTodayAt") return `今天 ${options?.time}`;
  if (key === "subscriptionTomorrowAt") return `明天 ${options?.time}`;
  if (key === "subscriptionDaysWithDate") return `剩余 ${options?.count} 天 · ${options?.date}`;
  if (key === "subscriptionExpiredWithDate") return `已过期 · ${options?.date}`;
  if (key === "subscriptionDays") return `${options?.count} 天`;
  if (key === "usageResetsTodayAt") return `今天 ${options?.time}`;
  if (key === "usageResetsTomorrowAt") return `明天 ${options?.time}`;
  if (key === "usageResetsInWithDate") return `将在 ${options?.count} 天后重置 · ${options?.date}`;
  if (key === "usageResetPassedWithDate") return `重置日期已过 · ${options?.date}`;
  if (key === "quotaResetTodayAt") return `额度重置 · 今天 ${options?.time}`;
  if (key === "quotaResetTomorrowAt") return `额度重置 · 明天 ${options?.time}`;
  if (key === "quotaResetDaysWithDate") return `额度重置 · 剩余 ${options?.count} 天 · ${options?.date}`;
  if (key === "quotaResetPassedWithDate") return `额度已重置 · ${options?.date}`;
  if (key === "quotaResetDays") return `${options?.count} 天`;
  if (key === "quotaResetPassed") return "额度已重置";
  if (key === "quotaResetToday") return "今天";
  if (key === "quotaResetTomorrow") return "明天";
  return `${key}:${options?.count ?? options?.amount ?? options?.percent ?? options?.time ?? options?.date ?? ""}`;
};

const account: Account = {
  id: "a",
  label: "A",
  importType: "oauth",
  subscription: { plan: "Pro" },
  daysRemaining: 2,
  isCurrent: false,
};

const overnightNow = new Date(2026, 8, 23, 20, 0, 0).getTime();
const overnightTarget = new Date(2026, 8, 24, 2, 42, 0);
const monthEndNow = new Date(2026, 0, 31, 22, 0, 0).getTime();
const monthEndTarget = new Date(2026, 1, 1, 2, 0, 0);

describe("account presentation", () => {
  it("does not default unqueried plans to Free", () => {
    expect(subscriptionPlanBadge({ ...account, application: "grok", subscription: {} }, t)).toEqual({
      name: "subscriptionUnknownPlan:",
      plan: "unknown",
    });
    expect(subscriptionLabel({ ...account, application: "cursor", subscription: {} }, t)).toMatchObject({
      name: "subscriptionUnknownPlan:",
      expiry: "subscriptionUnknownExpiry:",
      plan: "unknown",
    });
    expect(subscriptionPlanBadge({ ...account, application: "grok", subscription: { plan: "free" } }, t)).toEqual({
      name: "subscriptionPlans.free:",
      plan: "free",
    });
  });

  it("formats grok subscription days with local calendar days", () => {
    const end = new Date(overnightNow + 6 * 86_400_000);
    const result = subscriptionLabel({
      ...account,
      application: "grok",
      subscription: {
        plan: "supergrok_heavy",
        billingCycleEnd: end.toISOString(),
        expiresAt: Math.floor(end.getTime() / 1000),
      },
      daysRemaining: 5,
    }, t, overnightNow);
    expect(result.expiry).toBe("6 天");
    expect(result.expiryFull).toContain("剩余 6 天 ·");
    expect(result.expiryTitle).toBeTruthy();
  });

  it("does not treat grok quota reset as subscription expiry", () => {
    const end = new Date(overnightNow + 2 * 86_400_000);
    expect(subscriptionLabel({
      ...account,
      application: "grok",
      subscription: { plan: "supergrok_heavy" },
      resetAt: end.toISOString(),
      daysRemaining: undefined,
    }, t, overnightNow).expiry).toBe("subscriptionUnknownExpiry:");
  });

  it("formats subscription status from daysRemaining fallback when no timestamps", () => {
    const result = subscriptionLabel(account, t);
    expect(result).toMatchObject({
      name: "subscriptionPlans.pro:",
      expiry: "2 天",
      plan: "pro",
    });
    expect(result.expiryTitle).toBeUndefined();
    expect(result.expiryFull).toBe("2 天");
    expect(subscriptionLabel({ ...account, daysRemaining: -1 }, t)?.expiry).toBe("subscriptionExpired:");
  });

  it("formats currency usage and free accounts", () => {
    expect(usageLabel({ ...account, usage: { kind: "currency", used: 123, percent: 1 } }, t)?.text).toBe("usageSpent:$1.23");
    expect(usageLabel({ ...account, subscription: { plan: "Free" } }, t)?.text).toBe("usageFree:");
  });

  it("formats percent usage and endpoint hosts", () => {
    expect(usageLabel({ ...account, usage: { kind: "percent", used: 12, percent: 12 } }, t)?.text).toBe("usagePercent:12");
    expect(endpointHost("https://api.example.com/v1")).toBe("api.example.com");
  });

  it("appends grok quota reset days after usage with absolute title", () => {
    const inThreeDays = new Date(overnightNow + 3 * 86_400_000).toISOString();
    const result = usageLabel({
      ...account,
      application: "grok",
      usage: { kind: "percent", used: 45, percent: 45 },
      resetAt: inThreeDays,
    }, t, overnightNow);
    expect(result?.text).toBe("usagePercent:45 · grokBotResetDays:3");
    expect(result?.title).toBeTruthy();
  });

  it("maps ChatGPT import types to sign-in vs API Key", () => {
    expect(accountKindKey(account)).toBe("accountKind.account");
    expect(accountKindKey({ ...account, importType: "api_key" })).toBe("accountKind.apiKey");
  });

  it("formats grok bot usage badge with abbreviated reset and absolute title", () => {
    const inThreeDays = new Date(overnightNow + 3 * 86_400_000).toISOString();
    const withReset = grokBotUsageLabel({
      ...account,
      grokBotUsage: { kind: "percent", used: 45, percent: 45 },
      grokBotResetAt: inThreeDays,
    }, t, overnightNow);
    expect(withReset?.text).toBe("grokBotUsageBadge:45");
    expect(withReset?.title).toBeTruthy();
    expect(grokBotUsageLabel({ ...account, grokBotUsage: { kind: "percent", used: 10, percent: 10 } }, t)?.text).toBe(
      "grokBotUsagePercent:10",
    );
  });

  it("excludes unknown subscription, free plan, expired token, banned, and missing-credential accounts from grok bot lists", () => {
    expect(isGrokBotListEligible(account)).toBe(true);
    expect(isGrokBotListEligible({ ...account, subscription: {} })).toBe(false);
    expect(isGrokBotListEligible({ ...account, subscription: { plan: "free" } })).toBe(false);
    expect(isGrokBotListEligible({ ...account, subscription: { plan: "Free" } })).toBe(false);
    expect(isGrokBotListEligible({ ...account, status: "invalid" })).toBe(false);
    expect(isGrokBotListEligible({ ...account, status: "blocked" })).toBe(false);
    expect(isGrokBotListEligible({ ...account, status: "missing" })).toBe(false);
  });

  it("formats reset-passed grok usage as 0% · reset", () => {
    const yesterday = new Date(overnightNow - 86_400_000).toISOString();
    expect(usageLabel({
      ...account,
      application: "grok",
      usage: { kind: "percent", used: 100, percent: 100 },
      resetAt: yesterday,
    }, t, overnightNow)?.text).toBe("usagePercent:0 · grokBotResetPassed:");
  });

  it("formats reset-passed grok bot badge as 0% · reset", () => {
    const yesterday = new Date(overnightNow - 86_400_000).toISOString();
    expect(grokBotUsageLabel({
      ...account,
      grokBotUsage: { kind: "percent", used: 100, percent: 100 },
      grokBotResetAt: yesterday,
    }, t, overnightNow)?.text).toBe("grokBotUsageBadge:0");
  });
});

describe("subscriptionLabel calendar days (all providers)", () => {
  const subscriptionProviders: Array<Account["application"]> = ["cursor", "grok", undefined];

  for (const application of subscriptionProviders) {
    const label = application ?? "default/cursor";

    it(`${label}: overnight billingCycleEnd => tomorrow + absolute title`, () => {
      const result = subscriptionLabel({
        ...account,
        application,
        subscription: {
          plan: "Pro",
          billingCycleEnd: overnightTarget.toISOString(),
          expiresAt: Math.floor(overnightTarget.getTime() / 1000),
        },
      }, t, overnightNow);
      expect(result.expiry).toBe("subscriptionTomorrow:");
      expect(result.expiryFull).toBe("明天 02:42");
      expect(result.expiryTitle).toMatch(/02:42/);
    });

    it(`${label}: overnight expiresAt (unix sec) => tomorrow`, () => {
      const result = subscriptionLabel({
        ...account,
        application,
        subscription: {
          plan: "Pro",
          expiresAt: Math.floor(overnightTarget.getTime() / 1000),
        },
        daysRemaining: 0,
      }, t, overnightNow);
      expect(result.expiry).toBe("subscriptionTomorrow:");
      expect(result.expiryFull).toBe("明天 02:42");
    });

    it(`${label}: month-end rollover => tomorrow`, () => {
      expect(subscriptionLabel({
        ...account,
        application,
        subscription: {
          plan: "Pro",
          billingCycleEnd: monthEndTarget.toISOString(),
        },
      }, t, monthEndNow).expiryFull).toBe("明天 02:00");
    });
  }

  it("labels same-day upcoming expiry as today with clock", () => {
    const sameDayNow = new Date(2026, 8, 24, 1, 0, 0).getTime();
    const dated = subscriptionDatedLabel({
      ...account,
      subscription: { plan: "Pro", billingCycleEnd: overnightTarget.toISOString() },
    }, t, sameDayNow);
    expect(dated.short).toBe("subscriptionToday:");
    expect(dated.full).toBe("今天 02:42");
  });
});

describe("Codex quota reset labeling", () => {
  it("keeps ChatGPT plan name and labels the date as quota reset", () => {
    const result = subscriptionLabel({
      ...account,
      application: "codex",
      subscription: {
        plan: "ChatGPT",
        expiresAt: Math.floor(overnightTarget.getTime() / 1000),
      },
    }, t, overnightNow);
    expect(result.name).toBe("subscriptionPlans.chatgpt:");
    expect(result.plan).toBe("chatgpt");
    expect(result.expiry).toBe("明天");
    expect(result.expiryFull).toBe("额度重置 · 明天 02:42");
    expect(result.expiryFull).not.toContain("已过期");
  });

  it("past Codex reset shows 额度已重置, not 已过期", () => {
    const past = new Date(2026, 8, 20, 10, 0, 0);
    const result = subscriptionLabel({
      ...account,
      application: "codex",
      subscription: {
        plan: "ChatGPT",
        expiresAt: Math.floor(past.getTime() / 1000),
      },
      daysRemaining: -1,
    }, t, overnightNow);
    expect(result.expiry).toBe("额度已重置");
    expect(result.expiryFull).toBe("额度已重置 · 09/20 10:00");
    expect(result.expiry).not.toContain("已过期");
    expect(result.expiryFull).not.toContain("已过期");
  });

  it("zh compact N-day quota badge uses 天", () => {
    const inThree = new Date(overnightNow + 3 * 86_400_000);
    const result = subscriptionLabel({
      ...account,
      application: "codex",
      subscription: {
        plan: "ChatGPT",
        expiresAt: Math.floor(inThree.getTime() / 1000),
      },
    }, t, overnightNow);
    expect(result.expiry).toBe("3 天");
    expect(result.expiryFull).toContain("额度重置 · 剩余 3 天 ·");
  });
});

describe("zh compact subscription days", () => {
  it("formats N-day compact badge as '{{count}} 天'", () => {
    expect(subscriptionLabel({
      ...account,
      application: "cursor",
      subscription: { plan: "Pro" },
      daysRemaining: 18,
    }, t).expiry).toBe("18 天");
  });
});

describe("grokBotResetLabel calendar days", () => {
  it("uses tomorrow for an overnight reset instead of today", () => {
    expect(grokBotResetLabel(overnightTarget.toISOString(), t, overnightNow)).toBe("grokBotResetTomorrow:");
  });

  it("uses today for a same-day upcoming reset", () => {
    const now = new Date(2026, 8, 24, 1, 0, 0).getTime();
    expect(grokBotResetLabel(overnightTarget.toISOString(), t, now)).toBe("grokBotResetToday:");
  });

  it("uses tomorrow across month-end", () => {
    expect(grokBotResetLabel(monthEndTarget.toISOString(), t, monthEndNow)).toBe("grokBotResetTomorrow:");
  });
});
