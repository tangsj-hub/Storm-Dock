# Windows installer

Storm Dock ships two Windows installers from CI (`nsis` + `msi`). Prefer the **NSIS `.exe`** for normal downloads.

## NSIS (recommended)

- File: `Storm Dock_*_x64-setup.exe` (exact name depends on version / arch)
- Languages: Simplified Chinese and English, with a language selector at launch
- Install scope: user can choose **current user** or **all users** (`installMode: both`; choosing either still requires elevation for this mode)
- Compression: LZMA
- Start Menu folder: `Storm Dock`
- Branding: custom header / sidebar images under `src-tauri/windows/`
- WebView2: downloads the Microsoft bootstrapper silently if the runtime is missing

Build locally on Windows:

```bash
npm run tauri build -- --bundles nsis
```

Artifacts land under `src-tauri/target/release/bundle/nsis/`.

## MSI (WiX)

- File: `Storm Dock_*_x64_en-US.msi`
- Uses a **per-user** WiX template (`src-tauri/wix/per-user-main.wxs`) adapted from common Tauri patterns, so installation targets `%LocalAppData%\Programs\Storm Dock` with limited privileges (no forced machine-wide admin install)
- Useful for managed / enterprise distribution; the in-app updater may prefer MSI when both signatures exist

Build:

```bash
npm run tauri build -- --bundles msi
# or both:
npm run tauri build -- --bundles nsis,msi
```

## Notes

- macOS bundling is unchanged (`bundle.macOS.bundleName`).
- Publisher / homepage / license metadata are set in `src-tauri/tauri.conf.json` for ARP (Add/Remove Programs) and installer metadata.
- Cross-compiling a full Windows NSIS/MSI package from macOS is not supported by this project’s normal workflow; use a Windows machine or CI.
