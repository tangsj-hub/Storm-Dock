# Changelog

All notable changes to Storm Dock are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/tangsj-hub/Storm-Dock/compare/v1.4.2...HEAD
[1.4.2]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.4.2
[1.4.1]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.4.1
[1.4.0]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.4.0
[1.3.0]: https://github.com/tangsj-hub/Storm-Dock/releases/tag/v1.3.0
