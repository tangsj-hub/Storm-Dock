import i18n from "i18next";
import { initReactI18next } from "react-i18next";

const resources = {
  zh: { translation: {
    appName: "Storm Dock", settings: "设置", applications: "应用", refresh: "刷新本机状态", addAccount: "新增账户",
    accounts: "账户管理", addAccountTitle: "新增账户", back: "返回账户列表",
    cursor: "Cursor", codex: "Codex", unsupportedTitle: "Codex 桌面端待支持", unsupportedDescription: "当前版本不会读取或改写 Codex 的登录数据。",
    emptyTitle: "还没有账户", emptyDescription: "使用右上角的新增按钮添加 Cursor 账户。",
    current: "使用中", switch: "切换", retry: "重试", remove: "删除 {{account}}", drag: "拖动 {{account}}",
    officialLogin: "官方登录", importCurrent: "导入当前账户", tokenImport: "Token / JSON",
    officialLoginTitle: "官方登录", officialLoginDescription: "在 Cursor 中完成授权后，可返回此处继续管理账号。", startLogin: "开始登录",
    importCurrentTitle: "导入当前账户", importCurrentDescription: "读取当前 Cursor 的登录账户并加入列表。", cursorNotReady: "Cursor 未就绪。",
    tokenImportTitle: "Token / JSON 导入", tokenImportDescription: "粘贴 Access Token、JWT 或导出的 Cursor 凭证。", credential: "凭证内容", cancel: "取消", import: "导入账户",
    imported: "已导入 {{account}}。", deleted: "账户已删除。Cursor 当前登录状态未被修改。", reordered: "账户顺序已更新。",
    settingsTitle: "设置", language: "语言", languageDescription: "选择应用界面显示语言。", chinese: "简体中文", english: "English", languageSaved: "语言已切换为 {{language}}。",
    switchProgress: "账号切换进度", cursorLaunched: "账号已切换，Cursor 已启动。", cursorRestarted: "账号已切换，Cursor 已重新启动。", restartRequired: "账号已切换。需要重启 Cursor 才生效。",
    restartDialogTitle: "Cursor 正在运行", restartDialogDescription: "当前 Cursor 仍在使用旧登录状态。确认后将强制结束并重新启动 Cursor。", restartDialogWarning: "强制终止会丢失 Cursor 中未保存的内容。", cancelCountdown: "取消 ({{seconds}})", forceRestart: "强制终止并启动",
    switchStages: { loading: "读取账户凭证", applying: "写入并验证 Cursor 会话", persisting: "保存账户状态", launching: "启动 Cursor", terminating: "正在结束 Cursor", restartRequired: "等待重启确认", complete: "切换完成", error: "切换失败" },
    importTypes: { oauth: "OAuth", token: "TOKEN", jwt: "JWT", native: "本机" }
  }},
  en: { translation: {
    appName: "Storm Dock", settings: "Settings", applications: "Applications", refresh: "Refresh local state", addAccount: "Add account",
    accounts: "Account management", addAccountTitle: "Add account", back: "Back to accounts",
    cursor: "Cursor", codex: "Codex", unsupportedTitle: "Codex desktop support is coming", unsupportedDescription: "This version does not read or change Codex sign-in data.",
    emptyTitle: "No accounts yet", emptyDescription: "Use the Add account button to add a Cursor account.",
    current: "Current", switch: "Switch", retry: "Retry", remove: "Delete {{account}}", drag: "Drag {{account}}",
    officialLogin: "Official login", importCurrent: "Import current account", tokenImport: "Token / JSON",
    officialLoginTitle: "Official login", officialLoginDescription: "Complete authorization in Cursor, then return here to manage the account.", startLogin: "Start login",
    importCurrentTitle: "Import current account", importCurrentDescription: "Read the active Cursor account and add it to the list.", cursorNotReady: "Cursor is not ready.",
    tokenImportTitle: "Import Token / JSON", tokenImportDescription: "Paste an access token, JWT, or exported Cursor credential.", credential: "Credential", cancel: "Cancel", import: "Import account",
    imported: "Imported {{account}}.", deleted: "Account deleted. Cursor's active sign-in state was not changed.", reordered: "Account order updated.",
    settingsTitle: "Settings", language: "Language", languageDescription: "Choose the display language for the application.", chinese: "Simplified Chinese", english: "English", languageSaved: "Language changed to {{language}}.",
    switchProgress: "Account switch progress", cursorLaunched: "Account switched and Cursor started.", cursorRestarted: "Account switched and Cursor restarted.", restartRequired: "Account switched. Restart Cursor for the change to take effect.",
    restartDialogTitle: "Cursor is running", restartDialogDescription: "Cursor is still using the previous sign-in state. Confirm to force quit and restart it.", restartDialogWarning: "Force quitting may lose unsaved work in Cursor.", cancelCountdown: "Cancel ({{seconds}})", forceRestart: "Force quit and start",
    switchStages: { loading: "Reading account credentials", applying: "Writing and verifying the Cursor session", persisting: "Saving account state", launching: "Starting Cursor", terminating: "Quitting Cursor", restartRequired: "Waiting for restart confirmation", complete: "Switch complete", error: "Switch failed" },
    importTypes: { oauth: "OAuth", token: "TOKEN", jwt: "JWT", native: "Native" }
  }}
};

void i18n.use(initReactI18next).init({
  resources,
  lng: localStorage.getItem("language") ?? "zh",
  fallbackLng: "zh",
  interpolation: { escapeValue: false }
});

export default i18n;
