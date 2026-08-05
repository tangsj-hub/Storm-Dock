import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, Languages } from "lucide-react";
import { useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import i18n from "../../i18n";
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
  const current = languages.find((item) => item.code === language) ?? languages[0];
  const selectLanguage = async (code: "zh" | "en") => {
    localStorage.setItem("language", code);
    await i18n.changeLanguage(code);
    setLanguage(code);
    setNotice(t("languageSaved", { language: t(languages.find((item) => item.code === code)?.key ?? "chinese") }));
  };

  return <Toast.Provider><main className={styles.shell}>
    <header className={styles.header}><a className={styles.back} href="/" title={t("back")}>←</a><div><p>{t("appName")}</p><h1>{t("settingsTitle")}</h1></div></header>
    <section className={styles.workspace}>
      <div className={styles.settingRow}><div className={styles.settingCopy}><span className={styles.icon}><Languages aria-hidden="true" size={20} /></span><div><h2>{t("language")}</h2><p>{t("languageDescription")}</p></div></div>
        <DropdownMenu.Root><DropdownMenu.Trigger className={styles.languageTrigger}><span>{t(current.key)}</span><ChevronDown aria-hidden="true" size={16} /></DropdownMenu.Trigger><DropdownMenu.Portal><DropdownMenu.Content align="end" className={styles.menu} sideOffset={6}>
          {languages.map((item) => <DropdownMenu.Item className={styles.menuItem} key={item.code} onSelect={() => void selectLanguage(item.code)}><span>{t(item.key)}</span>{item.code === language && <Check aria-hidden="true" size={16} />}</DropdownMenu.Item>)}
        </DropdownMenu.Content></DropdownMenu.Portal></DropdownMenu.Root>
      </div>
    </section>
  </main><ToastMessage notice={notice} onOpenChange={(open) => { if (!open) setNotice(undefined); }} /><Toast.Viewport className={styles.toastViewport} /></Toast.Provider>;
}

createRoot(document.getElementById("root")!).render(<SettingsPage />);
