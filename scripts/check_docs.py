#!/usr/bin/env python3
"""Check public Markdown links, book coverage, punctuation and filler phrases."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
FILES = [
    ROOT / "README.md",
    ROOT / "ROADMAP.md",
    ROOT / "RELEASING.md",
    *sorted(ROOT.glob("adotenv-*/README.md")),
    *sorted(DOCS.rglob("*.md")),
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

    for match in re.finditer(r"```json\n(.*?)\n```", text, re.DOTALL):
        block = match.group(1)
        try:
            value = json.loads(block)
        except json.JSONDecodeError as error:
            errors.append(f"{file}: invalid JSON code block: {error}")
            continue
        expected = json.dumps(value, indent=2, ensure_ascii=False)
        if block.strip() != expected.strip():
            errors.append(f"{file}: JSON code block is not pretty-printed")

    targets = [match.group(1) for match in re.finditer(r"\[[^\]]*\]\(([^)]+)\)", text)]
    if DOCS in file.parents:
        # Raw HTML in book pages links to the rendered `.html` page.
        targets += [
            re.sub(r"\.html(?=$|#)", ".md", match.group(1))
            for match in re.finditer(r'href="([^"]+)"', text)
        ]
    for target in targets:
        target = target.strip()
        if target.startswith(("http://", "https://", "#", "mailto:")):
            continue
        target = target.split("#", 1)[0]
        if not target:
            continue
        resolved = (file.parent / target).resolve()
        if not resolved.exists():
            errors.append(f"{file}: missing link target {target!r}")
        elif DOCS in file.parents and DOCS not in resolved.parents:
            errors.append(f"{file}: link target {target!r} is outside the book")

summary = (DOCS / "SUMMARY.md").read_text()
listed = {
    (DOCS / target).resolve()
    for target in re.findall(r"\]\(([^)#]+\.md)\)", summary)
}
for page in sorted(DOCS.rglob("*.md")):
    if page.name != "SUMMARY.md" and page.resolve() not in listed:
        errors.append(f"{page}: not listed in docs/SUMMARY.md")

if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)
print(f"checked {len(FILES)} public Markdown files")
