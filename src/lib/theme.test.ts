import { describe, expect, it } from "vitest";
import { chromeColor, resolveTheme } from "./theme";

describe("resolveTheme", () => {
  it("follows system appearance", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });

  it("forces light or dark regardless of system", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("light", false)).toBe("light");
    expect(resolveTheme("dark", true)).toBe("dark");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("maps the page background onto native window chrome", () => {
    expect(chromeColor("light")).toEqual({ red: 255, green: 255, blue: 255, alpha: 255 });
    expect(chromeColor("dark")).toEqual({ red: 32, green: 32, blue: 32, alpha: 255 });
  });
});
