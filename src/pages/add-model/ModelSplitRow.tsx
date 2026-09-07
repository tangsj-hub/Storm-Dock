import { memo, type KeyboardEvent } from "react";
import type { RemoteModelHit } from "../../lib/types";
import { CapabilityIcons } from "./CapabilityIcons";
import { SplitTrailStats } from "./MetricCells";
import { ModelIdentity } from "./ModelIdentity";
import extra from "./discover.module.css";

/**
 * Split master row — denser: avatar | title/owner | caps | trailing quiet stats.
 * Preview pane role unchanged.
 */
export const ModelSplitRow = memo(function ModelSplitRow({
  hit,
  selected,
  onSelect,
}: {
  hit: RemoteModelHit;
  selected: boolean;
  onSelect: (hit: RemoteModelHit) => void;
}) {
  const select = () => onSelect(hit);
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      select();
    }
  };

  return (
    <div
      aria-pressed={selected}
      className={`${extra.splitRow} ${selected ? extra.splitRowSelected : ""}`}
      onClick={select}
      onKeyDown={onKeyDown}
      role="button"
      tabIndex={0}
    >
      <ModelIdentity hit={hit} variant="split" />
      <CapabilityIcons hit={hit} />
      <SplitTrailStats hit={hit} />
    </div>
  );
});
