import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { useTranslation } from "react-i18next";
import type { ToolInstallationReport } from "../../lib/tools";
import { ToolInstallRow } from "./ToolInstallRow";
import styles from "./page.module.css";

export function ToolUninstallConfirmDialog({
  displayName,
  onCancel,
  onConfirm,
  open,
  report
}: {
  displayName: (tool: string) => string;
  onCancel: () => void;
  onConfirm: () => void;
  open: boolean;
  report: ToolInstallationReport | null;
}) {
  const { t } = useTranslation();
  const defaultInstall = report?.installs.find((inst) => inst.is_path_default) ?? report?.installs[0];
  return <AlertDialog.Root onOpenChange={(next) => { if (!next) onCancel(); }} open={open}>
    <AlertDialog.Portal>
      <AlertDialog.Overlay className={styles.dialogOverlay} />
      <AlertDialog.Content className={styles.dialogContent}>
        <AlertDialog.Title>{t("toolUninstallConfirmTitle")}</AlertDialog.Title>
        <AlertDialog.Description>{t("toolUninstallConfirmHint", { name: report ? displayName(report.tool) : "" })}</AlertDialog.Description>
        {report ? <div className={styles.dialogPlans}>
          <div className={styles.conflict}>
            {defaultInstall ? <ul className={styles.installList}><li><ToolInstallRow inst={defaultInstall} /></li></ul> : null}
            <p className={styles.conflictHint}>{t("toolUpgradeWillRun")}</p>
            <code className={styles.commandPreview}>{report.uninstall_command}</code>
          </div>
        </div> : null}
        <div className={styles.dialogActions}>
          <AlertDialog.Cancel asChild><button className={styles.dialogCancel} type="button">{t("cancel")}</button></AlertDialog.Cancel>
          <AlertDialog.Action asChild><button autoFocus className={styles.databaseButton} onClick={onConfirm} type="button">{t("toolUninstallConfirmBtn")}</button></AlertDialog.Action>
        </div>
      </AlertDialog.Content>
    </AlertDialog.Portal>
  </AlertDialog.Root>;
}
