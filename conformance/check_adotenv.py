#!/usr/bin/env python3
"""Gate literal adotenv presets against applicable pinned upstream observations.

By default run.py first replays every real upstream API and checks expected.json.
--skip-upstream-replay uses that recorded snapshot for local iteration only.
No Compose interpolation-off oracle exists: only resolution-insensitive inputs
(no dollar sign and no controlled lookup environment) are compared here.
"""

import argparse
import json
from pathlib import Path
import subprocess
import sys

import run as upstream

ROOT = Path(__file__).absolute().parent
PRESETS = {"portable", "node_dotenv", "python_dotenv", "compose", "docker_raw"}
PROFILE_PRESETS = {
    "node-parse-regex-string": "node_dotenv",
    "node-parse-regex-buffer": "node_dotenv",
    "python-values-literal-stream": "python_dotenv",
    "python-bindings-stream": "python_dotenv",
    "docker-raw-no-lookup": "docker_raw",
    "compose-parser-default": "compose",
}


def validate_expected(expected, snapshot, identifiers):
    upstream.require_schema(expected, "expected observations")
    upstream.validate_provenance(expected.get("recorded_with"))
    metadata = {name: upstream.snapshot_document(snapshot, f"{name}/profiles.json")
                for name in upstream.ADAPTERS}
    upstream.validate_manifest(metadata, snapshot)
    for name in upstream.ADAPTERS:
        upstream.validate_runtime(name, expected["recorded_with"]["runtimes"][name], metadata)
    if upstream.canonical(expected.get("inputs")) != upstream.canonical(upstream.input_hashes(snapshot)):
        raise ValueError("expected.json input hashes differ; replay/review upstream corpus first")
    declarations = [profile["id"] for item in metadata.values() for profile in item["profiles"]]
    if len(declarations) != len(set(declarations)):
        raise ValueError("duplicate declared upstream profile")
    observations = expected.get("observations")
    if not isinstance(observations, dict) or set(observations) != set(declarations):
        raise ValueError("expected.json profile coverage differs from declarations")
    if not PROFILE_PRESETS.keys() <= observations.keys():
        raise ValueError("required literal comparison profiles missing")
    for profile, results in observations.items():
        if not isinstance(results, dict) or set(results) != identifiers:
            raise ValueError(f"{profile}: expected.json omitted or invented corpus cases")
        for observation in results.values():
            upstream.validate_observation(profile, observation)
    return observations


def validate_probe(output, identifiers):
    if not isinstance(output, dict) or set(output) != PRESETS:
        raise ValueError("Rust probe must return exactly the five preset objects")
    for preset, cases in output.items():
        if not isinstance(cases, dict) or set(cases) != identifiers:
            raise ValueError(f"{preset}: Rust probe omitted or invented corpus cases")
        for identifier, result in cases.items():
            label = f"{preset} / {identifier}"
            if not isinstance(result, dict) or set(result) != {"values", "lines", "error"}:
                raise ValueError(f"{label}: malformed Rust probe observation fields")
            if type(result["error"]) is not bool:
                raise ValueError(f"{label}: error must be boolean")
            if not isinstance(result["values"], dict) or not all(
                isinstance(key, str) and (value is None or isinstance(value, str))
                for key, value in result["values"].items()
            ):
                raise ValueError(f"{label}: values must map strings to strings or null")
            if not isinstance(result["lines"], list) or not all(
                isinstance(line, str) for line in result["lines"]
            ):
                raise ValueError(f"{label}: lines must be an array of strings")
            if preset != "docker_raw" and result["lines"]:
                raise ValueError(f"{label}: only Docker emits ordered lines")


def exclusion_reason(profile):
    if profile.startswith("node-parse-fast-"):
        return "distinct opt-in Node fast grammar, not default regex preset"
    if profile == "python-values-literal-file":
        return "file I/O universal-newline conversion, not literal text-stream input"
    return "resolution/loading API, not literal parsing"


def compare(cases, observations, probe):
    mismatches = []
    checked = excluded = compared_fields = failfast = 0
    reports = []

    def equal(label, field, expected, actual):
        nonlocal compared_fields
        compared_fields += 1
        if upstream.canonical(expected) != upstream.canonical(actual):
            mismatches.append(f"{label} / {field}\n"
                              f"  expected: {json.dumps(expected, ensure_ascii=True)}\n"
                              f"  observed: {json.dumps(actual, ensure_ascii=True)}")

    for profile in sorted(observations):
        preset = PROFILE_PRESETS.get(profile)
        count = omitted = errors_only = 0
        for case in cases["cases"]:
            identifier = case["id"]
            if preset is None or (preset == "compose" and
                                  ("$" in case["source"] or case.get("environment", {}))):
                omitted += 1
                continue
            count += 1
            observed = probe[preset][identifier]
            expected = observations[profile][identifier]
            label = f"{preset} / {profile} / {identifier}"
            if preset == "node_dotenv":
                equal(label, "error", False, observed["error"])
                equal(label, "values", expected["values"], observed["values"])
            elif preset == "python_dotenv":
                bindings = observations["python-bindings-stream"][identifier]["bindings"]
                error = any(binding["error"] for binding in bindings)
                equal(label, "error", error, observed["error"])
                # Python recovers: valid bindings after errors must still compare.
                values = expected.get("values") if profile == "python-values-literal-stream" else {
                    binding["key"]: binding["value"] for binding in bindings
                    if binding["key"] is not None and not binding["error"]
                }
                equal(label, "values", values, observed["values"])
            else:
                equal(label, "error", expected["error"], observed["error"])
                if expected["error"]:
                    # Upstream fails fast; its partial/null result is not a
                    # literal oracle for our source-preserving recovering doc.
                    errors_only += 1
                else:
                    field = "lines" if preset == "docker_raw" else "values"
                    equal(label, field, expected[field], observed[field])
        checked += count
        excluded += omitted
        failfast += errors_only
        reason = ("source contains '$' or supplied lookup environment" if preset == "compose"
                  else exclusion_reason(profile) if preset is None else "none")
        reports.append(f"  {profile}: checked={count}, excluded={omitted}, "
                       f"failfast-error-only={errors_only}; exclusions: {reason}")
    print("\n".join(reports))
    print(f"Applicable observations: checked={checked}, excluded={excluded}, "
          f"field-comparisons={compared_fields}, failfast-error-only={failfast}.")
    print(f"Probe coverage: {len(PRESETS)} presets x {len(cases['cases'])} cases; "
          "Portable has no upstream-equivalence assertion.")
    print("Compose applicability is a resolution-insensitive subset of its real default "
          "parser API, NOT an interpolation-off oracle. No loading/inheritance is asserted.")
    for mismatch in mismatches:
        print("Mismatch: " + mismatch, file=sys.stderr)
    if mismatches:
        raise ValueError(f"{len(mismatches)} literal preset field mismatches")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    replay = parser.add_mutually_exclusive_group()
    replay.add_argument("--skip-upstream-replay", action="store_true",
                        help="local iteration only: compare recorded oracles without replay")
    replay.add_argument("--setup", action="store_true",
                        help="install locked upstream dependencies before replay and comparison")
    args = parser.parse_args()
    snapshot = upstream.snapshot_inputs()
    cases = upstream.snapshot_document(snapshot, "cases.json")
    identifiers = upstream.validate_cases(cases)
    expected = upstream.load(ROOT / "expected.json")
    observations = validate_expected(expected, snapshot, identifiers)
    if args.skip_upstream_replay:
        print("WARNING: upstream replay skipped; using validated recorded observations.", file=sys.stderr)
    else:
        command = [sys.executable, str(ROOT / "run.py")]
        if args.setup:
            command.append("--setup")
        upstream.command(command)
    upstream.require_unchanged(snapshot)
    result = upstream.command(
        [upstream.executable("cargo"), "+stable", "run", "--locked", "--quiet",
         "--manifest-path", str(ROOT / "rust/Cargo.toml")],
        input=json.dumps(cases, allow_nan=False), text=True, capture_output=True,
    )
    probe = upstream.decode(result.stdout)
    validate_probe(probe, identifiers)
    upstream.require_unchanged(snapshot)
    compare(cases, observations, probe)
    print("Applicable pinned literal observations match adotenv presets.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, TypeError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"adotenv conformance failed: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            print(error.stderr, file=sys.stderr)
        raise SystemExit(1) from error
