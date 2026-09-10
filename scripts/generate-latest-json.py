#!/usr/bin/env python3
"""Assemble Tauri updater latest.json from downloaded release assets in ./dl."""
from __future__ import annotations

import json
import os
import pathlib
import re
import subprocess
import sys
from datetime import datetime, timezone


def changelog_notes(tag: str) -> str:
    env = os.environ.copy()
    env["TAG"] = tag
    out = subprocess.check_output(
        [sys.executable, "scripts/changelog-notes.py"],
        env=env,
        text=True,
    )
    return json.loads(out)


def arch_keys_from_name(name: str) -> list[str]:
    lower = name.lower()
    keys: list[str] = []
    if "aarch64" in lower or "arm64" in lower:
        keys.extend(["darwin-aarch64-app", "darwin-aarch64"])
    elif re.search(r"(x64|x86_64|amd64)", lower):
        keys.extend(["darwin-x86_64-app", "darwin-x86_64"])
    else:
        # Unknown mac artifact naming: advertise both so aarch64 clients can still update
        # when CI only ships one host arch under a generic name.
        keys.extend(
            [
                "darwin-aarch64-app",
                "darwin-aarch64",
                "darwin-x86_64-app",
                "darwin-x86_64",
            ]
        )
    return keys


def main() -> None:
    repo = os.environ["REPO"]
    tag = os.environ["TAG"]
    version = tag.lstrip("v")
    base_url = f"https://github.com/{repo}/releases/download/{tag}"
    dl = pathlib.Path("dl")
    if not dl.is_dir():
        raise SystemExit("dl/ directory missing")

    platforms: dict[str, dict[str, str]] = {}
    mac_found = False
    win_found = False

    for sig in sorted(dl.glob("*.sig")):
        artifact = pathlib.Path(str(sig)[: -len(".sig")])
        fname = artifact.name
        if not artifact.exists():
            # gh may download only .sig in odd cases; skip
            continue
        signature = sig.read_text(encoding="utf-8").replace("\r", "").replace("\n", "")
        url = f"{base_url}/{fname}"

        if fname.endswith(".app.tar.gz") or (
            fname.endswith(".tar.gz") and "windows" not in fname.lower()
        ):
            mac_found = True
            for key in arch_keys_from_name(fname):
                platforms[key] = {"signature": signature, "url": url}
        elif fname.endswith(".msi"):
            win_found = True
            platforms["windows-x86_64"] = {"signature": signature, "url": url}
        elif fname.endswith(".exe") and "windows-x86_64" not in platforms:
            win_found = True
            platforms["windows-x86_64"] = {"signature": signature, "url": url}
        elif fname.endswith(".nsis.zip"):
            win_found = True
            platforms["windows-x86_64"] = {"signature": signature, "url": url}

    if not platforms:
        raise SystemExit("No signed updater artifacts found for latest.json")
    if not mac_found:
        raise SystemExit(
            "Missing macOS updater artifact (*.app.tar.gz + .sig). "
            "Refusing to publish latest.json without darwin platforms."
        )
    if not win_found:
        raise SystemExit(
            "Missing Windows updater artifact (*.msi/*.exe + .sig). "
            "Refusing to publish latest.json without windows-x86_64."
        )

    payload = {
        "version": version,
        "notes": changelog_notes(tag),
        "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
    }
    pathlib.Path("latest.json").write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(payload, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
