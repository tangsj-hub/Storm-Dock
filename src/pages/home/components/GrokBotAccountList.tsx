import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors } from "@dnd-kit/core";
import { SortableContext, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { ChartNoAxesCombined, FileOutput, GripVertical, Trash2, Zap } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useLocalCalendarDay } from "../../../lib/useLocalCalendarDay";
import { CurrentLaunchBadge } from "../../../components/CurrentLaunchBadge";
import { Tooltip } from "../../../components/Tooltip";
import grokBotIcon from "../../../assets/tools/grok-bot.png";
import { usagePath, type Account } from "../../../lib/types";
import {
  canLaunchGrokBot,
  grokBotSourceKey,
  grokBotUsageLabel,
  isGrokBotFreePlan,
  isGrokBotListEligible,
  subscriptionLabel,
  subscriptionPlanBadge,
} from "../lib/accountPresentation";
import styles from "../page.module.css";

type Props = {
  accounts: Account[];
  busy: boolean;
  onExport: (account: Account) => void;
  onLaunchBot: (account: Account) => void;
  onRemove: (account: Account) => void;
  onReorder: (activeId: string, targetId?: string) => void;
};

function SortableAccount({
  account,
  busy,
  onExport,
  onLaunchBot,
  onRemove,
  dayKey,
}: Omit<Props, "accounts" | "onReorder"> & { account: Account; dayKey: string }) {
  const { t } = useTranslation();
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    disabled: busy,
    id: account.id,
  });
  const subscription = subscriptionLabel(account, t);
  const planBadge = subscriptionPlanBadge(account, t);
  void dayKey;
  const grokBotUsage = grokBotUsageLabel(account, t);
  const isActive = Boolean(account.isGrokBotCurrent);
  const launchable = canLaunchGrokBot(account);
  const freePlan = isGrokBotFreePlan(account);
  const sourceKey = grokBotSourceKey(account);
  const sourceClass = sourceKey === "grok" ? styles.kindGrok : styles.kindAccount;
  const usageKind = account.application === "grok" ? "grok" : "cursor";

  return (
    <article
      className={`${styles.accountCard} ${isActive ? styles.current : ""} ${isDragging ? styles.dragging : ""}`}
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
    >
      <GripVertical
        aria-label={t("drag", { account: account.label })}
        className={styles.dragHandle}
        size={24}
        {...attributes}
        {...listeners}
      />
      <div className={styles.accountCopy}>
        <strong>{account.label}</strong>
        <div className={styles.accountMeta}>
          <span className={`${styles.kindBadge} ${sourceClass}`}>
            {t(sourceKey === "grok" ? "grokBotSourceGrok" : "grokBotSourceCursor")}
          </span>
          {subscription?.expiryTitle ? (
            <Tooltip content={subscription.expiryFull ?? subscription.expiryTitle}>
              <span className={`${styles.metaBadge} ${styles[`plan-${planBadge.plan}`] ?? styles.planDefault}`} title={subscription.expiryTitle}>
                {planBadge.name}
                {subscription ? ` · ${subscription.expiry}` : ""}
              </span>
            </Tooltip>
          ) : (
            <span className={`${styles.metaBadge} ${styles[`plan-${planBadge.plan}`] ?? styles.planDefault}`}>
              {planBadge.name}
              {subscription ? ` · ${subscription.expiry}` : ""}
            </span>
          )}
          {grokBotUsage && (grokBotUsage.title ? (
            <Tooltip content={grokBotUsage.title}>
              <span className={styles.metaBadge} title={grokBotUsage.title}>{grokBotUsage.text}</span>
            </Tooltip>
          ) : (
            <span className={styles.metaBadge}>{grokBotUsage.text}</span>
          ))}
          {account.status === "blocked" && (
            <span className={styles.blockedBadge}>{t("tokenBlocked", { defaultValue: "账号已封禁" })}</span>
          )}
          {account.status === "invalid" && (
            <span className={styles.invalidBadge}>{t("tokenInvalid", { defaultValue: "Token已失效" })}</span>
          )}
          {account.status === "missing" && (
            <span className={styles.missingBadge}>{t("credentialMissing", { defaultValue: "凭证缺失" })}</span>
          )}
        </div>
      </div>
      <div className={styles.accountActions}>
        {isActive ? (
          <CurrentLaunchBadge busy={busy} onLaunch={() => onLaunchBot(account)} />
        ) : null}
        <Tooltip content={launchable ? (isActive ? t("grokBotCurrent") : t("launchGrokBot")) : t("grokBotFreeNotLaunchable")}>
          <button
            aria-label={launchable ? (isActive ? t("grokBotCurrent") : t("launchGrokBot")) : t("grokBotFreeNotLaunchable")}
            className={`${styles.iconButton} ${styles.grokBotButton}`}
            disabled={busy || !launchable}
            onClick={() => onLaunchBot(account)}
            type="button"
          >
            <img alt="" aria-hidden="true" className={styles.grokBotIcon} src={grokBotIcon} />
            {isActive && (
              <span aria-hidden="true" className={styles.grokBotCurrentBadge}>
                <Zap size={9} strokeWidth={2.6} />
              </span>
            )}
          </button>
        </Tooltip>
        <Tooltip content={t("usage")}>
          <a
            aria-label={t("viewUsage", { account: account.label })}
            className={styles.iconButton}
            href={usagePath(account.id, usageKind, "grokBot")}
          >
            <ChartNoAxesCombined aria-hidden="true" size={18} />
          </a>
        </Tooltip>
        <Tooltip content={t("exportGrokBot")}>
          <button
            aria-label={t("exportGrokBot")}
            className={styles.iconButton}
            disabled={busy || !launchable}
            onClick={() => onExport(account)}
            type="button"
          >
            <FileOutput aria-hidden="true" size={18} />
          </button>
        </Tooltip>
        <Tooltip content={t("delete")}>
          <button
            aria-label={t("remove", { account: account.label })}
            className={styles.iconButton}
            disabled={busy}
            onClick={() => onRemove(account)}
            type="button"
          >
            <Trash2 aria-hidden="true" size={19} />
          </button>
        </Tooltip>
      </div>
    </article>
  );
}

export function GrokBotAccountList({ accounts, onReorder, ...props }: Props) {
  const dayKey = useLocalCalendarDay();
  const { t } = useTranslation();
  // Backend already returns Cursor (non-free) + all Grok Build (incl. free).
  // Hide unknown subscription, free plan, expired token, and banned accounts.
  const botAccounts = accounts.filter(isGrokBotListEligible);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  if (botAccounts.length === 0) {
    return (
      <div className={styles.empty}>
        <img alt="" className={styles.grokBotStatusIcon} src={grokBotIcon} />
        <h2>{t("grokBotNoEligibleAccounts")}</h2>
      </div>
    );
  }

  return (
    <DndContext
      collisionDetection={closestCenter}
      onDragEnd={({ active, over }) => onReorder(String(active.id), over ? String(over.id) : undefined)}
      sensors={sensors}
    >
      <SortableContext items={botAccounts.map((account) => account.id)} strategy={verticalListSortingStrategy}>
        <div className={styles.accountList}>
          {botAccounts.map((account) => (
            <SortableAccount dayKey={dayKey} {...props} account={account} key={account.id} />
          ))}
        </div>
      </SortableContext>
    </DndContext>
  );
}
