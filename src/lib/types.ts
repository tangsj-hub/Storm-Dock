export type ApplicationKind = "cursor" | "codex";

export type ApplicationStatus = {
  kind: ApplicationKind;
  label: string;
  available: boolean;
  reason?: string;
};

export type Account = {
  id: string;
  label: string;
  importType: "oauth" | "token" | "jwt" | "native";
  isCurrent: boolean;
};
