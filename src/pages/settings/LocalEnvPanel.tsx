import { listen } from "@tauri-apps/api/event";
import { AlertCircle, ArrowUpCircle, CheckCircle2, ChevronDown, Copy, Download, Loader2, RefreshCw, Stethoscope, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  getToolVersions,
  mergeToolVersions,
  probeToolInstallations,
  runToolLifecycleAction,
  TOOL_DISPLAY_NAMES,
  TOOL_NAMES,
  type ToolInstallation,
  type ToolInstallationReport,
  type ToolLifecycleAction,
  type ToolName,
  type ToolVersion,
  type WslShellPreference
} from "../../lib/tools";
import { ToolInstallRow } from "./ToolInstallRow";
import { TOOL_ICONS } from "./toolIcons";
import { ToolUninstallConfirmDialog } from "./ToolUninstallConfirmDialog";
import { ToolUpgradeConfirmDialog } from "./ToolUpgradeConfirmDialog";
import styles from "./page.module.css";

let lastToolVersions: ToolVersion[] = [];
const WSL_SHELL_OPTIONS = ["sh", "bash", "zsh", "fish", "dash"] as const;
const WSL_SHELL_FLAG_OPTIONS = ["-lic", "-lc", "-c"] as const;
const posixScriptInstallCommand = (url: string) =>
  `bash -c 'tmp=$(mktemp) && curl -fsSL ${url} -o $tmp && bash $tmp; status=$?; rm -f $tmp; exit $status'`;
const POSIX_ONE_CLICK_INSTALL_COMMANDS = `# Claude Code
${posixScriptInstallCommand("https://claude.ai/install.sh")} || npm i -g @anthropic-ai/claude-code@latest
# Codex
npm i -g @openai/codex@latest
# Gemini CLI
npm i -g @google/gemini-cli@latest
# Grok Build
npm i -g @xai-official/grok@latest
# OpenCode
${posixScriptInstallCommand("https://opencode.ai/install")} || npm i -g opencode-ai@latest
# OpenClaw
npm i -g openclaw@latest
# Hermes
${posixScriptInstallCommand("https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh")}
# Pi
npm i -g @earendil-works/pi-coding-agent@latest`;
const WINDOWS_ONE_CLICK_INSTALL_COMMANDS = `# Claude Code
npm i -g @anthropic-ai/claude-code@latest
# Codex
npm i -g @openai/codex@latest
# Gemini CLI
npm i -g @google/gemini-cli@latest
# Grok Build
npm i -g @xai-official/grok@latest
# OpenCode
npm i -g opencode-ai@latest
# OpenClaw
npm i -g openclaw@latest
# Hermes
irm https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.ps1 | iex
# Pi
npm i -g @earendil-works/pi-coding-agent@latest`;
const ONE_CLICK_INSTALL_COMMANDS = navigator.userAgent.includes("Windows") ? WINDOWS_ONE_CLICK_INSTALL_COMMANDS : POSIX_ONE_CLICK_INSTALL_COMMANDS;

function toolDisplayName(tool: string) {
  return TOOL_DISPLAY_NAMES[tool as ToolName] ?? tool;
}

function noticeText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function LocalEnvPanel({ onNotice }: { onNotice: (message: string, status?: "success" | "error") => void }) {
  const { t } = useTranslation();
  const [toolVersions, setToolVersions] = useState<ToolVersion[]>(() => lastToolVersions);
  const [isLoadingTools, setIsLoadingTools] = useState(() => lastToolVersions.length === 0);
  const [toolActions, setToolActions] = useState<Partial<Record<ToolName, ToolLifecycleAction>>>({});
  const [batchAction, setBatchAction] = useState<ToolLifecycleAction | null>(null);
  const [showInstallCommands, setShowInstallCommands] = useState(false);
  const [wslShellByTool, setWslShellByTool] = useState<Record<string, WslShellPreference>>({});
  const [loadingTools, setLoadingTools] = useState<Record<string, boolean>>({});
  const [toolDiagnostics, setToolDiagnostics] = useState<Partial<Record<ToolName, ToolInstallation[]>>>({});
  const [isDiagnosingAll, setIsDiagnosingAll] = useState(false);
  const [pendingUpgrade, setPendingUpgrade] = useState<{ toolNames: ToolName[]; plans: ToolInstallationReport[]; fromBatchEntry: boolean } | null>(null);
  const [pendingUninstall, setPendingUninstall] = useState<ToolInstallationReport | null>(null);
  const [preflightTools, setPreflightTools] = useState<Set<ToolName>>(() => new Set());
  const toolVersionByName = useMemo(() => new Map(toolVersions.map((tool) => [tool.name, tool])), [toolVersions]);
  const updatableToolNames = useMemo(
    () => TOOL_NAMES.filter((name) => toolVersionByName.get(name)?.update_available),
    [toolVersionByName]
  );

  const refreshToolVersions = useCallback(async (toolNames: ToolName[], wslOverrides?: Record<string, WslShellPreference>) => {
    if (toolNames.length === 0) return [];
    setLoadingTools((prev) => {
      const next = { ...prev };
      for (const name of toolNames) next[name] = true;
      return next;
    });
    try {
      const updated = await getToolVersions(toolNames, wslOverrides, true);
      const next = mergeToolVersions(lastToolVersions, updated);
      lastToolVersions = next;
      setToolVersions(next);
      return updated;
    } catch (error) {
      onNotice(noticeText(error), "error");
      return [];
    } finally {
      setLoadingTools((prev) => {
        const next = { ...prev };
        for (const name of toolNames) next[name] = false;
        return next;
      });
    }
  }, [onNotice]);

  const loadAllToolVersions = useCallback(async (options?: { force?: boolean }) => {
    setIsLoadingTools(lastToolVersions.length === 0);
    try {
      const data = await getToolVersions(undefined, wslShellByTool, options?.force);
      lastToolVersions = mergeToolVersions(lastToolVersions, data);
      setToolVersions(lastToolVersions);
    } catch (error) {
      onNotice(noticeText(error), "error");
    } finally {
      setIsLoadingTools(false);
    }
  }, [onNotice, wslShellByTool]);

  useEffect(() => {
    let disposed = false;
    let unlisten = () => {};
    void (async () => {
      const stop = await listen<ToolVersion[]>("tool-versions", ({ payload }) => {
        lastToolVersions = mergeToolVersions(lastToolVersions, payload);
        setToolVersions(lastToolVersions);
      });
      if (disposed) {
        stop();
        return;
      }
      unlisten = stop;
      await loadAllToolVersions();
    })();
    return () => {
      disposed = true;
      unlisten();
    };
    // Mount-only: shell changes refresh a single tool via handlers.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleToolShellChange = async (toolName: ToolName, field: "wslShell" | "wslShellFlag", value: string) => {
    const nextPref: WslShellPreference = { ...(wslShellByTool[toolName] ?? {}), [field]: value === "auto" ? null : value };
    setWslShellByTool((prev) => ({ ...prev, [toolName]: nextPref }));
    await refreshToolVersions([toolName], { [toolName]: nextPref });
  };

  const diagnoseToolSilently = useCallback(async (toolName: ToolName) => {
    try {
      const [report] = await probeToolInstallations([toolName]);
      setToolDiagnostics((prev) => {
        if (report?.is_conflict) return { ...prev, [toolName]: report.installs };
        if (!(toolName in prev)) return prev;
        const next = { ...prev };
        delete next[toolName];
        return next;
      });
    } catch {
      /* silent */
    }
  }, []);

  const handleDiagnoseAll = useCallback(async () => {
    setIsDiagnosingAll(true);
    try {
      const reports = await probeToolInstallations([...TOOL_NAMES]);
      const next: Partial<Record<ToolName, ToolInstallation[]>> = {};
      let conflicts = 0;
      for (const report of reports) {
        if (report.is_conflict) {
          next[report.tool as ToolName] = report.installs;
          conflicts += 1;
        }
      }
      setToolDiagnostics(next);
      onNotice(conflicts === 0 ? t("toolDiagnoseNoConflict") : t("toolConflictTitle"));
    } catch (error) {
      onNotice(noticeText(error) || t("toolDiagnoseFailed"), "error");
    } finally {
      setIsDiagnosingAll(false);
    }
  }, [onNotice, t]);

  const executeRun = useCallback(async (toolNames: ToolName[], action: ToolLifecycleAction) => {
    const isBatch = toolNames.length > 1;
    if (isBatch) setBatchAction(action);
    const failures: { toolName: ToolName; detail: string; soft: boolean; kind?: "notRunnable" | "versionUnchanged" }[] = [];
    let succeeded = 0;
    for (const toolName of toolNames) {
      setToolActions((prev) => ({ ...prev, [toolName]: action }));
      try {
        const previous = toolVersionByName.get(toolName);
        await runToolLifecycleAction([toolName], action, wslShellByTool);
        const refreshed = await refreshToolVersions([toolName], wslShellByTool);
        const tool = refreshed.find((item) => item.name === toolName);
        if (action === "uninstall") {
          succeeded += 1;
        } else if (tool?.version) {
          const latestVersion = tool.latest_version ?? previous?.latest_version ?? null;
          if (action === "update" && previous?.version && tool.version === previous.version && tool.update_available) {
            failures.push({ toolName, detail: t("toolActionVersionUnchanged", { version: tool.version, latest: latestVersion ?? t("unknown") }), soft: true, kind: "versionUnchanged" });
            void diagnoseToolSilently(toolName);
          } else {
            succeeded += 1;
            if (action === "update") void diagnoseToolSilently(toolName);
          }
        } else {
          failures.push({ toolName, detail: tool?.error?.trim() || t("toolNotRunnable"), soft: true, kind: "notRunnable" });
          void diagnoseToolSilently(toolName);
        }
      } catch (error) {
        failures.push({ toolName, detail: noticeText(error), soft: false });
      } finally {
        setToolActions((prev) => {
          const next = { ...prev };
          delete next[toolName];
          return next;
        });
      }
    }
    if (isBatch) setBatchAction(null);
    const actionLabel = action === "install" ? t("toolInstall") : action === "uninstall" ? t("toolUninstall") : t("toolUpdate");
    if (failures.length === 0) {
      onNotice(t("toolActionDone", { count: succeeded, action: actionLabel }));
      return;
    }
    const lastLine = (text: string) => text.trim().split("\n").filter(Boolean).at(-1) ?? text;
    const description = isBatch ? failures.map((item) => `${TOOL_DISPLAY_NAMES[item.toolName]}: ${lastLine(item.detail)}`).join("\n") : failures[0]?.detail;
    const hard = failures.filter((item) => !item.soft);
    if (succeeded === 0 && hard.length === 0) onNotice(description || t("toolActionInstalledNotRunnable"), "error");
    else if (succeeded === 0) onNotice(description || t("toolActionFailed"), "error");
    else onNotice(t("toolActionPartial", { succeeded, failed: failures.length, action: actionLabel }), "error");
  }, [diagnoseToolSilently, onNotice, refreshToolVersions, t, toolVersionByName, wslShellByTool]);

  const handleRunToolAction = useCallback(async (toolNames: ToolName[], action: ToolLifecycleAction, options?: { fromBatchEntry?: boolean }) => {
    if (toolNames.length === 0) return;
    if (toolNames.some((name) => preflightTools.has(name) || toolActions[name] !== undefined)) return;
    setPreflightTools((prev) => {
      const next = new Set(prev);
      toolNames.forEach((name) => next.add(name));
      return next;
    });
    const fromBatchEntry = options?.fromBatchEntry ?? false;
    if (fromBatchEntry) setBatchAction(action);
    try {
      if (action === "install") {
        await executeRun(toolNames, action);
        return;
      }
      try {
        const reports = await probeToolInstallations(toolNames);
        const needConfirm = reports.filter((report) => report.needs_confirmation);
        if (needConfirm.length === 0) await executeRun(toolNames, action);
        else setPendingUpgrade({ toolNames, plans: needConfirm, fromBatchEntry });
      } catch {
        await executeRun(toolNames, action);
      }
    } finally {
      if (fromBatchEntry) setBatchAction(null);
      setPreflightTools((prev) => {
        const next = new Set(prev);
        toolNames.forEach((name) => next.delete(name));
        return next;
      });
    }
  }, [executeRun, preflightTools, toolActions]);

  const handleUninstall = useCallback(async (toolName: ToolName) => {
    if (preflightTools.has(toolName) || toolActions[toolName] !== undefined) return;
    setPreflightTools((prev) => new Set(prev).add(toolName));
    try {
      const [report] = await probeToolInstallations([toolName]);
      if (!report?.uninstall_command) {
        onNotice(t("toolUninstallUnsupported"), "error");
        return;
      }
      setPendingUninstall(report);
    } catch (error) {
      onNotice(noticeText(error) || t("toolUninstallUnsupported"), "error");
    } finally {
      setPreflightTools((prev) => {
        const next = new Set(prev);
        next.delete(toolName);
        return next;
      });
    }
  }, [onNotice, preflightTools, t, toolActions]);

  const isAnyBusy = Boolean(batchAction) || Object.keys(toolActions).length > 0 || preflightTools.size > 0;

  return <div className={styles.env}>
    <div className={styles.envHead}>
      <h2>{t("localEnvCheck")}</h2>
      <div className={styles.envActions}>
        <button className={styles.ghost} disabled={isLoadingTools || isAnyBusy || isDiagnosingAll} onClick={() => void handleDiagnoseAll()} type="button">
          {isDiagnosingAll ? <Loader2 className={styles.spin} size={14} /> : <Stethoscope size={14} />}
          {isDiagnosingAll ? t("toolDiagnosing") : t("toolDiagnose")}
        </button>
        <button className={styles.ghost} disabled={isLoadingTools || isAnyBusy} onClick={() => void loadAllToolVersions({ force: true })} type="button">
          <RefreshCw className={isLoadingTools ? styles.spin : undefined} size={14} />
          {isLoadingTools ? t("refreshing") : t("refresh")}
        </button>
        <button className={styles.databaseButton} disabled={isLoadingTools || isAnyBusy || updatableToolNames.length === 0} onClick={() => void handleRunToolAction(updatableToolNames, "update", { fromBatchEntry: true })} type="button">
          {batchAction === "update" ? <Loader2 className={styles.spin} size={14} /> : <ArrowUpCircle size={14} />}
          {t("updateAllTools", { count: updatableToolNames.length })}
        </button>
      </div>
    </div>
    <div className={styles.toolGrid}>
      {TOOL_NAMES.map((toolName) => {
        const tool = toolVersionByName.get(toolName);
        const isToolVersionLoading = Boolean(loadingTools[toolName]) || (isLoadingTools && !toolVersionByName.has(toolName));
        const isOutdated = Boolean(tool?.update_available);
        const installedButBroken = Boolean(tool?.installed_but_broken);
        const action: ToolLifecycleAction | null = isToolVersionLoading || installedButBroken ? null : !tool?.version ? "install" : isOutdated ? "update" : null;
        const runningAction = toolActions[toolName];
        const title = tool?.version || tool?.error || t("unknown");
        const conflicts = toolDiagnostics[toolName];
        return <article className={styles.toolCard} key={toolName}>
          <div className={styles.toolTop}>
            <div className={styles.toolIdentity}>
              <span className={styles.toolIcon}><img alt="" height={20} src={TOOL_ICONS[toolName]} width={20} /></span>
              <div className={styles.toolCopy}>
                <h3>{TOOL_DISPLAY_NAMES[toolName]}</h3>
                {tool?.env_type && tool.env_type !== "unknown" ? <span className={`${styles.envBadge} ${styles[`env_${tool.env_type}`]}`}>{t(`envBadge.${tool.env_type}`)}{tool.wsl_distro ? ` · ${tool.wsl_distro}` : ""}</span> : null}
              </div>
            </div>
            {isToolVersionLoading ? <Loader2 className={styles.spin} size={16} /> : tool?.version ? isOutdated ? <span className={styles.outdated}>{t("updateAvailableShort")}</span> : <CheckCircle2 className={styles.readyIcon} size={16} /> : <AlertCircle className={styles.warnIcon} size={16} />}
          </div>
          <dl className={styles.toolMeta}>
            <div><dt>{t("currentVersion")}</dt><dd title={title}>{isToolVersionLoading ? t("loading") : tool?.version ? tool.version : installedButBroken ? t("installedNotRunnable") : t("notInstalled")}</dd></div>
            <div><dt>{t("latestVersion")}</dt><dd>{isToolVersionLoading || tool?.latest_pending ? t("loading") : tool?.latest_version || t("unknown")}</dd></div>
          </dl>
          {!isToolVersionLoading && !tool?.version && tool?.error ? <p className={styles.toolError}>{tool.error}</p> : null}
          {tool?.env_type === "wsl" ? <div className={styles.wslRow}>
            <select disabled={isToolVersionLoading || isAnyBusy} onChange={(event) => void handleToolShellChange(toolName, "wslShell", event.target.value)} value={wslShellByTool[toolName]?.wslShell || "auto"}>
              <option value="auto">{t("auto")}</option>
              {WSL_SHELL_OPTIONS.map((shell) => <option key={shell} value={shell}>{shell}</option>)}
            </select>
            <select disabled={isToolVersionLoading || isAnyBusy} onChange={(event) => void handleToolShellChange(toolName, "wslShellFlag", event.target.value)} value={wslShellByTool[toolName]?.wslShellFlag || "auto"}>
              <option value="auto">{t("auto")}</option>
              {WSL_SHELL_FLAG_OPTIONS.map((flag) => <option key={flag} value={flag}>{flag}</option>)}
            </select>
          </div> : null}
          {conflicts && conflicts.length > 0 ? <div className={styles.conflict}>
            <div className={styles.conflictTitle}>{t("toolConflictTitle")}</div>
            <p className={styles.conflictHint}>{t("toolConflictHint")}</p>
            <ul className={styles.installList}>{conflicts.map((inst) => <li key={inst.path}><ToolInstallRow inst={inst} /></li>)}</ul>
          </div> : null}
          <div className={styles.toolFooter}>
            {isToolVersionLoading ? <span>{t("loading")}</span> : <>
              {tool?.version || installedButBroken ? <button className={styles.ghost} disabled={isAnyBusy} onClick={() => void handleUninstall(toolName)} type="button">
                {runningAction === "uninstall" ? <Loader2 className={styles.spin} size={14} /> : <Trash2 size={14} />}
                {t("toolUninstall")}
              </button> : null}
              {installedButBroken ? <span className={styles.warnText}>{t("toolCheckEnv")}</span> : action ? <button className={action === "install" ? styles.ghost : styles.databaseButton} disabled={isAnyBusy} onClick={() => void handleRunToolAction([toolName], action)} type="button">
                {runningAction || preflightTools.has(toolName) ? <Loader2 className={styles.spin} size={14} /> : action === "install" ? <Download size={14} /> : <ArrowUpCircle size={14} />}
                {action === "install" ? t("toolInstall") : t("toolUpdate")}
              </button> : <span>{t("toolReady")}</span>}
            </>}
          </div>
        </article>;
      })}
    </div>
    <button aria-expanded={showInstallCommands} className={styles.fold} onClick={() => setShowInstallCommands((open) => !open)} type="button">
      <ChevronDown className={showInstallCommands ? undefined : styles.foldClosed} size={14} />
      {t("manualInstallCommands")}
    </button>
    {showInstallCommands ? <div className={styles.commands}>
      <div className={styles.commandBar}>
        <p>{t("oneClickInstallHint")}</p>
        <button className={styles.ghost} onClick={() => void navigator.clipboard.writeText(ONE_CLICK_INSTALL_COMMANDS).then(() => onNotice(t("installCommandsCopied"))).catch(() => onNotice(t("installCommandsCopyFailed"), "error"))} type="button"><Copy size={14} />{t("copy")}</button>
      </div>
      <pre>{ONE_CLICK_INSTALL_COMMANDS}</pre>
    </div> : null}
    <ToolUpgradeConfirmDialog
      displayName={toolDisplayName}
      onCancel={() => setPendingUpgrade(null)}
      onConfirm={() => {
        if (!pendingUpgrade) return;
        const { toolNames, fromBatchEntry } = pendingUpgrade;
        if (fromBatchEntry) setBatchAction("update");
        void executeRun(toolNames, "update").finally(() => { if (fromBatchEntry) setBatchAction(null); });
        setPendingUpgrade(null);
      }}
      open={pendingUpgrade !== null}
      plans={pendingUpgrade?.plans ?? []}
    />
    <ToolUninstallConfirmDialog
      displayName={toolDisplayName}
      onCancel={() => setPendingUninstall(null)}
      onConfirm={() => {
        if (!pendingUninstall) return;
        const toolName = pendingUninstall.tool as ToolName;
        setPendingUninstall(null);
        void executeRun([toolName], "uninstall");
      }}
      open={pendingUninstall !== null}
      report={pendingUninstall}
    />
  </div>;
}
