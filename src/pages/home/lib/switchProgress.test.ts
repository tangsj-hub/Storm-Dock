import { describe, expect, it } from "vitest";
import type { SwitchProgress } from "../types";
import { progressForAccount, shouldApplySwitchProgress } from "./switchProgress";

const progress = (overrides: Partial<SwitchProgress> = {}): SwitchProgress => ({
  operationId: "op-1",
  accountId: "acct-a",
  stage: "loading",
  percent: 15,
  status: "running",
  ...overrides,
});

describe("progressForAccount", () => {
  it("returns progress only for the matching account id", () => {
    const current = progress();
    expect(progressForAccount(current, "acct-a")).toBe(current);
    expect(progressForAccount(current, "acct-b")).toBeUndefined();
    expect(progressForAccount(undefined, "acct-a")).toBeUndefined();
  });
});

describe("shouldApplySwitchProgress", () => {
  it("applies live switch progress and ignores confirmation waiting", () => {
    expect(shouldApplySwitchProgress(progress())).toBe(true);
    expect(shouldApplySwitchProgress(progress({ status: "success", stage: "complete" }))).toBe(true);
    expect(shouldApplySwitchProgress(progress({ status: "error", stage: "error" }))).toBe(true);
    expect(shouldApplySwitchProgress(progress({ status: "waiting", stage: "restartRequired" }))).toBe(false);
    expect(shouldApplySwitchProgress(progress({ stage: "restartRequired" }))).toBe(false);
  });
});
