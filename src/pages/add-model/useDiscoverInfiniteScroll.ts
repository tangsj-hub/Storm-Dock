import { useEffect, useRef } from "react";

const AUTO_FILL_CAP = 3;

export function useDiscoverInfiniteScroll(opts: {
  enabled: boolean;
  hasMore: boolean;
  isLoading: boolean;
  isLoadingMore: boolean;
  hitsLength: number;
  error?: string;
  fetchMore: () => void;
  root?: Element | null;
}) {
  const sentinelRef = useRef<HTMLDivElement | null>(null);
  const autoFillRef = useRef(0);
  const fetchMoreRef = useRef(opts.fetchMore);
  fetchMoreRef.current = opts.fetchMore;

  useEffect(() => {
    autoFillRef.current = 0;
  }, [opts.enabled]);

  // Auto-fill a few pages when the list is shorter than the viewport.
  useEffect(() => {
    if (!opts.enabled || !opts.hasMore || opts.isLoading || opts.isLoadingMore || opts.error) return;
    if (autoFillRef.current >= AUTO_FILL_CAP) return;
    const sentinel = sentinelRef.current;
    if (!sentinel) return;
    const rootRect = opts.root?.getBoundingClientRect();
    const sentinelTop = sentinel.getBoundingClientRect().top;
    const bottom = rootRect ? rootRect.bottom : window.innerHeight;
    if (sentinelTop > bottom + 80) return;
    autoFillRef.current += 1;
    fetchMoreRef.current();
  }, [
    opts.enabled,
    opts.error,
    opts.hasMore,
    opts.hitsLength,
    opts.isLoading,
    opts.isLoadingMore,
    opts.root,
  ]);

  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel || !opts.enabled) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        if (!opts.hasMore || opts.isLoading || opts.isLoadingMore || opts.error) return;
        fetchMoreRef.current();
      },
      { root: opts.root ?? null, rootMargin: "240px 0px", threshold: 0 },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [opts.enabled, opts.error, opts.hasMore, opts.isLoading, opts.isLoadingMore, opts.root]);

  return { sentinelRef };
}
