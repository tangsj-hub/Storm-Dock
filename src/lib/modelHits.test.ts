import { describe, expect, it } from "vitest";
import { agoParts, formatCount, hitCapabilities, isGgufHit } from "./modelHits";

describe("model search hits", () => {
  it("formats compact download counts", () => {
    expect(formatCount(243)).toBe("243");
    expect(formatCount(484_700)).toBe("484.7K");
  });

  it("marks gguf from name or tags", () => {
    expect(isGgufHit({ name: "MiniMax-H3-GGUF", repo: "unsloth/MiniMax-H3-GGUF", tags: [] })).toBe(true);
    expect(isGgufHit({ name: "Qwen", repo: "Qwen/Qwen2.5", tags: ["safetensors"] })).toBe(false);
  });

  it("maps vision pipelines to capability icons", () => {
    expect(hitCapabilities({ pipeline: "image-text-to-text", tags: [] })).toEqual(["vision", "image"]);
    expect(hitCapabilities({ pipeline: "text-generation", tags: [] })).toEqual([]);
    expect(hitCapabilities({ pipeline: "automatic-speech-recognition", tags: [] })).toEqual(["audio"]);
  });

  it("turns timestamps into day-scale ago parts", () => {
    const now = Date.parse("2026-09-01T00:00:00.000Z");
    expect(agoParts("2026-08-15T00:00:00.000Z", now)).toEqual({ key: "days", count: 17 });
    expect(agoParts("1710000000", 1_710_000_000_000 + 10_000)).toEqual({ key: "justNow" });
  });
});
