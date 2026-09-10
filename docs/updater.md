# In-app updates (Tauri updater)

Storm Dock uses the [Tauri 2 updater](https://v2.tauri.app/plugin/updater/) with minisign-signed artifacts published to GitHub Releases.

## Signing keys

A keypair lives under `.tauri/` (gitignored):

- Private key: `.tauri/storm-dock.key` — **never commit**
- Public key: embedded in `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`

Generate a new pair only if you rotate keys:

```bash
npx tauri signer generate -w .tauri/storm-dock.key
```

Then replace `plugins.updater.pubkey` with the new public key string from `.tauri/storm-dock.key.pub`.

## GitHub Actions secrets

Add these repository secrets (Settings → Secrets and variables → Actions):

| Secret | Value |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Full contents of `.tauri/storm-dock.key` (minisign private key text, or its base64 encoding) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Optional. Set if the private key was generated with a password |

Without `TAURI_SIGNING_PRIVATE_KEY`, tagged release builds fail (updater artifacts cannot be signed). Manual `workflow_dispatch` builds also require the secret when `createUpdaterArtifacts` is enabled.

## Cutting a release

1. Bump `version` in `package.json` and `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml`.
2. Add a Keep a Changelog section in `CHANGELOG.md`.
3. Commit, then:

```bash
git tag v1.3.1
git push origin v1.3.1
# or: git push github v1.3.1
```

4. The `Build installers` workflow builds macOS + Windows installers, uploads signed updater artifacts (`.tar.gz` / `.msi` + `.sig`), creates the GitHub Release, and assembles `latest.json`.

## How users update

1. Open **Settings → About**.
2. Click **检查更新 / Check for updates**.
3. If a newer version is listed, review the notes and click **下载并安装 / Download and install**.
4. The app downloads the signed package, installs it, and relaunches.

Updater endpoint:

`https://github.com/tangsj-hub/Storm-Dock/releases/latest/download/latest.json`
