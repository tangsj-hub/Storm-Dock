export type WorkspaceSection = "accounts" | "sessions" | "plugins" | "mcp";

export type SwitchProgress = {
  operationId: string;
  accountId: string;
  stage: string;
  percent: number;
  status: "running" | "waiting" | "success" | "error";
};
