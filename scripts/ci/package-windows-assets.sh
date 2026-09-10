#!/usr/bin/env bash
# Collect Windows NSIS/MSI + updater signatures into release-assets/ with stable names.
set -euo pipefail

VERSION_TAG="${VERSION_TAG:-${GITHUB_REF_NAME:-}}"
if [ -z "$VERSION_TAG" ]; then
  echo "❌ VERSION_TAG / GITHUB_REF_NAME required" >&2
  exit 1
fi
VERSION="${VERSION_TAG#v}"
PREFIX="Storm-Dock-${VERSION}-Windows-x64"

mkdir -p release-assets

ROOTS=(
  "src-tauri/target/release"
  "src-tauri/target/x86_64-pc-windows-msvc/release"
)

MSI=""
EXE=""
for root in "${ROOTS[@]}"; do
  [ -d "$root/bundle" ] || continue
  echo "=== windows root: $root/bundle ==="
  find "$root/bundle" -type f \( -name '*.msi' -o -name '*.exe' -o -name '*.sig' \) -print || true
  if [ -z "$MSI" ]; then
    MSI=$(find "$root/bundle/msi" -maxdepth 1 -name '*.msi' -type f 2>/dev/null | head -1 || true)
  fi
  if [ -z "$EXE" ]; then
    EXE=$(find "$root/bundle/nsis" -maxdepth 1 -name '*.exe' -type f 2>/dev/null | head -1 || true)
  fi
done

if [ -z "$MSI" ] || [ ! -f "$MSI" ]; then
  echo "❌ No Windows .msi found" >&2
  exit 1
fi
if [ -z "$EXE" ] || [ ! -f "$EXE" ]; then
  echo "❌ No Windows NSIS .exe found" >&2
  exit 1
fi

if [ ! -f "${MSI}.sig" ]; then
  echo "❌ Missing ${MSI}.sig (createUpdaterArtifacts / signing failed)" >&2
  exit 1
fi
if [ ! -f "${EXE}.sig" ]; then
  echo "❌ Missing ${EXE}.sig (createUpdaterArtifacts / signing failed)" >&2
  exit 1
fi

cp "$MSI" "release-assets/${PREFIX}.msi"
cp "${MSI}.sig" "release-assets/${PREFIX}.msi.sig"
cp "$EXE" "release-assets/${PREFIX}-setup.exe"
cp "${EXE}.sig" "release-assets/${PREFIX}-setup.exe.sig"

echo "✅ Windows release-assets:"
ls -la release-assets/
test -s "release-assets/${PREFIX}.msi"
test -s "release-assets/${PREFIX}.msi.sig"
test -s "release-assets/${PREFIX}-setup.exe"
test -s "release-assets/${PREFIX}-setup.exe.sig"
