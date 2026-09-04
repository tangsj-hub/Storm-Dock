import * as AlertDialog from "@radix-ui/react-alert-dialog";
import {
  CheckSquare,
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  Clock3,
  Copy,
  FolderOpen,
  MessageSquareText,
  Play,
  RefreshCw,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Tooltip } from "../../../components/Tooltip";
import type { LocalSession, LocalSessionMessage, SessionDeleteBatchResult } from "../../../lib/types";
import styles from "../page.module.css";
import {
  filterSessions,
  formatRelativeSessionTime,
  groupSessions,
  removeSelectedIds,
  toggleSelectedIds,
  uniqueSessions,
} from "../lib/sessionPresentation";
import { useLatestRequest } from "../hooks/useLatestRequest";

export type SessionProvider = {
  id: string;
  label: string;
  icon: string;
  list: () => Promise<LocalSession[]>;
  loadMessages: (id: string) => Promise<LocalSessionMessage[]>;
  remove?: (id: string) => Promise<unknown>;
  removeMany?: (ids: string[]) => Promise<SessionDeleteBatchResult>;
  launch?: (id: string) => Promise<unknown>;
};

type Props = {
  provider: SessionProvider;
  onError: (error: unknown) => void;
  onNotice: (message: string) => void;
  refreshKey: number;
  onRefreshingChange: (refreshing: boolean) => void;
};

export const SessionWorkspace = memo(function SessionWorkspace({ provider, onError, onNotice, refreshKey, onRefreshingChange }: Props) {
  const { t } = useTranslation();
  const [sessions, setSessions] = useState<LocalSession[]>([]);
  const [refreshing, setRefreshing] = useState(true);
  const [selectedId, setSelectedId] = useState<string>();
  const [messages, setMessages] = useState<LocalSessionMessage[]>([]);
  const [messagesLoading, setMessagesLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [selectionMode, setSelectionMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
  const [deleteTargets, setDeleteTargets] = useState<string[]>();
  const [deleting, setDeleting] = useState(false);
  const [expandedProjects, setExpandedProjects] = useState<Set<string>>(() => new Set());
  const beginMessageRequest = useLatestRequest();

  const loadSessions = useCallback(async () => {
    setRefreshing(true);
    onRefreshingChange(true);
    try {
      const next = uniqueSessions(await provider.list());
      setSessions(next);
      setExpandedProjects((current) => {
        const projects = new Set(next.map((session) => session.projectDir?.trim() || "__unknown__"));
        return new Set([...current].filter((project) => projects.has(project)));
      });
    } catch (error) {
      onError(error);
    } finally {
      setRefreshing(false);
      onRefreshingChange(false);
    }
  }, [onError, onRefreshingChange, provider]);

  useEffect(() => {
    setSelectedId(undefined);
    setMessages([]);
    setSearch("");
    setSelectionMode(false);
    setSelectedIds(new Set());
    void loadSessions();
  }, [loadSessions, refreshKey]);

  useEffect(() => {
    if (selectedId && sessions.some((session) => session.id === selectedId)) return;
    setSelectedId(sessions[0]?.id);
  }, [selectedId, sessions]);

  useEffect(() => {
    if (!selectedId) {
      setMessages([]);
      return;
    }
    const isCurrent = beginMessageRequest();
    setMessagesLoading(true);
    void provider.loadMessages(selectedId)
      .then((next) => { if (isCurrent()) setMessages(next); })
      .catch(onError)
      .finally(() => { if (isCurrent()) setMessagesLoading(false); });
  }, [beginMessageRequest, onError, provider, selectedId]);

  const visibleSessions = useMemo(() => filterSessions(sessions, search), [search, sessions]);
  const projects = useMemo(() => groupSessions(visibleSessions), [visibleSessions]);
  const selectedSession = sessions.find((session) => session.id === selectedId);
  const canDelete = Boolean(provider.remove);

  const copy = async (value: string) => {
    try { await navigator.clipboard.writeText(value); onNotice(t("copied")); }
    catch (error) { onError(error); }
  };
  const deleteSessions = async () => {
    const targets = deleteTargets ?? [];
    if ((!provider.remove && !provider.removeMany) || !targets.length || deleting) return;
    setDeleting(true);
    let deleted = new Set<string>();
    let failedCount = targets.length;
    try {
      if (provider.removeMany) {
        const result = await provider.removeMany(targets);
        deleted = new Set(result.deletedIds);
        failedCount = result.failedIds.length;
      } else if (provider.remove) {
        const result = await Promise.allSettled(targets.map(provider.remove));
        deleted = new Set(targets.filter((_, index) => result[index].status === "fulfilled"));
        failedCount = targets.length - deleted.size;
      }
    } catch (error) {
      onError(error);
    }
    setSessions((current) => current.filter((session) => !deleted.has(session.id)));
    setSelectedIds((current) => removeSelectedIds(current, deleted));
    if (selectedId && deleted.has(selectedId)) setSelectedId(undefined);
    setDeleteTargets(undefined);
    setDeleting(false);
    if (failedCount) onError(t("sessionsBatchDeleteFailed", { count: failedCount }));
    else onNotice(provider.id === "cursor"
      ? t("sessionsCursorRestartRequired", { count: deleted.size })
      : t("sessionsBatchDeleted", { count: deleted.size }));
  };

  if (refreshing && !sessions.length) return <div className={styles.empty}><RefreshCw aria-hidden="true" className={styles.spinning} size={32} /><h2>{t("sessionsLoading")}</h2></div>;
  if (!sessions.length) return <div className={styles.empty}><MessageSquareText aria-hidden="true" size={32} /><h2>{t("sessionsEmptyTitle")}</h2><p>{t("sessionsEmptyDescription", { application: provider.label })}</p></div>;

  return <>
    <div className={styles.sessionsLayout}>
      <div className={styles.sessionPane}>
        <header className={styles.sessionToolbar}>
          {searchOpen ? <div className={styles.sessionSearch}><Search aria-hidden="true" size={15} /><input autoFocus onChange={(event) => setSearch(event.target.value)} onKeyDown={(event) => { if (event.key === "Escape") { setSearch(""); setSearchOpen(false); } }} placeholder={t("searchSessions")} value={search} /><button aria-label={t("close")} onClick={() => { setSearch(""); setSearchOpen(false); }} type="button"><X aria-hidden="true" size={15} /></button></div> : <><div className={styles.sessionToolbarTitle}><strong>{t("sessionsTitle")}</strong><span>{visibleSessions.length}</span></div><div className={styles.sessionToolbarActions}><Tooltip content={t("collapseSessionProjects")}><button aria-label={t("collapseSessionProjects")} onClick={() => setExpandedProjects(new Set())} type="button"><ChevronsDownUp aria-hidden="true" size={16} /></button></Tooltip>{canDelete && <Tooltip content={selectionMode ? t("exitSessionBatch") : t("manageSessionBatch")}><button aria-pressed={selectionMode} className={selectionMode ? styles.sessionToolbarActive : undefined} onClick={() => setSelectionMode((active) => !active)} type="button"><CheckSquare aria-hidden="true" size={16} /></button></Tooltip>}<Tooltip content={t("searchSessions")}><button onClick={() => setSearchOpen(true)} type="button"><Search aria-hidden="true" size={16} /></button></Tooltip></div></>}
        </header>
        {canDelete && selectionMode && <div className={styles.sessionBatchBar}><span>{t("sessionsSelected", { count: selectedIds.size })}</span><button onClick={() => setSelectedIds((current) => toggleSelectedIds(current, visibleSessions.map((session) => session.id)))} type="button">{visibleSessions.every((session) => selectedIds.has(session.id)) ? t("sessionsClearAll") : t("sessionsSelectAll")}</button><button onClick={() => setSelectedIds(new Set())} type="button">{t("sessionsClearSelection")}</button><button className={styles.sessionBatchDelete} disabled={!selectedIds.size || deleting} onClick={() => setDeleteTargets([...selectedIds])} type="button"><Trash2 aria-hidden="true" size={14} />{deleting ? t("sessionsDeleting") : t("sessionsDeleteSelected")}</button></div>}
        <div className={styles.sessionProjects}>{[...projects].map(([project, projectSessions]) => { const expanded = expandedProjects.has(project); const label = project === "__unknown__" ? t("sessionsUnknownProject") : (project.split("/").filter(Boolean).at(-1) ?? project); const allSelected = projectSessions.every((session) => selectedIds.has(session.id)); return <section className={styles.sessionProject} key={project}><div className={styles.sessionProjectHeader}>{canDelete && selectionMode && <input aria-label={t("selectSessionProject", { project: label })} checked={allSelected} onChange={(event) => setSelectedIds((current) => { const next = new Set(current); projectSessions.forEach((session) => event.target.checked ? next.add(session.id) : next.delete(session.id)); return next; })} type="checkbox" />}<button aria-expanded={expanded} aria-label={t("toggleSessionProject", { project: label })} className={styles.sessionProjectTrigger} onClick={() => setExpandedProjects((current) => { const next = new Set(current); next.has(project) ? next.delete(project) : next.add(project); return next; })} type="button">{expanded ? <ChevronDown aria-hidden="true" size={15} /> : <ChevronRight aria-hidden="true" size={15} />}<FolderOpen aria-hidden="true" size={16} /><span>{label}</span><small className={styles.sessionProjectCount}>{projectSessions.length}</small></button></div>{expanded && <div className={styles.sessionList}>{projectSessions.map((session) => <div className={`${styles.sessionCard} ${session.id === selectedId ? styles.sessionCardActive : ""}`} key={session.id}>{canDelete && selectionMode && <input aria-label={t("selectSession", { session: session.title })} checked={selectedIds.has(session.id)} onChange={(event) => setSelectedIds((current) => { const next = new Set(current); event.target.checked ? next.add(session.id) : next.delete(session.id); return next; })} type="checkbox" />}<button aria-current={session.id === selectedId ? "page" : undefined} onClick={() => setSelectedId(session.id)} type="button"><strong>{session.title}</strong><span>{formatRelativeSessionTime(session.updatedAt, t)}</span></button></div>)}</div>}</section>; })}</div>
      </div>
      <section className={styles.sessionDetail}>{!selectedSession ? <div className={styles.sessionDetailEmpty}><MessageSquareText aria-hidden="true" size={32} /><p>{t("sessionsSelect")}</p></div> : <><header className={styles.sessionDetailHeader}><div className={styles.sessionDetailTop}><h2>{selectedSession.title}</h2><div className={styles.sessionDetailActions}>{provider.launch && <Tooltip content={t("launchSession")}><button aria-label={t("launchSession")} onClick={() => void provider.launch?.(selectedSession.id).catch(onError)} type="button"><Play aria-hidden="true" size={17} /></button></Tooltip>}{provider.remove && <Tooltip content={t("deleteSession")}><button aria-label={t("deleteSession")} onClick={() => setDeleteTargets([selectedSession.id])} type="button"><Trash2 aria-hidden="true" size={17} /></button></Tooltip>}</div></div><div className={styles.sessionDetailMeta}><Clock3 aria-hidden="true" size={13} /><span>{new Date(selectedSession.updatedAt).toLocaleString()}</span>{selectedSession.projectDir && <><FolderOpen aria-hidden="true" size={13} /><span>{selectedSession.projectDir.split("/").filter(Boolean).at(-1)}</span></>}</div><dl className={styles.sessionDetailFields}><div><dt>{t("sessionsSourcePath")}</dt><dd><code>{selectedSession.sourcePath}</code><Tooltip content={t("copy")}><button aria-label={t("copy")} onClick={() => void copy(selectedSession.sourcePath)} type="button"><Copy aria-hidden="true" size={14} /></button></Tooltip></dd></div>{provider.launch && <div><dt>{t("sessionsResumeCommand")}</dt><dd><code>{`codex resume ${selectedSession.id}`}</code><Tooltip content={t("copy")}><button aria-label={t("copy")} onClick={() => void copy(`codex resume ${selectedSession.id}`)} type="button"><Copy aria-hidden="true" size={14} /></button></Tooltip></dd></div>}</dl></header><div className={styles.sessionMessages}>{messagesLoading ? <div className={styles.sessionDetailEmpty}><RefreshCw aria-hidden="true" className={styles.spinning} size={24} /><p>{t("sessionsMessagesLoading")}</p></div> : messages.length ? messages.map((message, index) => <article className={`${styles.sessionMessage} ${message.role === "user" ? styles.sessionMessageUser : styles.sessionMessageAssistant}`} key={`${message.timestamp ?? index}-${index}`}><header>{message.role === "user" ? <strong>{t("sessionsRoleUser")}</strong> : <img alt={provider.label} className={styles.sessionMessageAppIcon} src={provider.icon} />} {message.timestamp && <time>{new Date(message.timestamp).toLocaleString()}</time>}</header><p>{message.content}</p></article>) : <div className={styles.sessionDetailEmpty}><p>{t("sessionsMessagesEmpty")}</p></div>}</div></>}</section>
    </div>
    <AlertDialog.Root onOpenChange={(open) => { if (!open && !deleting) setDeleteTargets(undefined); }} open={Boolean(deleteTargets)}><AlertDialog.Portal><AlertDialog.Overlay className={styles.dialogOverlay} /><AlertDialog.Content className={styles.dialogContent}><AlertDialog.Title>{t("sessionsBatchDeleteTitle")}</AlertDialog.Title><AlertDialog.Description>{t("sessionsBatchDeleteConfirm", { count: deleteTargets?.length ?? 0 })}</AlertDialog.Description><div className={styles.dialogActions}><AlertDialog.Cancel asChild><button className={styles.dialogCancel} disabled={deleting} type="button">{t("cancel")}</button></AlertDialog.Cancel><AlertDialog.Action asChild><button autoFocus className={styles.dialogConfirm} disabled={deleting} onClick={(event) => { event.preventDefault(); void deleteSessions(); }} type="button">{deleting ? t("sessionsDeleting") : t("sessionsDeleteSelected")}</button></AlertDialog.Action></div></AlertDialog.Content></AlertDialog.Portal></AlertDialog.Root>
  </>;
});
