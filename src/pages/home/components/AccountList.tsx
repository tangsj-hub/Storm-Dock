import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors } from "@dnd-kit/core";
import { SortableContext, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import * as Progress from "@radix-ui/react-progress";
import { Activity, ChartNoAxesCombined, Copy, FileOutput, GripVertical, KeyRound, LogIn, Pencil, RefreshCw, Trash2, UserRound, Zap } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useLocalCalendarDay } from "../../../lib/useLocalCalendarDay";
import { CurrentLaunchBadge } from "../../../components/CurrentLaunchBadge";
import { Tooltip } from "../../../components/Tooltip";
import grokBotIcon from "../../../assets/tools/grok-bot.png";
import { usagePath, canSwitchToDesktop, editAccountPath, type Account, type ApplicationKind } from "../../../lib/types";
import { accountKindKey, canLaunchGrokBot, endpointHost, grokBotUsageLabel, subscriptionLabel, subscriptionPlanBadge, usageLabel } from "../lib/accountPresentation";
import { progressForAccount } from "../lib/switchProgress";
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
  onLaunchCurrent?: (account: Account) => void;
  onLaunchBot: (account: Account) => void;
  onReorder: (activeId: string, targetId?: string) => void;
};

function SortableAccount({ account, kind, busy, testingId, onDuplicate, onExport, onRemove, onSwitch, onLaunchCurrent, onLaunchBot, onTest, progress, dayKey }: Omit<Props, "accounts" | "onReorder"> & { account: Account; dayKey: string }) {
  const { t } = useTranslation();
  void dayKey;
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ disabled: busy, id: account.id });
  const subscription = subscriptionLabel(account, t);
  const planBadge = kind === "grok" ? subscriptionPlanBadge(account, t) : undefined;
  const usage = usageLabel(account, t);
  const grokBotUsage = grokBotUsageLabel(account, t);
  const host = kind !== "cursor" ? endpointHost(account.baseUrl) : undefined;
  const isApiKey = account.importType === "api_key";
  const canLaunchBot = kind === "cursor" && canLaunchGrokBot(account);
  const isActive = account.isCurrent;
  return <article className={`${styles.accountCard} ${isActive ? styles.current : ""} ${isDragging ? styles.dragging : ""}`} ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition }}>
    <GripVertical aria-label={t("drag", { account: account.label })} className={styles.dragHandle} size={24} {...attributes} {...listeners} />
    <div className={styles.accountCopy}><strong>{account.label}</strong><div className={styles.accountMeta}>
      {kind === "grok" && planBadge ? <span className={`${styles.kindBadge} ${styles[`plan-${planBadge.plan}`] ?? styles.kindGrok}`}>{planBadge.name}</span> : kind !== "cursor" ? <span className={`${styles.kindBadge} ${isApiKey ? styles.kindApiKey : styles.kindAccount}`}>{isApiKey ? <KeyRound aria-hidden="true" size={11} /> : <UserRound aria-hidden="true" size={11} />}{t(accountKindKey(account))}</span> : null}
      {kind === "grok" && subscription ? (subscription.expiryTitle ? <Tooltip content={subscription.expiryTitle}><span className={styles.metaBadge} title={subscription.expiryTitle}>{subscription.expiry}</span></Tooltip> : <span className={styles.metaBadge}>{subscription.expiry}</span>) : null}
      {kind !== "grok" && !isApiKey && subscription && (subscription.expiryTitle ? <Tooltip content={subscription.expiryFull ?? subscription.expiryTitle}><span className={`${styles.metaBadge} ${styles[`plan-${subscription.plan}`] ?? styles.planDefault}`} title={subscription.expiryTitle}>{subscription.name} · {subscription.expiry}</span></Tooltip> : <span className={`${styles.metaBadge} ${styles[`plan-${subscription.plan}`] ?? styles.planDefault}`}>{subscription.name} · {subscription.expiry}</span>)}
      {usage && (usage.title ? <Tooltip content={usage.title}><span className={styles.metaBadge} title={usage.title}>{usage.text}</span></Tooltip> : <span className={styles.metaBadge}>{usage.text}</span>)}
      {grokBotUsage && (grokBotUsage.title ? <Tooltip content={grokBotUsage.title}><span className={styles.metaBadge} title={grokBotUsage.title}>{grokBotUsage.text}</span></Tooltip> : <span className={styles.metaBadge}>{grokBotUsage.text}</span>)}
      {host && <span className={styles.metaBadge}>{host}</span>}
      {account.status === "blocked" && <span className={styles.blockedBadge}>{t("tokenBlocked", { defaultValue: "账号已封禁" })}</span>}
      {account.status === "invalid" && <span className={styles.invalidBadge}>{t("tokenInvalid", { defaultValue: "Token已失效" })}</span>}
      {account.status === "missing" && <span className={styles.missingBadge}>{t("credentialMissing", { defaultValue: "凭证缺失" })}</span>}
    </div></div>
    <div className={styles.accountActions}>
      {progress ? <div className={styles.progress}><span>{t(`switchStages.${progress.stage}`)}</span><Progress.Root aria-label={t("switchProgress")} className={styles.progressRoot} value={progress.percent}><Progress.Indicator className={progress.status === "error" ? styles.progressError : styles.progressIndicator} style={{ transform: `translateX(-${100 - progress.percent}%)` }} /></Progress.Root></div> : isActive ? <CurrentLaunchBadge busy={busy} onLaunch={(kind === "cursor" || kind === "codex") && onLaunchCurrent ? () => onLaunchCurrent(account) : undefined} /> : canSwitchToDesktop(account) ? <button className={styles.activate} disabled={busy} onClick={() => onSwitch(account)} type="button"><LogIn aria-hidden="true" size={17} />{t("switch")}</button> : null}
      {progress?.status === "error" && canSwitchToDesktop(account) && <button className={styles.activate} onClick={() => onSwitch(account)} type="button"><RefreshCw aria-hidden="true" size={16} />{t("retry")}</button>}
      {canLaunchBot && <Tooltip content={account.isGrokBotCurrent ? t("grokBotCurrent") : t("launchGrokBot")}><button aria-label={account.isGrokBotCurrent ? t("grokBotCurrent") : t("launchGrokBot")} className={`${styles.iconButton} ${styles.grokBotButton}`} disabled={busy} onClick={() => onLaunchBot(account)} type="button"><img alt="" aria-hidden="true" className={styles.grokBotIcon} src={grokBotIcon} />{account.isGrokBotCurrent && <span className={styles.grokBotCurrentBadge} aria-hidden="true"><Zap size={9} strokeWidth={2.6} /></span>}</button></Tooltip>}
      {isApiKey ? <>
        <Tooltip content={t("edit")}><a aria-disabled={busy || undefined} aria-label={t("editAccount", { account: account.label })} className={styles.iconButton} href={busy ? undefined : editAccountPath(kind, account.id)}><Pencil aria-hidden="true" size={16} /></a></Tooltip>
        <Tooltip content={t("duplicate")}><button aria-label={t("duplicate")} className={styles.iconButton} disabled={busy} onClick={() => onDuplicate(account)} type="button"><Copy aria-hidden="true" size={16} /></button></Tooltip>
        <Tooltip content={t("testConnection")}><button aria-label={t("testConnection")} className={styles.iconButton} disabled={busy || testingId === account.id} onClick={() => onTest(account)} type="button"><Activity aria-hidden="true" className={testingId === account.id ? styles.spinning : undefined} size={16} /></button></Tooltip>
      </> : (kind === "cursor" || kind === "grok") ? <Tooltip content={t("usage")}><a aria-label={t("viewUsage", { account: account.label })} className={styles.iconButton} href={usagePath(account.id, kind === "grok" ? "grok" : "cursor")}><ChartNoAxesCombined aria-hidden="true" size={18} /></a></Tooltip> : null}
      {!isApiKey && <Tooltip content={t("export")}><button aria-label={t("exportAccount", { account: account.label })} className={styles.iconButton} disabled={busy} onClick={() => onExport(account)} type="button"><FileOutput aria-hidden="true" size={18} /></button></Tooltip>}
      <Tooltip content={t("delete")}><button aria-label={t("remove", { account: account.label })} className={styles.iconButton} disabled={busy} onClick={() => onRemove(account)} type="button"><Trash2 aria-hidden="true" size={19} /></button></Tooltip>
    </div>
  </article>;
}

export function AccountList({ accounts, onReorder, progress, ...props }: Props) {
  const dayKey = useLocalCalendarDay();
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }), useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }));
  return <DndContext collisionDetection={closestCenter} onDragEnd={({ active, over }) => onReorder(String(active.id), over ? String(over.id) : undefined)} sensors={sensors}><SortableContext items={accounts.map((account) => account.id)} strategy={verticalListSortingStrategy}><div className={styles.accountList}>{accounts.map((account) => <SortableAccount dayKey={dayKey} {...props} account={account} key={account.id} progress={progressForAccount(progress, account.id)} />)}</div></SortableContext></DndContext>;
}
