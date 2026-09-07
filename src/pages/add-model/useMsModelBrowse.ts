import { useCallback, useEffect, useRef, useState } from "react";
import { browseRemoteModels } from "../../lib/api";
import type { ModelFormat, RemoteModelHit } from "../../lib/types";
import type { BrowseState } from "./useHfModelBrowse";

const MS_LIMIT = 48;

function invokeMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function useMsModelBrowse(query: string, format: ModelFormat, enabled: boolean): BrowseState {
  const [hits, setHits] = useState<RemoteModelHit[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [error, setError] = useState<string>();
  const cursorRef = useRef<string | null>(null);
  const busyRef = useRef(false);
  const generationRef = useRef(0);

  const loadPage = useCallback(async (opts: { append: boolean; query: string; format: ModelFormat; gen: number }) => {
    busyRef.current = true;
    if (opts.append) setIsLoadingMore(true);
    else {
      setIsLoading(true);
      setHits([]);
      setHasMore(false);
      cursorRef.current = null;
    }
    setError(undefined);
    try {
      const result = await browseRemoteModels(
        "modelscope",
        opts.query,
        opts.format,
        opts.append ? cursorRef.current : null,
        MS_LIMIT,
      );
      if (opts.gen !== generationRef.current) return;
      setHits((prev) => (opts.append ? [...prev, ...result.hits] : result.hits));
      cursorRef.current = result.nextCursor ?? null;
      setHasMore(Boolean(result.hasMore && result.nextCursor));
    } catch (err) {
      if (opts.gen !== generationRef.current) return;
      if (!opts.append) setHits([]);
      setHasMore(false);
      setError(invokeMessage(err));
    } finally {
      if (opts.gen === generationRef.current) {
        setIsLoading(false);
        setIsLoadingMore(false);
        busyRef.current = false;
      }
    }
  }, []);

  useEffect(() => {
    if (!enabled) return;
    const gen = ++generationRef.current;
    const timer = window.setTimeout(() => {
      void loadPage({ append: false, query, format, gen });
    }, 350);
    return () => {
      window.clearTimeout(timer);
      generationRef.current += 1;
    };
  }, [enabled, format, loadPage, query]);

  const fetchMore = useCallback(() => {
    if (!enabled || busyRef.current || isLoading || isLoadingMore || !hasMore || error) return;
    const gen = generationRef.current;
    void loadPage({ append: true, query, format, gen });
  }, [enabled, error, format, hasMore, isLoading, isLoadingMore, loadPage, query]);

  const reload = useCallback(() => {
    const gen = ++generationRef.current;
    void loadPage({ append: false, query, format, gen });
  }, [format, loadPage, query]);

  return { hits, isLoading, isLoadingMore, hasMore, error, fetchMore, reload };
}
