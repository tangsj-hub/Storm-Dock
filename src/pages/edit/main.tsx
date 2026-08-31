import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, KeyRound } from "lucide-react";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import "../../i18n";
import { applicationKindFromQuery, homePath } from "../../lib/types";
import "../../styles/global.css";
import styles from "../add/page.module.css";

type CodexApiKeyAccount = { label: string; apiKey: string; baseUrl?: string };

function EditPage() {
  const { t } = useTranslation();
  const kind = applicationKindFromQuery();
  const id = new URLSearchParams(window.location.search).get("id") ?? "";
  const home = homePath(kind);
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string>();
  const showError = (error: unknown) => setNotice(error instanceof Error ? error.message : String(error));

  useEffect(() => {
    if (!id) {
      window.location.replace(home);
      return;
    }
    void invoke<CodexApiKeyAccount>("get_codex_api_key_account", { id })
      .then((record) => {
        setApiKey(record.apiKey);
        setBaseUrl(record.baseUrl ?? "");
        setNote(record.label);
      })
      .catch(showError);
  }, [home, id]);

  const save = async () => {
    setBusy(true);
    setNotice(undefined);
    try {
      await invoke("update_codex_api_key_account", {
        id,
        apiKey: apiKey.trim(),
        baseUrl: baseUrl.trim() || null,
        label: note.trim() || null,
      });
      window.location.assign(homePath(kind, t("apiKeyUpdated")));
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Toast.Provider>
      <main className={styles.shell}>
        <header className={styles.header}>
          <a aria-label={t("back")} className={styles.back} href={home}><ArrowLeft aria-hidden="true" size={20} /></a>
          <h1>{t("editApiKeyTitle")}</h1>
        </header>
        <section className={styles.workspace}>
          <form className={`${styles.content} ${styles.panel}`} onSubmit={(event) => { event.preventDefault(); void save(); }}>
            <div className={styles.heading}>
              <KeyRound aria-hidden="true" size={22} />
              <div>
                <h2>{t("editApiKeyTitle")}</h2>
                <p>{t("apiKeyImportDescription")}</p>
              </div>
            </div>
            <div className={styles.credentialBlock}>
              <label>{t("apiKeyField")}<input autoFocus autoComplete="off" onChange={(event) => setApiKey(event.target.value)} spellCheck={false} type="text" value={apiKey} /></label>
              <label>{t("baseUrlField")}<input autoComplete="off" onChange={(event) => setBaseUrl(event.target.value)} placeholder={t("baseUrlOptional")} spellCheck={false} type="url" value={baseUrl} /></label>
              <label>{t("accountNote")}<input autoComplete="off" onChange={(event) => setNote(event.target.value)} type="text" value={note} /></label>
            </div>
            <div className={styles.actions}>
              <a className={styles.secondary} href={home}>{t("cancel")}</a>
              <button className={styles.primary} disabled={busy || !apiKey.trim()} type="submit">{t("save")}</button>
            </div>
          </form>
        </section>
      </main>
      <ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} />
      <Toast.Viewport className={styles.toastViewport} />
    </Toast.Provider>
  );
}

createRoot(document.getElementById("root")!).render(<EditPage />);
