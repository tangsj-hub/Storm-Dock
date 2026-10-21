import { describe, expect, it } from "vitest";
import { isBotTemplateUrl, isDescriptionTruncated, truncateDescription } from "./botTemplates";

describe("isBotTemplateUrl", () => {
  it("accepts official x.ai bot share links", () => {
    expect(isBotTemplateUrl("https://x.ai/bot/hXd0y-LKCK729av9pzg_X")).toBe(true);
    expect(isBotTemplateUrl(" https://www.x.ai/bot/abc/?ref=1 ")).toBe(true);
  });
  it("rejects other hosts and paths", () => {
    expect(isBotTemplateUrl("https://evil.com/bot/abc")).toBe(false);
    expect(isBotTemplateUrl("https://x.ai/bot/")).toBe(false);
    expect(isBotTemplateUrl("https://x.ai/other/abc")).toBe(false);
    expect(isBotTemplateUrl("hello")).toBe(false);
  });
});

describe("truncateDescription", () => {
  it("keeps short text and cuts long text with an ellipsis", () => {
    expect(truncateDescription("短描述")).toBe("短描述");
    expect(isDescriptionTruncated("短描述")).toBe(false);
    const long = "字".repeat(41);
    expect(isDescriptionTruncated(long)).toBe(true);
    expect(truncateDescription(long)).toBe(`${"字".repeat(40)}…`);
    expect(truncateDescription("😀".repeat(3), 2)).toBe("😀😀…");
  });
});
