import type { SwitchProgress } from "../types";

export function progressForAccount(progress: SwitchProgress | undefined, accountId: string) {
  return progress?.accountId === accountId ? progress : undefined;
}

export function shouldApplySwitchProgress(progress: SwitchProgress) {
  return progress.status !== "waiting" && progress.stage !== "restartRequired";
}
