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
import extra from "./page.module.css";

const LIST_GAP = 8;
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
}: {
  view: DiscoverListView;
  hits: RemoteModelHit[];
  context: DiscoverHitContext;
  footer?: ReactNode;
  onScrollRoot?: (node: HTMLDivElement | null) => void;
}) {
  const [cardColumns, setCardColumns] = useState(2);
  const [cardsHost, setCardsHost] = useState<HTMLDivElement | null>(null);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);

  const selectedHit = useMemo(() => {
    if (!selectedKey) return null;
    return hits.find((hit) => hitKey(hit) === selectedKey) ?? null;
  }, [hits, selectedKey]);

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
  }, []);

  if (view === "split") {
    return (
      <div className={extra.splitLayout}>
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
            resetKey="split"
          />
        </div>
        <SplitPreviewPane context={context} hit={selectedHit} />
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
