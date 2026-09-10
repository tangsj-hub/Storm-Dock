# macOS DMG layout

Storm Dock ships a styled disk image for macOS. Open the `.dmg`, then drag **Storm Dock** onto **Applications**.

## Layout (Tauri `bundle.macOS.dmg`)

- Background: `src-tauri/macos/dmg-background.png` (660×400)
- Window: 660×400
- App icon position: (180, 170)
- Applications folder position: (480, 170)
- Minimum macOS: 12.0

## Build

```bash
npm run tauri build -- --bundles dmg
```

Artifacts: `src-tauri/target/release/bundle/dmg/`.

## Not included yet

Apple Developer ID signing and notarization are not configured. Gatekeeper may warn on first open until signing is added later.
