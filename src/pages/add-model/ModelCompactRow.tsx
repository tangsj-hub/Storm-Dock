import { Download, Heart } from "lucide-react";
import { useTranslation } from "react-i18next";
import { formatCount } from "../../lib/modelHits";
import { modelDetailPath, type ModelCenterTab, type ModelFormat, type ModelSource, type RemoteModelHit } from "../../lib/types";
import { CapabilityIcons } from "./CapabilityIcons";
import { agoLabel } from "./MetricCells";
import { ModelIdentity } from "./ModelIdentity";
import extra from "./page.module.css";

export type DiscoverHitContext = {
  query: string;
  source: ModelSource;
  format: ModelFormat;
  tab: ModelCenterTab;
};

/**
 * Compact table row — same grid columns as CompactColumnHeader for vertical alignment.
 */
export function ModelCompactRow({
  hit,
  context,
}: {
  hit: RemoteModelHit;
  context: DiscoverHitContext;
}) {
  const { t } = useTranslation();
  const ago = agoLabel(hit.updatedAt, t);

  return (
    <a className={extra.hitCard} href={modelDetailPath(hit.source, hit.repo, context)}>
      <ModelIdentity hit={hit} variant="compact" />
      <CapabilityIcons hit={hit} />
      <span className={extra.hitMetric}>{hit.params || "—"}</span>
      <span className={extra.hitMetric}>{ago || "—"}</span>
      <span className={extra.hitStat}>
        {hit.downloads != null ? (
          <>
            <Download aria-hidden="true" size={11} strokeWidth={1.75} />
            {formatCount(hit.downloads)}
          </>
        ) : (
          "—"
        )}
      </span>
      <span className={extra.hitStat}>
        {hit.likes != null ? (
          <>
            <Heart aria-hidden="true" size={11} strokeWidth={1.75} />
            {formatCount(hit.likes)}
          </>
        ) : (
          "—"
        )}
      </span>
      <span aria-hidden="true" className={extra.hitDownload}>
        <Download size={15} strokeWidth={1.75} />
      </span>
    </a>
  );
}

/** @deprecated Prefer ModelCompactRow — kept as a thin alias for any residual imports. */
export const HitRow = ModelCompactRow;
