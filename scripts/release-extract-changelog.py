#!/usr/bin/env python3
"""Extract one release section body from CHANGELOG.md."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


def extract_section(text: str, section: str) -> str:
    match = re.search(
        rf"^## \[{re.escape(section)}\][^\n]*\n(.*?)(?=^## \[|\Z)",
        text,
        re.MULTILINE | re.DOTALL,
    )
    if match is None or not match.group(1).strip():
        raise ValueError(f"CHANGELOG.md has no non-empty section for [{section}]")
    return match.group(1).strip()


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Extract one release section body from CHANGELOG.md."
    )
    parser.add_argument(
        "section",
        help="Heading token inside brackets, e.g. '1.7.1' or 'adotenv-lint 0.2.3'",
    )
    parser.add_argument(
        "--changelog",
        default="CHANGELOG.md",
        help="Path to the changelog file (default: CHANGELOG.md)",
    )
    args = parser.parse_args()
    changelog = Path(args.changelog)
    try:
        text = changelog.read_text(encoding="utf-8")
        body = extract_section(text, args.section)
    except (OSError, ValueError) as error:
        print(f"::error::{error}", file=sys.stderr)
        return 1
    print(body)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
