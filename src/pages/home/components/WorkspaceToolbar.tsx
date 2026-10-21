import { ChevronsDownUp, ChevronsUpDown, Download, Import, Plus, RefreshCw } from "lucide-react";
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
  showAddAccount?: boolean;
  sessionsRefreshLabel?: string;
  onExport: () => void;
  onPluginsExpandedChange: (expanded: boolean) => void;
  onRefresh: () => void;
  onSessionsRefresh: () => void;
  onAddBotTemplate?: () => void;
  onImportAllBotTemplates?: () => void;
  onExportBotTemplates?: () => void;
};

export function WorkspaceToolbar(props: Props) {
  const { t } = useTranslation();
  const {
    section,
    busy,
    canManageAccounts,
    hasAccounts,
    kind,
    pluginsExpanded,
    refreshing,
    sessionsRefreshing,
    showAddAccount = true,
    sessionsRefreshLabel,
  } = props;
  const sessionsLabel = sessionsRefreshLabel ?? t("refreshSessions");
  return <div aria-label={t("sectionActions")} className={styles.contextToolbar} data-section={section}>
    {section === "accounts" && <>
      {!showAddAccount ? (
        <Tooltip content={t("refreshGrokBotAllUsage")}><button aria-label={t("refreshGrokBotAllUsage")} className={styles.iconButton} disabled={busy || refreshing} onClick={props.onRefresh} type="button"><RefreshCw aria-hidden="true" className={refreshing ? styles.spinning : undefined} size={19} /></button></Tooltip>
      ) : <>
        <Tooltip content={t("export")}><button aria-label={t("export")} className={styles.iconButton} disabled={busy || !canManageAccounts || !hasAccounts} onClick={props.onExport} type="button"><Download aria-hidden="true" size={19} /></button></Tooltip>
        <Tooltip content={t("refresh")}><button aria-label={t("refresh")} className={styles.iconButton} disabled={busy || !canManageAccounts} onClick={props.onRefresh} type="button"><RefreshCw aria-hidden="true" className={refreshing ? styles.spinning : undefined} size={19} /></button></Tooltip>
        <a aria-disabled={busy || !canManageAccounts} className={styles.addButton} href={busy || !canManageAccounts ? undefined : `/add.html?kind=${kind}`}><Plus aria-hidden="true" size={18} />{t("addAccount")}</a>
      </>}
    </>}
    {section === "plugins" && <Tooltip content={pluginsExpanded ? t("collapsePluginChildren") : t("expandPluginChildren")}><button aria-expanded={pluginsExpanded} aria-label={pluginsExpanded ? t("collapsePluginChildren") : t("expandPluginChildren")} className={styles.iconButton} onClick={() => props.onPluginsExpandedChange(!pluginsExpanded)} type="button">{pluginsExpanded ? <ChevronsDownUp aria-hidden="true" size={19} /> : <ChevronsUpDown aria-hidden="true" size={19} />}</button></Tooltip>}
    {section === "botTemplates" && <Tooltip content={t("botImportAll")}><button aria-label={t("botImportAll")} className={styles.iconButton} onClick={props.onImportAllBotTemplates} type="button"><Import aria-hidden="true" size={19} /></button></Tooltip>}
    {section === "botTemplates" && <Tooltip content={t("botTemplateExport")}><button aria-label={t("botTemplateExport")} className={styles.iconButton} onClick={props.onExportBotTemplates} type="button"><Download aria-hidden="true" size={19} /></button></Tooltip>}
    {section === "botTemplates" && <button className={styles.addButton} onClick={props.onAddBotTemplate} type="button"><Plus aria-hidden="true" size={18} />{t("botTemplateAddTitle")}</button>}
    {section === "sessions" && <Tooltip content={sessionsLabel}><button aria-label={sessionsLabel} className={styles.iconButton} disabled={sessionsRefreshing} onClick={props.onSessionsRefresh} type="button"><RefreshCw aria-hidden="true" className={sessionsRefreshing ? styles.spinning : undefined} size={19} /></button></Tooltip>}
  </div>;
}
