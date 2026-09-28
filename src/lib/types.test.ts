import { describe, expect, it } from "vitest";
import { APPLICATION_KINDS, homeKindFromUsageQuery, usagePath } from "./types";

describe("application kinds", () => {
  it("keeps app switcher limited to account hosts", () => {
    expect(APPLICATION_KINDS).toEqual(["cursor", "codex", "grok"]);
  });
});

describe("usage navigation", () => {
  it("builds usage URLs and preserves grokBot as the back target", () => {
    expect(usagePath("acc-1", "cursor")).toBe("/usage.html?accountId=acc-1&kind=cursor");
    expect(usagePath("acc-1", "grok", "grokBot")).toBe(
      "/usage.html?accountId=acc-1&kind=grok&from=grokBot",
    );
    expect(homeKindFromUsageQuery("?accountId=acc-1&kind=cursor")).toBe("cursor");
    expect(homeKindFromUsageQuery("?accountId=acc-1&kind=grok")).toBe("grok");
    expect(homeKindFromUsageQuery("?accountId=acc-1&kind=cursor&from=grokBot")).toBe("grokBot");
    expect(homeKindFromUsageQuery("?accountId=acc-1&kind=grok&from=grokBot")).toBe("grokBot");
  });
});
