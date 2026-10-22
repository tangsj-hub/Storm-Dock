import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import "../../i18n";
import { confirmLabels, offerMessage, respondSwitchOffer, SWITCH_POPUP_SECONDS, type SwitchOffer } from "../../lib/accountSwitcher";
import { cancelBotImport, dismissBotImport, getBotImportProgress } from "../../lib/api";
import type { BotImportProgress } from "../../lib/botTemplates";
import { setPopupHover, usePopupPayload } from "../../lib/popup";
import { NativePopup } from "../../components/NativePopup";

// This window is created once and reused: every show pushes a new generation,
// local state is keyed on it so it resets without remounting.

function SwitcherPopup() {
  const { t } = useTranslation();
  const envelope = usePopupPayload<SwitchOffer>();
  const [answeredGen, setAnsweredGen] = useState(0);
  const [hovering, setHovering] = useState(false);
  const answeredRef = useRef(0);
  const generation = envelope?.generation ?? 0;
  const offer = envelope?.payload;
  const busy = generation > 0 && answeredGen === generation;
  // Local mirror of the backend 5s auto-hide; pauses on hover and while confirm/switch is in flight.
  const seconds = useAutoHideSeconds(generation, SWITCH_POPUP_SECONDS, hovering || busy);
  useEffect(() => { setHovering(false); }, [generation]);

  const answer = (accept: boolean, instant = false) => {
    if (!offer || answeredRef.current === generation) return;
    answeredRef.current = generation;
    setAnsweredGen(generation);
    // Backend hides this window; ignore / cancel keeps the current account.
    void respondSwitchOffer(generation, accept, accept ? confirmLabels(offer, t) : undefined, instant).catch(() => undefined);
  };

  if (!offer) return null;
  return <NativePopup
    actions={[
      { label: t("switcherIgnore"), onClick: () => answer(false), disabled: busy },
      { label: t("switcherSwitch"), onClick: () => answer(true), disabled: busy, primary: true },
    ]}
    ariaLabel={t("switcherTitle", { provider: offer.providerName })}
    onDismiss={busy ? undefined : () => answer(false, true)}
    onHover={(next) => {
      setHovering(next);
      if (generation) void setPopupHover(generation, next);
    }}
    body={offerMessage(offer, t)}
    preview={offer.preview ? t("switcherPreviewTag") : undefined}
    resetKey={generation}
    role="alertdialog"
    time={t("switcherCountdownShort", { count: seconds })}
    title={offer.providerName}
  />;
}

/** Counts down from `total` each time `resetKey` changes; freezes while `paused`. */
function useAutoHideSeconds(resetKey: number, total: number, paused: boolean) {
  const [seconds, setSeconds] = useState(total);
  const pausedRef = useRef(paused);
  pausedRef.current = paused;
  useEffect(() => {
    if (!resetKey) return;
    setSeconds(total);
    const id = window.setInterval(() => {
      if (pausedRef.current) return;
      setSeconds((value) => (value > 0 ? value - 1 : 0));
    }, 1000);
    return () => window.clearInterval(id);
  }, [resetKey, total]);
  return seconds;
}

/** Debug-trigger preview of the import banner (sample data, buttons do nothing). */
const PREVIEW_PROGRESS: BotImportProgress = { total: 3, position: 2, currentName: "Sample Bot", done: false, cancelled: false, imported: ["Bot A"], skipped: [], blockedVersion: null, requiredVersion: "" };

/** Bot模版 import queue banner: stays up while running, backend hides it ~5 s after the summary. */
function ImportProgressBanner() {
  const { t } = useTranslation();
  const envelope = usePopupPayload<{ preview: boolean }>();
  const [live, setLive] = useState<BotImportProgress | null>(null);
  const [cancelledGen, setCancelledGen] = useState(0);
  const generation = envelope?.generation ?? 0;
  const preview = envelope?.payload.preview ?? false;
  const cancelling = generation > 0 && cancelledGen === generation;

  useEffect(() => {
    const unlisten = listen<BotImportProgress>("bot-import-progress", (event) => setLive(event.payload));
    return () => { void unlisten.then((stop) => stop()); };
  }, []);
  // New banner: re-pull so a previous run's summary never flashes.
  useEffect(() => {
    if (!generation || preview) return;
    void getBotImportProgress().then(setLive).catch(() => setLive(null));
  }, [generation, preview]);

  const progress = preview ? PREVIEW_PROGRESS : live;
  const dismiss = (instant = false) => { if (!preview) void dismissBotImport(generation, instant); };
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
      ? { label: t("botImportClose"), onClick: () => dismiss() }
      : { label: t("botImportCancelRemaining"), onClick: () => { if (preview) return; setCancelledGen(generation); void cancelBotImport(); }, disabled: cancelling }]}
    body={message}
    detail={detail}
    onDismiss={progress.done && !preview ? () => dismiss(true) : undefined}
    onHover={(hovering) => { if (generation) void setPopupHover(generation, hovering); }}
    preview={preview ? t("switcherPreviewTag") : undefined}
    resetKey={generation}
    time={`${progress.position}/${progress.total}`}
    title={t("botImportTitle")}
  />;
}

const params = new URLSearchParams(window.location.search);
const mode = params.get("mode");
createRoot(document.getElementById("root")!).render(mode === "import" ? <ImportProgressBanner /> : <SwitcherPopup />);
