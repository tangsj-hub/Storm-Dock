import { useTranslation } from "react-i18next";
import extra from "../add-model/page.module.css";

/** Layout-matching skeleton for model detail / split preview. */
export function DetailSkeleton({ readmeOnly = false }: { readmeOnly?: boolean }) {
  const { t } = useTranslation();

  const readme = (
    <div className={extra.detailSkeletonReadme}>
      {!readmeOnly ? <span className={extra.introLabel}>{t("modelReadme")}</span> : null}
      <div aria-hidden="true" className={extra.detailSkeletonReadmeBody}>
        <div className={`${extra.skeletonBlock} ${extra.detailSkeletonLineTitle}`} />
        <div className={`${extra.skeletonBlock} ${extra.detailSkeletonLine} ${extra.detailSkeletonLineWide}`} />
        <div className={`${extra.skeletonBlock} ${extra.detailSkeletonLine} ${extra.detailSkeletonLineMid}`} />
        <div className={`${extra.skeletonBlock} ${extra.detailSkeletonLine} ${extra.detailSkeletonLineShort}`} />
        <div className={`${extra.skeletonBlock} ${extra.detailSkeletonLine} ${extra.detailSkeletonLineWide}`} />
        <div className={`${extra.skeletonBlock} ${extra.detailSkeletonLine} ${extra.detailSkeletonLineMid}`} />
        <div className={`${extra.skeletonBlock} ${extra.detailSkeletonLine} ${extra.detailSkeletonLineShort}`} />
      </div>
    </div>
  );

  if (readmeOnly) {
    return (
      <div aria-busy="true" aria-label={t("modelReadmeLoading")} className={extra.detailSkeleton} role="status">
        {readme}
      </div>
    );
  }

  return (
    <div aria-busy="true" aria-label={t("loading")} className={extra.detailSkeleton} role="status">
      <div className={extra.detailSkeletonMeta}>
        <div className={extra.detailSkeletonHead}>
          <div className={`${extra.skeletonBlock} ${extra.detailSkeletonAvatar}`} />
          <div className={extra.skeletonLines} style={{ flex: 1 }}>
            <div className={`${extra.skeletonBlock} ${extra.skeletonLine}`} />
            <div className={`${extra.skeletonBlock} ${extra.skeletonLineShort}`} />
          </div>
        </div>
        <div className={`${extra.skeletonBlock} ${extra.detailSkeletonDownload}`} />
        <div className={extra.detailSkeletonStats}>
          <div className={`${extra.skeletonBlock} ${extra.detailSkeletonStat}`} />
          <div className={`${extra.skeletonBlock} ${extra.detailSkeletonStat}`} />
          <div className={`${extra.skeletonBlock} ${extra.detailSkeletonStat}`} />
        </div>
      </div>
      {readme}
    </div>
  );
}
