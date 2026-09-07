import { Download, Heart } from "lucide-react";
import { useTranslation } from "react-i18next";
import { agoParts, formatCount } from "../../lib/modelHits";
import type { RemoteModelHit } from "../../lib/types";
import extra from "./page.module.css";

export function agoLabel(value: string | undefined, t: (key: string, opts?: { count: number }) => string) {
  const ago = agoParts(value);
  if (!ago) return;
  if (ago.key === "justNow") return t("modelAgoJustNow");
  if (ago.key === "minutes") return t("modelAgoMinutes", { count: ago.count });
  if (ago.key === "hours") return t("modelAgoHours", { count: ago.count });
  return t("modelAgoDays", { count: ago.count });
}

/** Compact mid-zone secondary text (params · updated). */
export function MetricSecondary({ hit }: { hit: RemoteModelHit }) {
  const { t } = useTranslation();
  const parts: string[] = [];
  if (hit.params) parts.push(hit.params);
  const ago = agoLabel(hit.updatedAt, t);
  if (ago) parts.push(ago);
  if (parts.length === 0) return null;
  return <span className={extra.hitSecondary}>{parts.join(" · ")}</span>;
}

/** Quiet download/like pills for compact scan path. */
export function CompactStatPills({ hit }: { hit: RemoteModelHit }) {
  if (hit.downloads == null && hit.likes == null) {
    return null;
  }
  return (
    <div className={extra.hitStatPills}>
      {hit.downloads != null ? (
        <span className={extra.metaTag}>
          <Download aria-hidden="true" size={11} strokeWidth={1.75} />
          {formatCount(hit.downloads)}
        </span>
      ) : null}
      {hit.likes != null ? (
        <span className={extra.metaTag}>
          <Heart aria-hidden="true" size={11} strokeWidth={1.75} />
          {formatCount(hit.likes)}
        </span>
      ) : null}
    </div>
  );
}

/** @deprecated Spreadsheet cells — prefer CompactStatPills / MetricSecondary. */
export function MetricCells({ hit }: { hit: RemoteModelHit }) {
  return (
    <>
      <MetricSecondary hit={hit} />
      <CompactStatPills hit={hit} />
    </>
  );
}

/** Card meta: secondary text + quiet download/like pills. */
export function MetricSummary({ hit }: { hit: RemoteModelHit }) {
  const { t } = useTranslation();
  const textParts: string[] = [];
  if (hit.params) textParts.push(hit.params);
  const ago = agoLabel(hit.updatedAt, t);
  if (ago) textParts.push(ago);
  const hasChips = hit.downloads != null || hit.likes != null;
  if (textParts.length === 0 && !hasChips) return null;
  return (
    <div className={extra.cardMetrics}>
      {textParts.length > 0 ? <span className={extra.cardMetricsText}>{textParts.join(" · ")}</span> : null}
      {hit.downloads != null ? (
        <span className={extra.metaTag}>
          <Download aria-hidden="true" size={11} strokeWidth={1.75} />
          {formatCount(hit.downloads)}
        </span>
      ) : null}
      {hit.likes != null ? (
        <span className={extra.metaTag}>
          <Heart aria-hidden="true" size={11} strokeWidth={1.75} />
          {formatCount(hit.likes)}
        </span>
      ) : null}
    </div>
  );
}

/** Split master trailing stats (likes / downloads / updated). */
export function SplitTrailStats({ hit }: { hit: RemoteModelHit }) {
  const { t } = useTranslation();
  const ago = agoLabel(hit.updatedAt, t);
  if (hit.downloads == null && hit.likes == null && !ago) return null;
  return (
    <div className={extra.splitTrail}>
      <div className={extra.splitTrailStats}>
        {hit.likes != null ? (
          <span className={extra.hitStat}>
            <Heart aria-hidden="true" size={11} strokeWidth={1.75} />
            {formatCount(hit.likes)}
          </span>
        ) : null}
        {hit.downloads != null ? (
          <span className={extra.hitStat}>
            <Download aria-hidden="true" size={11} strokeWidth={1.75} />
            {formatCount(hit.downloads)}
          </span>
        ) : null}
      </div>
      {ago ? <span className={extra.splitTrailAgo}>{ago}</span> : null}
    </div>
  );
}
