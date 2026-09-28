import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import * as Tabs from "@radix-ui/react-tabs";
import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors } from "@dnd-kit/core";
import { SortableContext, arrayMove, horizontalListSortingStrategy, sortableKeyboardCoordinates, useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { ArrowLeft, Check, ChevronDown, Database, Download, FolderSync, Globe, GripVertical, KeyRound, Languages, LayoutList, Monitor, PanelTop, Power, RefreshCw } from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { useTranslation } from "react-i18next";
import { ReleaseNotes } from "../../components/ReleaseNotes";
import { Toast, ToastMessage } from "../../components/ToastMessage";
import { WindowDragSurface } from "../../components/WindowDragSurface";
import i18n from "../../i18n";
import { exportDatabase, getDatabasePath, getPreserveCodexOfficialAuth, importDatabase, moveDatabase, setPreserveCodexOfficialAuth } from "../../lib/api";
import { getCloseBehavior, setCloseBehavior, type CloseBehavior } from "../../lib/closeBehavior";
import { getHomeTabs, resolvedHomePath, setHomeTabs, type HomeTabId, type HomeTabPref } from "../../lib/homeTabs";
import {
  ASK_BROWSER,
  LAST_BROWSER,
  getLoginBrowserPref,
  setLoginBrowserPref,
  type LoginBrowser,
} from "../../lib/loginBrowser";
import { getPreference, setPreference, type ThemePreference } from "../../lib/theme";
import { syncDocumentAppKind } from "../../lib/types";
import logo from "../../assets/logo.svg";
import cursorIcon from "../../assets/cursor.svg";
import codexIcon from "../../assets/codex.svg";
import grokIcon from "../../assets/tools/grok.svg";
import grokBotIcon from "../../assets/tools/grok-bot.png";
import { checkForAppUpdate, installUpdateAndRestart } from "../../lib/updater";
import { LocalEnvPanel } from "./LocalEnvPanel";
import "../../styles/global.css";
import styles from "./page.module.css";

const languages = [
  { code: "zh", key: "chinese" },
  { code: "en", key: "english" }
] as const;

const themes = [
  { code: "light", key: "themeLight" },
  { code: "dark", key: "themeDark" },
  { code: "system", key: "themeSystem" }
] as const;

const sqlFilters = [{ name: "SQL", extensions: ["sql"] }];
const aboutApps = [
  { icon: cursorIcon, nameKey: "cursor", detailKey: "aboutAppCursor" },
  { icon: codexIcon, nameKey: "codex", detailKey: "aboutAppCodex" },
  { icon: grokIcon, nameKey: "grok", detailKey: "aboutAppGrok" }
] as const;

const HOME_TAB_ICONS: Record<HomeTabId, string> = {
  cursor: cursorIcon,
  codex: codexIcon,
  grok: grokIcon,
  grokBot: grokBotIcon
};

function SortableHomeTab({
  lastVisible,
  onToggle,
  tab
}: {
  lastVisible: boolean;
  onToggle: (id: HomeTabId) => void;
  tab: HomeTabPref;
}) {
  const { t } = useTranslation();
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: tab.id });
  const label = t(tab.id);
  return <div className={`${styles.homeTabChip} ${tab.visible ? "" : styles.homeTabChipOff} ${isDragging ? styles.homeTabRowDragging : ""}`} ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition }}>
    <GripVertical aria-label={t("dragHomeTab", { tab: label })} className={styles.homeTabDrag} size={16} {...attributes} {...listeners} />
    <button aria-checked={tab.visible} className={styles.homeTabToggle} disabled={tab.visible && lastVisible} onClick={() => onToggle(tab.id)} role="switch" type="button">
      <img alt="" className={`${styles.homeTabIcon} ink`} src={HOME_TAB_ICONS[tab.id]} />
      <span>{label}</span>
    </button>
  </div>;
}

function HomeTabsSettings() {
  const { t } = useTranslation();
  const [tabs, setTabs] = useState(getHomeTabs);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }), useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }));
  const persist = (next: HomeTabPref[]) => setTabs(setHomeTabs(next));
  return <div className={`${styles.row} ${styles.homeTabsRow}`}>
    <div className={styles.settingCopy}><span className={styles.icon}><LayoutList aria-hidden="true" size={20} /></span><div><h2>{t("homeTabs")}</h2><p>{t("homeTabsDescription")}</p></div></div>
    <DndContext collisionDetection={closestCenter} onDragEnd={({ active, over }) => {
      if (!over || active.id === over.id) return;
      const from = tabs.findIndex((tab) => tab.id === active.id);
      const to = tabs.findIndex((tab) => tab.id === over.id);
      if (from < 0 || to < 0) return;
      persist(arrayMove(tabs, from, to));
    }} sensors={sensors}>
      <SortableContext items={tabs.map((tab) => tab.id)} strategy={horizontalListSortingStrategy}>
        <div className={styles.homeTabList}>
          {tabs.map((tab) => <SortableHomeTab key={tab.id} lastVisible={tabs.filter((item) => item.visible).length === 1} onToggle={(id) => persist(tabs.map((item) => item.id === id ? { ...item, visible: !item.visible } : item))} tab={tab} />)}
        </div>
      </SortableContext>
    </DndContext>
  </div>;
}

function LoginBrowserSettings() {
  const { t } = useTranslation();
  const [pref, setPref] = useState(getLoginBrowserPref);
  const [browsers, setBrowsers] = useState<LoginBrowser[]>([{ id: "default", name: "System default" }]);
  useEffect(() => {
    void invoke<LoginBrowser[]>("list_login_browsers")
      .then((items) => { if (items.length > 0) setBrowsers(items); })
      .catch(() => {});
  }, []);
  const modes = [
    { id: ASK_BROWSER, label: t("loginBrowserAsk") },
    { id: LAST_BROWSER, label: t("loginBrowserLast") }
  ];
  const browserLabel = (id: string) => id === "default" ? t("systemDefaultBrowser") : browsers.find((item) => item.id === id)?.name ?? id;
  const currentLabel = pref === ASK_BROWSER ? t("loginBrowserAsk") : pref === LAST_BROWSER ? t("loginBrowserLast") : browserLabel(pref);
  const select = (id: string) => {
    setLoginBrowserPref(id);
    setPref(id);
  };
  return <div className={styles.row}><div className={styles.settingCopy}><span className={styles.icon}><Globe aria-hidden="true" size={20} /></span><div><h2>{t("loginBrowser")}</h2><p>{t("loginBrowserDescription")}</p></div></div>
    <DropdownMenu.Root><DropdownMenu.Trigger className={styles.languageTrigger}><span>{currentLabel}</span><ChevronDown aria-hidden="true" size={16} /></DropdownMenu.Trigger><DropdownMenu.Portal><DropdownMenu.Content align="end" className={styles.menu} sideOffset={6}>
      {modes.map((item) => <DropdownMenu.Item className={styles.menuItem} key={item.id} onSelect={() => select(item.id)}><span>{item.label}</span>{item.id === pref && <Check aria-hidden="true" size={16} />}</DropdownMenu.Item>)}
      <DropdownMenu.Separator className={styles.menuSeparator} />
      {browsers.map((item) => <DropdownMenu.Item className={styles.menuItem} key={item.id} onSelect={() => select(item.id)}><span>{browserLabel(item.id)}</span>{item.id === pref && <Check aria-hidden="true" size={16} />}</DropdownMenu.Item>)}
    </DropdownMenu.Content></DropdownMenu.Portal></DropdownMenu.Root>
  </div>;
}

const SETTINGS_TABS = ["general", "data", "local", "about"] as const;

function settingsTabFromQuery(search = window.location.search) {
  const tab = new URLSearchParams(search).get("tab");
  return SETTINGS_TABS.includes(tab as (typeof SETTINGS_TABS)[number]) ? tab! : "general";
}

function SettingsPage() {
  const { t } = useTranslation();
  const [language, setLanguage] = useState(i18n.language);
  const [theme, setTheme] = useState<ThemePreference>(getPreference);
  const [notice, setNotice] = useState<string>();
  const [noticeStatus, setNoticeStatus] = useState<"success" | "error">("success");
  const [databasePath, setDatabasePath] = useState("");
  const [databaseBusy, setDatabaseBusy] = useState(false);
  const [pendingImport, setPendingImport] = useState<string>();
  const [launchAtLogin, setLaunchAtLogin] = useState(false);
  const [closeBehavior, setCloseBehaviorState] = useState<CloseBehavior>(() => getCloseBehavior());
  const [preserveCodexAuth, setPreserveCodexAuth] = useState(true);
  const [appVersion, setAppVersion] = useState("1.1.0");
  const [updateBusy, setUpdateBusy] = useState(false);
  const [updateInstalling, setUpdateInstalling] = useState(false);
  const [updateStatus, setUpdateStatus] = useState<"idle" | "up-to-date" | "available" | "error">("idle");
  const [updateVersion, setUpdateVersion] = useState<string>();
  const [updateNotes, setUpdateNotes] = useState<string>();
  const [updateError, setUpdateError] = useState<string>();
  useEffect(() => { setCloseBehavior(closeBehavior); }, [closeBehavior]);
  useEffect(() => {
    const onChanged = (event: Event) => {
      const detail = (event as CustomEvent<CloseBehavior>).detail;
      if (detail === "ask" || detail === "tray" || detail === "quit") setCloseBehaviorState(detail);
    };
    window.addEventListener("close-behavior-changed", onChanged);
    return () => window.removeEventListener("close-behavior-changed", onChanged);
  }, []);
  const current = languages.find((item) => item.code === language) ?? languages[0];
  const currentTheme = themes.find((item) => item.code === theme) ?? themes[2];
  const selectTheme = (pref: ThemePreference) => {
    setPreference(pref);
    setTheme(pref);
  };
  const selectLanguage = async (code: "zh" | "en") => {
    localStorage.setItem("language", code);
    await i18n.changeLanguage(code);
    setLanguage(code);
    setNotice(t("languageSaved", { language: t(languages.find((item) => item.code === code)?.key ?? "chinese") }));
  };
  useEffect(() => { void getDatabasePath().then(setDatabasePath).catch((error) => setNotice(String(error))); }, []);
  useEffect(() => { void invoke<boolean>("is_launch_at_login_enabled").then(setLaunchAtLogin).catch((error) => setNotice(String(error))); }, []);
  useEffect(() => { void getPreserveCodexOfficialAuth().then(setPreserveCodexAuth).catch((error) => setNotice(String(error))); }, []);
  useEffect(() => { void getVersion().then(setAppVersion).catch(() => setAppVersion("1.1.0")); }, []);
  const toggleLaunchAtLogin = async () => { try { if (launchAtLogin) await invoke("disable_launch_at_login"); else await invoke("enable_launch_at_login"); setLaunchAtLogin(!launchAtLogin); } catch (error) { setNotice(error instanceof Error ? error.message : String(error)); } };
  const closeBehaviorOptions = [
    { code: "ask" as const, key: "closeBehaviorAsk" },
    { code: "tray" as const, key: "closeBehaviorTray" },
    { code: "quit" as const, key: "closeBehaviorQuit" }
  ];
  const currentCloseBehavior = closeBehaviorOptions.find((item) => item.code === closeBehavior) ?? closeBehaviorOptions[0];
  const selectCloseBehavior = (code: CloseBehavior) => {
    setCloseBehaviorState(code);
    setCloseBehavior(code);
  };
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


  const checkUpdates = async () => {
    setUpdateBusy(true);
    setUpdateError(undefined);
    setUpdateStatus("idle");
    try {
      const result = await checkForAppUpdate();
      if (result.status === "up-to-date") {
        setUpdateStatus("up-to-date");
        setUpdateVersion(undefined);
        setUpdateNotes(undefined);
        setNoticeStatus("success");
        setNotice(t("updateUpToDate"));
      } else {
        setUpdateStatus("available");
        setUpdateVersion(result.version);
        setUpdateNotes(result.notes);
      }
    } catch (error) {
      setUpdateStatus("error");
      const message = error instanceof Error ? error.message : String(error);
      setUpdateError(message);
      setNoticeStatus("error");
      setNotice(t("updateCheckFailed", { error: message }));
    } finally {
      setUpdateBusy(false);
    }
  };
  const downloadAndInstallUpdate = async () => {
    setUpdateInstalling(true);
    setUpdateError(undefined);
    try {
      await installUpdateAndRestart();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setUpdateError(message);
      setUpdateStatus("error");
      setNoticeStatus("error");
      setNotice(t("updateInstallFailed", { error: message }));
      setUpdateInstalling(false);
    }
  };

  return <Toast.Provider><main className={styles.shell}>
    <WindowDragSurface />
    <header className={styles.header}><a aria-label={t("back")} className={styles.back} href={resolvedHomePath()}><ArrowLeft aria-hidden="true" size={20} /></a><h1>{t("settingsTitle")}</h1></header>
    <Tabs.Root className={styles.layout} defaultValue={settingsTabFromQuery()} orientation="vertical">
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
          <div className={styles.row}><div className={styles.settingCopy}><span className={styles.icon}><Monitor aria-hidden="true" size={20} /></span><div><h2>{t("theme")}</h2><p>{t("themeDescription")}</p></div></div>
            <DropdownMenu.Root><DropdownMenu.Trigger className={styles.languageTrigger}><span>{t(currentTheme.key)}</span><ChevronDown aria-hidden="true" size={16} /></DropdownMenu.Trigger><DropdownMenu.Portal><DropdownMenu.Content align="end" className={styles.menu} sideOffset={6}>
              {themes.map((item) => <DropdownMenu.Item className={styles.menuItem} key={item.code} onSelect={() => selectTheme(item.code)}><span>{t(item.key)}</span>{item.code === theme && <Check aria-hidden="true" size={16} />}</DropdownMenu.Item>)}
            </DropdownMenu.Content></DropdownMenu.Portal></DropdownMenu.Root>
          </div>
          <HomeTabsSettings />
          <div className={styles.sectionTitle}><PanelTop aria-hidden="true" size={20} /><h2>{t("windowBehavior")}</h2></div>
          <div className={styles.behaviorList}>
            <div className={styles.row}><div className={styles.settingCopy}><span className={`${styles.icon} ${styles.powerIcon}`}><Power aria-hidden="true" size={20} /></span><div><h2>{t("launchAtLogin")}</h2><p>{t("launchAtLoginDescription")}</p></div></div><button aria-checked={launchAtLogin} className={styles.switch} onClick={() => void toggleLaunchAtLogin()} role="switch" type="button"><span /></button></div>
            <div className={styles.row}><div className={styles.settingCopy}><span className={`${styles.icon} ${styles.windowIcon}`}><PanelTop aria-hidden="true" size={20} /></span><div><h2>{t("closeBehavior")}</h2><p>{t("closeBehaviorDescription")}</p></div></div>
              <DropdownMenu.Root><DropdownMenu.Trigger className={styles.languageTrigger}><span>{t(currentCloseBehavior.key)}</span><ChevronDown aria-hidden="true" size={16} /></DropdownMenu.Trigger><DropdownMenu.Portal><DropdownMenu.Content align="end" className={styles.menu} sideOffset={6}>
                {closeBehaviorOptions.map((item) => <DropdownMenu.Item className={styles.menuItem} key={item.code} onSelect={() => selectCloseBehavior(item.code)}><span>{t(item.key)}</span>{item.code === closeBehavior && <Check aria-hidden="true" size={16} />}</DropdownMenu.Item>)}
              </DropdownMenu.Content></DropdownMenu.Portal></DropdownMenu.Root>
            </div>
          </div>
          <LoginBrowserSettings />
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
      <Tabs.Content className={styles.pane} forceMount value="about">
        <div className={styles.stack}>
          <article className={styles.about}>
            <header className={styles.aboutPlate}>
              <span className={styles.aboutMark}><img alt="" src={logo} /></span>
              <h2>{t("appName")}</h2>
              <p className={styles.aboutTagline}>{t("aboutTagline")}</p>
              <p className={styles.aboutVersion}>{t("aboutVersion", { version: appVersion })}</p>
              <div className={styles.updateRow}>
                <button className={styles.databaseButton} disabled={updateBusy || updateInstalling} onClick={() => void checkUpdates()} type="button">
                  <RefreshCw aria-hidden="true" className={updateBusy ? styles.spin : undefined} size={14} />
                  {updateBusy ? t("updateChecking") : t("updateCheck")}
                </button>
                {updateStatus === "available" && updateVersion && (
                  <button className={styles.databaseButton} disabled={updateInstalling} onClick={() => void downloadAndInstallUpdate()} type="button">
                    <Download aria-hidden="true" size={14} />
                    {updateInstalling ? t("updateInstalling") : t("updateDownloadInstall", { version: updateVersion })}
                  </button>
                )}
              </div>
              {updateStatus === "up-to-date" && <p className={styles.updateHint}>{t("updateUpToDate")}</p>}
              {updateStatus === "available" && updateVersion && (
                <div className={styles.updateCard}>
                  <p className={styles.updateAvailable}>{t("updateAvailable", { version: updateVersion })}</p>
                  <ReleaseNotes markdown={updateNotes} />
                </div>
              )}
              {updateStatus === "error" && updateError && <p className={styles.updateError}>{t("updateCheckFailed", { error: updateError })}</p>}
            </header>
            <div className={styles.aboutBody}>
              <section className={styles.aboutSection}>
                <p>{t("aboutIntro")}</p>
              </section>
              <section className={styles.aboutSection}>
                <h3>{t("aboutAppsTitle")}</h3>
                <ul className={styles.aboutRoster}>
                  {aboutApps.map((app) => <li className={styles.aboutBerth} key={app.nameKey}>
                    <img alt="" className="ink" src={app.icon} />
                    <strong>{t(app.nameKey)}</strong>
                    <span>{t(app.detailKey)}</span>
                  </li>)}
                </ul>
              </section>
              <section className={styles.aboutSection}>
                <h3>{t("aboutDataTitle")}</h3>
                <p>{t("aboutData")}</p>
              </section>
            </div>
            <p className={styles.aboutFoot}>{t("aboutCopyright", { year: new Date().getFullYear() })}</p>
          </article>
        </div>
      </Tabs.Content>
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

syncDocumentAppKind();
createRoot(document.getElementById("root")!).render(<SettingsPage />);
