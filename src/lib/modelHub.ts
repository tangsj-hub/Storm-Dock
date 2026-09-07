import type { ModelSource } from "./types";

/** Public hub page URL for a model repo (HF or ModelScope). */
export function modelHubUrl(source: ModelSource, repo: string) {
  if (source === "huggingface") {
    return `https://huggingface.co/${repo}`;
  }
  return `https://www.modelscope.cn/models/${repo}`;
}

/** i18n key for the "Open on …" hub link label. */
export function modelHubOpenLabelKey(source: ModelSource) {
  return source === "huggingface" ? "modelOpenOnHuggingFace" : "modelOpenOnModelScope";
}
