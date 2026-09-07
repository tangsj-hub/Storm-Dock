import { Download } from "lucide-react";
import { memo } from "react";
import { modelDetailPath, type RemoteModelHit } from "../../lib/types";
import { CapabilityIcons } from "./CapabilityIcons";
import { MetricSummary } from "./MetricCells";
import { ModelIdentity } from "./ModelIdentity";
import type { DiscoverHitContext } from "./ModelCompactRow";
import extra from "./discover.module.css";

/**
 * Soft discover card — zones: avatar | title stack | meta/tags | quiet stats | trailing action.
 * Structure inspired by Hub ResultCard maturity; Storm tokens only (no hub.css).
 */
export const ModelCard = memo(function ModelCard({
  hit,
  context,
}: {
  hit: RemoteModelHit;
  context: DiscoverHitContext;
}) {
  return (
    <a className={extra.softCard} href={modelDetailPath(hit.source, hit.repo, context)}>
      <div className={extra.softCardBody}>
        <ModelIdentity hit={hit} variant="card" />
        <span aria-hidden="true" className={extra.hitDownload}>
          <Download size={15} strokeWidth={1.75} />
        </span>
      </div>
      <div className={extra.softCardMeta}>
        <CapabilityIcons hit={hit} />
        <MetricSummary hit={hit} />
      </div>
    </a>
  );
});
