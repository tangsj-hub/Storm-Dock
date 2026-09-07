import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useRef, type ReactNode } from "react";
import type { RemoteModelHit } from "../../lib/types";
import extra from "./page.module.css";

export type VirtualLaneConfig = {
  columns: number;
  /** Fixed stride passed to estimateSize (includes gap). */
  rowHeight: number;
  /** Visual cell height inside the stride. */
  cellHeight: number;
  columnGap?: number;
};

/**
 * Lane-aware virtualizer with a fixed estimateSize stride.
 * No measureElement — avoids scroll jumps when infinite scroll appends rows.
 */
export function VirtualModelRows({
  hits,
  config,
  renderHit,
  footer,
  onScrollRoot,
  resetKey,
  minWidth,
}: {
  hits: RemoteModelHit[];
  config: VirtualLaneConfig;
  renderHit: (hit: RemoteModelHit, index: number) => ReactNode;
  footer?: ReactNode;
  onScrollRoot?: (node: HTMLDivElement | null) => void;
  /** Changing this resets scrollTop (e.g. view mode switch). */
  resetKey?: string | number;
  minWidth?: number | string;
}) {
  const parentRef = useRef<HTMLDivElement | null>(null);
  const lanes = Math.max(1, config.columns);
  const rowCount = Math.ceil(hits.length / lanes);
  const columnGap = config.columnGap ?? 8;

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => config.rowHeight,
    overscan: 10,
    getItemKey: (rowIndex) => {
      const hit = hits[rowIndex * lanes];
      return hit ? `${hit.source}:${hit.repo}` : `row-${rowIndex}`;
    },
  });

  useEffect(() => {
    onScrollRoot?.(parentRef.current);
  }, [onScrollRoot, hits.length, resetKey]);

  useEffect(() => {
    const el = parentRef.current;
    if (!el) return;
    el.scrollTop = 0;
  }, [resetKey]);

  return (
    <div
      className={extra.virtualList}
      ref={(node) => {
        parentRef.current = node;
        onScrollRoot?.(node);
      }}
    >
      <div
        className={extra.virtualInner}
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          minWidth: minWidth ?? undefined,
          overflowAnchor: "none",
        }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const startIndex = virtualRow.index * lanes;
          return (
            <div
              className={extra.virtualRow}
              data-index={virtualRow.index}
              key={virtualRow.key}
              style={{
                transform: `translateY(${virtualRow.start}px)`,
                height: `${config.rowHeight}px`,
                contain: "layout",
              }}
            >
              <div
                className={extra.virtualLane}
                style={{
                  gridTemplateColumns: `repeat(${lanes}, minmax(0, 1fr))`,
                  columnGap: `${columnGap}px`,
                  height: `${config.cellHeight}px`,
                }}
              >
                {Array.from({ length: lanes }, (_, lane) => {
                  const hit = hits[startIndex + lane];
                  if (!hit) return <div key={`empty-${lane}`} />;
                  return (
                    <div className={extra.virtualCell} key={`${hit.source}:${hit.repo}`}>
                      {renderHit(hit, startIndex + lane)}
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}
      </div>
      {footer}
    </div>
  );
}
