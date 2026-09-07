import { listModels } from "@huggingface/hub";
import type { ModelFormat, RemoteModelHit } from "../../lib/types";
import { fetchWithTimeout, HF_FETCH_TIMEOUT_MS } from "../../lib/fetchTimeout";

export { HF_FETCH_TIMEOUT_MESSAGE, HF_FETCH_TIMEOUT_MS } from "../../lib/fetchTimeout";

export const HF_BATCH_SIZE = 48;

const FORMAT_TAG: Partial<Record<ModelFormat, string>> = {
  gguf: "gguf",
  safetensors: "safetensors",
  mlx: "mlx",
  finetune: "peft",
};

type HfListEntry = {
  name: string;
  downloads?: number;
  likes?: number;
  task?: string;
  updatedAt?: Date;
  author?: string;
  library_name?: string;
  tags?: string[];
  safetensors?: { total?: number };
};

function formatParams(total?: number) {
  if (total == null || !Number.isFinite(total) || total <= 0) return;
  if (total >= 1_000_000_000) {
    const scaled = total / 1_000_000_000;
    return Number.isInteger(scaled) ? `${scaled}B` : `${scaled.toFixed(1)}B`;
  }
  if (total >= 1_000_000) {
    const scaled = total / 1_000_000;
    return Number.isInteger(scaled) ? `${scaled}M` : `${scaled.toFixed(1)}M`;
  }
  return String(total);
}

function paramsFromLabel(raw: string) {
  const tag = raw.trim();
  const unit = tag.slice(-1).toUpperCase();
  if (unit !== "B" && unit !== "M") return;
  const number = tag.slice(0, -1);
  const value = Number(number);
  if (!Number.isFinite(value) || value <= 0) return;
  return `${number}${unit}`;
}

export function mapHfEntry(entry: HfListEntry): RemoteModelHit {
  const repo = entry.name;
  const slash = repo.indexOf("/");
  const author = entry.author ?? (slash >= 0 ? repo.slice(0, slash) : "");
  const name = slash >= 0 ? repo.slice(slash + 1) : repo;
  const tags = Array.isArray(entry.tags) ? entry.tags.filter(Boolean) : [];
  return {
    source: "huggingface",
    repo,
    name,
    author,
    downloads: entry.downloads,
    likes: entry.likes,
    library: entry.library_name,
    pipeline: entry.task,
    tags,
    params: formatParams(entry.safetensors?.total) ?? tags.map(paramsFromLabel).find(Boolean),
    updatedAt: entry.updatedAt instanceof Date ? entry.updatedAt.toISOString() : undefined,
  };
}

export type HfModelIterator = AsyncGenerator<HfListEntry, void, unknown>;

/** Hub Discover default: trendingScore (hot), desc only. */
export const HF_BROWSE_SORT = "trendingScore" as const;

/**
 * Ensure Hub list requests use trendingScore with direction=-1.
 * `@huggingface/hub` listModels sets `sort` but not `direction`; HF only allows
 * descending for trendingScore (asc is rejected).
 */
export function makeSortFetch(
  sortBy: typeof HF_BROWSE_SORT = HF_BROWSE_SORT,
  baseFetch: typeof fetch = fetch,
  timeoutMs: number = HF_FETCH_TIMEOUT_MS,
): typeof fetch {
  return (input, init) => {
    const rawUrl =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.toString()
          : input.url;
    const url = new URL(rawUrl);
    url.searchParams.set("sort", sortBy);
    // trendingScore is desc-only on Hub.
    url.searchParams.set("direction", "-1");
    return fetchWithTimeout(url, { ...init, timeoutMs, fetch: baseFetch });
  };
}

function composeSignals(a: AbortSignal, b?: AbortSignal): AbortSignal {
  if (!b) return a;
  const anyFn = (AbortSignal as typeof AbortSignal & { any?: (signals: AbortSignal[]) => AbortSignal }).any;
  if (typeof anyFn === "function") return anyFn([a, b]);
  if (a.aborted || b.aborted) {
    const controller = new AbortController();
    controller.abort();
    return controller.signal;
  }
  const controller = new AbortController();
  const onAbort = () => controller.abort();
  a.addEventListener("abort", onAbort, { once: true });
  b.addEventListener("abort", onAbort, { once: true });
  return controller.signal;
}

export function createHfModelIterator(opts: {
  query: string;
  format: ModelFormat;
  accessToken?: string;
  fetch?: typeof fetch;
  /** When set, every Hub list request is aborted with this signal (composed with per-request timeout). */
  signal?: AbortSignal;
}): HfModelIterator {
  const query = opts.query.trim();
  const tag = FORMAT_TAG[opts.format];
  const innerFetch = opts.fetch ?? fetch;
  const userSignal = opts.signal;
  const baseFetch: typeof fetch = userSignal
    ? (input, init) => innerFetch(input, { ...init, signal: composeSignals(userSignal, init?.signal ?? undefined) })
    : innerFetch;
  return listModels({
    search: {
      ...(query ? { query } : {}),
      ...(tag ? { tags: [tag] } : {}),
    },
    sort: HF_BROWSE_SORT,
    additionalFields: ["author", "library_name", "tags", "safetensors"],
    ...(opts.accessToken ? { accessToken: opts.accessToken } : {}),
    fetch: makeSortFetch(HF_BROWSE_SORT, baseFetch),
  }) as unknown as HfModelIterator;
}

/** Pull up to `size` models from an async iterator (Unsloth-style batching, clean reimplementation). */
export async function pullBatch(
  iterator: AsyncIterator<HfListEntry>,
  size: number,
  signal?: AbortSignal,
): Promise<{ hits: RemoteModelHit[]; done: boolean }> {
  const hits: RemoteModelHit[] = [];
  for (let i = 0; i < size; i += 1) {
    if (signal?.aborted) {
      throw new DOMException("Aborted", "AbortError");
    }
    const next = await iterator.next();
    if (next.done) return { hits, done: true };
    hits.push(mapHfEntry(next.value));
  }
  return { hits, done: false };
}
