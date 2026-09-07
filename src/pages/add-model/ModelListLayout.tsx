import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import type { RemoteModelHit } from "../../lib/types";
import type { DiscoverListView } from "./discoverListView";
import type { DiscoverHitContext } from "./ModelCompactRow";
import { ModelCard } from "./ModelCard";
import { CompactColumnHeader } from "./CompactColumnHeader";
import { ModelCompactRow } from "./ModelCompactRow";
import { ModelSplitRow } from "./ModelSplitRow";
import { SplitPreviewPane } from "./SplitPreviewPane";
import { VirtualModelRows, type VirtualLaneConfig } from "./VirtualModelRows";
import extra from "./discover.module.css";

const LIST_GAP = 8;
const NARROW_SPLIT_MQ = "(max-width: 860px)";
/** Compact scan-path row (avatar 36 + padding). */
const COMPACT_CONFIG: VirtualLaneConfig = { columns: 1, cellHeight: 64, rowHeight: 72, columnGap: LIST_GAP };
/** Split master denser row with trailing stats. */
const SPLIT_CONFIG: VirtualLaneConfig = { columns: 1, cellHeight: 60, rowHeight: 68, columnGap: LIST_GAP };
/** Soft-card visual height (14px pad + identity ~52 + meta). */
export const CARDS_CELL_HEIGHT = 112;
/** Virtual row stride = cell + 8px gap. */
export const CARDS_ROW_HEIGHT = 120;

function cardsConfig(columns: number): VirtualLaneConfig {
  return { columns, cellHeight: CARDS_CELL_HEIGHT, rowHeight: CARDS_ROW_HEIGHT, columnGap: LIST_GAP };
}

function hitKey(hit: RemoteModelHit) {
  return `${hit.source}:${hit.repo}`;
}

export function ModelListLayout({
  view,
  hits,
  context,
  footer,
  onScrollRoot,
  onNotice,
}: {
  view: DiscoverListView;
  hits: RemoteModelHit[];
  context: DiscoverHitContext;
  footer?: ReactNode;
  onScrollRoot?: (node: HTMLDivElement | null) => void;
  onNotice?: (message: string, failed?: boolean) => void;
}) {
  const [cardColumns, setCardColumns] = useState(2);
  const [cardsHost, setCardsHost] = useState<HTMLDivElement | null>(null);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [narrow, setNarrow] = useState(() =>
    typeof window !== "undefined" ? window.matchMedia(NARROW_SPLIT_MQ).matches : false,
  );
  /** Narrow split: list vs detail (detail-only by default). */
  const [narrowShowList, setNarrowShowList] = useState(false);

  const selectedHit = useMemo(() => {
    if (!selectedKey) return null;
    return hits.find((hit) => hitKey(hit) === selectedKey) ?? null;
  }, [hits, selectedKey]);

  useEffect(() => {
    const mq = window.matchMedia(NARROW_SPLIT_MQ);
    const sync = () => {
      const next = mq.matches;
      setNarrow(next);
      if (next) setNarrowShowList(false);
    };
    sync();
    mq.addEventListener("change", sync);
    return () => mq.removeEventListener("change", sync);
  }, []);

  // Keep selection valid; auto-select first row when entering split with no selection.
  useEffect(() => {
    if (view !== "split") return;
    if (hits.length === 0) {
      setSelectedKey(null);
      return;
    }
    if (selectedKey && hits.some((hit) => hitKey(hit) === selectedKey)) return;
    setSelectedKey(hitKey(hits[0]));
  }, [view, hits, selectedKey]);

  useEffect(() => {
    if (view !== "cards" || !cardsHost) return;
    const sync = () => setCardColumns(cardsHost.clientWidth >= 720 ? 2 : 1);
    sync();
    const observer = new ResizeObserver(sync);
    observer.observe(cardsHost);
    return () => observer.disconnect();
  }, [view, cardsHost]);

  const onSelect = useCallback((hit: RemoteModelHit) => {
    setSelectedKey(hitKey(hit));
    setNarrowShowList(false);
  }, []);

  if (view === "split") {
    const mode = narrow ? (narrowShowList ? "list" : "detail") : "both";
    const showMaster = mode === "both" || mode === "list";
    const showDetail = mode === "both" || mode === "detail";

    return (
      <div className={extra.splitLayout} data-mode={mode}>
        {showMaster ? (
          <div className={extra.splitMaster}>
            <VirtualModelRows
              config={SPLIT_CONFIG}
              footer={footer}
              hits={hits}
              onScrollRoot={onScrollRoot}
              renderHit={(hit) => (
                <ModelSplitRow
                  hit={hit}
                  onSelect={onSelect}
                  selected={hitKey(hit) === selectedKey}
                />
              )}
              resetKey={`split:${mode}`}
            />
          </div>
        ) : null}
        {showDetail ? (
          <SplitPreviewPane
            hit={selectedHit}
            onBack={narrow ? () => setNarrowShowList(true) : undefined}
            onNotice={onNotice}
          />
        ) : null}
      </div>
    );
  }

  if (view === "cards") {
    return (
      <div className={extra.cardsHost} ref={setCardsHost}>
        <VirtualModelRows
          config={cardsConfig(cardColumns)}
          footer={footer}
          hits={hits}
          onScrollRoot={onScrollRoot}
          renderHit={(hit) => <ModelCard context={context} hit={hit} />}
          resetKey={`cards:${cardColumns}:${CARDS_ROW_HEIGHT}`}
        />
      </div>
    );
  }

  return (
    <div className={extra.compactHost}>
      <CompactColumnHeader />
      <VirtualModelRows
        config={COMPACT_CONFIG}
        footer={footer}
        hits={hits}
        onScrollRoot={onScrollRoot}
        renderHit={(hit) => <ModelCompactRow context={context} hit={hit} />}
        resetKey="compact"
      />
    </div>
  );
}
