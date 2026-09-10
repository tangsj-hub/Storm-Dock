# Storm Dock

[中文](./README.md) | [Gitee](https://gitee.com/mqlwyz/storm-dock) | [GitHub](https://github.com/tangsj-hub/Storm-Dock)

Storm Dock is a local desktop application for managing multiple accounts used by
developer tools and switching the active local login when needed. Cursor and
ChatGPT desktop and Grok Build are currently supported.

## Feature status

| Platform | Status | Capabilities |
| --- | --- | --- |
| Cursor | Supported | Official OAuth login, import current account, Access Token / JSON import, account ordering and switching |
| ChatGPT desktop | Supported | Official device OAuth login, current-login import, API key / custom Base URL import and switching, account refresh |
| Grok Build | Supported | xAI device login, current-login import, API key / custom Base URL import and switching, account refresh |

Storm Dock supports macOS and Windows. Cursor switching updates Cursor's local
login state. If Cursor is running, Storm Dock asks for confirmation before
restarting it, helping avoid loss of unsaved work.

## Data and security

- Account data and complete sessions are stored in a local SQLite database,
  `storm-dock.db`. The location can be moved to a user-selected sync directory.
- Sessions are not sent to the frontend, exported to JSON, or written to logs.
  A database in a sync directory is unencrypted; access is managed by the user
  and the sync provider.
- Sync directories support serial device use only: close Storm Dock on one
  device and wait for synchronization before using another device.
- The current Cursor session is backed up before switching. The write is
  verified and a failed verification triggers an attempted restore.
- Only recognized Cursor data structures are modified; unknown structures are
  rejected.
- Official login opens Cursor's authorization page and uses the same OAuth/PKCE
  polling flow as the Cursor CLI. The verifier is never placed in the login URL.

## Session and plugin management

- Browse Cursor, ChatGPT, and Grok Build sessions grouped by project, read messages,
  launch sessions, and delete sessions in bulk.
- Manage Cursor, Claude, ChatGPT, and Grok Build plugins by enabling, disabling,
  or removing them, with separate controls for Skills, MCP, and Hooks.
- Manage MCP configurations for Cursor and ChatGPT. Some changes
  take effect after restarting the corresponding application.

## Technology

- Tauri v2 / Rust
- React / TypeScript / Vite
- SQLite (single-file sync mode)

## Changelog

See [CHANGELOG.md](./CHANGELOG.md). In-app updates: [docs/updater.md](./docs/updater.md). Windows installers (NSIS recommended, Chinese/English): [docs/windows-installer.md](./docs/windows-installer.md). macOS DMG layout: [docs/macos-dmg.md](./docs/macos-dmg.md).

## License

Storm Dock is licensed under the MIT License. See
[`THIRD_PARTY_NOTICES`](./THIRD_PARTY_NOTICES) for the licenses of npm and Cargo
dependencies. Exact dependency versions are recorded in `package-lock.json` and
`src-tauri/Cargo.lock`.

## Third-party brands and services

Storm Dock is an independent, unofficial open-source project. It is not
affiliated with, authorized, sponsored, or endorsed by Cursor, OpenAI, ChatGPT, or
any other third party. Product names and trademarks belong to their respective
owners. The MIT License covers this project's code only; it does not grant any
right to use third-party trademarks or services, and does not permit bypassing
login, access controls, rate limits, or other service restrictions.

Before connecting Storm Dock to a third-party service, read and follow its
current terms and policies, including [Cursor Terms of Service](https://www.cursor.com/terms-of-service)
and [OpenAI Terms of Use](https://openai.com/policies/terms-of-use). You are
responsible for confirming that your account, region, and use case comply with
those terms.

## Development

Requirements: Node.js 20+, stable Rust, and the Tauri system dependencies for
your target platform.

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

Before a release, test with your own Cursor account on macOS and Windows:
import, account switching, login state after restart, and restoration after a
failed write verification.
