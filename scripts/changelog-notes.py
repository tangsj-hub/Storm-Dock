#!/usr/bin/env python3
"""Print JSON-encoded release notes from CHANGELOG.md for TAG / VERSION."""
import json
import os
import pathlib
import re
import sys

tag = os.environ.get("TAG") or (sys.argv[1] if len(sys.argv) > 1 else "")
if not tag:
    raise SystemExit("TAG env or argv required")
version = tag.lstrip("v")
notes = f"Release {tag}"
path = pathlib.Path("CHANGELOG.md")
if path.exists():
    text = path.read_text(encoding="utf-8")
    pattern = rf"^## \[?{re.escape(version)}\]?[^\n]*\n(.*?)(?=^## |\Z)"
    match = re.search(pattern, text, flags=re.M | re.S)
    if match:
        body = re.sub(r"<!--.*?-->", "", match.group(1), flags=re.S).strip()
        if body:
            notes = body[:4000]
print(json.dumps(notes))
