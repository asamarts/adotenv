#!/usr/bin/env python3
"""Measure clean build time, feature-set size and startup time."""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
COMPARE = ROOT / "benchmarks" / "compare"
TARGET = ROOT / "target" / "engineering"
EMPTY = TARGET / "empty.env"
OUTPUT = COMPARE / "engineering.json"
BINS = ["adotenv", "dotenvy", "dotenv_compat", "dotenv_ng", "dotenv_parser"]
FEATURES = [
    ("alloc", ["--no-default-features"]),
    ("std", []),
    ("load", ["--features", "load"]),
    ("serde", ["--features", "serde"]),
    ("load-serde", ["--features", "load,serde"]),
    ("exec", ["--features", "exec"]),
]


def run(cmd: list[str], *, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=ROOT, env=env, check=True, text=True, capture_output=True)


def metric(path: Path) -> float:
    data = json.loads(path.read_text())["results"][0]
    summary = data.get("summary")
    if summary is not None:
        return summary["time_wall_clock"].get("value", summary["time_wall_clock"].get("median"))
    return data.get("median", data.get("mean"))


def build_bin(name: str) -> dict:
    target = TARGET / f"clean-{name}"
    shutil.rmtree(target, ignore_errors=True)
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target)
    started = time.perf_counter()
    run(
        [
            "cargo",
            "build",
            "--release",
            "--locked",
            "--manifest-path",
            str(COMPARE / "Cargo.toml"),
            "--bin",
            name,
        ],
        env=env,
    )
    elapsed = time.perf_counter() - started
    binary = target / "release" / name
    return {"name": name, "seconds": elapsed, "bytes": binary.stat().st_size}


def build_feature(name: str, args: list[str]) -> dict:
    target = TARGET / f"feature-{name}"
    shutil.rmtree(target, ignore_errors=True)
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target)
    started = time.perf_counter()
    run(
        [
            "cargo",
            "build",
            "-p",
            "adotenv",
            "--lib",
            "--release",
            "--locked",
            "--target-dir",
            str(target),
            *args,
        ],
        env=env,
    )
    elapsed = time.perf_counter() - started
    rlibs = list((target / "release" / "deps").glob("libadotenv-*.rlib"))
    size = max((path.stat().st_size for path in rlibs), default=0)
    return {"name": name, "seconds": elapsed, "bytes": size}


def startup(name: str) -> dict:
    export = TARGET / f"startup-{name}.json"
    command = f"{TARGET / 'compare' / 'release' / name} {EMPTY}"
    # The compare binary is built in target/compare, not target/engineering.
    command = f"{ROOT / 'target' / 'compare' / 'cargo' / 'release' / name} {EMPTY}"
    subprocess.run(
        [
            "hyperfine",
            "--warmup",
            "5",
            "--runs",
            "20",
            "--export-json",
            str(export),
            command,
        ],
        cwd=ROOT,
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return {"name": name, "seconds": metric(export)}


def main() -> int:
    TARGET.mkdir(parents=True, exist_ok=True)
    EMPTY.write_text("")
    if shutil.which("cargo") is None or shutil.which("hyperfine") is None:
        print("cargo and hyperfine are required", file=sys.stderr)
        return 1

    data = {
        "binaries": [build_bin(name) for name in BINS],
        "features": [build_feature(name, args) for name, args in FEATURES],
        "startup": [startup(name) for name in BINS],
    }
    OUTPUT.write_text(json.dumps(data, indent=2) + "\n")
    for section, rows in data.items():
        print(section)
        for row in rows:
            if "bytes" in row:
                print(f"  {row['name']}: {row['seconds']:.2f}s {row['bytes'] / 1024:.0f}KB")
            else:
                print(f"  {row['name']}: {row['seconds'] * 1_000_000:.1f}us")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
