# Storm Dock 发版与应用内更新

对齐 cc-switch 的思路：**构建 → 稳定命名产物 → 发布 Release → 严格组装 `latest.json`**。缺 Mac 或 Windows 更新包时，流水线直接失败，不会再发出半成品清单。

## 架构

```
tag vX.Y.Z
  ├─ build / macOS (macos-14)
  │    tauri build --bundles app,dmg
  │    scripts/ci/package-macos-assets.sh  → release-assets/
  ├─ build / Windows
  │    tauri build --bundles nsis,msi
  │    scripts/ci/package-windows-assets.sh → release-assets/
  ├─ publish-release（资产门禁）
  │    Softprops → GitHub Release
  └─ assemble-latest-json
       scripts/generate-latest-json.py（强制 darwin + windows）
       上传并 curl 校验公开 latest.json
```

工作流文件：`.github/workflows/build.yml`（name: **Release**）。

## 稳定产物名

| 文件 | 用途 |
| --- | --- |
| `Storm-Dock-<ver>-macOS.dmg` | 手动安装 |
| `Storm-Dock-<ver>-macOS.app.tar.gz` + `.sig` | macOS 应用内更新 |
| `Storm-Dock-<ver>-Windows-x64-setup.exe` + `.sig` | NSIS 安装包 |
| `Storm-Dock-<ver>-Windows-x64.msi` + `.sig` | MSI + 应用内更新（优先） |
| `latest.json` | Updater 清单 |

`latest.json` 至少包含：

- `darwin-aarch64` / `darwin-aarch64-app`（及 x86_64 别名，指向同一 macOS 包）
- `windows-x86_64`

客户端 endpoint：

`https://github.com/tangsj-hub/Storm-Dock/releases/latest/download/latest.json`

## 签名密钥

本地（勿提交）：

- 私钥：`.tauri/storm-dock.key`
- 公钥：写入 `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`

轮换：

```bash
npx tauri signer generate -w .tauri/storm-dock.key
# 把 .pub 内容写回 tauri.conf.json
```

### GitHub Secrets

| Secret | 说明 |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | `.tauri/storm-dock.key` 全文（或 base64） |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 可选 |

没有私钥时构建会失败（`createUpdaterArtifacts` 已开启）。

## 发版清单

1. 同步版本号：`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`
2. 在 `CHANGELOG.md` 写好 `## [X.Y.Z]` 段落（会进 Release notes / updater notes）
3. 提交并推主分支
4. 打 tag 并推送：

```bash
git tag vX.Y.Z
git push github vX.Y.Z
# 或: git push origin vX.Y.Z
```

5. Actions → **Release** 跑完后检查：
   - Release 附件含上表全部文件
   - `latest.json` 含 darwin + windows 平台
   - 本机 Settings → About → 检查更新

## 本地脚本

| 脚本 | 作用 |
| --- | --- |
| `scripts/ci/prepare-signing-key.sh` | 规范化私钥并写入 `GITHUB_ENV` |
| `scripts/ci/package-macos-assets.sh` | 收集/补签 macOS 更新包，稳定命名，缺一即失败 |
| `scripts/ci/package-windows-assets.sh` | 收集 Windows MSI/NSIS + sig，稳定命名 |
| `scripts/generate-latest-json.py` | 从 Release 资产组装清单（fail-closed） |
| `scripts/changelog-notes.py` | 从 CHANGELOG 抽 notes |

## 用户侧

设置 → 关于 → **检查更新** → **下载并安装**。安装走 Rust 命令 `install_update_and_restart`。

## 尚未纳入（刻意延后）

- Apple Developer ID 签名 / 公证（Gatekeeper 仍可能提示）
- Cloudflare R2 镜像（cc-switch 的 `sync-r2`；当前只用 GitHub Releases）
- Linux / Windows arm64 矩阵

这些可以后续加 job，不改变本流水线的「完整产物 + 严格清单」契约。
