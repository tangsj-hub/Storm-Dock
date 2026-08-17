import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import * as Tabs from "@radix-ui/react-tabs";
import { ArrowLeft, ChevronDown, ExternalLink, KeyRound, LogIn, RefreshCw } from "lucide-react";
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
type LoginStage = "started" | "waiting" | "importing";

function isCancelledLogin(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  return message.includes("登录已取消") || message.toLowerCase().includes("cancelled");
}

const TOKEN_EXAMPLES = [
  { key: "tokenExampleSession", sample: "user_01XXXXXXXX::eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..." },
  { key: "tokenExampleJwt", sample: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..." },
  { key: "tokenExampleJson", sample: "{\n  \"access_token\": \"eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...\",\n  \"refresh_token\": \"optional-refresh-token\",\n  \"email\": \"you@example.com\"\n}" }
] as const;

function AddPage() {
  const { t } = useTranslation();
  const [applications, setApplications] = useState<ApplicationStatus[]>([]);
  const [method, setMethod] = useState<AddMethod>("login");
  const [payload, setPayload] = useState("");
  const [busy, setBusy] = useState(false);
  const [loginStage, setLoginStage] = useState<LoginStage>();
  const [loginUrl, setLoginUrl] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const current = applications.find((application) => application.kind === "cursor");
  const showError = (error: unknown) => setNotice(error instanceof Error ? error.message : String(error));

  useEffect(() => { void listApplications().then(setApplications).catch(showError); }, []);
  useEffect(() => {
    let unlisten = () => {};
    void listen<{ stage: LoginStage; loginUrl?: string }>("official-login-status", (event) => {
      setLoginStage(event.payload.stage);
      if (event.payload.loginUrl) setLoginUrl(event.payload.loginUrl);
    }).then((stop) => { unlisten = stop; });
    return () => {
      unlisten();
      void invoke("cancel_official_login");
    };
  }, []);

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
  const startOfficialLogin = () => act(async () => {
    try {
      const account = await invoke<{ label: string }>("start_official_login", { kind: "cursor", label: null });
      returnWithNotice(t("imported", { account: account.label }));
    } catch (error) {
      setLoginStage(undefined);
      if (isCancelledLogin(error)) {
        setNotice(t("officialLoginCancelled"));
        return;
      }
      throw error;
    }
  });
  const cancelOfficialLogin = () => { void invoke("cancel_official_login"); };
  const reopenOfficialLogin = () => { void invoke("open_official_login_url").catch(showError); };
  const importPayload = () => act(async () => {
    const account = await invoke<{ label: string }>("import_token_or_json", { kind: "cursor", label: null, payload });
    returnWithNotice(t("imported", { account: account.label }));
  });

  return <Toast.Provider><main className={styles.shell}>
    <header className={styles.header}><a aria-label={t("back")} className={styles.back} href="/"><ArrowLeft aria-hidden="true" size={20} /></a><div><p>{t("accounts")}</p><h1>{t("addAccountTitle")}</h1></div></header>
    <section className={styles.workspace}>
      <Tabs.Root className={styles.addPage} onValueChange={(value) => setMethod(value as AddMethod)} value={method}>
        <Tabs.List className={styles.tabs} aria-label={t("addAccountTitle")}>
          <Tabs.Trigger className={styles.tab} value="login"><LogIn aria-hidden="true" size={17} />{t("officialLogin")}</Tabs.Trigger>
          <Tabs.Trigger className={styles.tab} value="current"><RefreshCw aria-hidden="true" size={17} />{t("importCurrent")}</Tabs.Trigger>
          <Tabs.Trigger className={styles.tab} value="token"><KeyRound aria-hidden="true" size={17} />{t("tokenImport")}</Tabs.Trigger>
        </Tabs.List>
        <Tabs.Content className={styles.content} value="login"><section className={styles.panel}><div className={styles.heading}><LogIn aria-hidden="true" size={22} /><div><h2>{t("officialLoginTitle")}</h2><p>{t("officialLoginDescription")}</p></div></div>{loginStage ? <p className={styles.status}>{loginStage === "importing" ? t("officialLoginImporting") : t("officialLoginWaiting")}</p> : null}{loginStage ? <div className={styles.actions}><button className={styles.secondary} disabled={!loginUrl || loginStage === "importing"} onClick={reopenOfficialLogin} type="button"><ExternalLink aria-hidden="true" size={16} />{t("officialLoginOpenBrowser")}</button><button className={styles.secondary} disabled={loginStage === "importing"} onClick={cancelOfficialLogin} type="button">{t("cancelLogin")}</button></div> : <button className={styles.primary} disabled={busy} onClick={startOfficialLogin} type="button"><LogIn aria-hidden="true" size={18} />{t("startLogin")}</button>}</section></Tabs.Content>
        <Tabs.Content className={styles.content} value="current"><section className={styles.panel}><div className={styles.heading}><RefreshCw aria-hidden="true" size={22} /><div><h2>{t("importCurrentTitle")}</h2><p>{current?.available ? t("importCurrentDescription") : current?.reason ?? t("cursorNotReady")}</p></div></div><button className={styles.primary} disabled={busy || !current?.available} onClick={importCurrent} type="button"><RefreshCw aria-hidden="true" size={18} />{t("importCurrent")}</button></section></Tabs.Content>
        <Tabs.Content className={styles.content} value="token"><form className={styles.panel} onSubmit={(event) => { event.preventDefault(); void importPayload(); }}><div className={styles.heading}><KeyRound aria-hidden="true" size={22} /><div><h2>{t("tokenImportTitle")}</h2><p>{t("tokenImportDescription")}</p></div></div><div className={styles.credentialBlock}><details className={styles.examples}><summary><ChevronDown aria-hidden="true" size={16} />{t("tokenExampleTitle")}</summary><div className={styles.exampleList}>{TOKEN_EXAMPLES.map((example) => <div key={example.key}><p>{t(example.key)}</p><pre>{example.sample}</pre></div>)}</div></details><label>{t("credential")}<textarea autoFocus onChange={(event) => setPayload(event.target.value)} rows={8} value={payload} /></label></div><div className={styles.actions}><a className={styles.secondary} href="/">{t("cancel")}</a><button className={styles.primary} disabled={busy || !payload.trim()} type="submit">{t("import")}</button></div></form></Tabs.Content>
      </Tabs.Root>
    </section>
  </main><ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} /><Toast.Viewport className={styles.toastViewport} /></Toast.Provider>;
}

createRoot(document.getElementById("root")!).render(<AddPage />);
