import type { GrokBotStatus } from "../../../lib/types";
import type { Translate } from "./accountPresentation";

export function grokBotAvailabilityLabel(status: GrokBotStatus, t: Translate) {
  if (status.available) return t("grokBotReady");
  return status.reason?.trim() || t("grokBotUnavailable");
}

export function grokBotSignInLabel(status: GrokBotStatus, t: Translate) {
  if (status.currentAccountLabel) return t("grokBotSignedInAs", { account: status.currentAccountLabel });
  if (status.signedIn) return t("grokBotSignedIn");
  return t("grokBotNotSignedIn");
}
