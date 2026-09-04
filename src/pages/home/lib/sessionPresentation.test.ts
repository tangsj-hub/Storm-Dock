import { describe, expect, it } from "vitest";
import type { LocalSession } from "../../../lib/types";
import { filterSessions, groupSessions, removeSelectedIds, toggleSelectedIds, uniqueSessions } from "./sessionPresentation";

const sessions: LocalSession[] = [
  { id: "one", title: "Fix login", projectDir: "/work/dock", sourcePath: "/one.jsonl", updatedAt: 1 },
  { id: "two", title: "Review", projectDir: "/work/app", sourcePath: "/two.jsonl", updatedAt: 2 },
  { id: "three", title: "Untitled", sourcePath: "/three.jsonl", updatedAt: 3 },
];

describe("session presentation", () => {
  it("filters title, project path, and id without mutating source data", () => {
    expect(filterSessions(sessions, "DOCK").map((session) => session.id)).toEqual(["one"]);
    expect(filterSessions(sessions, "two").map((session) => session.id)).toEqual(["two"]);
    expect(sessions).toHaveLength(3);
  });

  it("keeps the first copy when Cursor mirrors a session into empty-window", () => {
    const duplicated: LocalSession[] = [
      { id: "one", title: "Fix login", projectDir: "/work/dock", sourcePath: "/one.jsonl", updatedAt: 2 },
      { id: "one", title: "Fix login", projectDir: "empty-window", sourcePath: "/empty.jsonl", updatedAt: 3 },
      { id: "two", title: "Review", projectDir: "/work/app", sourcePath: "/two.jsonl", updatedAt: 1 },
    ];
    expect(uniqueSessions(duplicated).map((session) => session.projectDir)).toEqual(["/work/dock", "/work/app"]);
  });

  it("groups missing projects and toggles visible selection", () => {
    expect([...groupSessions(sessions).keys()]).toEqual(["/work/dock", "/work/app", "__unknown__"]);
    expect([...toggleSelectedIds(new Set(["one"]), ["one", "two"])]).toEqual(["one", "two"]);
    expect([...toggleSelectedIds(new Set(["one", "two"]), ["one", "two"])]).toEqual([]);
  });

  it("removes deleted session ids from the batch selection", () => {
    expect([...removeSelectedIds(new Set(["one", "two"]), new Set(["two"]))]).toEqual(["one"]);
  });
});
