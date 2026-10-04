import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, CircleAlert, ExternalLink, LoaderCircle, Play, RefreshCw, Zap } from "lucide-react";
import { useMemo, useState } from "react";
import { useLocalCalendarDay } from "../../../lib/useLocalCalendarDay";
import { useTranslation } from "react-i18next";
import grokBotIcon from "../../../assets/tools/grok-bot.png";
import { openExternalUrl } from "../../../lib/api";
import type { Account, GrokBotStatus } from "../../../lib/types";
import { canLaunchGrokBot, grokBotUsageLabel, isGrokBotListEligible } from "../lib/accountPresentation";
import styles from "../page.module.css";

const GROK_BOT_WEBSITE = "https://cursor.com/download/bot";

export function GrokBotStatusCard({
  status,
  loading,
  accounts,
  busy,
  onSwitchAccount,
  onRefresh,
  refreshing,
  onError,
}: {
  status?: GrokBotStatus;
  loading?: boolean;
  accounts: Account[];
  busy?: boolean;
  onSwitchAccount: (account: Account) => void;
  onRefresh?: () => void;
  refreshing?: boolean;
  onError?: (error: unknown) => void;
}) {
  const { t } = useTranslation();
  const dayKey = useLocalCalendarDay();
  void dayKey;
  const [websiteOpen, setWebsiteOpen] = useState(false);
  const [openingWebsite, setOpeningWebsite] = useState(false);
  // Same predicate as GrokBotAccountList so the switcher and the cards stay the same set.
  const botAccounts = useMemo(
    () => accounts.filter(isGrokBotListEligible),
    [accounts],
  );
  const currentAccount = useMemo(() => {
    if (status?.currentAccountId) {
      return (
        botAccounts.find((account) => account.id === status.currentAccountId) ??
        accounts.find((account) => account.id === status.currentAccountId)
      );
    }
    return (
      botAccounts.find((account) => account.isGrokBotCurrent) ??
      accounts.find((account) => account.isGrokBotCurrent)
    );
  }, [accounts, botAccounts, status?.currentAccountId]);
  const usage = currentAccount ? grokBotUsageLabel(currentAccount, t) : undefined;
  const usageText = usage?.text;
  const usageTitle = usage?.title;
  const installed = Boolean(status?.installed);
  const canLaunchCurrent = Boolean(
    currentAccount && canLaunchGrokBot(currentAccount) && isGrokBotListEligible(currentAccount),
  );

  const confirmOpenWebsite = async () => {
    setOpeningWebsite(true);
    try {
      await openExternalUrl(GROK_BOT_WEBSITE);
      setWebsiteOpen(false);
    } catch (error) {
      onError?.(error);
    } finally {
      setOpeningWebsite(false);
    }
  };

  return (
    <>
      <article className={styles.grokBotStatus}>
        <img alt="" className={styles.grokBotStatusIcon} src={grokBotIcon} />
        <div className={styles.grokBotStatusCopy}>
          <div className={styles.grokBotStatusTitleRow}>
            <strong>{t("grokBotClientLabel")}</strong>
            <button
              aria-label={t("grokBotOpenWebsite")}
              className={styles.grokBotTitleLinkButton}
              onClick={() => setWebsiteOpen(true)}
              type="button"
            >
              <ExternalLink aria-hidden="true" size={14} />
            </button>
          </div>
          <div className={styles.grokBotStatusMeta}>
            {loading && !status ? (
              <span className={styles.metaBadge}>
                <LoaderCircle aria-hidden="true" className={styles.spinning} size={11} />
                {t("grokBotStatusLoading")}
              </span>
            ) : status ? (
              <>
                <span className={`${styles.metaBadge} ${installed ? styles.grokBotReadyBadge : styles.grokBotUnavailableBadge}`}>
                  {installed ? <Check aria-hidden="true" size={11} /> : <CircleAlert aria-hidden="true" size={11} />}
                  {installed ? t("grokBotClientInstalled") : t("grokBotClientNotInstalled")}
                </span>
                {(currentAccount?.label || status.currentAccountLabel) && (
                  <span className={`${styles.metaBadge} ${styles.grokBotCurrentAccountBadge}`}>
                    <Zap aria-hidden="true" size={11} strokeWidth={2.4} />
                    {currentAccount?.label ?? status.currentAccountLabel}
                  </span>
                )}
                <span className={`${styles.metaBadge} ${styles.grokBotUsageBadge}`} title={usageTitle}>
                  {usageText ?? t("grokBotUsageUnknown")}
                </span>
              </>
            ) : (
              <span className={styles.metaBadge}>{t("grokBotUnavailable")}</span>
            )}
          </div>
        </div>
        <div className={styles.grokBotStatusActions}>
          <button
            aria-label={t("refreshGrokBot")}
            className={styles.iconButton}
            disabled={busy || refreshing || !onRefresh}
            onClick={() => onRefresh?.()}
            type="button"
          >
            <RefreshCw aria-hidden="true" className={refreshing ? styles.spinning : undefined} size={16} />
          </button>
          <button
            aria-label={t("launchApp")}
            className={styles.grokBotAccountSwitch}
            disabled={busy || !canLaunchCurrent}
            onClick={() => {
              if (currentAccount) onSwitchAccount(currentAccount);
            }}
            type="button"
          >
            <Play aria-hidden="true" size={14} />
            <span className={styles.grokBotAccountSwitchLabel}>{t("launchApp")}</span>
          </button>
        <DropdownMenu.Root>
          <DropdownMenu.Trigger asChild>
            <button
              aria-label={t("switch")}
              className={styles.grokBotAccountSwitch}
              disabled={busy}
              type="button"
            >
              <span className={styles.grokBotAccountSwitchLabel}>{t("switch")}</span>
              <ChevronDown aria-hidden="true" size={14} />
            </button>
          </DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content align="end" className={styles.grokBotAccountMenu} sideOffset={6}>
              {botAccounts.length === 0 ? (
                <div className={styles.grokBotAccountMenuEmpty}>{t("grokBotNoEligibleAccounts")}</div>
              ) : (
                botAccounts.map((account) => {
                  const accountUsage = grokBotUsageLabel(account, t)?.text ?? t("grokBotUsageUnknown");
                  const isCurrent =
                    account.id === currentAccount?.id || Boolean(account.isGrokBotCurrent);
                  return (
                    <DropdownMenu.Item
                      className={styles.grokBotAccountMenuItem}
                      disabled={busy}
                      key={account.id}
                      onSelect={() => onSwitchAccount(account)}
                    >
                      <div className={styles.grokBotAccountMenuItemTop}>
                        <span className={styles.grokBotAccountMenuItemName}>{account.label}</span>
                        {isCurrent && <Check aria-hidden="true" size={14} />}
                      </div>
                      <span className={styles.grokBotAccountMenuItemMeta}>
                        {account.email ? `${account.email} · ${accountUsage}` : accountUsage}
                      </span>
                    </DropdownMenu.Item>
                  );
                })
              )}
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
        </div>
      </article>

      <AlertDialog.Root onOpenChange={setWebsiteOpen} open={websiteOpen}>
        <AlertDialog.Portal>
          <AlertDialog.Overlay className={styles.dialogOverlay} />
          <AlertDialog.Content className={styles.dialogContent}>
            <AlertDialog.Title>{t("grokBotOpenWebsiteTitle")}</AlertDialog.Title>
            <AlertDialog.Description>
              {t("grokBotOpenWebsiteDescription", { url: GROK_BOT_WEBSITE })}
            </AlertDialog.Description>
            <div className={styles.dialogActions}>
              <AlertDialog.Cancel asChild>
                <button className={styles.dialogCancel} disabled={openingWebsite} type="button">
                  {t("cancel")}
                </button>
              </AlertDialog.Cancel>
              <AlertDialog.Action asChild>
                <button
                  autoFocus
                  className={styles.dialogPrimary}
                  disabled={openingWebsite}
                  onClick={(event) => {
                    event.preventDefault();
                    void confirmOpenWebsite();
                  }}
                  type="button"
                >
                  {openingWebsite ? t("grokBotStatusLoading") : t("grokBotOpenWebsiteConfirm")}
                </button>
              </AlertDialog.Action>
            </div>
          </AlertDialog.Content>
        </AlertDialog.Portal>
      </AlertDialog.Root>
    </>
  );
}
