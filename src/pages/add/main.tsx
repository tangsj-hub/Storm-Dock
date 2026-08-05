import { invoke } from "@tauri-apps/api/core";
import * as Tabs from "@radix-ui/react-tabs";
import { ArrowLeft, KeyRound, LogIn, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import "../../i18n";
import { listApplications } from "../../lib/api";
import type { ApplicationStatus } from "../../lib/types";
import "../../styles/global.css";
import styles from "./page.module.css";

type AddMethod = "login" | "current" | "token";

function AddPage() {
  const { t } = useTranslation();
  const [applications, setApplications] = useState<ApplicationStatus[]>([]);
  const [method, setMethod] = useState<AddMethod>("login");
  const [payload, setPayload] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string>();
  const current = applications.find((application) => application.kind === "cursor");
  const showError = (error: unknown) => setNotice(error instanceof Error ? error.message : String(error));

  useEffect(() => { void listApplications().then(setApplications).catch(showError); }, []);

  const act = async (work: () => Promise<void>) => {
    setBusy(true);
    setNotice(undefined);
    try { await work(); return true; } catch (error) { showError(error); return false; } finally { setBusy(false); }
  };
  const returnWithNotice = (message: string) => window.location.assign(`/?notice=${encodeURIComponent(message)}`);
  const importCurrent = () => act(async () => {
    const account = await invoke<{ label: string }>("import_current_account", { kind: "cursor", label: null });
    returnWithNotice(t("imported", { account: account.label }));
  });
  const startOfficialLogin = () => act(async () => { setNotice(await invoke<string>("start_official_login", { kind: "cursor" })); });
  const importPayload = () => act(async () => {
    const account = await invoke<{ label: string }>("import_token_or_json", { kind: "cursor", label: null, payload });
    returnWithNotice(t("imported", { account: account.label }));
  });

  return <Toast.Provider><main className={styles.shell}>
    <header className={styles.header}><a className={styles.back} href="/" title={t("back")}><ArrowLeft aria-hidden="true" size={20} /></a><div><p>{t("accounts")}</p><h1>{t("addAccountTitle")}</h1></div></header>
    <section className={styles.workspace}>
      <Tabs.Root className={styles.addPage} onValueChange={(value) => setMethod(value as AddMethod)} value={method}>
        <Tabs.List className={styles.tabs} aria-label={t("addAccountTitle")}>
          <Tabs.Trigger className={styles.tab} value="login"><LogIn aria-hidden="true" size={17} />{t("officialLogin")}</Tabs.Trigger>
          <Tabs.Trigger className={styles.tab} value="current"><RefreshCw aria-hidden="true" size={17} />{t("importCurrent")}</Tabs.Trigger>
          <Tabs.Trigger className={styles.tab} value="token"><KeyRound aria-hidden="true" size={17} />{t("tokenImport")}</Tabs.Trigger>
        </Tabs.List>
        <Tabs.Content className={styles.content} value="login"><section className={styles.panel}><div className={styles.heading}><LogIn aria-hidden="true" size={22} /><div><h2>{t("officialLoginTitle")}</h2><p>{t("officialLoginDescription")}</p></div></div><button className={styles.primary} disabled={busy} onClick={startOfficialLogin} type="button"><LogIn aria-hidden="true" size={18} />{t("startLogin")}</button></section></Tabs.Content>
        <Tabs.Content className={styles.content} value="current"><section className={styles.panel}><div className={styles.heading}><RefreshCw aria-hidden="true" size={22} /><div><h2>{t("importCurrentTitle")}</h2><p>{current?.available ? t("importCurrentDescription") : current?.reason ?? t("cursorNotReady")}</p></div></div><button className={styles.primary} disabled={busy || !current?.available} onClick={importCurrent} type="button"><RefreshCw aria-hidden="true" size={18} />{t("importCurrent")}</button></section></Tabs.Content>
        <Tabs.Content className={styles.content} value="token"><form className={styles.panel} onSubmit={(event) => { event.preventDefault(); void importPayload(); }}><div className={styles.heading}><KeyRound aria-hidden="true" size={22} /><div><h2>{t("tokenImportTitle")}</h2><p>{t("tokenImportDescription")}</p></div></div><label>{t("credential")}<textarea autoFocus onChange={(event) => setPayload(event.target.value)} rows={8} value={payload} /></label><div className={styles.actions}><a className={styles.secondary} href="/">{t("cancel")}</a><button className={styles.primary} disabled={busy || !payload.trim()} type="submit">{t("import")}</button></div></form></Tabs.Content>
      </Tabs.Root>
    </section>
  </main><ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} /><Toast.Viewport className={styles.toastViewport} /></Toast.Provider>;
}

createRoot(document.getElementById("root")!).render(<AddPage />);
