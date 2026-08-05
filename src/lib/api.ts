import { invoke } from "@tauri-apps/api/core";
import type { Account, ApplicationKind, ApplicationStatus } from "./types";

export const listApplications = () => invoke<ApplicationStatus[]>("list_applications");
export const listAccounts = (kind: ApplicationKind) => invoke<Account[]>("list_accounts", { kind });
export const getDatabasePath = () => invoke<string>("get_database_path");
export const moveDatabase = (directory: string) => invoke<string>("move_database", { directory });
