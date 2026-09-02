import type { RemoteModelHit } from "./types";

export type HitCapability = "vision" | "image" | "audio";
export type AgoParts = { key: "justNow" } | { key: "minutes" | "hours" | "days"; count: number };

export function formatCount(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
  return String(value);
}

export function isGgufHit(hit: Pick<RemoteModelHit, "name" | "repo" | "library" | "tags">) {
  return [hit.name, hit.repo, hit.library ?? "", ...hit.tags].join(" ").toLowerCase().includes("gguf");
}

export function hitCapabilities(hit: Pick<RemoteModelHit, "pipeline" | "tags">): HitCapability[] {
  const hay = [hit.pipeline ?? "", ...hit.tags].join(" ").toLowerCase().replace(/[_/]/g, "-");
  const out: HitCapability[] = [];
  if (/(vision|image-text|image-to-text|video-text|any-to-any|multimodal|vlm)/.test(hay)) out.push("vision");
  if (/(text-to-image|image-to-image|image-class|object-detect|image-segment|zero-shot-image|text-to-video|image-text)/.test(hay)) out.push("image");
  if (/(audio|speech|asr|tts|text-to-speech|text-to-audio|voice)/.test(hay)) out.push("audio");
  return out;
}

export function agoParts(value?: string, now = Date.now()): AgoParts | undefined {
  if (!value) return;
  const date = parseStamp(value);
  if (!date) return;
  const minutes = Math.max(0, Math.round((now - date.getTime()) / 60_000));
  if (minutes < 1) return { key: "justNow" };
  if (minutes < 60) return { key: "minutes", count: minutes };
  const hours = Math.round(minutes / 60);
  if (hours < 48) return { key: "hours", count: hours };
  return { key: "days", count: Math.round(hours / 24) };
}

export function avatarTone(author: string) {
  return [...author].reduce((sum, ch) => sum + ch.charCodeAt(0), 0) % 3;
}

function parseStamp(value: string) {
  const trimmed = value.trim();
  if (/^\d+$/.test(trimmed)) {
    const n = Number(trimmed);
    const date = new Date(n > 1e12 ? n : n * 1000);
    return Number.isNaN(date.getTime()) ? undefined : date;
  }
  const date = new Date(trimmed);
  return Number.isNaN(date.getTime()) ? undefined : date;
}
