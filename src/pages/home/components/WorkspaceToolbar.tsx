import { ChevronsDownUp, ChevronsUpDown, Download, Plus, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Tooltip } from "../../../components/Tooltip";
import type { ApplicationKind } from "../../../lib/types";
import type { WorkspaceSection } from "../types";
import styles from "../page.module.css";

type Props = {
  section: WorkspaceSection;
  busy: boolean;
  canManageAccounts: boolean;
  kind: ApplicationKind;
  hasAccounts: boolean;
  pluginsExpanded: boolean;
  refreshing: boolean;
  sessionsRefreshing: boolean;
  onExport: () => void;
  onPluginsExpandedChange: (expanded: boolean) => void;
  onRefresh: () => void;
  onSessionsRefresh: () => void;
};

export function WorkspaceToolbar(props: Props) {
  const { t } = useTranslation();
  const { section, busy, canManageAccounts, hasAccounts, kind, pluginsExpanded, refreshing, sessionsRefreshing } = props;
  return <div aria-label={t("sectionActions")} className={styles.contextToolbar} data-section={section}>
    {section === "accounts" && <>
      <Tooltip content={t("export")}><button aria-label={t("export")} className={styles.iconButton} disabled={busy || !canManageAccounts || !hasAccounts} onClick={props.onExport} type="button"><Download aria-hidden="true" size={19} /></button></Tooltip>
      <Tooltip content={t("refresh")}><button aria-label={t("refresh")} className={styles.iconButton} disabled={busy || !canManageAccounts} onClick={props.onRefresh} type="button"><RefreshCw aria-hidden="true" className={refreshing ? styles.spinning : undefined} size={19} /></button></Tooltip>
      <a aria-disabled={busy || !canManageAccounts} className={styles.addButton} href={busy || !canManageAccounts ? undefined : `/add.html?kind=${kind}`}><Plus aria-hidden="true" size={18} />{t("addAccount")}</a>
    </>}
    {section === "plugins" && <Tooltip content={pluginsExpanded ? t("collapsePluginChildren") : t("expandPluginChildren")}><button aria-expanded={pluginsExpanded} aria-label={pluginsExpanded ? t("collapsePluginChildren") : t("expandPluginChildren")} className={styles.iconButton} onClick={() => props.onPluginsExpandedChange(!pluginsExpanded)} type="button">{pluginsExpanded ? <ChevronsDownUp aria-hidden="true" size={19} /> : <ChevronsUpDown aria-hidden="true" size={19} />}</button></Tooltip>}
    {(section === "sessions" || section === "grokBot") && <Tooltip content={t("refreshSessions")}><button aria-label={t("refreshSessions")} className={styles.iconButton} disabled={sessionsRefreshing} onClick={props.onSessionsRefresh} type="button"><RefreshCw aria-hidden="true" className={sessionsRefreshing ? styles.spinning : undefined} size={19} /></button></Tooltip>}
  </div>;
}
