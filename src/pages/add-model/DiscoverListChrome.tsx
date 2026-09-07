import { useTranslation } from "react-i18next";
import type { DiscoverListView } from "./discoverListView";
import { ViewToggle } from "./ViewToggle";
import extra from "./page.module.css";

export function DiscoverListChrome({
  view,
  onViewChange,
}: {
  view: DiscoverListView;
  onViewChange: (view: DiscoverListView) => void;
}) {
  const { t } = useTranslation();

  return (
    <div className={extra.listChrome}>
      <div className={extra.listChromeTop}>
        <h2 className={extra.listTitle}>{t("modelListHotTitle")}</h2>
        <ViewToggle onChange={onViewChange} value={view} />
      </div>
    </div>
  );
}
