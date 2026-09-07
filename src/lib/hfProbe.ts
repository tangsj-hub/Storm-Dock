import { downloadFile, listFiles, modelInfo } from "@huggingface/hub";
import {
  HF_FETCH_TIMEOUT_MESSAGE,
  fetchWithTimeout,
  isAbortError,
} from "./fetchTimeout";
import type {
  ModelFit,
  RemoteModelCard,
  RemoteModelFile,
  RemoteModelProbe,
  RemoteModelVariant,
} from "./types";

function timedFetch(timeoutMs: number): typeof fetch {
  return (input, init) => fetchWithTimeout(input, { ...init, timeoutMs });
}

function withTimeout<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error(message)), ms);
    promise.then(
      (value) => {
        window.clearTimeout(timer);
        resolve(value);
      },
      (error) => {
        window.clearTimeout(timer);
        reject(error);
      },
    );
  });
}

function mapHfError(error: unknown): Error {
  if (isAbortError(error)) return new Error(HF_FETCH_TIMEOUT_MESSAGE);
  if (error instanceof Error) {
    const msg = error.message || "";
    if (/timeout|network|fetch|Failed to fetch|Load failed|aborted/i.test(msg)) {
      return new Error(HF_FETCH_TIMEOUT_MESSAGE);
    }
    return error;
  }
  return new Error(HF_FETCH_TIMEOUT_MESSAGE);
}

function fileName(path: string) {
  const parts = path.split("/");
  return parts[parts.length - 1] || path;
}

function isSidecar(path: string) {
  const name = fileName(path).toLowerCase();
  return (
    name.endsWith(".json") ||
    name.endsWith(".txt") ||
    name.endsWith(".md") ||
    name.endsWith(".jinja") ||
    name.endsWith(".py") ||
    name.includes("tokenizer") ||
    name.includes("config")
  );
}

function ggufGroupId(path: string) {
  const name = fileName(path).replace(/\.gguf$/i, "");
  return `gguf:${name.replace(/-[0-9]+-of-[0-9]+$/i, "")}`;
}

function ggufLabel(path: string) {
  return fileName(path).replace(/\.gguf$/i, "");
}

function makeVariant(
  id: string,
  label: string,
  paths: string[],
  sidecars: string[],
  byPath: Map<string, number>,
): RemoteModelVariant {
  const unique = [...new Set([...paths, ...sidecars])];
  const size = unique.reduce((sum, path) => sum + (byPath.get(path) ?? 0), 0);
  return { id, label, size, files: unique, fit: "unknown" as ModelFit };
}

/** Lightweight FE port of Rust group_weight_variants (fit left unknown). */
export function groupWeightVariants(files: RemoteModelFile[]): {
  variants: RemoteModelVariant[];
  defaultVariantId: string;
} {
  const byPath = new Map(files.map((file) => [file.path, file.size]));
  const sidecars: string[] = [];
  const safetensors: string[] = [];
  const pytorch: string[] = [];
  const gguf = new Map<string, { label: string; paths: string[] }>();
  const others: string[] = [];

  for (const file of files) {
    const name = fileName(file.path);
    const lower = name.toLowerCase();
    if (isSidecar(file.path) && !lower.endsWith(".safetensors") && !lower.endsWith(".gguf") && !lower.endsWith(".bin")) {
      sidecars.push(file.path);
      continue;
    }
    if (lower.endsWith(".gguf")) {
      const id = ggufGroupId(file.path);
      const entry = gguf.get(id) ?? { label: ggufLabel(file.path), paths: [] };
      entry.paths.push(file.path);
      gguf.set(id, entry);
      continue;
    }
    if (lower.endsWith(".safetensors") || lower.endsWith(".safetensors.index.json")) {
      safetensors.push(file.path);
      continue;
    }
    if (lower.startsWith("pytorch_model") && lower.endsWith(".bin")) {
      pytorch.push(file.path);
      continue;
    }
    if (lower.endsWith(".bin") || lower.endsWith(".pt") || lower.endsWith(".pth") || lower.endsWith(".onnx")) {
      others.push(file.path);
    }
  }

  const variants: RemoteModelVariant[] = [];
  for (const [id, group] of [...gguf.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
    variants.push(makeVariant(id, group.label, group.paths.sort(), sidecars, byPath));
  }
  if (safetensors.length) {
    variants.push(makeVariant("safetensors", "Safetensors", safetensors.sort(), sidecars, byPath));
  }
  if (pytorch.length) {
    variants.push(makeVariant("pytorch", "PyTorch", pytorch.sort(), sidecars, byPath));
  }
  for (const path of others) {
    variants.push(makeVariant(`other:${path}`, fileName(path), [path], sidecars, byPath));
  }

  if (!variants.length) {
    variants.push({ id: "empty", label: "—", size: 0, files: [], fit: "unknown" });
  }

  const defaultVariantId =
    variants.find((item) => item.id.startsWith("gguf:"))?.id ??
    variants.find((item) => item.id === "safetensors")?.id ??
    variants[0]?.id ??
    "";

  return { variants, defaultVariantId };
}

function cardFromInfo(info: {
  name: string;
  author?: string;
  task?: string;
  likes?: number;
  downloads?: number;
  updatedAt?: Date;
  tags?: string[];
  library_name?: string;
  cardData?: Record<string, unknown> | null;
  safetensors?: { total?: number };
}): RemoteModelCard {
  const repo = info.name;
  const slash = repo.indexOf("/");
  const author = (info.author ?? (slash >= 0 ? repo.slice(0, slash) : "")).trim();
  const name = slash >= 0 ? repo.slice(slash + 1) : repo;
  const cardData = info.cardData ?? {};
  const license =
    typeof cardData.license === "string"
      ? cardData.license
      : Array.isArray(cardData.license) && typeof cardData.license[0] === "string"
        ? cardData.license[0]
        : undefined;
  const baseModel =
    typeof cardData.base_model === "string"
      ? cardData.base_model
      : Array.isArray(cardData.base_model) && typeof cardData.base_model[0] === "string"
        ? cardData.base_model[0]
        : undefined;
  const description =
    typeof cardData.description === "string"
      ? cardData.description
      : typeof (cardData as { summary?: unknown }).summary === "string"
        ? String((cardData as { summary: string }).summary)
        : "";
  const tags = Array.isArray(info.tags) ? info.tags.filter(Boolean) : [];
  const total = info.safetensors?.total;
  let params: string | undefined;
  if (typeof total === "number" && total > 0) {
    if (total >= 1_000_000_000) {
      const scaled = total / 1_000_000_000;
      params = Number.isInteger(scaled) ? `${scaled}B` : `${scaled.toFixed(1)}B`;
    } else if (total >= 1_000_000) {
      const scaled = total / 1_000_000;
      params = Number.isInteger(scaled) ? `${scaled}M` : `${scaled.toFixed(1)}M`;
    }
  }

  return {
    author,
    name,
    description,
    tags,
    license,
    library: info.library_name,
    pipeline: info.task,
    baseModel,
    downloads: info.downloads,
    likes: info.likes,
    params,
    updatedAt: info.updatedAt instanceof Date ? info.updatedAt.toISOString() : undefined,
  };
}

async function collectFiles(repo: string, revision: string, fetchImpl: typeof fetch) {
  const files: RemoteModelFile[] = [];
  for await (const entry of listFiles({
    repo,
    recursive: true,
    revision,
    fetch: fetchImpl,
  })) {
    if (entry.type !== "file") continue;
    files.push({
      path: entry.path,
      size: entry.lfs?.size ?? entry.size ?? 0,
    });
  }
  return files;
}

/** Probe HF via the same browser Hub path as Discover (not Rust reqwest). */
export async function probeHfModel(repo: string, revision = "main"): Promise<RemoteModelProbe> {
  const fetchImpl = timedFetch(12_000);
  let info: Parameters<typeof cardFromInfo>[0];
  try {
    info = (await modelInfo({
      name: repo,
      revision,
      additionalFields: ["author", "cardData", "tags", "library_name", "sha", "safetensors"],
      fetch: fetchImpl,
    })) as Parameters<typeof cardFromInfo>[0];
  } catch (error) {
    throw mapHfError(error);
  }

  const card = cardFromInfo(info);
  let files: RemoteModelFile[] = [];
  try {
    files = await withTimeout(collectFiles(repo, revision, fetchImpl), 15_000, "files-timeout");
  } catch {
    files = [];
  }

  const { variants, defaultVariantId } = groupWeightVariants(files);
  return {
    source: "huggingface",
    repo,
    revision,
    files,
    variants,
    defaultVariantId,
    card,
  };
}

/** Fetch README.md through Hub downloadFile (same network path as list). */
export async function fetchHfReadme(repo: string, revision = "main"): Promise<string> {
  try {
    const blob = await downloadFile({
      repo,
      path: "README.md",
      revision,
      fetch: timedFetch(15_000),
    });
    if (!blob) return "";
    return (await blob.text()).trim();
  } catch (error) {
    throw mapHfError(error);
  }
}
