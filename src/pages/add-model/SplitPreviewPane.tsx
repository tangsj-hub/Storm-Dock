import { Download, Heart } from "lucide-react";
import { useTranslation } from "react-i18next";
import { formatCount } from "../../lib/modelHits";
import { modelDetailPath, type RemoteModelHit } from "../../lib/types";
import { CapabilityIcons } from "./CapabilityIcons";
import { agoLabel } from "./MetricCells";
import type { DiscoverHitContext } from "./ModelCompactRow";
import { ModelIdentity } from "./ModelIdentity";
import extra from "./page.module.css";
import styles from "../add/page.module.css";

export function SplitPreviewPane({
  hit,
  context,
}: {
  hit: RemoteModelHit | null;
  context: DiscoverHitContext;
}) {
  const { t } = useTranslation();
  if (!hit) {
    return (
      <aside className={extra.splitPreview}>
        <p className={extra.splitPreviewEmpty}>{t("modelSelectToPreview")}</p>
      </aside>
    );
  }

  const detailHref = modelDetailPath(hit.source, hit.repo, context);
  const updated = agoLabel(hit.updatedAt, t);

  return (
    <aside className={extra.splitPreview}>
      <ModelIdentity hit={hit} variant="card" />
      <CapabilityIcons hit={hit} />
      <dl className={extra.splitPreviewStats}>
        {hit.params ? (
          <div>
            <dt>{t("modelColSize")}</dt>
            <dd>{hit.params}</dd>
          </div>
        ) : null}
        {updated ? (
          <div>
            <dt>{t("modelStatUpdated")}</dt>
            <dd>{updated}</dd>
          </div>
        ) : null}
        {hit.downloads != null ? (
          <div>
            <dt>{t("modelStatDownloads")}</dt>
            <dd><Download aria-hidden="true" size={13} />{formatCount(hit.downloads)}</dd>
          </div>
        ) : null}
        {hit.likes != null ? (
          <div>
            <dt>{t("modelStatLikes")}</dt>
            <dd><Heart aria-hidden="true" size={13} />{formatCount(hit.likes)}</dd>
          </div>
        ) : null}
        {hit.library ? (
          <div>
            <dt>{t("modelStatLibrary")}</dt>
            <dd>{hit.library}</dd>
          </div>
        ) : null}
        {hit.pipeline ? (
          <div>
            <dt>{t("modelColCapabilities")}</dt>
            <dd>{hit.pipeline}</dd>
          </div>
        ) : null}
      </dl>
      <a className={styles.primary} href={detailHref}>{t("modelOpenDetail")}</a>
    </aside>
  );
}
