#!/usr/bin/env bash
# Normalize TAURI_SIGNING_PRIVATE_KEY into a two-line minisign file and export
# TAURI_SIGNING_PRIVATE_KEY (base64) for the Tauri CLI.
set -euo pipefail

if [ -z "${TAURI_SIGNING_PRIVATE_KEY_RAW:-}" ]; then
  echo "❌ TAURI_SIGNING_PRIVATE_KEY is empty or missing." >&2
  echo "Add the contents of .tauri/storm-dock.key as a repository secret." >&2
  echo "See docs/updater.md." >&2
  exit 1
fi

KEY_PATH="${TAURI_SIGNING_KEY_PATH:-${RUNNER_TEMP:-/tmp}/tauri_signing.key}"
RAW="$TAURI_SIGNING_PRIVATE_KEY_RAW"

write_key() {
  printf '%s\n' "$1" > "$KEY_PATH"
}

decode_b64() {
  printf '%s' "$1" | (base64 --decode 2>/dev/null || base64 -D 2>/dev/null)
}

if echo "$RAW" | head -n1 | grep -q '^untrusted comment:'; then
  write_key "$RAW"
  echo "✅ Using raw two-line key file format"
else
  DECODED=""
  if DECODED=$(decode_b64 "$RAW") && echo "$DECODED" | head -n1 | grep -q '^untrusted comment:'; then
    write_key "$DECODED"
    echo "✅ Decoded base64-wrapped key"
  elif echo "$RAW" | grep -Eq '^[A-Za-z0-9+/=[:space:]]+$'; then
    ONE=$(printf '%s' "$RAW" | tr -d '\r\n[:space:]')
    if DECODED=$(decode_b64 "$ONE") && echo "$DECODED" | head -n1 | grep -q '^untrusted comment:'; then
      write_key "$DECODED"
      echo "✅ Decoded single-line base64 key"
    else
      printf '%s\n%s\n' "untrusted comment: tauri signing key" "$ONE" > "$KEY_PATH"
      echo "✅ Constructed two-line file from one-line Base64"
    fi
  else
    echo "❌ TAURI_SIGNING_PRIVATE_KEY format not recognized" >&2
    exit 1
  fi
fi

if ! head -n1 "$KEY_PATH" | grep -q '^untrusted comment:'; then
  echo "❌ Signing key file missing minisign header" >&2
  exit 1
fi

if command -v base64 >/dev/null 2>&1; then
  KEY_B64=$(base64 < "$KEY_PATH" | tr -d '\r\n')
else
  KEY_B64=$(KEY_PATH="$KEY_PATH" node -e "process.stdout.write(require('fs').readFileSync(process.env.KEY_PATH).toString('base64'))")
fi
if [ -z "$KEY_B64" ]; then
  echo "❌ Failed to base64-encode signing key" >&2
  exit 1
fi

{
  echo "TAURI_SIGNING_PRIVATE_KEY=$KEY_B64"
  echo "TAURI_SIGNING_KEY_PATH=$KEY_PATH"
  if [ -n "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ]; then
    echo "TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$TAURI_SIGNING_PRIVATE_KEY_PASSWORD"
  fi
} >> "$GITHUB_ENV"

echo "✅ Tauri signing key prepared at $KEY_PATH"
