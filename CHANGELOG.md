# 更新日志

Storm Dock 的重要变更记录在此。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [1.6.0] - 2026-10-08

### Fixed

- 首页顶栏激活页签悬停 ▶ 启动按钮可点击（修正 z-index / stacking context，避免被 idle 图标挡住）。
- 顶栏一键启动失败时显示错误 Toast，并补充中英 i18n `appLaunchFailed`。

## [1.5.9] - 2026-10-04

### Fixed

- Grok Bot 右上角刷新的账号数量与下方列表对齐，使用同一资格规则；Free、未知订阅、Token 失效、已封禁和凭证缺失的账号不再计入。

## [1.5.8] - 2026-09-28

### Added

- README 补充开源社区交流群 QQ。
- 顶栏悬停图标一键启动 Cursor / ChatGPT / Grok Bot。
- 自实现 Windows 开机启动，并为 Run 路径加引号。

### Changed

- Windows 更新 `latest.json` 以 NSIS 为通用更新目标，保留用户安装路径，避免落到 WiX 默认目录。
- Grok Bot 用量返回原标签；详情页取消 max-width 居中。

## [1.5.7] - 2026-09-24

### Added

- Grok Bot 会话附件正确解析与展示（卡片、图片缩略图），支持本机与远程沙箱附件下载。
- 附件可用性：视口内探测（非阻塞）、异步 Tauri 命令；已清理附件显示「链接已失效」。
- 用量页与账号卡展示订阅/额度重置的本地具体日期时间（今天/明天含时刻，其余为「剩余 N 天 · 日期时间」）。

### Changed

- Grok Bot 会话消息列表虚拟化，长会话滚动更顺畅；会话页移除客户端卡片。
- 统一细滚动条样式。
- Codex（ChatGPT）额度时间改为「额度重置」文案（来自 rate-limit reset，而非订阅到期）；过期窗口显示「额度已重置」。
- 中文紧凑天数由 `18day` / `Nd` 改为「N 天」。

### Fixed

- 各客户端「今天 / 明天 / 剩余天数」改为按本地日历日计算（前端 + Rust），修复隔夜重置被标成「今天」的问题。

## [1.5.6] - 2026-09-20

### Added

- Cursor 官方登录可选同时获取 `workos_token`（勾选后从浏览器 Cookie 读取 `WorkosCursorSessionToken`）；登录后亦可粘贴令牌。
- macOS / Windows 自动读取 Chromium Cookie（Windows：Local State DPAPI + AES-256-GCM v10）；App-Bound Encryption（v20）暂不支持，可改用粘贴。
- 导出对话框在 JSON 上方单独展示 `workos_token` 与一键复制（有令牌时显示）。
- 导出 JSON 预览重构：编辑器风格 pretty-print、敏感令牌折叠/展开与悬停复制。

### Changed

- Grok Bot 客户端卡增加启动按钮，并过滤不可用账号。
- macOS 授权相关路径统一走应用内原生调用（Security.framework / 进程信号等），提示应显示为 Storm Dock。

### Fixed

- 加强代理超时与账号封禁展示，并修正额度重置标签。

## [1.5.5] - 2026-09-16

### Added

- 启动约 2.5 秒后自动检查更新（非 Tauri / 检查失败时静默跳过）；有新版本时右下角 Toast（与 ToastMessage 同款），「去更新」跳转设置关于页，关闭记住已提示版本。
- 关于页检查更新变更说明：轻量 Markdown Release Notes 排版（可滚动）。

### Changed

- 启动更新提示由居中对话框改为右下角 Toast；检查更新日志与可靠性增强。

## [1.5.4] - 2026-09-16

### Added

- 官方登录支持选择浏览器（系统默认 / Chrome / Edge / Safari / Firefox / Brave / Arc）；Cursor、ChatGPT、Grok OAuth 共用；设置可配置「每次询问」「记住上次」或固定浏览器。
- 设置页新增顶部标签栏调整：可拖拽排序首页标签。

### Changed

- 优化应用启动逻辑，统一 HTTP 请求层。
- 优化设置页排版。
- Grok 订阅有效期改为官方有效订阅到期日（不再误用周额度窗口）；额度展示官方总池百分比；用量详情补齐 Build/Imagine 拆分与重置时间。

### Fixed

- 修复主题「跟随系统」无效。
- ChatGPT API Key 账号列表不再显示「未知订阅」。

## [1.5.3] - 2026-09-14

### Fixed

- 修复发版构建：`canLaunchGrokBot` 在排除 `grok` 后仍比较 `"grok"`，触发 TypeScript TS2367，导致 `npm run build` 失败。

## [1.5.2] - 2026-09-14

### Added

- 独立 **Grok Bot** 顶栏入口：左侧「账号 / 会话」布局，状态卡片、会话列表与 Cursor 账号列表解耦（新建 `GrokBotAccountList` / `GrokBotSessionWorkspace` / `GrokBotStatusCard`）。
- Grok Build 订阅档位徽章（Free / SuperGrok 等），刷新走 `cli-chat-proxy.grok.com/v1/user?include=subscription`；`subscriptionTier` 为空时记为 Free，不再显示「未知订阅」。
- Grok Build 账号「用量查询」：对接 `cli-chat-proxy.grok.com/v1/billing`，用量页在 `kind=grok` 时使用 Grok 文案与水印（本月额度 / 按需用量 / 近几月）。
- Grok Bot 专用导出 `kind: grok-bot-client`，以及账号列表刷新 `refresh_grok_bot_accounts` / Grok 订阅刷新 `refresh_all_grok_accounts`。

### Fixed

- Cursor ↔ Grok Bot 同应用切换只关闭模式，不再清空账号列表（避免 `selected` 仍为 cursor 时列表被清空且不重载）。
- 顶栏账号刷新仅刷新 Bot 可用账号（非 Free Cursor + Grok Build），会话顶栏刷新只刷新会话；状态卡片刷新当前账号额度与会话。
- 再次点击 Storm Dock 图标时聚焦已有窗口，而不再打开第二个实例。

### Changed

- Grok Build 账号页刷新改为真正拉取订阅；Grok 用量页返回目标为 Grok Build 账号列表。

## [1.5.1] - 2026-09-12

### Fixed

- Windows: 切换 Cursor 账号时不再弹出控制台窗口（用 Win32 API 替代 `tasklist` / `taskkill` / `cmd start`）。
- Cursor 正在运行、需要确认强制重启时，不再提前写入会话或显示切换进度。

## [1.5.0] - 2026-09-10

### Changed

- **发版流水线重写**（对齐 cc-switch 思路，不再打补丁）：
  - 稳定产物命名：`Storm-Dock-<ver>-macOS.*` / `Storm-Dock-<ver>-Windows-x64.*`
  - CI 脚本：`scripts/ci/prepare-signing-key.sh`、`package-macos-assets.sh`、`package-windows-assets.sh`
  - 发布前资产门禁：缺 `.app.tar.gz`/`.sig`/`.dmg` 或 Windows MSI+sig 则失败
  - `latest.json` 强制包含 `darwin-aarch64`（含 `-app`）与 `windows-x86_64`，并 curl 校验公开清单
  - macOS runner 固定 `macos-14`；Release 并发组按 tag 串行
- 文档：`docs/updater.md` 改为完整发版清单

## [1.4.2] - 2026-09-10

### Fixed

- macOS 应用内更新：CI 始终产出并发布已签名的 `.app.tar.gz` 更新包，并在 `latest.json` 中包含 `darwin-aarch64` 平台。

## [1.4.1] - 2026-09-10

### Fixed

- 修复 CI `build.yml` YAML 解析错误（会阻断 v1.4.0 标签工作流）；将 CHANGELOG notes 生成挪到 `scripts/changelog-notes.py`。

## [1.4.0] - 2026-09-10

### Added

- 关闭窗口行为：每次询问、最小化到托盘、或退出（可记住选项），支持 macOS 与 Windows。
- Cursor 账号卡片上的 Grok Bot 用量徽章（百分比 + 短重置文案），以及当前 Grok Bot 账号识别与启动按钮绿色闪电标记。
- Cursor 用量详情中展示 Grok Bot 具体重置时间与相对倒计时。
- 设置 → 关于：应用内检查 / 下载 / 安装更新（Tauri updater + CI `latest.json`）。
- 优化 Windows NSIS/MSI 安装体验：简体中文 + 英文语言选择、安装范围、LZMA 压缩、开始菜单、品牌图、每用户 WiX 模板、WebView2 bootstrapper。
- 美化 macOS DMG 布局（背景、图标位置、最低系统版本 12.0）。

### Changed

- 设置中的窗口行为改为三选一（询问 / 托盘 / 退出）。
- 最小化到托盘时，账号列表与托盘显隐对 Dock / 任务栏状态的恢复更一致。

<!--
发版步骤：
1. 将 Unreleased 条目移入新的 ## [X.Y.Z] - YYYY-MM-DD 小节（默认中文）。
2.  bump package.json、src-tauri/tauri.conf.json、src-tauri/Cargo.toml 版本号。
3. git tag vX.Y.Z && git push <remote> vX.Y.Z
4. CI 发布安装包与 latest.json；notes 优先取本 CHANGELOG 对应小节。
-->

[Unreleased]: https://github.com/tangsj-hub/Storm-Dock/compare/v1.5.4...HEAD
[1.5.4]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.5.4
[1.5.3]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.5.3
[1.5.2]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.5.2
[1.5.1]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.5.1
[1.5.0]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.5.0
[1.4.2]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.4.2
[1.4.1]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.4.1
[1.4.0]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.4.0
[1.3.0]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.3.0
