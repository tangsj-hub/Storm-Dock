import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import "../../i18n";
import { SWITCH_POPUP_SECONDS, confirmLabels, getSwitchOffer, offerMessage, respondSwitchOffer, type SwitchOffer } from "../../lib/accountSwitcher";
import { cancelBotImport, dismissBotImport, getBotImportProgress } from "../../lib/api";
import type { BotImportProgress } from "../../lib/botTemplates";
import { NativePopup } from "../../components/NativePopup";

function SwitcherPopup() {
  const { t } = useTranslation();
  const [offer, setOffer] = useState<SwitchOffer | null>();
  const [left, setLeft] = useState(SWITCH_POPUP_SECONDS);
  const [busy, setBusy] = useState(false);
  const answered = useRef(false);

  const answer = (accept: boolean) => {
    if (answered.current) return;
    answered.current = true;
    setBusy(true);
    // busy pauses the countdown while the native confirm is open. Backend closes this
    // window; default (timeout / ignore / cancel) keeps the current account.
    void respondSwitchOffer(accept, accept && offer ? confirmLabels(offer, t) : undefined).catch(() => undefined);
  };

  useEffect(() => { void getSwitchOffer().then(setOffer).catch(() => setOffer(null)); }, []);
  useEffect(() => { if (offer === null) answer(false); }, [offer]);
  useEffect(() => {
    if (!offer || busy) return;
    if (left <= 0) { answer(false); return; }
    const timer = window.setTimeout(() => setLeft((value) => value - 1), 1000);
    return () => window.clearTimeout(timer);
  }, [offer, left, busy]);

  if (!offer) return null;
  return <NativePopup
    actions={[
      { label: t("switcherIgnore"), onClick: () => answer(false), disabled: busy },
      { label: t("switcherSwitch"), onClick: () => answer(true), disabled: busy, primary: true },
    ]}
    ariaLabel={t("switcherTitle", { provider: offer.providerName })}
    body={offerMessage(offer, t)}
    preview={offer.preview ? t("switcherPreviewTag") : undefined}
    role="alertdialog"
    time={busy ? t("switcherNow") : t("switcherCountdownShort", { count: left })}
    title={offer.providerName}
  />;
}

/** Debug-trigger preview of the import banner (sample data, buttons do nothing). */
const PREVIEW = new URLSearchParams(window.location.search).get("preview") === "1";
const PREVIEW_PROGRESS: BotImportProgress = { total: 3, position: 2, currentName: "Sample Bot", done: false, cancelled: false, imported: ["Bot A"], skipped: [], blockedVersion: null, requiredVersion: "" };

/** Bot模版 import queue banner: stays up while running, backend hides it ~5 s after the summary. */
function ImportProgressBanner() {
  const { t } = useTranslation();
  const [progress, setProgress] = useState<BotImportProgress | null>();
  const [cancelling, setCancelling] = useState(false);

  useEffect(() => {
    if (PREVIEW) { setProgress(PREVIEW_PROGRESS); return; }
    void getBotImportProgress().then(setProgress).catch(() => setProgress(null));
    const unlisten = listen<BotImportProgress>("bot-import-progress", (event) => setProgress(event.payload));
    return () => { void unlisten.then((stop) => stop()); };
  }, []);

  if (!progress) return null;
  const blocked = progress.blockedVersion != null;
  const message = blocked
    ? t("botImportNeedsVersion", { required: progress.requiredVersion, current: progress.blockedVersion || t("botImportVersionUnknown") })
    : progress.done
    ? t("botImportSummary", { imported: progress.imported.length, skipped: progress.skipped.length })
    : t("botImportRunning", { position: progress.position, total: progress.total, name: progress.currentName ?? "" });
  const detail = blocked
    ? t("botImportBlockedDetail", { count: progress.skipped.length })
    : progress.done
    ? progress.skipped.length ? t("botImportSkippedList", { names: progress.skipped.join("、") }) : undefined
    : t("botImportConfirmHint");
  return <NativePopup
    actions={[progress.done
      ? { label: t("botImportClose"), onClick: () => { if (!PREVIEW) void dismissBotImport(); } }
      : { label: t("botImportCancelRemaining"), onClick: () => { if (PREVIEW) return; setCancelling(true); void cancelBotImport(); }, disabled: cancelling }]}
    body={message}
    detail={detail}
    preview={PREVIEW ? t("switcherPreviewTag") : undefined}
    time={`${progress.position}/${progress.total}`}
    title={t("botImportTitle")}
  />;
}

const params = new URLSearchParams(window.location.search);
const mode = params.get("mode");
createRoot(document.getElementById("root")!).render(mode === "import" ? <ImportProgressBanner /> : <SwitcherPopup />);
