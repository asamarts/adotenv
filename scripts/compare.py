#!/usr/bin/env python3
"""Run the adotenv comparison harness and render its benchmark tables."""

from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import shutil
import socket
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
COMPARE = ROOT / "benchmarks" / "compare"
TARGET = ROOT / "target" / "compare"
CARGO_TARGET = TARGET / "cargo"
FIXTURES = TARGET / "fixtures"
RESULTS = TARGET / "results.json"
REPORT = TARGET / "report.md"
DOC = ROOT / "docs" / "design" / "competitive-analysis.md"
BEGIN = "<!-- BEGIN GENERATED BENCHMARKS -->"
END = "<!-- END GENERATED BENCHMARKS -->"

MODES = [
    ("adotenv_parse", "adotenv parse"),
    ("adotenv_parse_values", "adotenv parse + values"),
    ("adotenv_map", "adotenv parse + map"),
    ("dotenvy_map", "dotenvy iterator + map"),
    ("dotenv_compat", "dotenv-compat parse"),
    ("dotenv_ng", "dotenv-ng input-only"),
    ("dotenv_parser", "dotenv-parser map"),
]

LOAD_MODES = [
    ("adotenv_values", "values-only load"),
    ("adotenv_load_default", "default load"),
    ("adotenv_load_full", "full-report load"),
]

BINS = {
    "adotenv_parse": "adotenv",
    "adotenv_parse_values": "adotenv",
    "adotenv_map": "adotenv",
    "dotenvy_map": "dotenvy",
    "dotenv_compat": "dotenv_compat",
    "dotenv_ng": "dotenv_ng",
    "dotenv_parser": "dotenv_parser",
}

BUDGETS = {
    "adotenv_parse": {"instructions": 260_000_000, "retained": 29.0},
    "adotenv_parse_values": {
        "instructions": 400_000_000,
        "peak": 12.0,
        "retained": 12.0,
    },
    "adotenv_map": {"instructions": 335_000_000, "retained": 16.0},
    "adotenv_values": {
        "instructions": 400_000_000,
        "peak": 13.0,
        "retained": 11.5,
    },
    "adotenv_load_default": {
        "instructions": 560_000_000,
        "peak": 50.0,
        "retained": 30.0,
    },
    "adotenv_load_full": {
        "instructions": 750_000_000,
        "peak": 60.0,
        "retained": 40.0,
    },
}

BINARY_BUDGET = 500 * 1024


def run(cmd: list[str], *, cwd: Path = ROOT) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=cwd, check=True, text=True, capture_output=True)


def build() -> None:
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(CARGO_TARGET)
    subprocess.run(
        [
            "cargo",
            "build",
            "--release",
            "--locked",
            "--manifest-path",
            str(COMPARE / "Cargo.toml"),
            "--bins",
        ],
        cwd=ROOT,
        env=env,
        check=True,
    )


def make_fixtures() -> tuple[Path, Path]:
    FIXTURES.mkdir(parents=True, exist_ok=True)
    small = FIXTURES / "small.env"
    large = FIXTURES / "large.env"
    small.write_text("".join(f"KEY_{i:03}=value_{i:03}\n" for i in range(128)))
    large.write_text("".join(f"KEY_{i:05}=value_{i:05}\n" for i in range(65_536)))
    return small, large


def main_binary() -> Path:
    return CARGO_TARGET / "release" / "adotenv-compare"


def binary(name: str) -> Path:
    return CARGO_TARGET / "release" / name


def rss(mode: str, fixture: Path) -> tuple[float, float]:
    result = run([str(main_binary()), f"rss_{mode}", "1", str(fixture)])
    match = re.search(r"rss_kb=(\d+) hwm_kb=(\d+)", result.stdout)
    if not match:
        raise RuntimeError(result.stdout)
    return int(match.group(1)) / 1024, int(match.group(2)) / 1024


def wall_time(mode: str, fixture: Path, iterations: int) -> float:
    export = TARGET / f"hyperfine-{mode}-{fixture.stem}.json"
    command = " ".join(
        shlex.quote(str(part))
        for part in [str(main_binary()), mode, str(iterations), str(fixture)]
    )
    subprocess.run(
        [
            "hyperfine",
            "--warmup",
            "1",
            "--runs",
            "5",
            "--export-json",
            str(export),
            command,
        ],
        cwd=ROOT,
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return json.loads(export.read_text())["results"][0]["median"] / iterations


def instructions(mode: str, fixture: Path) -> int:
    output = TARGET / f"callgrind-{mode}.out"
    subprocess.run(
        [
            "valgrind",
            "--tool=callgrind",
            f"--callgrind-out-file={output}",
            str(main_binary()),
            mode,
            "1",
            str(fixture),
        ],
        cwd=ROOT,
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    with output.open() as handle:
        for line in reversed(handle.readlines()):
            if line.startswith("totals:"):
                return int(line.split()[1])
    raise RuntimeError(f"no callgrind totals in {output}")


def duration(value: float) -> str:
    if value >= 1.0:
        return f"{value:.2f} s"
    if value >= 0.001:
        return f"{value * 1000:.1f} ms"
    return f"{value * 1_000_000:.1f} us"


def memory(megabytes: float) -> str:
    return f"{megabytes:.1f} MB"


def make_report(
    small: Path,
    large: Path,
    *,
    include_instructions: bool,
) -> tuple[str, list[dict]]:
    small_iterations = 100
    large_iterations = 5
    base_rss, base_hwm = rss("noop", large)
    noop_instructions = instructions("noop", large) if include_instructions else 0

    rows = []
    for mode, label in MODES + LOAD_MODES:
        small_time = wall_time(mode, small, small_iterations)
        large_time = wall_time(mode, large, large_iterations)
        retained, peak = rss(mode, large)
        instruction_count = (
            instructions(mode, large) - noop_instructions
            if include_instructions
            else None
        )
        rows.append(
            {
                "mode": mode,
                "label": label,
                "small": small_time,
                "large": large_time,
                "instructions": instruction_count,
                "peak": peak - base_hwm,
                "retained": retained - base_rss,
            }
        )

    lines = [
        BEGIN,
        "Generated by `python3 scripts/compare.py`.",
        "",
        "### Parse and map build",
        "",
        "Per parse on the 65,536-entry fixture:",
        "",
        "| Implementation | Wall time | Instructions | Peak RSS | Retained RSS |",
        "|---|---:|---:|---:|---:|",
    ]
    core_rows = rows[: len(MODES)]
    load_rows = rows[len(MODES) :]
    for row in core_rows:
        instructions_text = (
            f"{row['instructions'] / 1_000_000:.0f}M"
            if row["instructions"] is not None
            else "not measured"
        )
        lines.append(
            f"| {row['label']} | {duration(row['large'])} | "
            f"{instructions_text} | {memory(row['peak'])} | "
            f"{memory(row['retained'])} |"
        )
    lines += [
        "",
        "Small-file parsing, in microseconds per parse:",
        "",
        "| Implementation | 128 entries |",
        "|---|---:|",
    ]
    for row in core_rows:
        lines.append(f"| {row['label']} | {duration(row['small'])} |")
    lines += [
        "",
        "### Loader profiles",
        "",
        "Per load on the 65,536-entry fixture:",
        "",
        "| Profile | Wall time | Instructions | Peak RSS | Retained RSS |",
        "|---|---:|---:|---:|---:|",
    ]
    for row in load_rows:
        instructions_text = (
            f"{row['instructions'] / 1_000_000:.0f}M"
            if row["instructions"] is not None
            else "not measured"
        )
        lines.append(
            f"| {row['label']} | {duration(row['large'])} | "
            f"{instructions_text} | {memory(row['peak'])} | "
            f"{memory(row['retained'])} |"
        )
    lines += [
        "",
        "### Parse-only binaries",
        "",
        "| Binary | Stripped size |",
        "|---|---:|",
    ]
    for name in sorted({BINS[mode] for mode, _ in MODES}):
        size = binary(name).stat().st_size
        lines.append(f"| {name} | {size / 1024:.0f} KB |")
    lines += ["", END]
    return "\n".join(lines), rows


def check_budgets(rows: list[dict]) -> int:
    failed = 0
    size = binary("adotenv").stat().st_size
    if size > BINARY_BUDGET:
        print(
            f"budget failed: adotenv parse binary={size} limit={BINARY_BUDGET}",
            file=sys.stderr,
        )
        failed += 1
    for row in rows:
        budget = BUDGETS.get(row["mode"])
        if not budget:
            continue
        for key, limit in budget.items():
            value = row[key]
            if value is None or value > limit:
                print(f"budget failed: {row['label']} {key}={value} limit={limit}", file=sys.stderr)
                failed += 1
    return failed


def update_doc(report: str) -> None:
    text = DOC.read_text()
    start = text.index(BEGIN)
    end = text.index(END, start) + len(END)
    DOC.write_text(text[:start] + report + text[end:])


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--smoke", action="store_true", help="small fixture, no instructions")
    parser.add_argument("--update-doc", action="store_true", help="replace the generated doc section")
    parser.add_argument("--check-budgets", action="store_true", help="fail when a budget is exceeded")
    args = parser.parse_args()

    TARGET.mkdir(parents=True, exist_ok=True)
    required = ["cargo"] if args.smoke else ["cargo", "hyperfine", "valgrind"]
    missing = [name for name in required if shutil.which(name) is None]
    if missing:
        print(f"missing tools: {', '.join(missing)}", file=sys.stderr)
        return 1

    build()
    small, large = make_fixtures()
    if args.smoke:
        for mode, label in MODES:
            result = run([str(main_binary()), mode, "1", str(small)])
            print(f"{label}: {result.stdout.strip()}")
        return 0

    report, rows = make_report(small, large, include_instructions=True)
    failed = check_budgets(rows) if args.check_budgets else 0
    REPORT.write_text(report + "\n")
    RESULTS.write_text(
        json.dumps(
            {
                "host": socket.gethostname(),
                "fixtures": {"small": str(small), "large": str(large)},
                "rows": rows,
            },
            indent=2,
        )
        + "\n"
    )
    if args.update_doc:
        update_doc(report)
    print(report)
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
