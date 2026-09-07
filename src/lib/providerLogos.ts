/**
 * Local vendor/provider logo registry for Storm Dock Discover.
 * Logos are Vite-imported from src/assets/providers (bundled for Tauri).
 *
 * Match order in resolveProviderLogo:
 * 1. repo-name prefixes (declaration order — most specific first)
 * 2. exact owner/org (case-insensitive)
 * 3. word-boundary stems on the repo name
 */

import cohereUrl from "../assets/providers/cohere.png";
import deepseekUrl from "../assets/providers/deepseek.svg";
import googleUrl from "../assets/providers/google.png";
import hfUrl from "../assets/providers/hf.svg";
import ibmUrl from "../assets/providers/ibm.png";
import metaUrl from "../assets/providers/meta.svg";
import microsoftUrl from "../assets/providers/microsoft.svg";
import minimaxUrl from "../assets/providers/minimax-color.png";
import mistralUrl from "../assets/providers/mistral.svg";
import moonshotUrl from "../assets/providers/moonshot.jpg";
import nvidiaUrl from "../assets/providers/nvidia.svg";
import openaiUrl from "../assets/providers/openai.svg";
import qwenUrl from "../assets/providers/qwen.png";
import xaiUrl from "../assets/providers/xai.svg";
import zaiUrl from "../assets/providers/zai.svg";

export type LogoTreatment = "original" | "mono-theme";
export type LogoBackground = "white" | "transparent";
export type LogoFit = "contain" | "cover";

export type ProviderLogo = {
  id: string;
  name: string;
  /** Vite-imported asset URL */
  src: string;
  treatment?: LogoTreatment;
  background?: LogoBackground;
  /** Only for treatment "original". Defaults to "contain". */
  fit?: LogoFit;
  /** Case-insensitive exact org / author match */
  owners?: readonly string[];
  /** Match repo name (after owner/) — first prefix wins in declaration order */
  prefixes?: readonly string[];
  /** Optional word-boundary fallback on lowercased repo name */
  stems?: readonly string[];
};

export const PROVIDER_LOGOS: readonly ProviderLogo[] = [
  // Most-specific prefixes first so they beat broader Llama/Mistral/Qwen families.
  {
    id: "nvidia",
    name: "NVIDIA",
    src: nvidiaUrl,
    treatment: "original",
    background: "white",
    owners: ["nvidia", "NVIDIA"],
    prefixes: [
      "Llama-3.1-Nemotron-",
      "Llama-3.3-Nemotron-",
      "Llama-3.1-Minitron-",
      "NVIDIA-Nemotron-",
      "Nemotron-3-",
      "Nemotron-4-",
      "Nemotron-H-",
      "Minitron-",
      "Mistral-NeMo-",
      "OpenReasoning-Nemotron-",
      "OpenCodeReasoning",
      "Cosmos-",
    ],
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    src: deepseekUrl,
    treatment: "original",
    background: "white",
    owners: ["deepseek-ai", "deepseek", "DeepSeek"],
    prefixes: ["DeepSeek-R1-Distill-", "DeepSeek-", "deepseek-", "deepseek-llm-", "deepseek-coder-"],
  },
  {
    id: "microsoft",
    name: "Microsoft",
    src: microsoftUrl,
    treatment: "original",
    background: "white",
    owners: ["microsoft", "Microsoft"],
    prefixes: ["MAI-DS-R1", "NextCoder-", "Phi-3-", "Phi-3.5-", "Phi-4", "phi-1", "phi-2", "phi-"],
  },
  {
    id: "qwen",
    name: "Qwen",
    src: qwenUrl,
    treatment: "original",
    background: "white",
    owners: ["Qwen", "qwen", "Alibaba", "alibaba-nlp", "Alibaba-NLP"],
    prefixes: ["Qwen", "QwQ-", "QVQ-"],
  },
  {
    id: "moonshot",
    name: "Moonshot AI",
    src: moonshotUrl,
    treatment: "original",
    background: "transparent",
    fit: "cover",
    owners: ["moonshotai", "moonshot", "MoonshotAI"],
    prefixes: ["Kimi-", "Moonlight-"],
  },
  {
    id: "zai",
    name: "Z.ai",
    src: zaiUrl,
    treatment: "original",
    background: "transparent",
    fit: "cover",
    owners: ["THUDM", "thudm", "zai-org", "ZhipuAI", "zhipuai"],
    prefixes: ["GLM-", "glm-", "chatglm", "codegeex"],
  },
  {
    id: "xai",
    name: "xAI",
    src: xaiUrl,
    treatment: "mono-theme",
    background: "white",
    owners: ["xai-org", "xai", "xAI"],
    prefixes: ["grok-"],
  },
  {
    id: "minimax",
    name: "MiniMax",
    src: minimaxUrl,
    treatment: "original",
    background: "white",
    owners: ["MiniMaxAI", "minimax", "MiniMax"],
    prefixes: ["MiniMax-"],
  },
  {
    id: "huggingface",
    name: "Hugging Face",
    src: hfUrl,
    treatment: "original",
    background: "white",
    owners: ["huggingface", "HuggingFace", "HuggingFaceTB", "HuggingFaceH4", "hf"],
    prefixes: ["SmolLM"],
  },
  {
    id: "ibm",
    name: "IBM",
    src: ibmUrl,
    treatment: "original",
    background: "transparent",
    fit: "cover",
    owners: ["ibm-granite", "ibm", "IBM"],
    prefixes: ["granite-", "granitelib-"],
  },
  {
    id: "cohere",
    name: "Cohere",
    src: cohereUrl,
    treatment: "original",
    background: "white",
    owners: ["CohereForAI", "Cohere", "cohere"],
    prefixes: ["c4ai-command", "aya-"],
  },
  {
    id: "openai",
    name: "OpenAI",
    src: openaiUrl,
    treatment: "mono-theme",
    background: "transparent",
    owners: ["openai", "OpenAI"],
    prefixes: ["gpt-oss-"],
  },
  {
    id: "google",
    name: "Google",
    src: googleUrl,
    treatment: "original",
    background: "white",
    owners: ["google", "Google", "google-bert", "google-research"],
    prefixes: [
      "gemma-",
      "codegemma-",
      "recurrentgemma-",
      "shieldgemma-",
      "medgemma-",
      "functiongemma-",
      "translategemma-",
      "t5gemma-",
      "embeddinggemma-",
      "paligemma-",
      "txgemma-",
      "bert-",
    ],
    stems: ["gemma"],
  },
  {
    id: "mistral",
    name: "Mistral AI",
    src: mistralUrl,
    treatment: "original",
    background: "white",
    owners: ["mistralai", "mistral", "MistralAI"],
    prefixes: ["Mistral-", "Mixtral-", "Codestral-", "Pixtral-", "Devstral-", "Ministral-", "Voxtral-", "Magistral-"],
  },
  // Last among Llama-prefix providers so NVIDIA Nemotron/Minitron win first.
  {
    id: "meta",
    name: "Meta",
    src: metaUrl,
    treatment: "original",
    background: "white",
    owners: ["meta-llama", "meta-models", "facebook", "Meta", "llamafactory"],
    prefixes: [
      "Meta-Llama-",
      "Llama-Guard-",
      "LlamaGuard-",
      "CodeLlama-",
      "Llama-",
      "llama-",
      "meta-",
      "Muse-Glimmer",
    ],
  },
];

function stemMatchesAtBoundary(haystack: string, stem: string): boolean {
  for (let at = haystack.indexOf(stem); at !== -1; at = haystack.indexOf(stem, at + 1)) {
    const next = haystack[at + stem.length];
    if (next === undefined || next < "a" || next > "z") return true;
  }
  return false;
}

function matchByPrefix(repoName: string): ProviderLogo | null {
  for (const logo of PROVIDER_LOGOS) {
    if (logo.prefixes?.some((prefix) => repoName.startsWith(prefix))) return logo;
  }
  return null;
}

function matchByOwner(owner: string): ProviderLogo | null {
  const needle = owner.trim().toLowerCase();
  if (!needle) return null;
  for (const logo of PROVIDER_LOGOS) {
    if (logo.owners?.some((org) => org.toLowerCase() === needle)) return logo;
  }
  return null;
}

function matchByStem(repoName: string): ProviderLogo | null {
  const lower = repoName.toLowerCase();
  for (const logo of PROVIDER_LOGOS) {
    if (logo.stems?.some((stem) => stemMatchesAtBoundary(lower, stem))) return logo;
  }
  return null;
}

/**
 * Resolve a vendor logo for a model list row.
 * @param owner Hub/ModelScope author or org
 * @param repoName Repo name after `owner/` (not the full `owner/name`)
 */
export function resolveProviderLogo(owner: string, repoName?: string): ProviderLogo | null {
  const name = repoName?.trim() || "";
  if (name) {
    const byPrefix = matchByPrefix(name);
    if (byPrefix) return byPrefix;
  }
  const byOwner = matchByOwner(owner);
  if (byOwner) return byOwner;
  if (name) {
    const byStem = matchByStem(name);
    if (byStem) return byStem;
  }
  return null;
}

/** Split `owner/name` into parts; bare names become owner="" / repoName=full. */
export function splitModelRepo(repo: string): { owner: string; repoName: string } {
  const trimmed = repo.trim();
  const slash = trimmed.indexOf("/");
  if (slash < 0) return { owner: "", repoName: trimmed };
  return { owner: trimmed.slice(0, slash), repoName: trimmed.slice(slash + 1) };
}
