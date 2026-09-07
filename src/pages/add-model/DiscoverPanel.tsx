import { Search } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { modelCenterPath, type ModelCenterTab, type ModelFormat, type ModelSource } from "../../lib/types";
import styles from "../add/page.module.css";
import { DiscoverListChrome } from "./DiscoverListChrome";
import { DiscoverToolbar } from "./DiscoverToolbar";
import { ModelListLayout } from "./ModelListLayout";
import { readDiscoverListView, writeDiscoverListView, type DiscoverListView } from "./discoverListView";
import { useDiscoverInfiniteScroll } from "./useDiscoverInfiniteScroll";
import { useRemoteModelBrowse } from "./useRemoteModelBrowse";
import extra from "./page.module.css";

function DiscoverSkeleton() {
  return (
    <div aria-busy="true" className={extra.skeletonList}>
      {Array.from({ length: 6 }, (_, index) => (
        <div className={extra.skeletonRow} key={index}>
          <div className={`${extra.skeletonBlock} ${extra.skeletonAvatar}`} />
          <div className={extra.skeletonLines}>
            <div className={`${extra.skeletonBlock} ${extra.skeletonLine}`} />
            <div className={`${extra.skeletonBlock} ${extra.skeletonLineShort}`} />
          </div>
          <div className={extra.skeletonBlock} />
          <div className={extra.skeletonBlock} />
        </div>
      ))}
    </div>
  );
}

export function DiscoverPanel({
  query,
  source,
  format,
  tab,
  onQueryChange,
  onSourceChange,
  onFormatChange,
  onNotice,
}: {
  query: string;
  source: ModelSource;
  format: ModelFormat;
  tab: ModelCenterTab;
  onQueryChange: (value: string) => void;
  onSourceChange: (value: ModelSource) => void;
  onFormatChange: (value: ModelFormat) => void;
  onNotice: (message: string, failed?: boolean) => void;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(query);
  const [listView, setListView] = useState<DiscoverListView>(() => readDiscoverListView());
  const [scrollRoot, setScrollRoot] = useState<Element | null>(null);
  const browse = useRemoteModelBrowse(source, query, format, tab === "discover");
  const onScrollRoot = useCallback((node: HTMLDivElement | null) => {
    setScrollRoot(node);
  }, []);
  const { sentinelRef } = useDiscoverInfiniteScroll({
    enabled: tab === "discover" && browse.hits.length > 0,
    hasMore: browse.hasMore,
    isLoading: browse.isLoading,
    isLoadingMore: browse.isLoadingMore,
    hitsLength: browse.hits.length,
    error: browse.error,
    fetchMore: browse.fetchMore,
    root: scrollRoot,
  });

  useEffect(() => {
    setDraft(query);
  }, [query]);

  useEffect(() => {
    if (browse.error) onNotice(browse.error, true);
  }, [browse.error, onNotice]);

  useEffect(() => {
    window.history.replaceState({}, "", modelCenterPath({ query, source, format, tab: "discover" }));
  }, [format, query, source]);

  const onViewChange = useCallback((view: DiscoverListView) => {
    setListView(view);
    writeDiscoverListView(view);
  }, []);

  const busy = browse.isLoading;
  const context = { query, source, format, tab: "discover" as const };

  return (
    <form
      className={extra.discoverPanel}
      onSubmit={(event) => {
        event.preventDefault();
        if (!busy) onQueryChange(draft.trim());
      }}
    >
      <DiscoverToolbar
        busy={busy}
        draft={draft}
        format={format}
        onClearSearch={() => {
          setDraft("");
          onQueryChange("");
        }}
        onDraftChange={setDraft}
        onFormatChange={onFormatChange}
        onSourceChange={onSourceChange}
        source={source}
      />

      <div className={extra.hitList}>
        <DiscoverListChrome onViewChange={onViewChange} view={listView} />
        <div className={extra.listBody}>
          {browse.isLoading && browse.hits.length === 0 ? (
            <div aria-label={t("modelBrowseLoading")} role="status">
              <DiscoverSkeleton />
            </div>
          ) : browse.hits.length === 0 && !browse.isLoading ? (
            <div className={extra.emptyState}>
              <div aria-hidden="true" className={extra.emptyWell}>
                <Search size={22} strokeWidth={1.75} />
              </div>
              <h3 className={extra.emptyTitle}>{browse.error ? browse.error : t("modelSearchEmpty")}</h3>
              {!browse.error ? <p className={extra.emptyBody}>{t("modelListEmptyHint")}</p> : null}
            </div>
          ) : (
            <ModelListLayout
              context={context}
              onNotice={onNotice}
              footer={(
                <>
                  <div className={extra.sentinel} ref={sentinelRef} />
                  <div className={extra.loadMoreRow}>
                    {browse.isLoadingMore ? <span className={extra.loadMoreStatus}>{t("modelLoadingMore")}</span> : null}
                    {!browse.isLoadingMore && browse.hasMore ? (
                      <button className={styles.secondary} onClick={() => browse.fetchMore()} type="button">
                        {t("modelLoadMore")}
                      </button>
                    ) : null}
                    {browse.error ? (
                      <button className={styles.secondary} onClick={() => browse.reload()} type="button">
                        {t("modelRetry")}
                      </button>
                    ) : null}
                  </div>
                </>
              )}
              hits={browse.hits}
              onScrollRoot={onScrollRoot}
              view={listView}
            />
          )}
        </div>
      </div>
    </form>
  );
}
