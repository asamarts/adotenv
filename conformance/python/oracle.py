#!/usr/bin/env python3
"""Pinned python-dotenv observations; read one protocol request from stdin."""

import importlib.metadata
import io
import json
import logging
import os
import platform
import sys
import tempfile
from contextlib import contextmanager
from pathlib import Path

from dotenv import dotenv_values, load_dotenv
from dotenv.parser import parse_stream


VERSION = "1.2.4"
PROFILE_IDS = (
    "python-bindings-stream",
    "python-values-literal-stream",
    "python-values-literal-file",
    "python-values-resolved-stream",
    "python-load-literal-keep",
    "python-load-literal-override",
    "python-load-resolved-keep",
    "python-load-resolved-override",
)


@contextmanager
def controlled_environment(environment):
    previous = dict(os.environ)
    try:
        os.environ.clear()
        os.environ.update(environment)
        yield
    finally:
        os.environ.clear()
        os.environ.update(previous)


@contextmanager
def suppressed_dotenv_logging():
    logger = logging.getLogger("dotenv.main")
    previous = logger.disabled
    try:
        logger.disabled = True
        yield
    finally:
        logger.disabled = previous


def observe(profile, case):
    source = case["source"]
    with controlled_environment(case.get("environment", {})), suppressed_dotenv_logging():
        if profile == "python-values-literal-file":
            with tempfile.TemporaryDirectory(prefix="dotenv-conformance-") as directory:
                path = Path(directory) / "input.env"
                # Write the exact UTF-8 fixture bytes; upstream text-open performs
                # universal-newline translation, rather than this adapter.
                path.write_bytes(source.encode("utf-8"))
                return {"values": dict(dotenv_values(
                    dotenv_path=path, encoding="utf-8", verbose=False,
                    interpolate=False,
                ))}

        # StringIO's default newline='\n' retains existing CR and CRLF.
        with io.StringIO(source) as stream:
            if profile == "python-bindings-stream":
                return {"bindings": [
                    {
                        "key": binding.key,
                        "value": binding.value,
                        "original": {
                            "string": binding.original.string,
                            "line": binding.original.line,
                        },
                        "error": binding.error,
                    }
                    for binding in parse_stream(stream)
                ]}
            if profile in (
                "python-values-literal-stream", "python-values-resolved-stream"
            ):
                return {"values": dict(dotenv_values(
                    stream=stream, verbose=False, encoding="utf-8",
                    interpolate=profile == "python-values-resolved-stream",
                ))}
            try:
                loaded = load_dotenv(
                    stream=stream, verbose=False, encoding="utf-8",
                    interpolate=profile.startswith("python-load-resolved-"),
                    override=profile.endswith("-override"),
                )
            except ValueError as error:
                # The real process-environment API rejects embedded NULs and
                # invalid environment names. Retain any preceding assignments.
                return {
                    "loaded": None,
                    "environment": dict(os.environ),
                    "error": type(error).__name__,
                }
            return {"loaded": loaded, "environment": dict(os.environ), "error": None}


def main():
    installed_version = importlib.metadata.version("python-dotenv")
    if installed_version != VERSION:
        raise RuntimeError(
            f"Expected python-dotenv {VERSION}, installed {installed_version}"
        )
    request = json.load(sys.stdin)
    cases = request["cases"]
    observations = {
        profile: {case["id"]: observe(profile, case) for case in cases}
        for profile in PROFILE_IDS
    }
    json.dump(
        {
            "runtime": {
                "language": platform.python_version(),
                "package": installed_version,
            },
            "profiles": observations,
        },
        sys.stdout,
        ensure_ascii=True,
        allow_nan=False,
    )
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
