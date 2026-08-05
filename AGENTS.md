# Plat. Dock

- Stack: Tauri v2, Rust, React, TypeScript, Vite.
- Frontend pages: index.html, add.html, settings.html; keep them as separate Vite entries.
- Shared frontend code: src/lib, src/components, src/i18n.ts; page styles use CSS Modules and only resets/tokens go in src/styles/global.css.
- Use existing Radix primitives for accessible interactive UI. Add strings to both zh and en resources in src/i18n.ts.
- For dialog icon close buttons, prevent pointer focus on `onPointerDown` and never make them the dialog's initial focus; focus the primary dialog action instead, while retaining the global `:focus-visible` style for keyboard users.
- Account data and sessions belong in a local SQLite database, which may live in a user-selected sync directory for cross-device sharing. Never expose or log credentials in the frontend or accounts.json.
- Preserve Cursor write verification and rollback behavior. Never force-quit Cursor without an explicit user confirmation.
- Validate frontend changes with npm run build; validate Rust changes with cargo test --manifest-path src-tauri/Cargo.toml.
- Run the native test app with npm run tauri dev; npm run dev starts only Vite.
