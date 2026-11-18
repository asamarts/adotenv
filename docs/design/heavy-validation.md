# Heavy validation

Status: adopted 2026-11-18 after a local mutation campaign exhausted the
shared development host.

## Policy

Run resource-heavy validation on `kbench` through the serialized `benchq`
pueue queue, or in GitHub Actions. Do not run it on the shared development
host.

Heavy validation includes:

- `cargo test --workspace` or any full-feature test suite;
- mutation testing with `cargo mutants`;
- fuzzing with `cargo +nightly fuzz`;
- benchmarks, deterministic performance runs and benchmark smoke runs;
- conformance setup and the full conformance corpus;
- any command that runs more than two mutation workers, more than one test
  binary at once, or more than two Cargo build jobs.

The local checks that remain safe are formatting, documentation checks,
`cargo check` for one package, and a focused single-package test with explicit
thread and build-job caps. Most changes should still be validated by CI.

## What happened on 2026-11-18

A references mutation shard was started with:

```console
cargo mutants --workspace --all-features --file src/references.rs \
  --jobs 8 --timeout 90 --baseline skip
```

`cargo-mutants` 27.1.0 copies the workspace once per worker. Its debug log
reported 3,166,037,205 bytes and 16,629 files per copy, and reflinks were not
supported, so each copy was a regular file copy. With eight workers this was
about 25 GB of scratch data and page cache. The copy included
`conformance/rust/target` (2.8 GB) because the default is `--gitignore false`.
The root `target/` was excluded by the default `--copy-target false`, but the
ignored conformance target was not.

Each worker then ran the package suite, in effect:

```console
cargo test --package=adotenv@1.5.4 --all-features
```

The package has 475 tests, 20 property-test blocks and deliberately large
inputs, including 200,000 bare keys, a one-million-character comment, 100,000
nested forms and a 2 MB value. With eight workers and the default 24 test
threads per test harness, the campaign could run up to 192 test threads plus
eight concurrent rustc/linker processes. The 90-second timeout limits wall
time but not resident memory. Seventeen of the 41 outcomes that completed
were timeouts, mostly scanner index and increment mutations that stop making
progress.

The host has 24 CPUs and 62 GiB of RAM, no cgroup memory limit, and is shared
with other agents and CI. The combination of copied trees, concurrent builds,
parallel tests and runaway mutant allocations produced more than 20 GB of
memory pressure and starved CPU. The operator killed the processes; a server
restart then cancelled the shard. No product failure was demonstrated.

## Local guardrails

The repository defaults in `.cargo/mutants.toml` now ignore `.gitignore`
artifacts, cap Cargo at two build jobs and keep the test harness
single-threaded. Those defaults do not set the mutation worker count or the
wall-clock timeout, so a local invocation must still be bounded explicitly:

```console
cargo mutants --file src/references.rs --jobs 1 --timeout 60 \
  --gitignore true --baseline skip
```

Do not override this with `--jobs` greater than two on any host without a
memory limit. If a local test is unavoidable, use a temporary target directory,
`CARGO_BUILD_JOBS=2`, `RUST_TEST_THREADS=1` and a memory cap such as
`ulimit -v 12582912` (12 GiB). Stop the run if memory or load rises.

## kbench procedure

`kbench` currently has 8 CPUs, 15 GiB of RAM, no swap, a 442 GB disk, pueue
4.0.4 and a serialized `bench` group with one slot. Its `benchq` daemon runs
with `ProtectHome`, so it cannot use a toolchain under `/home`. As of
2026-11-18 it also has no Rust toolchain installed.

Before the first test or mutation run, provision these once, as an
administrator:

1. A pinned Rust toolchain readable by `benchq`, for example under
   `/var/lib/benchq/toolchain`, with `PATH`, `CARGO_HOME` and `RUSTUP_HOME`
   pointed at the provisioned directories.
2. A dedicated source checkout at `/var/lib/benchq/adotenv-src`, owned by
   `benchq`, populated from tracked files only. Do not copy `target/`,
   `conformance/rust/target/`, `mutants.out/` or other ignored scratch data.

If those prerequisites are absent, use GitHub Actions. Do not fall back to a
heavy local run.

To refresh the tracked source and queue a bounded workspace test:

```console
tmp=$(mktemp -d ~/.tmp/adotenv-kbench.XXXXXX)
git archive --format=tar.gz HEAD > "$tmp/src.tar.gz"
ssh kbench 'sudo -n -u benchq mkdir -p /var/lib/benchq/adotenv-src'
ssh kbench 'sudo -n -u benchq find /var/lib/benchq/adotenv-src \
  -mindepth 1 -maxdepth 1 -exec rm -rf {} +'
ssh kbench 'sudo -n -u benchq tar -xzf - \
  -C /var/lib/benchq/adotenv-src' < "$tmp/src.tar.gz"
rm -rf "$tmp"

ssh kbench "PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml pueue add \
  --group bench --working-directory /var/lib/benchq/adotenv-src \
  --label adotenv-tests --print-task-id -- \
  bash -lc 'ulimit -v 12582912; \
    export CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2; \
    exec cargo test --workspace --all-features --locked'"
```

Use the task id printed by `pueue add` to wait and collect the log:

```console
PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml pueue wait <task>
PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml pueue log <task>
```

For mutation testing, keep one worker and one test thread:

```console
ssh kbench "PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml pueue add \
  --group bench --working-directory /var/lib/benchq/adotenv-src \
  --label adotenv-mutants --print-task-id -- \
  bash -lc 'ulimit -v 12582912; \
    export CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1; \
    exec cargo mutants --workspace --all-features \
      --file src/references.rs --jobs 1 --timeout 60 \
      --gitignore true --baseline skip -- --test-threads=1'"
```

The two `--jobs` flags are different: the first after `cargo mutants` is the
number of mutant workers, while `--jobs 2` in `.cargo/mutants.toml` is Cargo
build parallelism. `--in-place` is acceptable only in the dedicated remote
checkout with one mutant worker; it avoids the per-worker copy but mutates the
source tree.

The queue has one slot. Do not run a mutation or test task at the same time as
a benchmark, and do not submit the same campaign twice. Remove
`mutants.out/` and remote scratch data after collecting results.

## Fallback and CI

The full test matrix, fuzzing, benchmarks, conformance and the sharded mutation
workflow run in GitHub Actions. The mutation workflow uses two workers and
takes a tracked-only checkout, so it does not copy conformance build
artifacts. If kbench is unavailable or not provisioned, use CI rather than the
shared host.

Local cleanup after an interrupted campaign:

```console
rm -rf mutants.out mutants.out.old ~/.tmp/cargo-mutants-*
```
