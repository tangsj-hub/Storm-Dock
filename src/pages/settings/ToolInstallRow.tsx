import { useTranslation } from "react-i18next";
import type { ToolInstallation } from "../../lib/tools";
import styles from "./page.module.css";

export function ToolInstallRow({ inst }: { inst: ToolInstallation }) {
  const { t } = useTranslation();
  return <div className={styles.installRow}>
    <span className={styles.installSource}>{inst.source}</span>
    <span className={styles.installPath} title={inst.path}>{inst.path}</span>
    <span className={inst.runnable ? styles.installVersion : styles.installBroken}>{inst.runnable ? inst.version : t("toolConflictNotRunnable")}</span>
    {inst.is_path_default ? <span className={styles.installDefault}>{t("toolConflictDefault")}</span> : null}
  </div>;
}
