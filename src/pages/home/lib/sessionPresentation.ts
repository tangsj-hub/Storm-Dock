import type { CodexSession } from "../../../lib/types";
import type { Translate } from "./accountPresentation";

export const UNKNOWN_PROJECT = "__unknown__";

export function formatRelativeSessionTime(timestamp: number, t: Translate) {
  const elapsed = Math.max(0, Date.now() - timestamp);
  const minutes = Math.floor(elapsed / 60_000);
  const hours = Math.floor(elapsed / 3_600_000);
  const days = Math.floor(elapsed / 86_400_000);
  if (minutes < 1) return t("sessionsJustNow");
  if (minutes < 60) return t("sessionsMinutesAgo", { count: minutes });
  if (hours < 24) return t("sessionsHoursAgo", { count: hours });
  if (days < 7) return t("sessionsDaysAgo", { count: days });
  return new Date(timestamp).toLocaleDateString();
}

export function filterSessions(sessions: CodexSession[], query: string) {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return sessions;
  return sessions.filter((session) =>
    [session.title, session.projectDir, session.id]
      .filter((value): value is string => Boolean(value))
      .some((value) => value.toLocaleLowerCase().includes(normalized)),
  );
}

export function groupSessions(sessions: CodexSession[]) {
  const groups = new Map<string, CodexSession[]>();
  for (const session of sessions) {
    const project = session.projectDir?.trim() || UNKNOWN_PROJECT;
    groups.set(project, [...(groups.get(project) ?? []), session]);
  }
  return groups;
}

export function toggleSelectedIds(current: Set<string>, ids: string[]) {
  return ids.every((id) => current.has(id))
    ? new Set([...current].filter((id) => !ids.includes(id)))
    : new Set([...current, ...ids]);
}

export function removeSelectedIds(current: Set<string>, deleted: Set<string>) {
  return new Set([...current].filter((id) => !deleted.has(id)));
}
