import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { useTranslation } from "react-i18next";
import type { ToolInstallationReport } from "../../lib/tools";
import { ToolInstallRow } from "./ToolInstallRow";
import styles from "./page.module.css";

export function ToolUpgradeConfirmDialog({
  open,
  plans,
  displayName,
  onConfirm,
  onCancel
}: {
  open: boolean;
  plans: ToolInstallationReport[];
  displayName: (tool: string) => string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  return <AlertDialog.Root onOpenChange={(next) => { if (!next) onCancel(); }} open={open}>
    <AlertDialog.Portal>
      <AlertDialog.Overlay className={styles.dialogOverlay} />
      <AlertDialog.Content className={styles.dialogContent}>
        <AlertDialog.Title>{t("toolUpgradeConfirmTitle")}</AlertDialog.Title>
        <AlertDialog.Description>{t("toolUpgradeConfirmHint")}</AlertDialog.Description>
        <div className={styles.dialogPlans}>
          {plans.map((plan) => <div className={styles.conflict} key={plan.tool}>
            <div className={styles.conflictTitle}>{displayName(plan.tool)}</div>
            {!plan.anchored ? <p className={styles.conflictHint}>{t("toolUpgradeUnanchoredHint")}</p> : null}
            <ul className={styles.installList}>{plan.installs.map((inst) => <li key={inst.path}><ToolInstallRow inst={inst} /></li>)}</ul>
            <p className={styles.conflictHint}>{t("toolUpgradeWillRun")}</p>
            <code className={styles.commandPreview}>{plan.command}</code>
          </div>)}
        </div>
        <div className={styles.dialogActions}>
          <AlertDialog.Cancel asChild><button className={styles.dialogCancel} type="button">{t("cancel")}</button></AlertDialog.Cancel>
          <AlertDialog.Action asChild><button autoFocus className={styles.databaseButton} onClick={onConfirm} type="button">{t("toolUpgradeConfirmBtn")}</button></AlertDialog.Action>
        </div>
      </AlertDialog.Content>
    </AlertDialog.Portal>
  </AlertDialog.Root>;
}
