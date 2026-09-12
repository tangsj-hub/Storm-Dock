import { Check, CircleAlert, LoaderCircle, Zap } from "lucide-react";
import { useTranslation } from "react-i18next";
import grokBotIcon from "../../../assets/tools/grok-bot.png";
import type { GrokBotStatus } from "../../../lib/types";
import { grokBotAvailabilityLabel, grokBotSignInLabel } from "../lib/grokBotStatus";
import styles from "../page.module.css";

export function GrokBotStatusCard({
  status,
  loading,
}: {
  status?: GrokBotStatus;
  loading?: boolean;
}) {
  const { t } = useTranslation();
  return (
    <article className={styles.grokBotStatus}>
      <img alt="" className={styles.grokBotStatusIcon} src={grokBotIcon} />
      <div className={styles.grokBotStatusCopy}>
        <strong>{t("grokBot")}</strong>
        <div className={styles.accountMeta}>
          {loading && !status ? (
            <span className={styles.metaBadge}>
              <LoaderCircle aria-hidden="true" className={styles.spinning} size={11} />
              {t("grokBotStatusLoading")}
            </span>
          ) : status ? (
            <>
              <span className={`${styles.metaBadge} ${status.available ? styles.grokBotReadyBadge : styles.grokBotUnavailableBadge}`}>
                {status.available ? <Check aria-hidden="true" size={11} /> : <CircleAlert aria-hidden="true" size={11} />}
                {grokBotAvailabilityLabel(status, t)}
              </span>
              <span className={styles.metaBadge}>{grokBotSignInLabel(status, t)}</span>
              {status.installed && (
                <span className={styles.metaBadge}>
                  {status.running ? t("grokBotRunning") : t("grokBotNotRunning")}
                </span>
              )}
              {status.currentAccountLabel && (
                <span className={`${styles.metaBadge} ${styles.grokBotCurrentAccountBadge}`}>
                  <Zap aria-hidden="true" size={11} strokeWidth={2.4} />
                  {status.currentAccountLabel}
                </span>
              )}
            </>
          ) : (
            <span className={styles.metaBadge}>{t("grokBotUnavailable")}</span>
          )}
        </div>
      </div>
    </article>
  );
}
