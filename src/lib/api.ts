import { invoke } from "@tauri-apps/api/core";
import type { Account, ApplicationKind, ApplicationStatus } from "./types";

export const listApplications = () => invoke<ApplicationStatus[]>("list_applications");
export const listAccounts = (kind: ApplicationKind) => invoke<Account[]>("list_accounts", { kind });
