#!/usr/bin/env python3
"""Check public Markdown links, punctuation and common filler phrases."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FILES = [
    ROOT / "README.md",
    ROOT / "ROADMAP.md",
    ROOT / "RELEASING.md",
    *sorted(ROOT.glob("adotenv-*/README.md")),
    *sorted((ROOT / "docs").rglob("*.md")),
]
SMART = "—–’“”…•"
BANNED = re.compile(
    r"seamless|robust|comprehensive|delve|powerful|effortless|"
    r"whether you|in today|it's worth noting|crucial|moreover|furthermore|"
    r"overall|key takeaway|unlock|leverage|game[- ]chang|revolution|"
    r"cutting[- ]edge|state[- ]of[- ]the[- ]art|not just|isn't just|"
    r"at its core|in conclusion|simply put|look no further|"
    r"designed to|ensures that|this means that",
    re.IGNORECASE,
)

errors: list[str] = []
for file in FILES:
    text = file.read_text()
    for token in SMART:
        if token in text:
            errors.append(f"{file}: contains smart punctuation {token!r}")
    for match in BANNED.finditer(text):
        errors.append(f"{file}: contains banned phrase {match.group(0)!r}")

    for match in re.finditer(r"\[[^\]]*\]\(([^)]+)\)", text):
        target = match.group(1).strip()
        if target.startswith(("http://", "https://", "#", "mailto:")):
            continue
        target = target.split("#", 1)[0]
        if not target:
            continue
        resolved = (file.parent / target).resolve()
        if not resolved.exists():
            errors.append(f"{file}: missing link target {target!r}")

if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)
print(f"checked {len(FILES)} public Markdown files")
