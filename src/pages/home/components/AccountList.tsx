import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors } from "@dnd-kit/core";
import { SortableContext, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import * as Progress from "@radix-ui/react-progress";
import { Activity, Check, ChartNoAxesCombined, Copy, FileOutput, GripVertical, KeyRound, LogIn, Pencil, RefreshCw, Trash2, UserRound } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Tooltip } from "../../../components/Tooltip";
import { canSwitchToDesktop, editAccountPath, type Account, type ApplicationKind } from "../../../lib/types";
import { accountKindKey, endpointHost, subscriptionLabel, usageLabel } from "../lib/accountPresentation";
import type { SwitchProgress } from "../types";
import styles from "../page.module.css";

type Props = {
  accounts: Account[];
  kind: ApplicationKind;
  busy: boolean;
  progress?: SwitchProgress;
  onExport: (account: Account) => void;
  onDuplicate: (account: Account) => void;
  onTest: (account: Account) => void;
  testingId?: string;
  onRemove: (account: Account) => void;
  onSwitch: (account: Account) => void;
  onReorder: (activeId: string, targetId?: string) => void;
};

function SortableAccount({ account, kind, busy, testingId, onDuplicate, onExport, onRemove, onSwitch, onTest, progress }: Omit<Props, "accounts" | "onReorder"> & { account: Account }) {
  const { t } = useTranslation();
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ disabled: busy, id: account.id });
  const subscription = subscriptionLabel(account, t);
  const usage = usageLabel(account, t);
  const host = kind !== "cursor" ? endpointHost(account.baseUrl) : undefined;
  const isApiKey = account.importType === "api_key";
  return <article className={`${styles.accountCard} ${account.isCurrent ? styles.current : ""} ${isDragging ? styles.dragging : ""}`} ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition }}>
    <GripVertical aria-label={t("drag", { account: account.label })} className={styles.dragHandle} size={24} {...attributes} {...listeners} />
    <div className={styles.accountCopy}><strong>{account.label}</strong><div className={styles.accountMeta}>
      {kind !== "cursor" && <span className={`${styles.kindBadge} ${isApiKey ? styles.kindApiKey : styles.kindAccount}`}>{isApiKey ? <KeyRound aria-hidden="true" size={11} /> : <UserRound aria-hidden="true" size={11} />}{t(accountKindKey(account))}</span>}
      {subscription && <span className={`${styles.metaBadge} ${styles[`plan-${subscription.plan}`] ?? styles.planDefault}`}>{subscription.name} · {subscription.expiry}</span>}
      {usage && <span className={styles.metaBadge}>{usage}</span>}
      {host && <span className={styles.metaBadge}>{host}</span>}
      {account.status === "invalid" && <span className={styles.invalidBadge}>{t("tokenInvalid", { defaultValue: "Token已失效" })}</span>}
      {account.status === "missing" && <span className={styles.missingBadge}>{t("credentialMissing", { defaultValue: "凭证缺失" })}</span>}
    </div></div>
    <div className={styles.accountActions}>
      {progress ? <div className={styles.progress}><span>{t(`switchStages.${progress.stage}`)}</span><Progress.Root aria-label={t("switchProgress")} className={styles.progressRoot} value={progress.percent}><Progress.Indicator className={progress.status === "error" ? styles.progressError : styles.progressIndicator} style={{ transform: `translateX(-${100 - progress.percent}%)` }} /></Progress.Root></div> : account.isCurrent ? <span className={styles.currentBadge}><Check aria-hidden="true" size={16} />{t("current")}</span> : canSwitchToDesktop(account) ? <button className={styles.activate} disabled={busy} onClick={() => onSwitch(account)} type="button"><LogIn aria-hidden="true" size={17} />{t("switch")}</button> : null}
      {progress?.status === "error" && canSwitchToDesktop(account) && <button className={styles.activate} onClick={() => onSwitch(account)} type="button"><RefreshCw aria-hidden="true" size={16} />{t("retry")}</button>}
      {isApiKey ? <>
        <Tooltip content={t("edit")}><a aria-disabled={busy || undefined} aria-label={t("editAccount", { account: account.label })} className={styles.iconButton} href={busy ? undefined : editAccountPath(kind, account.id)}><Pencil aria-hidden="true" size={16} /></a></Tooltip>
        <Tooltip content={t("duplicate")}><button aria-label={t("duplicate")} className={styles.iconButton} disabled={busy} onClick={() => onDuplicate(account)} type="button"><Copy aria-hidden="true" size={16} /></button></Tooltip>
        <Tooltip content={t("testConnection")}><button aria-label={t("testConnection")} className={styles.iconButton} disabled={busy || testingId === account.id} onClick={() => onTest(account)} type="button"><Activity aria-hidden="true" className={testingId === account.id ? styles.spinning : undefined} size={16} /></button></Tooltip>
      </> : kind === "cursor" ? <Tooltip content={t("usage")}><a aria-label={t("viewUsage", { account: account.label })} className={styles.iconButton} href={`/usage.html?accountId=${encodeURIComponent(account.id)}&kind=${kind}`}><ChartNoAxesCombined aria-hidden="true" size={18} /></a></Tooltip> : null}
      {!isApiKey && <Tooltip content={t("export")}><button aria-label={t("exportAccount", { account: account.label })} className={styles.iconButton} disabled={busy} onClick={() => onExport(account)} type="button"><FileOutput aria-hidden="true" size={18} /></button></Tooltip>}
      <Tooltip content={t("delete")}><button aria-label={t("remove", { account: account.label })} className={styles.iconButton} disabled={busy} onClick={() => onRemove(account)} type="button"><Trash2 aria-hidden="true" size={19} /></button></Tooltip>
    </div>
  </article>;
}

export function AccountList({ accounts, onReorder, ...props }: Props) {
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }), useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }));
  return <DndContext collisionDetection={closestCenter} onDragEnd={({ active, over }) => onReorder(String(active.id), over ? String(over.id) : undefined)} sensors={sensors}><SortableContext items={accounts.map((account) => account.id)} strategy={verticalListSortingStrategy}><div className={styles.accountList}>{accounts.map((account) => <SortableAccount account={account} key={account.id} progress={props.progress?.accountId === account.id ? props.progress : undefined} {...props} />)}</div></SortableContext></DndContext>;
}
