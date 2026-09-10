#!/usr/bin/env python3
"""Assemble Tauri updater latest.json from downloaded release assets in ./dl.

Expects stable Storm Dock asset names produced by scripts/ci/package-*-assets.sh:

  Storm-Dock-<ver>-macOS.app.tar.gz[+.sig]
  Storm-Dock-<ver>-Windows-x64.msi[+.sig]

Fail closed unless both darwin and windows-x86_64 platforms are present.
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


def pick_windows(dl: pathlib.Path) -> tuple[pathlib.Path, pathlib.Path] | None:
    preferred = sorted(dl.glob("Storm-Dock-*-Windows-x64.msi"))
    candidates = preferred + sorted(dl.glob("*.msi"))
    # Prefer MSI for updater (matches Tauri Windows updater defaults)
    for art in candidates:
        if art.name.endswith(".sig"):
            continue
        sig = pathlib.Path(str(art) + ".sig")
        if art.is_file() and sig.is_file():
            return art, sig
    # Fallback: NSIS exe / nsis.zip
    for pattern in (
        "Storm-Dock-*-Windows-x64-setup.exe",
        "*.nsis.zip",
        "*-setup.exe",
    ):
        for art in sorted(dl.glob(pattern)):
            if art.name.endswith(".sig"):
                continue
            sig = pathlib.Path(str(art) + ".sig")
            if art.is_file() and sig.is_file():
                return art, sig
    return None


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
    if "macOS".lower() in lower or "macos" in lower or "universal" in lower:
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
    mac_entry = {
        "signature": read_sig(mac_sig),
        "url": f"{base_url}/{mac_art.name}",
    }
    for key in darwin_keys_for(mac_art.name):
        platforms[key] = mac_entry
    print(f"✅ macOS updater: {mac_art.name}")

    win = pick_windows(dl)
    if not win:
        raise SystemExit(
            "Missing Windows updater artifact (Storm-Dock-*-Windows-x64.msi + .sig). "
            "Refusing to publish latest.json."
        )
    win_art, win_sig = win
    platforms["windows-x86_64"] = {
        "signature": read_sig(win_sig),
        "url": f"{base_url}/{win_art.name}",
    }
    print(f"✅ Windows updater: {win_art.name}")

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
