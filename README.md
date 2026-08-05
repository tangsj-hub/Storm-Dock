# Storm Dock

Storm Dock 是一款本地桌面应用，用于集中管理开发工具的多个账号，并在需要时切换本机登录状态。当前支持 Cursor，Codex 桌面端已列入管理入口，认证数据支持仍在开发中。

## 功能状态

| 平台 | 状态 | 能力 |
| --- | --- | --- |
| Cursor | 已支持 | 官方登录接力、导入当前登录账户、Access Token / JSON 导入、账号排序与切换 |
| Codex 桌面端 | 开发中 | 当前不会读取、写入或导入 Codex 登录数据 |

支持 macOS 和 Windows。Cursor 切换会更新该应用的本地登录状态；若 Cursor 正在运行，Storm Dock 会要求确认后才强制重启，避免未保存内容丢失。

## 数据与安全

- 账号数据和完整会话保存在本地 SQLite 数据库 `storm-dock.db`；默认位于应用数据目录，可在设置中迁移到 Dropbox、OneDrive、iCloud、WebDAV 挂载目录等同步目录。
- 数据库会话不会发送到前端、导出 JSON 或记录到日志。同步目录中的数据库为明文，访问权限由同步服务和用户负责。
- 同步目录只支持设备间串行写入：切换设备前关闭另一台设备上的 Storm Dock 并等待同步完成。
- 切换 Cursor 前会备份当前会话；写入后会验证结果，验证失败即尝试恢复原会话。
- 只处理已识别的 Cursor 本地数据结构；未知结构会被拒绝。
- “官方登录”只启动 Cursor 完成其官方授权流程，Storm Dock 不接收 OAuth 回调或网页登录凭证。

## 技术栈

- Tauri v2 / Rust
- React / TypeScript / Vite
- SQLite（单文件同步模式）

## 开发

前置条件：Node.js 20+、Rust stable，以及目标平台所需的 Tauri 系统依赖。

```bash
npm install
npm run tauri dev
```

`npm run dev` 仅启动 Vite 前端；本地桌面功能请使用 `npm run tauri dev`。

## 构建与验证

```bash
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

发布前应在 macOS 与 Windows 上使用自有 Cursor 账号验证：导入、账号切换、重启后的身份状态，以及写入验证失败时的恢复行为。
