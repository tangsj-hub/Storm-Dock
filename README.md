# Storm Dock

[English](./README_EN.md) | [Gitee](https://gitee.com/mqlwyz/storm-dock) | [GitHub](https://github.com/tangsj-hub/Storm-Dock)

**多应用账号一键切换的本地开发坞。**

在同一台电脑上集中管理 Cursor、ChatGPT、Grok Build、WorkBuddy 与 Grok Bot 的登录身份：导入、切换、启动，并查看用量与额度——账号留在本机，不必在各应用里反复登录。

支持 **macOS** 与 **Windows**。

## 为什么用 Storm Dock

- **多账号并行**：每个应用可保存多个身份，列表排序、一键切换并启动桌面端。
- **用量一眼可见**：Cursor / Grok / WorkBuddy / Grok Bot 等支持查看额度、积分与调用明细（能力因平台而异）。
- **会话与扩展**：按项目浏览会话，管理插件与 MCP（按应用开放）。
- **本地优先**：SQLite 本机存储，可迁到你选择的同步目录；凭证不进界面、不写日志、不上我们的服务器。

## 支持的应用

| 应用 | 能力概览 |
| --- | --- |
| **Cursor** | 官方 OAuth、导入当前登录、Token / JSON 导入、切换与启动、用量、会话、插件 / MCP |
| **ChatGPT** | 设备 OAuth、导入当前登录、API Key / Base URL、切换、会话、插件 / MCP |
| **Grok Build** | xAI 设备登录、导入当前登录、API Key / Base URL、切换、用量、插件 |
| **WorkBuddy** | 官方登录、导入当前登录、Token / JSON 导入、切换与启动、积分与调用明细 |
| **Grok Bot** | 启动与切换、额度查看、模版管理、导出迁移 |

切换登录会更新对应应用的本机状态。若目标应用正在运行，Storm Dock 会先确认再重启，降低未保存内容丢失的风险。

## 数据与安全

- 账号与完整会话保存在本地 `storm-dock.db`；默认在应用数据目录，可在设置中迁到 Dropbox、OneDrive、iCloud、WebDAV 等同步目录。
- 会话凭证不会发到前端、不会进导出 JSON 或日志。同步目录中的库为明文，访问权由你和同步服务负责。
- 同步目录仅适合设备间串行使用：换机前请关闭另一台 Storm Dock 并等待同步完成。
- 切换前会备份当前会话；写入后校验，失败则尝试恢复。未知本地数据结构会被拒绝。

## 新增应用

要接入一种新的桌面应用（可切换账号 / 查用量），按 [docs/add-application.md](./docs/add-application.md) 的范式走：`ApplicationKind` → Adapter / store → 启停 → 前端与 i18n → 测试。

## 技术栈

- Tauri v2 / Rust
- React / TypeScript / Vite
- SQLite（单文件同步模式）

## 更新日志与打包

- 变更记录：[CHANGELOG.md](./CHANGELOG.md)
- 应用内更新：[docs/updater.md](./docs/updater.md)
- Windows 安装包：[docs/windows-installer.md](./docs/windows-installer.md)
- macOS DMG：[docs/macos-dmg.md](./docs/macos-dmg.md)

## 交流群

开源社区 QQ 群：1081527500

## 许可证

Storm Dock 使用 MIT License。第三方依赖许可证见 [`THIRD_PARTY_NOTICES`](./THIRD_PARTY_NOTICES)；精确版本以 `package-lock.json` 与 `src-tauri/Cargo.lock` 为准。

## 第三方品牌与服务

Storm Dock 是独立的非官方开源项目，与 Cursor、OpenAI、ChatGPT、xAI、WorkBuddy 或其他第三方公司不存在隶属、授权、赞助或认可关系。产品名称与商标归各自权利人所有；MIT License 仅授予本项目代码的版权许可，不授予第三方商标或服务使用权，也不允许绕过登录、访问控制、速率限制或其他服务限制。

使用前请阅读并遵守相应官方条款，包括 [Cursor Terms of Service](https://www.cursor.com/terms-of-service) 与 [OpenAI Terms of Use](https://openai.com/policies/terms-of-use)。账号、地区与使用场景是否合规，由用户自行确认。

## 开发

前置：Node.js 20+、Rust stable，以及目标平台的 Tauri 系统依赖。

```bash
npm install
npm run tauri dev
```

`npm run dev` 仅启动 Vite；桌面功能请用 `npm run tauri dev`。

## 构建与验证

```bash
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

发布前请在 macOS 与 Windows 上用自有账号验证：导入、切换、重启后的登录态，以及写入校验失败时的恢复。
