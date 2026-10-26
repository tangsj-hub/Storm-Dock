# 新增应用接入范式

面向 Storm Dock：把一个桌面应用做成可切换账号 / 查用量的「应用种类」。下文用 `<app>` 指代新应用（小写 slug，如 `cursor`）。结构参考现有完整接入，**不要**照抄某一应用的业务细节。

## 一句话流程

`ApplicationKind` → 后端模块 + Adapter → store / commands 注册 → 桌面启停 → 前端 Tab / 导入 / 列表 / 用量 → i18n → 测试。

---

## 1. 模型与类型

| 位置 | 改什么 |
| --- | --- |
| `src-tauri/src/models.rs` | `ApplicationKind` 增变体；`display_name()`；按需扩展 `Usage*` / `ProductUsage` 字段（额度桶、绝对积分、到期时间等） |
| `src/lib/types.ts` | `ApplicationKind` 联合类型、`APPLICATION_KINDS`、query/`usagePath`/`homeKindFromUsageQuery` 分支 |
| `src/lib/homeTabs.ts` | `HOME_TABS`；彩色品牌图勿加 `ink`（见 §5） |

SQLite 里账号按 `application` 列隔离；新 kind 的字符串要与 Rust `serde` / FE 一致（camelCase / 小写 slug）。

---

## 2. 后端模块（按需裁剪）

新建 `src-tauri/src/<app>/`，在 `src-tauri/src/lib.rs` 声明 `mod <app>;`。

典型文件：

| 文件 | 职责 |
| --- | --- |
| `mod.rs` | 子模块导出；邮箱等常量 key |
| `auth.rs` | 本机鉴权文件路径探测、读写、**登出标记**（若桌面端有） |
| `session.rs` | profile ↔ `Session`；导入解析；`matches_live` |
| `oauth.rs` | 官方浏览器授权（可选） |
| `usage.rs` | billing / meter API → 统一 `UsageDetails`；导入后异步刷新 |
| `crypto.rs` | 仅当 token 加密存储时需要 |

**路径约定（跨平台）**

- macOS：通常 `~/Library/Application Support/<Vendor>/...`
- Windows：官方与社区路径可能分裂（Roaming `APPDATA` vs Local `LOCALAPPDATA`）。**先探测已有目录，再回退默认**；写入与清登出标记必须落在同一目录。
- Linux：按官方 XDG / 厂商目录；无则跟已有应用一致。

**切换写入 checklist**

1. 把会话写到桌面端真实鉴权文件（格式与桌面端一致；加密态可先原样写入，后台再解密）。
2. 清除桌面端登出标记（文件名必须以桌面端为准，勿猜）。
3. 再启停进程；仅写凭证不清标记 = 仍显示未登录。

---

## 3. Adapter 与 Store

| 位置 | 改什么 |
| --- | --- |
| `src-tauri/src/apps/<app>.rs` | 实现 `ApplicationAdapter`：`kind` / `detect` / `import_current` / `apply` |
| `src-tauri/src/apps/mod.rs` | `mod` + `pub use` Adapter |
| `src-tauri/src/store.rs` | 持有 Adapter；`accounts(kind)` 映射；live match；邮箱 key；保存用量快照的方法 |

`ApplicationAdapter`（`apps/mod.rs`）：

```text
kind → detect → import_current → apply(session)
```

`apply` = 切换账号时写盘；`detect` / `import_current` 要尊重登出标记与「是否有登录材料」。

---

## 4. Commands / 桌面 / 注册

| 位置 | 改什么 |
| --- | --- |
| `src-tauri/src/commands.rs` | 导入分支、刷新全部、`get_<app>_usage` / `get_saved_<app>_usage`、OAuth complete、按 kind 的 match 臂 |
| `src-tauri/src/lib.rs` | `invoke_handler` 注册上述 command |
| `src-tauri/src/desktop/mod.rs` | `DesktopApp`：展示名、bundle id、Windows exe 名、进程名、快捷方式名 |
| `src-tauri/src/desktop/macos.rs`（及 Win 侧） | 进程探测 / 信号：Electron 壳用 **路径包含 `.app`**，勿只靠进程名 |
| `src/lib/api.ts` | `invoke` 封装（launch、refresh 等） |

需要独立「切换器」逻辑时再挂 `src-tauri/src/switcher/`（多数应用走 store `apply` + desktop launch 即可）。

---

## 5. 前端

| 区域 | 文件 / 要点 |
| --- | --- |
| 资源 | `src/assets/<app>.(svg|png)`；`global.css` 里 `html[data-app="<app>"]` 水印 |
| 深色主题 | 单色 SVG：加 `ink`（会 invert）；**彩色品牌 PNG：不要 `ink`，水印也不要 invert**（否则色相反掉）→ `homeTabs.ts` 的 `keepsBrandColor` + CSS 例外 |
| 首页 | `HomePage.tsx`：图标 map、Tab、launch、刷新 stream、分区显隐 |
| 列表徽章 | `accountPresentation.ts`：套餐 / 剩余额度 / 勿误标「已过期」 |
| 添加账号 | `pages/add/main.tsx`：当前登录 / OAuth / Token·JSON 文案与示例 |
| 用量页 | `pages/usage/main.tsx` + `format.ts`：主额度、分桶、近期图、明细；数字格式化抽统一函数（整数不强制小数） |
| 设置 Tab | 用户可开关/排序的 `HOME_TABS` 已覆盖；新 kind 进数组即可 |

用量单位若与 Cursor「美分」不同，在 `UsageMetric.kind`（如 `credits`）区分，FE 用对应 formatter，勿混用 `$`。

---

## 6. i18n

`src/i18n.ts` 中 **zh + en 同步**：

- 应用名、`importCurrent*` / `*NotReady` / `officialLogin*` / `tokenImport*`
- `usageTitle*` / `usageLoading*` / `usagePrimary*` / 周趋势与明细标题
- 列表徽章文案（如剩余额度）

避免只改中文或硬编码在组件里。

---

## 7. 测试期望

| 层 | 建议 |
| --- | --- |
| Rust | `auth` 路径探测与登出标记；`session` 导入样例；`apply` 写盘+清标记；`usage` 响应映射（含边界：0、小数、缺字段） |
| FE | `format` / `accountPresentation` / `homeTabs` 单测 |
| 校验命令 | `cargo test --manifest-path src-tauri/Cargo.toml`；`npm run build` |

真机冒烟：导入 → 列表有徽章 → 切换并启动桌面端已登录 → 用量详情分桶与明细 → 深色主题图标/水印颜色正常（Win 再确认鉴权目录）。

---

## 8. 最小 checklist（复制用）

- [ ] `ApplicationKind` + FE `types` / `HOME_TABS`
- [ ] `src-tauri/src/<app>/{auth,session,…}` + `lib.rs` mod
- [ ] `apps/<app>.rs` Adapter；`apps/mod.rs`；`store.rs` 接线
- [ ] commands + `invoke_handler` + `api.ts`
- [ ] `desktop` 启停与进程匹配
- [ ] 资源、水印、深色 invert 策略
- [ ] Home / Add / Usage / 列表徽章
- [ ] i18n zh+en
- [ ] 单测 + 本机切换登录冒烟（含 Windows 路径若适用）

---

## 9. 不必默认做的

- MCP / 插件管理（未支持就明确返回错误即可）
- 与 Cursor 相同的强制退出确认（仅在会杀进程且产品要求时）
- 把某一应用的加密、积分口径、API path 写死进通用层——留在 `<app>/` 内

