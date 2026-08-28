import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, FolderSync, Languages, PanelTop, Power } from "lucide-react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import i18n from "../../i18n";
import { getDatabasePath, moveDatabase } from "../../lib/api";
import "../../styles/global.css";
import styles from "./page.module.css";

const languages = [
  { code: "zh", key: "chinese" },
  { code: "en", key: "english" }
] as const;

function SettingsPage() {
  const { t } = useTranslation();
  const [language, setLanguage] = useState(i18n.language);
  const [notice, setNotice] = useState<string>();
  const [databasePath, setDatabasePath] = useState("");
  const [movingDatabase, setMovingDatabase] = useState(false);
  const [launchAtLogin, setLaunchAtLogin] = useState(false);
  const [closeToTray, setCloseToTray] = useState(() => localStorage.getItem("closeToTray") !== "false");
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
  const toggleLaunchAtLogin = async () => { try { if (launchAtLogin) await disable(); else await enable(); setLaunchAtLogin(!launchAtLogin); } catch (error) { setNotice(error instanceof Error ? error.message : String(error)); } };
  const toggleCloseToTray = () => { const next = !closeToTray; setCloseToTray(next); localStorage.setItem("closeToTray", String(next)); };
  const chooseDatabaseDirectory = async () => {
    const directory = await open({ directory: true, multiple: false, title: t("databaseChooseDirectory") });
    if (!directory) return;
    setMovingDatabase(true);
    try {
      setDatabasePath(await moveDatabase(directory));
      setNotice(t("databaseMoved"));
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setMovingDatabase(false);
    }
  };

  return <Toast.Provider><main className={styles.shell}>
    <header className={styles.header}><a aria-label={t("back")} className={styles.back} href="/">←</a><h1>{t("settingsTitle")}</h1></header>
    <section className={styles.workspace}>
      <div className={styles.sectionTitle}><PanelTop aria-hidden="true" size={20} /><h2>{t("windowBehavior")}</h2></div>
      <div className={styles.behaviorList}>
        <div className={styles.behaviorRow}><div className={styles.settingCopy}><span className={`${styles.icon} ${styles.powerIcon}`}><Power aria-hidden="true" size={20} /></span><div><h2>{t("launchAtLogin")}</h2><p>{t("launchAtLoginDescription")}</p></div></div><button aria-checked={launchAtLogin} className={styles.switch} onClick={() => void toggleLaunchAtLogin()} role="switch" type="button"><span /></button></div>
        <div className={styles.behaviorRow}><div className={styles.settingCopy}><span className={`${styles.icon} ${styles.windowIcon}`}><PanelTop aria-hidden="true" size={20} /></span><div><h2>{t("closeToTray")}</h2><p>{t("closeToTrayDescription")}</p></div></div><button aria-checked={closeToTray} className={styles.switch} onClick={toggleCloseToTray} role="switch" type="button"><span /></button></div>
      </div>
      <div className={styles.settingRow}><div className={styles.settingCopy}><span className={styles.icon}><Languages aria-hidden="true" size={20} /></span><div><h2>{t("language")}</h2><p>{t("languageDescription")}</p></div></div>
        <DropdownMenu.Root><DropdownMenu.Trigger className={styles.languageTrigger}><span>{t(current.key)}</span><ChevronDown aria-hidden="true" size={16} /></DropdownMenu.Trigger><DropdownMenu.Portal><DropdownMenu.Content align="end" className={styles.menu} sideOffset={6}>
          {languages.map((item) => <DropdownMenu.Item className={styles.menuItem} key={item.code} onSelect={() => void selectLanguage(item.code)}><span>{t(item.key)}</span>{item.code === language && <Check aria-hidden="true" size={16} />}</DropdownMenu.Item>)}
        </DropdownMenu.Content></DropdownMenu.Portal></DropdownMenu.Root>
      </div>
      <div className={styles.settingRow}><div className={styles.settingCopy}><span className={styles.icon}><FolderSync aria-hidden="true" size={20} /></span><div><h2>{t("database")}</h2><p className={styles.databasePath}>{databasePath || t("databaseLoading")}</p></div></div>
        <button className={styles.databaseButton} disabled={movingDatabase} onClick={() => void chooseDatabaseDirectory()} type="button">{t("databaseMove")}</button>
      </div>
    </section>
  </main><ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} /><Toast.Viewport className={styles.toastViewport} /></Toast.Provider>;
}

createRoot(document.getElementById("root")!).render(<SettingsPage />);
