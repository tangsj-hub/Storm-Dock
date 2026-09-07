import { useCallback, useEffect, useRef, useState } from "react";
import { browseRemoteModels } from "../../lib/api";
import type { ModelFormat, RemoteModelHit } from "../../lib/types";
import {
  BROWSE_DEBOUNCE_MS,
  browseFailSoftMessage,
  raceAbort,
  type BrowseState,
} from "./modelBrowseShared";

const MS_LIMIT = 48;

export function useMsModelBrowse(query: string, format: ModelFormat, enabled: boolean): BrowseState {
  const [hits, setHits] = useState<RemoteModelHit[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [error, setError] = useState<string>();
  const cursorRef = useRef<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const busyRef = useRef(false);
  const generationRef = useRef(0);
  const queryRef = useRef(query);
  const formatRef = useRef(format);
  queryRef.current = query;
  formatRef.current = format;

  const loadPage = useCallback(async (opts: {
    append: boolean;
    query: string;
    format: ModelFormat;
    gen: number;
  }) => {
    busyRef.current = true;
    if (opts.append) {
      setIsLoadingMore(true);
    } else {
      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;
      setIsLoading(true);
      setIsLoadingMore(false);
      setHits([]);
      setHasMore(false);
      cursorRef.current = null;
    }
    setError(undefined);
    const signal = abortRef.current?.signal;
    try {
      const result = await raceAbort(
        browseRemoteModels(
          "modelscope",
          opts.query,
          opts.format,
          opts.append ? cursorRef.current : null,
          MS_LIMIT,
        ),
        signal,
      );
      if (opts.gen !== generationRef.current) return;
      setHits((prev) => (opts.append ? [...prev, ...result.hits] : result.hits));
      cursorRef.current = result.nextCursor ?? null;
      setHasMore(Boolean(result.hasMore && result.nextCursor));
    } catch (err) {
      if (opts.gen !== generationRef.current) return;
      const message = browseFailSoftMessage(err);
      if (!message) return;
      if (!opts.append) setHits([]);
      setHasMore(false);
      setError(message);
    } finally {
      if (opts.gen === generationRef.current) {
        setIsLoading(false);
        setIsLoadingMore(false);
        busyRef.current = false;
      }
    }
  }, []);

  useEffect(() => {
    if (!enabled) {
      abortRef.current?.abort();
      generationRef.current += 1;
      setIsLoading(false);
      setIsLoadingMore(false);
      busyRef.current = false;
      return;
    }
    const gen = ++generationRef.current;
    const timer = window.setTimeout(() => {
      void loadPage({ append: false, query, format, gen });
    }, BROWSE_DEBOUNCE_MS);
    return () => {
      window.clearTimeout(timer);
      abortRef.current?.abort();
    };
  }, [enabled, format, loadPage, query]);

  const fetchMore = useCallback(() => {
    if (!enabled || busyRef.current || isLoading || isLoadingMore || !hasMore || error) return;
    const gen = generationRef.current;
    void loadPage({ append: true, query: queryRef.current, format: formatRef.current, gen });
  }, [enabled, error, hasMore, isLoading, isLoadingMore, loadPage]);

  const reload = useCallback(() => {
    if (!enabled) return;
    const gen = ++generationRef.current;
    void loadPage({ append: false, query: queryRef.current, format: formatRef.current, gen });
  }, [enabled, loadPage]);

  return { hits, isLoading, isLoadingMore, hasMore, error, fetchMore, reload };
}
