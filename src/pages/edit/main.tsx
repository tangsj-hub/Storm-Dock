import { invoke } from "@tauri-apps/api/core";
import { Activity, ArrowLeft, Eye, EyeOff, Save } from "lucide-react";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import { Tooltip } from "../../components/Tooltip";
import { WindowDragSurface } from "../../components/WindowDragSurface";
import "../../i18n";
import { applicationKindFromQuery, homePath } from "../../lib/types";
import "../../styles/global.css";
import styles from "./page.module.css";

type CodexApiKeyAccount = { label: string; apiKey: string; baseUrl?: string };
type Probe = { ok: boolean; text: string };

function EditPage() {
  const { t } = useTranslation();
  const kind = applicationKindFromQuery();
  const id = new URLSearchParams(window.location.search).get("id") ?? "";
  const home = homePath(kind);
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [note, setNote] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [testing, setTesting] = useState(false);
  const [probe, setProbe] = useState<Probe>();
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const showError = (cause: unknown) => setNotice(cause instanceof Error ? cause.message : String(cause));

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
      .catch((cause) => setError(cause instanceof Error ? cause.message : String(cause)));
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
    } catch (cause) {
      showError(cause);
    } finally {
      setBusy(false);
    }
  };

  const test = async () => {
    setTesting(true);
    setProbe(undefined);
    try {
      const result = await invoke<{ success: boolean; message: string; responseTimeMs?: number }>("test_codex_api_key_account", {
        id,
        baseUrl: baseUrl.trim() || null,
      });
      setProbe({
        ok: result.success,
        text: result.success ? t("connectionOk", { ms: result.responseTimeMs ?? 0 }) : t("connectionFail", { error: result.message }),
      });
    } catch (cause) {
      setProbe({ ok: false, text: t("connectionFail", { error: cause instanceof Error ? cause.message : String(cause) }) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <Toast.Provider>
      <form className={styles.shell} onSubmit={(event) => { event.preventDefault(); void save(); }}>
        <WindowDragSurface />
        <header className={styles.header}>
          <a aria-label={t("back")} className={styles.back} href={home}><ArrowLeft aria-hidden="true" size={20} /></a>
          <h1>{t("editApiKeyTitle")}</h1>
        </header>
        <div className={styles.body}>
          <div className={styles.fields}>
            {error ? <p className={styles.error}>{error}</p> : null}
            <label className={styles.field}><span>{t("accountNote")}</span><input autoComplete="off" onChange={(event) => setNote(event.target.value)} type="text" value={note} /></label>
            <div className={styles.field}>
              <span>{t("apiKeyField")}</span>
              <div className={styles.secret}>
                <input aria-label={t("apiKeyField")} autoComplete="off" autoFocus onChange={(event) => setApiKey(event.target.value)} spellCheck={false} type={showKey ? "text" : "password"} value={apiKey} />
                <Tooltip content={t(showKey ? "hideApiKey" : "showApiKey")}>
                  <button aria-label={t(showKey ? "hideApiKey" : "showApiKey")} className={styles.toggle} onClick={() => setShowKey((value) => !value)} type="button">
                    {showKey ? <EyeOff aria-hidden="true" size={16} /> : <Eye aria-hidden="true" size={16} />}
                  </button>
                </Tooltip>
              </div>
            </div>
            <div className={styles.field}>
              <span>{t("baseUrlField")}</span>
              <div className={styles.endpoint}>
                <input aria-label={t("baseUrlField")} autoComplete="off" onChange={(event) => { setBaseUrl(event.target.value); setProbe(undefined); }} placeholder={t("baseUrlOptional")} spellCheck={false} type="url" value={baseUrl} />
                <button className={styles.probe} disabled={testing} onClick={() => void test()} type="button">
                  <Activity aria-hidden="true" className={testing ? styles.spinning : undefined} size={16} />
                  {t("testConnection")}
                </button>
              </div>
              {probe ? <p className={`${styles.result} ${probe.ok ? styles.ok : styles.fail}`}>{probe.text}</p> : null}
            </div>
          </div>
        </div>
        <div className={styles.footer}>
          <div className={styles.actions}>
            <a className={styles.cancel} href={home}>{t("cancel")}</a>
            <button className={styles.save} disabled={busy || !apiKey.trim()} type="submit"><Save aria-hidden="true" size={16} />{t("save")}</button>
          </div>
        </div>
      </form>
      <ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} />
      <Toast.Viewport className={styles.toastViewport} />
    </Toast.Provider>
  );
}

createRoot(document.getElementById("root")!).render(<EditPage />);
