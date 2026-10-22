import { invoke } from "@tauri-apps/api/core";

/** Must match POPUP_AUTO_HIDE in switcher/runtime.rs */
export const SWITCH_POPUP_SECONDS = 5;

export type SwitchReason = "lowQuota" | "expiring";

export interface SwitcherAccount {
  id: string;
  label: string;
  current: boolean;
  remainingPercent?: number | null;
  /** unix seconds */
  expiresAt?: number | null;
}

export interface SwitchOffer {
  provider: string;
  providerName: string;
  reason: SwitchReason;
  severity: number;
  current: SwitcherAccount;
  candidate: SwitcherAccount;
  preview?: boolean;
}

export interface ProviderSwitcherSettings {
  enabled: boolean;
  minRemainingPercent: number;
  expiryWindowHours: number;
}

export interface SwitcherSettings {
  providers: Record<string, ProviderSwitcherSettings>;
}

export const SWITCHER_LIMITS = { percent: { min: 1, max: 90 }, hours: { min: 1, max: 168 } } as const;

export const DEFAULT_PROVIDER_SETTINGS: ProviderSwitcherSettings = { enabled: true, minRemainingPercent: 10, expiryWindowHours: 24 };

export interface SwitcherProvider { id: string; name: string }
export const listSwitcherProviders = () => invoke<SwitcherProvider[]>("list_account_switcher_providers");
export const getSwitcherSettings = () => invoke<SwitcherSettings>("get_account_switcher_settings");
export const setSwitcherSettings = (settings: SwitcherSettings) => invoke<SwitcherSettings>("set_account_switcher_settings", { settings });
export const previewSwitchOffer = (providerId: string) => invoke<void>("preview_switch_offer", { providerId });
export const getSwitchOffer = () => invoke<SwitchOffer | null>("get_switch_offer");
export interface ConfirmLabels { title: string; message: string; ok: string; cancel: string }
export const respondSwitchOffer = (generation: number, accept: boolean, confirm?: ConfirmLabels, instant = false) => invoke<void>("respond_switch_offer", { generation, accept, confirm: confirm ?? null, instant });

type T = (key: string, options?: Record<string, unknown>) => string;

export function percentText(value?: number | null) {
  if (value === null || value === undefined || !Number.isFinite(value)) return "--";
  return `${Math.max(0, Math.min(100, Math.round(value)))}%`;
}

export function hoursLeft(expiresAt: number | null | undefined, nowMs = Date.now()) {
  if (!expiresAt) return undefined;
  return Math.max(0, Math.ceil((expiresAt * 1000 - nowMs) / 3_600_000));
}

/** e.g. 当前账号额度剩余 8%，可切换到 xxx（剩余 70%） */
export function offerMessage(offer: SwitchOffer, t: T, nowMs = Date.now()) {
  const target = t("switcherTarget", { account: offer.candidate.label, percent: percentText(offer.candidate.remainingPercent) });
  if (offer.reason === "expiring") {
    const hours = hoursLeft(offer.current.expiresAt, nowMs) ?? 0;
    const head = hours <= 0 ? t("switcherExpired") : t("switcherExpiring", { count: hours });
    return `${head}${t("switcherJoin")}${target}`;
  }
  return `${t("switcherLowQuota", { percent: percentText(offer.current.remainingPercent) })}${t("switcherJoin")}${target}`;
}

/** Native pre-switch warning, reusing the manual launch button's text. */
export function confirmLabels(offer: SwitchOffer, t: T): ConfirmLabels {
  return {
    title: t("grokBotSwitchConfirm"),
    message: `${t("grokBotSwitchDescription", { account: offer.candidate.label })}\n${t("grokBotSwitchWarning")}`,
    ok: t("grokBotSwitchConfirm"),
    cancel: t("cancel"),
  };
}

export function providerSettings(settings: SwitcherSettings | undefined, id: string): ProviderSwitcherSettings {
  return { ...DEFAULT_PROVIDER_SETTINGS, ...(settings?.providers[id] ?? {}) };
}

export function withProviderSettings(settings: SwitcherSettings, id: string, patch: Partial<ProviderSwitcherSettings>): SwitcherSettings {
  const next = { ...providerSettings(settings, id), ...patch };
  next.minRemainingPercent = clampNumber(next.minRemainingPercent, SWITCHER_LIMITS.percent.min, SWITCHER_LIMITS.percent.max, DEFAULT_PROVIDER_SETTINGS.minRemainingPercent);
  next.expiryWindowHours = Math.round(clampNumber(next.expiryWindowHours, SWITCHER_LIMITS.hours.min, SWITCHER_LIMITS.hours.max, DEFAULT_PROVIDER_SETTINGS.expiryWindowHours));
  return { ...settings, providers: { ...settings.providers, [id]: next } };
}

function clampNumber(value: number, min: number, max: number, fallback: number) {
  if (!Number.isFinite(value)) return fallback;
  return Math.max(min, Math.min(max, value));
}
