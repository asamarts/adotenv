#!/usr/bin/env python3
"""Execute pinned upstream APIs; compare observations or explicitly record them."""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).absolute().parent
ADAPTERS = ("node", "python", "go")
LOCK_FILES = {
    "node": ("node/package-lock.json",),
    "python": ("python/requirements.txt",),
    "go": ("go/go.mod", "go/go.sum"),
}


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def invalid_constant(value):
    raise ValueError(f"non-JSON numeric constant: {value}")


def decode(text):
    return json.loads(text, object_pairs_hook=unique_object, parse_constant=invalid_constant)


def load(path):
    return decode(path.read_text(encoding="utf-8"))


def snapshot_inputs():
    paths = ("cases.json", "manifest.json", *(f"{name}/profiles.json" for name in ADAPTERS),
             *(path for paths in LOCK_FILES.values() for path in paths))
    snapshot = {path: (ROOT / path).read_bytes() for path in paths}
    require_unchanged(snapshot)
    return snapshot


def require_unchanged(snapshot):
    for path, content in snapshot.items():
        if (ROOT / path).read_bytes() != content:
            raise ValueError(f"conformance input changed during execution: {path}")


def snapshot_document(snapshot, path):
    return decode(snapshot[path].decode("utf-8"))


def input_hashes(snapshot):
    hashes = {path: hashlib.sha256(content).hexdigest() for path, content in snapshot.items()}
    return {"cases_sha256": hashes["cases.json"],
            "profiles_sha256": {name: hashes[f"{name}/profiles.json"] for name in ADAPTERS},
            "manifest_sha256": hashes["manifest.json"],
            "locks_sha256": {path: hashes[path] for paths in LOCK_FILES.values() for path in paths}}


def canonical(value):
    # JSON types matter: Python otherwise considers False == 0 and True == 1.
    return json.dumps(value, sort_keys=True, ensure_ascii=True, allow_nan=False)


def require_schema(document, name):
    if not isinstance(document, dict) or type(document.get("schema")) is not int or document["schema"] != 1:
        raise ValueError(f"{name} requires integer schema version 1")


def validate_provenance(provenance):
    if (not isinstance(provenance, dict)
            or not isinstance(provenance.get("platform"), str) or not provenance["platform"]
            or not isinstance(provenance.get("utc"), str)
            or not isinstance(provenance.get("runtimes"), dict)):
        raise ValueError("recorded_with requires platform, timestamp and runtime provenance")
    timestamp = datetime.datetime.fromisoformat(provenance["utc"])
    if timestamp.tzinfo is None:
        raise ValueError("recorded_with requires a timezone-aware timestamp")
    if set(provenance["runtimes"]) != set(ADAPTERS):
        raise ValueError("recorded_with requires every adapter runtime")
    for runtime in provenance["runtimes"].values():
        if not isinstance(runtime, dict) or not all(
            isinstance(runtime.get(field), str) and runtime[field] for field in ("language", "package")
        ):
            raise ValueError("runtime provenance requires language and package versions")


def validate_manifest(metadata, snapshot):
    manifest = snapshot_document(snapshot, "manifest.json")
    require_schema(manifest, "manifest.json")
    if set(manifest["implementations"]) != set(ADAPTERS):
        raise ValueError("manifest must identify every adapter")
    for name in ADAPTERS:
        declaration = manifest["implementations"][name]
        if declaration["profiles"] != f"{name}/profiles.json":
            raise ValueError(f"incorrect {name} profile reference in manifest")
        locks = declaration.get("integrity_files", [declaration.get("integrity_file")])
        if set(locks) != set(LOCK_FILES[name]):
            raise ValueError(f"incorrect {name} integrity references in manifest")
        if name != "go" and declaration["version"] != metadata[name]["version"]:
            raise ValueError(f"inconsistent {name} version provenance")
        if name == "go":
            for module, field in (("github.com/compose-spec/compose-go/v2", "compose_go"), ("github.com/docker/cli", "docker_cli")):
                if metadata[name]["dependencies"][module]["version"] != declaration[field]:
                    raise ValueError(f"inconsistent {module} version provenance")
        for profile in metadata[name]["profiles"]:
            if profile["stage"] not in ("literal", "resolution", "loading"):
                raise ValueError(f"invalid profile stage: {profile['id']}")
            if type(profile["capabilities"]["ordered_bindings"]) is not bool:
                raise ValueError(f"ordered_bindings must be boolean: {profile['id']}")
    node = snapshot_document(snapshot, "node/package-lock.json")["packages"]
    version = metadata["node"]["version"]
    artifact = metadata["node"]["artifact"]
    if (node[""]["dependencies"]["dotenv"] != version
            or node["node_modules/dotenv"]["version"] != version
            or node["node_modules/dotenv"]["resolved"] != artifact["url"]
            or node["node_modules/dotenv"]["integrity"] != artifact["integrity"]):
        raise ValueError("Node artifact lock differs from declared version/artifact")
    python = metadata["python"]
    artifact = python["artifact"]
    requirement = (f"python-dotenv @ {artifact['url']} "
                   f"--hash=sha256:{artifact['sha256']}")
    if (snapshot["python/requirements.txt"].decode("utf-8").strip() != requirement
            or artifact["url"].rsplit("/", 1)[-1] !=
            f"python_dotenv-{python['version']}-py3-none-any.whl"):
        raise ValueError("Python exact wheel pin differs from declared version/artifact")
    go_mod = snapshot["go/go.mod"].decode("utf-8")
    # Comments cannot introduce directives; replacements (including unrelated
    # modules) would break the reviewed dependency graph.
    uncommented = re.sub(r"/\*.*?\*/|//[^\n]*", "", go_mod, flags=re.DOTALL)
    directives = [line.strip() for line in uncommented.splitlines()]
    if any(re.match(r"replace(?:\s|\(|$)", line) for line in directives):
        raise ValueError("Go oracle must not use module replacements")
    for module, dependency in metadata["go"]["dependencies"].items():
        required = [match.group(1) for line in directives
                    if (match := re.fullmatch(r"(?:require\s+)?" + re.escape(module) + r"\s+(\S+)", line))]
        if required != [dependency["version"]]:
            raise ValueError(f"Go module requirement differs from declared version: {module}")


def validate_runtime(name, runtime, metadata):
    if not isinstance(runtime, dict) or not all(
        isinstance(runtime.get(field), str) and runtime[field]
        for field in ("language", "package")
    ):
        raise ValueError(f"{name} runtime requires language and package versions")
    if name == "go":
        dependencies = metadata[name]["dependencies"]
        package = (f"compose-go {dependencies['github.com/compose-spec/compose-go/v2']['version']}; "
                   f"docker/cli {dependencies['github.com/docker/cli']['version']}")
    else:
        package = metadata[name]["version"]
    if runtime["package"] != package:
        raise ValueError(f"{name} executed package differs from declared artifact version")


def validate_observation(profile, observation):
    def require(condition):
        if not condition:
            raise ValueError(f"malformed observation for {profile}")

    def fields(value, names):
        require(isinstance(value, dict) and set(value) == set(names.split()))

    def values(value, nullable=False):
        require(isinstance(value, dict) and all(
            isinstance(key, str) and (isinstance(item, str) or nullable and item is None)
            for key, item in value.items()))

    def optional_string(value):
        require(value is None or isinstance(value, str))

    def prototype(value):
        require(isinstance(value, str) and value in ("null", "Object.prototype", "other"))

    if profile.startswith("node-parse-"):
        fields(observation, "values prototype diagnostics_supported")
        values(observation["values"])
        prototype(observation["prototype"])
        require(observation["diagnostics_supported"] is False)
    elif profile.startswith("node-load-"):
        fields(observation, "parsed parsed_prototype environment environment_prototype diagnostics_supported error")
        if observation["parsed"] is None:
            require(observation["parsed_prototype"] is None)
        else:
            values(observation["parsed"])
            prototype(observation["parsed_prototype"])
        values(observation["environment"])
        prototype(observation["environment_prototype"])
        require(observation["diagnostics_supported"] is False)
        if observation["error"] is not None:
            fields(observation["error"], "name code")
            optional_string(observation["error"]["name"])
            optional_string(observation["error"]["code"])
    elif profile == "python-bindings-stream":
        fields(observation, "bindings")
        require(isinstance(observation["bindings"], list))
        for binding in observation["bindings"]:
            fields(binding, "key value original error")
            optional_string(binding["key"])
            optional_string(binding["value"])
            require(type(binding["error"]) is bool)
            fields(binding["original"], "string line")
            require(isinstance(binding["original"]["string"], str))
            require(type(binding["original"]["line"]) is int and binding["original"]["line"] >= 1)
    elif profile.startswith("python-values-"):
        fields(observation, "values")
        values(observation["values"], nullable=True)
    elif profile.startswith("python-load-"):
        fields(observation, "loaded environment error")
        values(observation["environment"])
        if observation["error"] is None:
            require(type(observation["loaded"]) is bool)
        else:
            require(observation["error"] == "ValueError" and observation["loaded"] is None)
    elif profile.startswith("compose-"):
        fields(observation, "values error")
        require(type(observation["error"]) is bool)
        if observation["values"] is None:
            require(observation["error"] is True)
        else:
            values(observation["values"], nullable=profile.startswith("compose-service-"))
    elif profile.startswith("docker-raw-"):
        fields(observation, "lines error")
        require(type(observation["error"]) is bool)
        require(isinstance(observation["lines"], list)
                and all(isinstance(line, str) for line in observation["lines"]))
    else:
        raise ValueError(f"unknown observation protocol: {profile}")


def executable(name):
    path = shutil.which(name)
    if path is None:
        raise RuntimeError(f"required executable not found: {name}")
    return path


def command(arguments, cwd=ROOT, **kwargs):
    return subprocess.run(arguments, cwd=cwd, check=True, timeout=300, **kwargs)


def go_environment():
    # Ignore local workspaces, overlays, semantic flags and persisted go env.
    retained = ("PATH", "HOME", "GOCACHE", "GOMODCACHE", "GOPATH", "XDG_CACHE_HOME",
                "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY")
    environment = {key: os.environ[key] for key in retained if key in os.environ}
    environment.update(GOWORK="off", GOFLAGS="", GOTOOLCHAIN="local", GOENV="off")
    return environment


def setup():
    command([executable("npm"), "ci", "--ignore-scripts", "--no-audit", "--no-fund"], cwd=ROOT / "node")
    python = ROOT / "python/.venv/bin/python"
    if not python.exists():
        command([sys.executable, "-I", "-m", "venv", str(ROOT / "python/.venv")])
    command([str(python), "-I", "-m", "pip", "--isolated", "install",
             "--force-reinstall", "--no-deps", "--require-hashes", "--only-binary=:all:",
             "-r", str(ROOT / "python/requirements.txt")])
    command([executable("go"), "mod", "download"], cwd=ROOT / "go", env=go_environment())


def validate_cases(document):
    require_schema(document, "cases.json")
    if not isinstance(document.get("cases"), list):
        raise ValueError("cases.json requires a cases array")
    identifiers = set()
    for case in document["cases"]:
        identifier = case["id"]
        if not identifier or not identifier.isascii() or identifier in identifiers:
            raise ValueError(f"invalid or duplicate case id: {identifier!r}")
        identifiers.add(identifier)
        if not isinstance(case["source"], str):
            raise ValueError(f"case {identifier} has no text source")
        environment = case.get("environment", {})
        if not isinstance(environment, dict) or not all(
            isinstance(key, str) and isinstance(value, str)
            for key, value in environment.items()
        ):
            raise ValueError(f"case {identifier} has an invalid controlled environment")
    if not identifiers:
        raise ValueError("empty conformance corpus")
    return identifiers


def execute(cases, snapshot=None):
    if sys.platform != "linux":
        raise RuntimeError("these environment-loading observations are explicitly Linux-profiled")
    identifiers = validate_cases(cases)
    if snapshot is None:
        snapshot = snapshot_inputs()
    metadata = {name: snapshot_document(snapshot, f"{name}/profiles.json") for name in ADAPTERS}
    validate_manifest(metadata, snapshot)
    (ROOT / ".build").mkdir(exist_ok=True)
    go_binary = ROOT / ".build/go-oracle"
    command([executable("go"), "mod", "verify"], cwd=ROOT / "go", env=go_environment())
    command([executable("go"), "build", "-mod=readonly", "-o", str(go_binary), "."],
            cwd=ROOT / "go", env=go_environment())
    # Do not resolve the venv executable symlink: doing so bypasses the venv.
    python = ROOT / "python/.venv/bin/python"
    if not python.exists() or not (ROOT / "node/node_modules/dotenv").is_dir():
        raise RuntimeError("oracle dependencies missing; run with --setup")
    commands = {
        "node": [executable("node"), str(ROOT / "node/oracle.cjs")],
        "python": [str(python), str(ROOT / "python/oracle.py")],
        "go": [str(go_binary)],
    }
    observations = {}
    runtimes = {}
    for name in ADAPTERS:
        # API execution receives no ambient variables, credentials, HOME or
        # discovery/config overrides. Each case supplies its complete lookup.
        result = command(commands[name], input=json.dumps(cases, allow_nan=False), text=True,
                         capture_output=True, env={})
        output = decode(result.stdout)
        if not isinstance(output, dict) or set(output) != {"runtime", "profiles"}:
            raise ValueError(f"{name} returned a malformed adapter response")
        if not isinstance(output["profiles"], dict):
            raise ValueError(f"{name} profiles must be an object")
        validate_runtime(name, output["runtime"], metadata)
        declared = {profile["id"] for profile in metadata[name]["profiles"]}
        if len(declared) != len(metadata[name]["profiles"]) or declared & observations.keys():
            raise ValueError(f"duplicate profile declaration in {name}")
        if set(output["profiles"]) != declared:
            raise ValueError(f"{name} output profiles differ from metadata")
        for profile, values in output["profiles"].items():
            if not isinstance(values, dict) or set(values) != identifiers:
                raise ValueError(f"{profile} omitted or invented corpus cases")
            for observation in values.values():
                validate_observation(profile, observation)
        observations.update(output["profiles"])
        runtimes[name] = output["runtime"]
    validate_provenance({"platform": platform.platform(),
                         "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                         "runtimes": runtimes})
    return observations, runtimes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--setup", action="store_true", help="install integrity-pinned oracle dependencies before execution")
    parser.add_argument("--record", action="store_true", help="explicitly replace expected observations with executed upstream results")
    parser.add_argument("--expected", type=Path, default=ROOT / "expected.json", help="golden file to compare or record")
    args = parser.parse_args()
    snapshot = snapshot_inputs()
    inputs = input_hashes(snapshot)
    cases = snapshot_document(snapshot, "cases.json")
    metadata = {name: snapshot_document(snapshot, f"{name}/profiles.json") for name in ADAPTERS}
    validate_cases(cases)
    validate_manifest(metadata, snapshot)
    if args.setup or args.record:
        setup()
        require_unchanged(snapshot)
    observations, runtimes = execute(cases, snapshot)
    require_unchanged(snapshot)
    actual = {"schema": 1, "inputs": inputs, "observations": observations}
    count = len(cases["cases"]) * len(observations)
    if args.record:
        actual["recorded_with"] = {
            "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "platform": platform.platform(), "runtimes": runtimes,
        }
        require_unchanged(snapshot)
        args.expected.write_text(json.dumps(actual, ensure_ascii=True, sort_keys=True, indent=2, allow_nan=False) + "\n", encoding="utf-8")
        print(f"Recorded {count} executed observations ({len(cases['cases'])} cases, {len(observations)} profiles).")
        return 0
    expected = load(args.expected)
    require_schema(expected, "expected observations")
    validate_provenance(expected.get("recorded_with"))
    for name in ADAPTERS:
        validate_runtime(name, expected["recorded_with"]["runtimes"][name], metadata)
    require_unchanged(snapshot)
    if canonical(expected.get("inputs")) != canonical(inputs):
        print("Corpus inputs/provenance changed; review them and explicitly --record new upstream observations.", file=sys.stderr)
        return 1
    if canonical(expected.get("observations")) != canonical(observations):
        previous = expected.get("observations", {})
        differences = 0
        for profile in sorted(set(previous) | set(observations)):
            before, after = previous.get(profile, {}), observations.get(profile, {})
            for case in sorted(set(before) | set(after)):
                if canonical(before.get(case)) != canonical(after.get(case)):
                    differences += 1
                    if differences <= 10:
                        print(f"Mismatch: {profile} / {case}", file=sys.stderr)
                        print("  expected: " + json.dumps(before.get(case), ensure_ascii=True), file=sys.stderr)
                        print("  observed: " + json.dumps(after.get(case), ensure_ascii=True), file=sys.stderr)
        print(f"{differences} observation mismatches; do not re-record without investigating upstream/API changes.", file=sys.stderr)
        return 1
    print(f"Conformance observations match: {count} ({len(cases['cases'])} cases, {len(observations)} profiles).")
    print(json.dumps({"executed_with": runtimes}, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Conformance execution failed: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            print(error.stderr, file=sys.stderr)
        raise SystemExit(1) from error
