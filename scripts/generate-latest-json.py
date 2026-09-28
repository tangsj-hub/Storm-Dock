#!/usr/bin/env python3
"""Assemble Tauri updater latest.json from downloaded release assets in ./dl.

Expects stable Storm Dock asset names produced by scripts/ci/package-*-assets.sh:

  Storm-Dock-<ver>-macOS.app.tar.gz[+.sig]
  Storm-Dock-<ver>-Windows-x64-setup.exe[+.sig]  # NSIS (preferred updater)
  Storm-Dock-<ver>-Windows-x64.msi[+.sig]         # MSI (installer-specific key)

Fail closed unless both darwin and windows-x86_64 platforms are present.

Windows note:
  Users install via NSIS and may choose a custom directory / current-user vs
  all-users. Tauri's NSIS /UPDATE path restores $INSTDIR from the registry.
  Publishing MSI as windows-x86_64 made NSIS installs fall back to the per-user
  MSI default under %LocalAppData%\\Programs (often on C:), creating a second
  install. Prefer NSIS for the generic key, and advertise installer-specific
  keys so each existing install keeps matching its original installer type.
"""
from __future__ import annotations

import json
import os
import pathlib
import re
import subprocess
import sys
from datetime import datetime, timezone


REQUIRED_DARWIN = ("darwin-aarch64", "darwin-aarch64-app")
REQUIRED_WINDOWS = ("windows-x86_64",)


def changelog_notes(tag: str) -> str:
    env = os.environ.copy()
    env["TAG"] = tag
    out = subprocess.check_output(
        [sys.executable, "scripts/changelog-notes.py"],
        env=env,
        text=True,
    )
    return json.loads(out)


def read_sig(sig: pathlib.Path) -> str:
    return sig.read_text(encoding="utf-8").replace("\r", "").replace("\n", "")


def pick_signed(dl: pathlib.Path, patterns: tuple[str, ...]) -> tuple[pathlib.Path, pathlib.Path] | None:
    for pattern in patterns:
        for art in sorted(dl.glob(pattern)):
            if art.name.endswith(".sig"):
                continue
            sig = pathlib.Path(str(art) + ".sig")
            if art.is_file() and sig.is_file():
                return art, sig
    return None


def pick_mac(dl: pathlib.Path) -> tuple[pathlib.Path, pathlib.Path] | None:
    preferred = sorted(dl.glob("Storm-Dock-*-macOS.app.tar.gz"))
    preferred += sorted(dl.glob("Storm-Dock-*-macOS.tar.gz"))
    candidates = preferred + sorted(
        p
        for p in dl.glob("*.tar.gz")
        if not p.name.endswith(".sig") and "windows" not in p.name.lower()
    )
    for art in candidates:
        sig = pathlib.Path(str(art) + ".sig")
        if art.is_file() and sig.is_file():
            return art, sig
    return None


def pick_windows_nsis(dl: pathlib.Path) -> tuple[pathlib.Path, pathlib.Path] | None:
    return pick_signed(
        dl,
        (
            "Storm-Dock-*-Windows-x64-setup.exe",
            "*-setup.exe",
            "*.nsis.zip",
        ),
    )


def pick_windows_msi(dl: pathlib.Path) -> tuple[pathlib.Path, pathlib.Path] | None:
    return pick_signed(
        dl,
        (
            "Storm-Dock-*-Windows-x64.msi",
            "*.msi",
        ),
    )


def platform_entry(base_url: str, art: pathlib.Path, sig: pathlib.Path) -> dict[str, str]:
    return {
        "signature": read_sig(sig),
        "url": f"{base_url}/{art.name}",
    }


def darwin_keys_for(name: str) -> list[str]:
    """Advertise platform keys Tauri clients may request."""
    lower = name.lower()
    keys = [
        "darwin-aarch64",
        "darwin-aarch64-app",
        "darwin-x86_64",
        "darwin-x86_64-app",
    ]
    # Universal / stable Storm-Dock-*-macOS.* → all keys point at same artifact
    if "macos" in lower or "universal" in lower:
        return keys
    if "aarch64" in lower or "arm64" in lower:
        return ["darwin-aarch64", "darwin-aarch64-app"]
    if re.search(r"(x64|x86_64|amd64)", lower):
        return ["darwin-x86_64", "darwin-x86_64-app"]
    return keys


def main() -> None:
    repo = os.environ["REPO"]
    tag = os.environ["TAG"]
    version = tag.lstrip("v")
    base_url = f"https://github.com/{repo}/releases/download/{tag}"
    dl = pathlib.Path("dl")
    if not dl.is_dir():
        raise SystemExit("dl/ directory missing")

    print("Assets in dl/:")
    for p in sorted(dl.iterdir()):
        print(f"  {p.name}")

    platforms: dict[str, dict[str, str]] = {}

    mac = pick_mac(dl)
    if not mac:
        raise SystemExit(
            "Missing macOS updater artifact (Storm-Dock-*-macOS.app.tar.gz + .sig). "
            "Refusing to publish latest.json."
        )
    mac_art, mac_sig = mac
    mac_entry = platform_entry(base_url, mac_art, mac_sig)
    for key in darwin_keys_for(mac_art.name):
        platforms[key] = mac_entry
    print(f"✅ macOS updater: {mac_art.name}")

    # Prefer NSIS for the generic windows-x86_64 key so first-time / unmatched
    # clients update with the same installer users download manually. Also emit
    # installer-specific keys so an existing NSIS or MSI install keeps matching
    # its original installer (and therefore its InstallDir).
    nsis = pick_windows_nsis(dl)
    msi = pick_windows_msi(dl)
    if not nsis and not msi:
        raise SystemExit(
            "Missing Windows updater artifact "
            "(Storm-Dock-*-Windows-x64-setup.exe + .sig, or .msi + .sig). "
            "Refusing to publish latest.json."
        )

    if nsis:
        nsis_art, nsis_sig = nsis
        nsis_entry = platform_entry(base_url, nsis_art, nsis_sig)
        platforms["windows-x86_64-nsis"] = nsis_entry
        platforms["windows-x86_64"] = nsis_entry
        print(f"✅ Windows NSIS updater: {nsis_art.name}")

    if msi:
        msi_art, msi_sig = msi
        msi_entry = platform_entry(base_url, msi_art, msi_sig)
        platforms["windows-x86_64-msi"] = msi_entry
        if "windows-x86_64" not in platforms:
            platforms["windows-x86_64"] = msi_entry
        print(f"✅ Windows MSI updater: {msi_art.name}")

    if "windows-x86_64-nsis" not in platforms:
        print(
            "⚠️  NSIS setup.exe missing; windows-x86_64 falls back to MSI. "
            "NSIS-installed apps may get a second install under LocalAppData\\Programs."
        )

    for key in REQUIRED_DARWIN:
        if key not in platforms:
            raise SystemExit(f"latest.json missing required platform key: {key}")
    for key in REQUIRED_WINDOWS:
        if key not in platforms:
            raise SystemExit(f"latest.json missing required platform key: {key}")

    payload = {
        "version": version,
        "notes": changelog_notes(tag),
        "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
    }
    out = pathlib.Path("latest.json")
    out.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps(payload, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
