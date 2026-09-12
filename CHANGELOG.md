# Changelog

All notable changes to Storm Dock are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Cursor tab: Grok Bot sidebar entry with this machine's Grok Bot status, local session list, and rename/delete (including batch edit).

### Fixed

- Clicking the Storm Dock icon again focuses the existing window instead of opening a second instance (Tauri single-instance plugin; macOS dock reopen restores a hidden window).

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

- macOS in-app updates: CI now always produces and publishes signed `.app.tar.gz` updater artifacts and includes `darwin-aarch64` platforms in `latest.json`.

## [1.4.1] - 2026-09-10

### Fixed

- CI `build.yml` YAML parse error that blocked the v1.4.0 tag workflow (moved CHANGELOG notes generation into `scripts/changelog-notes.py`).

## [1.4.0] - 2026-09-10

### Added

- Close-window behavior: ask each time, minimize to tray, or quit (with remember option) on macOS and Windows.
- Grok Bot usage badges on Cursor account cards (percent + short reset), plus current Grok Bot account detection with a green lightning mark on the launch button.
- Cursor usage details show Grok Bot concrete reset time alongside the relative countdown.
- In-app update check / download / install from Settings → About (Tauri updater + CI `latest.json`).
- Polished Windows NSIS/MSI installer UX: Simplified Chinese + English with language selector, install-scope choice, LZMA compression, Start Menu folder, branding images, per-user WiX template, and WebView2 bootstrapper mode.
- Styled macOS DMG layout (background, icon positions, minimumSystemVersion 12.0).

### Changed

- Settings window-behavior control is now a three-way choice (ask / tray / quit).
- Account list and tray hide/show restore Dock / taskbar state more consistently when minimizing to tray.

<!--
How to release:
1. Move Unreleased items into a new ## [X.Y.Z] - YYYY-MM-DD section.
2. Bump version in package.json, src-tauri/tauri.conf.json, and src-tauri/Cargo.toml.
3. git tag vX.Y.Z && git push <remote> vX.Y.Z
4. CI publishes installers + latest.json; notes field prefers this CHANGELOG excerpt.
-->

[Unreleased]: https://github.com/tangsj-hub/Storm-Dock/compare/v1.5.1...HEAD
[1.5.1]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.5.1
[1.5.0]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.5.0
[1.4.2]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.4.2
[1.4.1]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.4.1
[1.4.0]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.4.0
[1.3.0]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.3.0
