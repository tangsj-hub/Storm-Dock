#!/usr/bin/env bash
# Collect macOS install + updater artifacts into release-assets/ with stable names.
# Fail closed if .dmg, .app.tar.gz, or .sig is missing.
set -euo pipefail

VERSION_TAG="${VERSION_TAG:-${GITHUB_REF_NAME:-}}"
if [ -z "$VERSION_TAG" ]; then
  echo "❌ VERSION_TAG / GITHUB_REF_NAME required" >&2
  exit 1
fi
VERSION="${VERSION_TAG#v}"
PREFIX="Storm-Dock-${VERSION}-macOS"

mkdir -p release-assets

MACOS_DIRS=(
  "src-tauri/target/universal-apple-darwin/release/bundle/macos"
  "src-tauri/target/aarch64-apple-darwin/release/bundle/macos"
  "src-tauri/target/x86_64-apple-darwin/release/bundle/macos"
  "src-tauri/target/release/bundle/macos"
)
DMG_DIRS=(
  "src-tauri/target/universal-apple-darwin/release/bundle/dmg"
  "src-tauri/target/aarch64-apple-darwin/release/bundle/dmg"
  "src-tauri/target/x86_64-apple-darwin/release/bundle/dmg"
  "src-tauri/target/release/bundle/dmg"
)

APP_PATH=""
TAR_GZ=""
DMG_PATH=""

for path in "${MACOS_DIRS[@]}"; do
  [ -d "$path" ] || continue
  echo "=== macos dir: $path ==="
  ls -la "$path" || true
  if [ -z "$APP_PATH" ]; then
    APP_PATH=$(find "$path" -maxdepth 1 -name '*.app' -type d | head -1 || true)
  fi
  if [ -z "$TAR_GZ" ]; then
    TAR_GZ=$(find "$path" -maxdepth 1 \( -name '*.app.tar.gz' -o -name '*.tar.gz' \) -type f | head -1 || true)
  fi
done

for path in "${DMG_DIRS[@]}"; do
  [ -d "$path" ] || continue
  echo "=== dmg dir: $path ==="
  ls -la "$path" || true
  if [ -z "$DMG_PATH" ]; then
    DMG_PATH=$(find "$path" -maxdepth 1 -name '*.dmg' -type f | head -1 || true)
  fi
done

if [ -z "$APP_PATH" ]; then
  echo "❌ No .app bundle found" >&2
  exit 1
fi
echo "Using .app: $APP_PATH"

if [ -z "$TAR_GZ" ]; then
  MAC_DIR=$(dirname "$APP_PATH")
  TAR_GZ="$MAC_DIR/$(basename "$APP_PATH").tar.gz"
  echo "Creating updater archive: $TAR_GZ"
  tar -czf "$TAR_GZ" -C "$MAC_DIR" "$(basename "$APP_PATH")"
fi

if [ ! -f "${TAR_GZ}.sig" ]; then
  KEY_PATH="${TAURI_SIGNING_KEY_PATH:-${RUNNER_TEMP:-/tmp}/tauri_signing.key}"
  if [ ! -f "$KEY_PATH" ]; then
    echo "❌ Missing signing key at $KEY_PATH to sign $TAR_GZ" >&2
    exit 1
  fi
  echo "Signing $TAR_GZ with $KEY_PATH"
  npx --yes tauri signer sign "$TAR_GZ" -f "$KEY_PATH"
fi

if [ ! -f "${TAR_GZ}.sig" ]; then
  echo "❌ Missing ${TAR_GZ}.sig after sign" >&2
  exit 1
fi

if [ -z "$DMG_PATH" ] || [ ! -f "$DMG_PATH" ]; then
  echo "❌ No .dmg found — refusing incomplete macOS release assets" >&2
  exit 1
fi

cp "$DMG_PATH" "release-assets/${PREFIX}.dmg"
cp "$TAR_GZ" "release-assets/${PREFIX}.app.tar.gz"
cp "${TAR_GZ}.sig" "release-assets/${PREFIX}.app.tar.gz.sig"

echo "✅ macOS release-assets:"
ls -la release-assets/
# Fail closed size checks
test -s "release-assets/${PREFIX}.dmg"
test -s "release-assets/${PREFIX}.app.tar.gz"
test -s "release-assets/${PREFIX}.app.tar.gz.sig"
