import { ArrowLeft } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { RemoteModelHit } from "../../lib/types";
import { ModelDetailView } from "../model-detail/ModelDetailView";
import extra from "./page.module.css";

export function SplitPreviewPane({
  hit,
  onNotice,
  onBack,
}: {
  hit: RemoteModelHit | null;
  onNotice?: (message: string, failed?: boolean) => void;
  onBack?: () => void;
}) {
  const { t } = useTranslation();
  if (!hit) {
    return (
      <aside className={extra.splitPreview}>
        {onBack ? (
          <button className={extra.splitBack} onClick={onBack} type="button">
            <ArrowLeft aria-hidden="true" size={16} />
            {t("modelBackToList")}
          </button>
        ) : null}
        <p className={extra.splitPreviewEmpty}>{t("modelSelectToPreview")}</p>
      </aside>
    );
  }

  return (
    <aside className={extra.splitPreview} key={`${hit.source}:${hit.repo}`}>
      {onBack ? (
        <button className={extra.splitBack} onClick={onBack} type="button">
          <ArrowLeft aria-hidden="true" size={16} />
          {t("modelBackToList")}
        </button>
      ) : null}
      <ModelDetailView embedded onNotice={onNotice} repo={hit.repo} source={hit.source} />
    </aside>
  );
}
