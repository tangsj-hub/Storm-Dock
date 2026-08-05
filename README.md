# Plat. Dock

跨平台本地账户切换器，使用 Tauri v2、Rust、React 和 TypeScript 构建。

## 支持状态

| 应用 | 状态 |
| --- | --- |
| Cursor | 支持官方登录接力、本机登录态、Access Token 与兼容导出 JSON 导入，以及账号切换。只识别 `state.vscdb` 的已验证结构。 |
| Codex 桌面端 | 待支持；当前版本不读取或写入其认证数据。 |

支持 macOS 与 Windows。切换只修改目标应用本地登录态；若应用正在运行，需完全退出后重新启动才会生效。

## 安全模型

- 账户名称和时间戳保存在应用数据目录的 `accounts.json`。
- Cursor 会话只保存在 macOS Keychain 或 Windows Credential Manager；不会传给 React、写入元数据或日志。
- 写入 Cursor 前读取原始会话；写后验证失败会立即恢复此前会话。
- 未识别的 Cursor 数据库结构会被拒绝，不执行写入。
- “官方登录”只启动 Cursor 官方界面完成 OAuth，Plat. Dock 不冒充 OAuth 客户端，也不接收网页登录回调。

## 开发

前置条件：Node.js 20+、Rust stable，以及对应平台的 Tauri 系统依赖。

```bash
npm install
npm run tauri dev
```

生产构建：

```bash
npm run tauri build
```

## 验证

```bash
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

发布前须在 macOS 与 Windows 分别使用两个自有 Cursor 账号完成导入、A/B 切换、应用重启后的身份验证，以及无权限和验证失败时的恢复测试。
