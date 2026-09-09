# Storm Dock

[English](./README_EN.md) | [Gitee](https://gitee.com/mqlwyz/storm-dock) | [GitHub](https://github.com/tangsj-hub/Storm-Dock)

Storm Dock 是一款本地桌面应用，用于集中管理开发工具的多个账号，并在需要时切换本机登录状态。当前支持 Cursor，Codex 桌面端已列入管理入口，认证数据支持仍在开发中。

## 功能状态

| 平台 | 状态 | 能力 |
| --- | --- | --- |
| Cursor | 已支持 | 官方 OAuth 登录、导入当前登录账户、Access Token / JSON 导入、账号排序与切换 |
| Codex 桌面端 | 开发中 | 当前不会读取、写入或导入 Codex 登录数据 |

支持 macOS 和 Windows。Cursor 切换会更新该应用的本地登录状态；若 Cursor 正在运行，Storm Dock 会要求确认后才强制重启，避免未保存内容丢失。

## 数据与安全

- 账号数据和完整会话保存在本地 SQLite 数据库 `storm-dock.db`；默认位于应用数据目录，可在设置中迁移到 Dropbox、OneDrive、iCloud、WebDAV 挂载目录等同步目录。
- 数据库会话不会发送到前端、导出 JSON 或记录到日志。同步目录中的数据库为明文，访问权限由同步服务和用户负责。
- 同步目录只支持设备间串行写入：切换设备前关闭另一台设备上的 Storm Dock 并等待同步完成。
- 切换 Cursor 前会备份当前会话；写入后会验证结果，验证失败即尝试恢复原会话。
- 只处理已识别的 Cursor 本地数据结构；未知结构会被拒绝。
- “官方登录”打开 Cursor 官方授权页，使用与 Cursor CLI 相同的 OAuth / PKCE 流程轮询获取会话后导入账户。Storm Dock 不会把 verifier 放进登录链接。

## 技术栈

- Tauri v2 / Rust
- React / TypeScript / Vite
- SQLite（单文件同步模式）

## 许可证

Storm Dock 使用 MIT License。第三方 npm 与 Cargo 依赖的许可证清单见
[`THIRD_PARTY_NOTICES`](./THIRD_PARTY_NOTICES)；依赖的精确版本分别以
`package-lock.json` 和 `src-tauri/Cargo.lock` 为准。

## 第三方品牌与服务

Storm Dock 是独立的非官方开源项目，与 Cursor、OpenAI、Codex 或其他第三方
公司不存在隶属、授权、赞助或认可关系。项目中出现的产品名称和商标归其各自
权利人所有；MIT License 仅授予本项目代码的版权许可，不授予任何第三方商标、
品牌或服务的使用权，也不允许绕过登录、访问控制、速率限制或其他服务限制。

使用 Storm Dock 连接第三方服务前，请阅读并遵守相应的最新条款及政策，包括
[Cursor Terms of Service](https://www.cursor.com/terms-of-service) 和
[OpenAI Terms of Use](https://openai.com/policies/terms-of-use)。用户应自行确认
其账号、地区和具体使用场景符合这些条款；第三方条款变更时，以官方页面为准。

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
