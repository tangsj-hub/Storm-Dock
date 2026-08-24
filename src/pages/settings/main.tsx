import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, FolderSync, Languages } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
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
  const current = languages.find((item) => item.code === language) ?? languages[0];
  const selectLanguage = async (code: "zh" | "en") => {
    localStorage.setItem("language", code);
    await i18n.changeLanguage(code);
    setLanguage(code);
    setNotice(t("languageSaved", { language: t(languages.find((item) => item.code === code)?.key ?? "chinese") }));
  };
  useEffect(() => { void getDatabasePath().then(setDatabasePath).catch((error) => setNotice(String(error))); }, []);
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
