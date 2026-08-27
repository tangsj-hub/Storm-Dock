import { convertFileSrc } from "@tauri-apps/api/core";
import { Puzzle, Trash2, Waypoints, Zap } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ApplicationKind, Plugin, PluginCapability } from "../../lib/types";
import { Tooltip } from "../Tooltip";
import styles from "./PluginCatalog.module.css";

export type PluginHost = {
  application: ApplicationKind;
  accountId?: string;
  list(): Promise<Plugin[]>;
  setEnabled(plugin: Plugin, enabled: boolean): Promise<void>;
  setCapabilityEnabled?(plugin: Plugin, capability: PluginCapability, enabled: boolean): Promise<void>;
  remove?(plugin: Plugin): Promise<void>;
};

type PluginCache = { application: ApplicationKind; accountId?: string; plugins: Plugin[] };
const cacheKey = "plugin-catalog-v1";
const pluginKey = (plugin: Pick<Plugin, "id" | "source">) => `${plugin.source}:${plugin.id}`;

function cacheFor(application: ApplicationKind, accountId?: string) {
  try {
    const value = JSON.parse(localStorage.getItem(cacheKey) ?? "null") as PluginCache | null;
    return value?.application === application && value.accountId === accountId && Array.isArray(value.plugins) ? value.plugins : undefined;
  } catch { return undefined; }
}

function persist(application: ApplicationKind, accountId: string | undefined, plugins: Plugin[]) {
  localStorage.setItem(cacheKey, JSON.stringify({ application, accountId, plugins } satisfies PluginCache));
}

export function cachedPluginCount(application: ApplicationKind, accountId?: string) {
  return cacheFor(application, accountId)?.length;
}

function iconSource(icon?: string) {
  if (!icon) return undefined;
  try { if (new URL(icon).protocol === "https:") return icon; } catch { /* Local paths are converted below. */ }
  return /^(?:\/|[A-Za-z]:[\\/])/.test(icon) ? convertFileSrc(icon) : undefined;
}

function PluginIcon({ icon }: Pick<Plugin, "icon">) {
  const [failed, setFailed] = useState(false);
  const src = iconSource(icon);
  return <span className={styles.icon}>{src && !failed ? <img alt="" decoding="async" loading="lazy" onError={() => setFailed(true)} src={src} /> : <Puzzle aria-hidden="true" size={20} />}</span>;
}

export function PluginCatalog({ host, disabled = false, expanded = false, onError, onChanged, onPluginsChange }: { host: PluginHost; disabled?: boolean; expanded?: boolean; onError(error: unknown): void; onChanged(): void; onPluginsChange?(plugins: Plugin[]): void }) {
  const { t } = useTranslation();
  const [plugins, setPlugins] = useState<Plugin[]>(() => cacheFor(host.application, host.accountId) ?? []);
  const [loading, setLoading] = useState(!cacheFor(host.application, host.accountId));
  const [pending, setPending] = useState<Set<string>>(() => new Set());
  const request = useRef(0);
  useEffect(() => {
    const current = ++request.current;
    const cached = cacheFor(host.application, host.accountId);
    setPlugins(cached ?? []); setLoading(!cached);
    void host.list().then((next) => {
      if (current !== request.current) return;
      setPlugins(next); persist(host.application, host.accountId, next);
    }).catch(onError).finally(() => { if (current === request.current) setLoading(false); });
  }, [host, onError]);
  useEffect(() => { onPluginsChange?.(plugins); }, [onPluginsChange, plugins]);
  const update = async (key: string, operation: () => Promise<void>, transform: (current: Plugin[]) => Plugin[]) => {
    setPending((current) => new Set(current).add(key));
    try { await operation(); setPlugins((current) => { const next = transform(current); persist(host.application, host.accountId, next); return next; }); onChanged(); }
    catch (error) { onError(error); }
    finally { setPending((current) => { const next = new Set(current); next.delete(key); return next; }); }
  };
  const sourceLabel = (source: Plugin["source"]) => t(`pluginSource${source[0].toUpperCase()}${source.slice(1)}`, { defaultValue: source });
  if (!plugins.length) return <div className={styles.empty}><Puzzle aria-hidden="true" size={32} /><h2>{loading ? t("refreshing") : t("pluginsEmptyTitle")}</h2>{!loading && <p>{t("pluginsEmptyDescription")}</p>}</div>;
  return <div className={styles.list}>{plugins.map((plugin) => {
    const key = pluginKey(plugin);
    const groups = (["skill", "mcp", "hook"] as const).map((kind) => ({ kind, capabilities: plugin.capabilities.filter((capability) => capability.kind === kind) })).filter(({ capabilities }) => capabilities.length);
    return <article className={styles.card} key={key}><header className={styles.header}><PluginIcon icon={plugin.icon} /><div className={styles.copy}><strong>{plugin.name}</strong>{plugin.description && <p>{plugin.description}</p>}</div><span className={styles.source} data-source={plugin.source}>{sourceLabel(plugin.source)}</span><div className={styles.actions}><Tooltip content={plugin.teamRequired ? t("pluginManagedByTeam") : plugin.enabled ? t("pluginDisable") : t("pluginEnable")}><button aria-checked={plugin.enabled} aria-label={plugin.enabled ? t("pluginDisable") : t("pluginEnable")} className={styles.switch} disabled={disabled || plugin.teamRequired || pending.has(key)} onClick={() => void update(key, () => host.setEnabled(plugin, !plugin.enabled), (items) => items.map((item) => pluginKey(item) === key ? { ...item, enabled: !plugin.enabled } : item))} role="switch" type="button"><span /></button></Tooltip>{host.remove && (plugin.source === "local" || plugin.source === "marketplace" || plugin.source === "codex") && <Tooltip content={t("pluginDelete")}><button aria-label={t("pluginDelete")} className={styles.delete} disabled={disabled || plugin.teamRequired || pending.has(key)} onClick={() => void update(key, () => host.remove!(plugin), (items) => items.filter((item) => pluginKey(item) !== key))} type="button"><Trash2 aria-hidden="true" size={18} /></button></Tooltip>}</div></header>{expanded && groups.map(({ kind, capabilities }) => <section className={styles.group} key={kind}><h2>{kind === "skill" ? t("pluginCapabilitySkills") : kind === "mcp" ? t("pluginCapabilityMcps") : t("pluginCapabilityHooks")} <span>{capabilities.length}</span></h2>{capabilities.map((capability) => { const capabilityKey = `${key}:${capability.kind}:${capability.id}`; const manageable = capability.kind !== "hook" && host.setCapabilityEnabled; const Icon = capability.kind === "skill" ? Puzzle : capability.kind === "mcp" ? Waypoints : Zap; return <div className={styles.capability} key={`${capability.kind}:${capability.id}`}><Icon aria-hidden="true" className={styles.capabilityIcon} size={20} /><div className={styles.copy}><strong>{capability.name}</strong>{capability.description && <p>{capability.description}</p>}</div>{manageable ? <button aria-checked={capability.enabled} aria-label={capability.enabled ? t("pluginDisable") : t("pluginEnable")} className={styles.switch} disabled={disabled || !plugin.enabled || pending.has(capabilityKey)} onClick={() => void update(capabilityKey, () => host.setCapabilityEnabled!(plugin, capability, !capability.enabled), (items) => items.map((item) => pluginKey(item) === key ? { ...item, capabilities: item.capabilities.map((entry) => entry.id === capability.id && entry.kind === capability.kind ? { ...entry, enabled: !capability.enabled } : entry) } : item))} role="switch" type="button"><span /></button> : capability.kind === "hook" && host.setCapabilityEnabled ? <span className={styles.notice}>{t("pluginHookTrustRequired")}</span> : null}</div>; })}</section>)}</article>;
  })}</div>;
}
