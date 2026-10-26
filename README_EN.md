# Storm Dock

[中文](./README.md) | [Gitee](https://gitee.com/mqlwyz/storm-dock) | [GitHub](https://github.com/tangsj-hub/Storm-Dock)

**A local dock to switch developer accounts across apps.**

Keep Cursor, ChatGPT, Grok Build, WorkBuddy, and Grok Bot identities on one computer: import, switch, launch, and check usage or credits—without signing in again inside each app.

Runs on **macOS** and **Windows**.

## Why Storm Dock

- **Multiple accounts per app**: save several identities, reorder the list, switch and launch the desktop client in one step.
- **Usage at a glance**: view quotas, credits, and request history where the platform supports it (Cursor / Grok / WorkBuddy / Grok Bot).
- **Sessions & extensions**: browse sessions by project; manage plugins and MCP where available.
- **Local-first**: SQLite on this machine, optional sync folder of your choice. Credentials stay out of the UI and logs, and are not uploaded to our servers.

## Supported apps

| App | Highlights |
| --- | --- |
| **Cursor** | Official OAuth, import current login, Token / JSON import, switch & launch, usage, sessions, plugins / MCP |
| **ChatGPT** | Device OAuth, import current login, API key / Base URL, switching, sessions, plugins / MCP |
| **Grok Build** | xAI device login, import current login, API key / Base URL, switching, usage, plugins |
| **WorkBuddy** | Official login, import current login, Token / JSON import, switch & launch, credits and request history |
| **Grok Bot** | Launch & switch, quota view, templates, export / migrate |

Switching updates the target app’s local login state. If the app is running, Storm Dock asks before restarting so unsaved work is less likely to be lost.

## Data and security

- Accounts and full sessions live in a local `storm-dock.db`. Default location is the app data directory; you can move it to Dropbox, OneDrive, iCloud, WebDAV, or another sync folder.
- Session credentials are not sent to the frontend, exported as JSON, or written to logs. A database in a sync folder is unencrypted; access is yours and your sync provider’s responsibility.
- Sync folders are for serial device use: close Storm Dock on one machine and wait for sync before using another.
- The current session is backed up before a switch; writes are verified and a failed check tries to restore. Unknown local data structures are rejected.

## Adding an application

To add a new desktop app kind (account switching / usage), follow [docs/add-application.md](./docs/add-application.md): `ApplicationKind` → Adapter / store → launch/quit → frontend & i18n → tests.

## Technology

- Tauri v2 / Rust
- React / TypeScript / Vite
- SQLite (single-file sync mode)

## Changelog and packaging

- Changelog: [CHANGELOG.md](./CHANGELOG.md)
- In-app updates: [docs/updater.md](./docs/updater.md)
- Windows installers: [docs/windows-installer.md](./docs/windows-installer.md)
- macOS DMG: [docs/macos-dmg.md](./docs/macos-dmg.md)

## Community

Open-source QQ group: 1081527500

## License

Storm Dock is MIT-licensed. See [`THIRD_PARTY_NOTICES`](./THIRD_PARTY_NOTICES) for dependency licenses. Exact versions are in `package-lock.json` and `src-tauri/Cargo.lock`.

## Third-party brands and services

Storm Dock is an independent, unofficial open-source project. It is not affiliated with, authorized, sponsored, or endorsed by Cursor, OpenAI, ChatGPT, xAI, WorkBuddy, or any other third party. Product names and trademarks belong to their owners. The MIT License covers this project’s code only; it does not grant rights to third-party marks or services, and does not permit bypassing login, access controls, rate limits, or other restrictions.

Before connecting to a third-party service, read and follow its current terms, including [Cursor Terms of Service](https://www.cursor.com/terms-of-service) and [OpenAI Terms of Use](https://openai.com/policies/terms-of-use). You are responsible for confirming that your account, region, and use case comply.

## Development

Requirements: Node.js 20+, stable Rust, and Tauri system dependencies for your platform.

```bash
npm install
npm run tauri dev
```

`npm run dev` starts Vite only. Use `npm run tauri dev` for desktop features.

## Build and verify

```bash
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

Before a release, verify on macOS and Windows with your own accounts: import, switch, login state after restart, and restore after a failed write verification.
