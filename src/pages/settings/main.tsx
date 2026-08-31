import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import * as Tabs from "@radix-ui/react-tabs";
import { Check, ChevronDown, Database, FolderSync, KeyRound, Languages, PanelTop, Power } from "lucide-react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { open, save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import { WindowDragSurface } from "../../components/WindowDragSurface";
import i18n from "../../i18n";
import { exportDatabase, getDatabasePath, getPreserveCodexOfficialAuth, importDatabase, moveDatabase, setPreserveCodexOfficialAuth } from "../../lib/api";
import { applicationKindFromQuery, homePath } from "../../lib/types";
import { LocalEnvPanel } from "./LocalEnvPanel";
import "../../styles/global.css";
import styles from "./page.module.css";

const languages = [
  { code: "zh", key: "chinese" },
  { code: "en", key: "english" }
] as const;

const sqlFilters = [{ name: "SQL", extensions: ["sql"] }];

function SettingsPage() {
  const { t } = useTranslation();
  const [language, setLanguage] = useState(i18n.language);
  const [notice, setNotice] = useState<string>();
  const [noticeStatus, setNoticeStatus] = useState<"success" | "error">("success");
  const [databasePath, setDatabasePath] = useState("");
  const [databaseBusy, setDatabaseBusy] = useState(false);
  const [pendingImport, setPendingImport] = useState<string>();
  const [launchAtLogin, setLaunchAtLogin] = useState(false);
  const [closeToTray, setCloseToTray] = useState(() => localStorage.getItem("closeToTray") !== "false");
  const [preserveCodexAuth, setPreserveCodexAuth] = useState(true);
  useEffect(() => { void invoke("set_close_to_tray", { enabled: closeToTray }); }, [closeToTray]);
  const current = languages.find((item) => item.code === language) ?? languages[0];
  const selectLanguage = async (code: "zh" | "en") => {
    localStorage.setItem("language", code);
    await i18n.changeLanguage(code);
    setLanguage(code);
    setNotice(t("languageSaved", { language: t(languages.find((item) => item.code === code)?.key ?? "chinese") }));
  };
  useEffect(() => { void getDatabasePath().then(setDatabasePath).catch((error) => setNotice(String(error))); }, []);
  useEffect(() => { void isEnabled().then(setLaunchAtLogin).catch((error) => setNotice(String(error))); }, []);
  useEffect(() => { void getPreserveCodexOfficialAuth().then(setPreserveCodexAuth).catch((error) => setNotice(String(error))); }, []);
  const toggleLaunchAtLogin = async () => { try { if (launchAtLogin) await disable(); else await enable(); setLaunchAtLogin(!launchAtLogin); } catch (error) { setNotice(error instanceof Error ? error.message : String(error)); } };
  const toggleCloseToTray = () => { const next = !closeToTray; setCloseToTray(next); localStorage.setItem("closeToTray", String(next)); };
  const togglePreserveCodexAuth = async () => {
    const next = !preserveCodexAuth;
    try {
      await setPreserveCodexOfficialAuth(next);
      setPreserveCodexAuth(next);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    }
  };
  const showError = (error: unknown) => {
    setNoticeStatus("error");
    setNotice(error instanceof Error ? error.message : String(error));
  };
  const chooseDatabaseDirectory = async () => {
    const directory = await open({ directory: true, multiple: false, title: t("databaseChooseDirectory") });
    if (!directory) return;
    setDatabaseBusy(true);
    try {
      setDatabasePath(await moveDatabase(directory));
      setNoticeStatus("success");
      setNotice(t("databaseMoved"));
    } catch (error) {
      showError(error);
    } finally {
      setDatabaseBusy(false);
    }
  };
  const exportSql = async () => {
    const file = await save({ defaultPath: "storm-dock.sql", filters: sqlFilters, title: t("sqlExportTitle") });
    if (!file) return;
    setDatabaseBusy(true);
    try {
      await exportDatabase(file);
      setNoticeStatus("success");
      setNotice(t("sqlExported"));
    } catch (error) {
      showError(error);
    } finally {
      setDatabaseBusy(false);
    }
  };
  const chooseImportFile = async () => {
    const file = await open({ filters: sqlFilters, multiple: false, title: t("sqlChooseFile") });
    if (typeof file === "string") setPendingImport(file);
  };
  const confirmImport = async () => {
    const file = pendingImport;
    if (!file) return;
    setPendingImport(undefined);
    setDatabaseBusy(true);
    try {
      setDatabasePath(await importDatabase(file));
      setNoticeStatus("success");
      setNotice(t("sqlImported"));
    } catch (error) {
      showError(error);
    } finally {
      setDatabaseBusy(false);
    }
  };

  return <Toast.Provider><main className={styles.shell}>
    <WindowDragSurface />
    <header className={styles.header}><a aria-label={t("back")} className={styles.back} href={homePath(applicationKindFromQuery())}>←</a><h1>{t("settingsTitle")}</h1></header>
    <Tabs.Root className={styles.layout} defaultValue="general" orientation="vertical">
      <Tabs.List aria-label={t("settingsTabs")} className={styles.nav}>
        <Tabs.Trigger className={styles.tab} value="general">{t("settingsTabGeneral")}</Tabs.Trigger>
        <Tabs.Trigger className={styles.tab} value="data">{t("settingsTabData")}</Tabs.Trigger>
        <Tabs.Trigger className={styles.tab} value="local">{t("settingsTabLocal")}</Tabs.Trigger>
        <Tabs.Trigger className={styles.tab} value="about">{t("settingsTabAbout")}</Tabs.Trigger>
      </Tabs.List>
      <div className={styles.stage}>
      <Tabs.Content className={styles.pane} forceMount value="general">
        <div className={styles.stack}>
          <div className={styles.row}><div className={styles.settingCopy}><span className={styles.icon}><Languages aria-hidden="true" size={20} /></span><div><h2>{t("language")}</h2><p>{t("languageDescription")}</p></div></div>
            <DropdownMenu.Root><DropdownMenu.Trigger className={styles.languageTrigger}><span>{t(current.key)}</span><ChevronDown aria-hidden="true" size={16} /></DropdownMenu.Trigger><DropdownMenu.Portal><DropdownMenu.Content align="end" className={styles.menu} sideOffset={6}>
              {languages.map((item) => <DropdownMenu.Item className={styles.menuItem} key={item.code} onSelect={() => void selectLanguage(item.code)}><span>{t(item.key)}</span>{item.code === language && <Check aria-hidden="true" size={16} />}</DropdownMenu.Item>)}
            </DropdownMenu.Content></DropdownMenu.Portal></DropdownMenu.Root>
          </div>
          <div className={styles.sectionTitle}><PanelTop aria-hidden="true" size={20} /><h2>{t("windowBehavior")}</h2></div>
          <div className={styles.behaviorList}>
            <div className={styles.row}><div className={styles.settingCopy}><span className={`${styles.icon} ${styles.powerIcon}`}><Power aria-hidden="true" size={20} /></span><div><h2>{t("launchAtLogin")}</h2><p>{t("launchAtLoginDescription")}</p></div></div><button aria-checked={launchAtLogin} className={styles.switch} onClick={() => void toggleLaunchAtLogin()} role="switch" type="button"><span /></button></div>
            <div className={styles.row}><div className={styles.settingCopy}><span className={`${styles.icon} ${styles.windowIcon}`}><PanelTop aria-hidden="true" size={20} /></span><div><h2>{t("closeToTray")}</h2><p>{t("closeToTrayDescription")}</p></div></div><button aria-checked={closeToTray} className={styles.switch} onClick={toggleCloseToTray} role="switch" type="button"><span /></button></div>
          </div>
          <div className={styles.sectionTitle}><KeyRound aria-hidden="true" size={20} /><h2>{t("codexAppEnhancement")}</h2></div>
          <div className={styles.behaviorList}>
            <div className={styles.row}><div className={styles.settingCopy}><span className={styles.icon}><KeyRound aria-hidden="true" size={20} /></span><div><h2>{t("preserveCodexOfficialAuth")}</h2><p>{t("preserveCodexOfficialAuthDescription")}</p></div></div><button aria-checked={preserveCodexAuth} className={styles.switch} onClick={() => void togglePreserveCodexAuth()} role="switch" type="button"><span /></button></div>
          </div>
        </div>
      </Tabs.Content>
      <Tabs.Content className={styles.pane} forceMount value="data">
        <div className={styles.stack}>
          <div className={styles.row}><div className={styles.settingCopy}><span className={styles.icon}><Database aria-hidden="true" size={20} /></span><div><h2>{t("sqlBackup")}</h2><p>{t("sqlBackupDescription")}</p></div></div>
            <div className={styles.rowActions}>
              <button className={styles.databaseButton} disabled={databaseBusy} onClick={() => void exportSql()} type="button">{t("sqlExport")}</button>
              <button className={styles.databaseButton} disabled={databaseBusy} onClick={() => void chooseImportFile()} type="button">{t("sqlImport")}</button>
            </div>
          </div>
          <div className={styles.row}><div className={styles.settingCopy}><span className={styles.icon}><FolderSync aria-hidden="true" size={20} /></span><div><h2>{t("database")}</h2><p className={styles.databasePath}>{databasePath || t("databaseLoading")}</p></div></div>
            <button className={styles.databaseButton} disabled={databaseBusy} onClick={() => void chooseDatabaseDirectory()} type="button">{t("databaseMove")}</button>
          </div>
        </div>
      </Tabs.Content>
      <Tabs.Content className={styles.pane} value="local">
        <LocalEnvPanel onNotice={(message, status) => { setNoticeStatus(status ?? "success"); setNotice(message); }} />
      </Tabs.Content>
      <Tabs.Content className={styles.pane} forceMount value="about" />
      </div>
    </Tabs.Root>
  </main>
    <AlertDialog.Root onOpenChange={(open) => { if (!open) setPendingImport(undefined); }} open={Boolean(pendingImport)}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay className={styles.dialogOverlay} />
        <AlertDialog.Content className={styles.dialogContent}>
          <AlertDialog.Title>{t("sqlImportTitle")}</AlertDialog.Title>
          <AlertDialog.Description>{t("sqlImportConfirm")}</AlertDialog.Description>
          <div className={styles.dialogActions}>
            <AlertDialog.Cancel asChild><button className={styles.dialogCancel} type="button">{t("cancel")}</button></AlertDialog.Cancel>
            <AlertDialog.Action asChild><button autoFocus className={styles.danger} onClick={(event) => { event.preventDefault(); void confirmImport(); }} type="button">{t("sqlImport")}</button></AlertDialog.Action>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
    <ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} status={noticeStatus} /><Toast.Viewport className={styles.toastViewport} /></Toast.Provider>;
}

createRoot(document.getElementById("root")!).render(<SettingsPage />);
