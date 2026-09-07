import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ModelFormat, RemoteModelHit } from "../../lib/types";
import { createHfModelIterator, HF_BATCH_SIZE, pullBatch, type HfModelIterator } from "../../lib/hfListModels";
import {
  BROWSE_DEBOUNCE_MS,
  browseFailSoftMessage,
  type BrowseState,
} from "./modelBrowseShared";

export type { BrowseState };

export function useHfModelBrowse(query: string, format: ModelFormat, enabled: boolean): BrowseState {
  const { t } = useTranslation();
  const [hits, setHits] = useState<RemoteModelHit[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [error, setError] = useState<string>();
  const iteratorRef = useRef<HfModelIterator | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const busyRef = useRef(false);
  const generationRef = useRef(0);
  const queryRef = useRef(query);
  const formatRef = useRef(format);
  queryRef.current = query;
  formatRef.current = format;

  const loadInitial = useCallback(async (nextQuery: string, nextFormat: ModelFormat, gen: number) => {
    busyRef.current = true;
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    iteratorRef.current = createHfModelIterator({
      query: nextQuery,
      format: nextFormat,
      signal: controller.signal,
    });
    setIsLoading(true);
    setIsLoadingMore(false);
    setError(undefined);
    setHits([]);
    setHasMore(false);
    try {
      const iterator = iteratorRef.current;
      const batch = await pullBatch(iterator, HF_BATCH_SIZE, controller.signal);
      if (gen !== generationRef.current) return;
      setHits(batch.hits);
      setHasMore(!batch.done);
    } catch (err) {
      if (gen !== generationRef.current) return;
      const message = browseFailSoftMessage(err, t("modelHfUnreachable"));
      if (message) {
        setHits([]);
        setHasMore(false);
        setError(message);
      }
    } finally {
      if (gen === generationRef.current) {
        setIsLoading(false);
        busyRef.current = false;
      }
    }
  }, [t]);

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
      void loadInitial(query, format, gen);
    }, BROWSE_DEBOUNCE_MS);
    return () => {
      window.clearTimeout(timer);
      abortRef.current?.abort();
    };
  }, [enabled, format, loadInitial, query]);

  const fetchMore = useCallback(() => {
    if (!enabled || busyRef.current || isLoading || isLoadingMore || !hasMore || error) return;
    const iterator = iteratorRef.current;
    if (!iterator) return;
    const gen = generationRef.current;
    busyRef.current = true;
    setIsLoadingMore(true);
    void (async () => {
      try {
        const batch = await pullBatch(iterator, HF_BATCH_SIZE, abortRef.current?.signal);
        if (gen !== generationRef.current) return;
        setHits((prev) => [...prev, ...batch.hits]);
        setHasMore(!batch.done);
      } catch (err) {
        if (gen !== generationRef.current) return;
        const message = browseFailSoftMessage(err, t("modelHfUnreachable"));
        if (message) setError(message);
      } finally {
        if (gen === generationRef.current) {
          setIsLoadingMore(false);
          busyRef.current = false;
        }
      }
    })();
  }, [enabled, error, hasMore, isLoading, isLoadingMore, t]);

  const reload = useCallback(() => {
    if (!enabled) return;
    const gen = ++generationRef.current;
    void loadInitial(queryRef.current, formatRef.current, gen);
  }, [enabled, loadInitial]);

  return { hits, isLoading, isLoadingMore, hasMore, error, fetchMore, reload };
}
