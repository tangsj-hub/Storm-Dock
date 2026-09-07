import { useTranslation } from "react-i18next";
import extra from "./discover.module.css";

export function CompactColumnHeader() {
  const { t } = useTranslation();
  return (
    <div className={extra.hitHead}>
      <span>{t("modelColModel")}</span>
      <span>{t("modelColCapabilities")}</span>
      <span>{t("modelColSize")}</span>
      <span>{t("modelStatUpdated")}</span>
      <span>{t("modelStatDownloads")}</span>
      <span>{t("modelStatLikes")}</span>
      <span />
    </div>
  );
}
