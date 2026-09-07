import { useEffect, useState } from "react";
import { fetchRemoteModelReadme } from "../../lib/api";
import type { ModelSource } from "../../lib/types";

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
    if (!enabled || !source || !repo || !revision) {
      setMarkdown(undefined);
      setLoading(false);
      setError(undefined);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(undefined);
    setMarkdown(undefined);
    void fetchRemoteModelReadme(source, repo, revision)
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
