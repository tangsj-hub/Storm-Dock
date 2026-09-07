import { useEffect, useState } from "react";
import { fetchRemoteModelReadme } from "../../lib/api";
import { HF_FETCH_TIMEOUT_MESSAGE } from "../../lib/fetchTimeout";
import type { ModelSource } from "../../lib/types";

/** Bound so a hung invoke cannot spin the README pane forever. */
const HF_README_UI_TIMEOUT_MS = 20_000;
const MS_README_UI_TIMEOUT_MS = 25_000;

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

/** HF raw/resolve is more reliable with a branch name than a 40-char commit SHA. */
export function readmeRevisionForFetch(source: ModelSource, revision: string | undefined) {
  const rev = (revision ?? "").trim();
  if (source === "huggingface" && (rev.length >= 40 || !rev)) return "main";
  return rev || undefined;
}

export function useModelReadme(
  source: ModelSource | undefined,
  repo: string,
  revision: string | undefined,
  enabled: boolean,
) {
  const [markdown, setMarkdown] = useState<string>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (!enabled || !source || !repo) {
      setMarkdown(undefined);
      setLoading(false);
      setError(undefined);
      return;
    }
    const fetchRev = readmeRevisionForFetch(source, revision);
    if (!fetchRev) {
      setMarkdown(undefined);
      setLoading(false);
      setError(undefined);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(undefined);
    setMarkdown(undefined);

    const timeoutMs = source === "huggingface" ? HF_README_UI_TIMEOUT_MS : MS_README_UI_TIMEOUT_MS;
    const timeoutMessage = source === "huggingface" ? HF_FETCH_TIMEOUT_MESSAGE : "modelReadmeFailed";

    void withTimeout(fetchRemoteModelReadme(source, repo, fetchRev), timeoutMs, timeoutMessage)
      .then((text) => {
        if (cancelled) return;
        setMarkdown(text.trim() || undefined);
      })
      .catch((err) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [enabled, source, repo, revision]);

  return { markdown, loading, error };
}
