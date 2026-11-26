# Heavy validation

Status: adopted 2026-11-18 after a local mutation campaign exhausted the
shared development host.

## Policy

Run resource-heavy validation on a dedicated remote validation host through its
single-slot serialized queue, or in GitHub Actions. Do not run it on the shared
development host.

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
cargo mutants --file src/references.rs --jobs 1 --timeout 120 \
  --gitignore true --baseline skip
```

Do not override this with more than one mutation worker on a remote validation
host or any host without an aggregate memory limit. If a local test is
unavoidable, use a temporary target directory, `CARGO_BUILD_JOBS=2`,
`RUST_TEST_THREADS=1` and a memory cap such as `ulimit -v 8388608` (8 GiB).
Stop the run if memory or load rises.

## Remote validation host procedure

The reference validation host has 8 logical CPUs, 15 GiB of RAM, no swap, and a
serialized single-slot `pueue` queue (`bench` group). When a queue service runs
without access to user home directories, provision the toolchain, tool
binaries, `CARGO_HOME`, and tracked-only source tree under a dedicated state
root (`$ADOTENV_BENCH_ROOT`):

- `$ADOTENV_BENCH_ROOT/toolchain` holds a Rust toolchain readable by the queue service;
- `$ADOTENV_BENCH_ROOT/bin/cargo-mutants` holds `cargo-mutants` 27.1.0;
- `$ADOTENV_BENCH_ROOT/cargo` is the `CARGO_HOME`; dependencies are fetched with
  `cargo fetch --locked`;
- `$ADOTENV_BENCH_ROOT/adotenv-src` is a tracked-only source tree.

The first shard (0/4) used a 12 GiB address-space limit, two Cargo build jobs,
one test thread and two mutation workers. It tested 102 mutants: **68 caught,
0 missed, 12 unviable and 22 timeouts**. The host reported 2.6 GiB in use
immediately after the task.

The next shard (1/4) tested 102 mutants: **53 caught, 0 missed, 33 timeouts
and 16 unviable**. During shard 2/4, one mutant test process reached **9.6 GiB
RSS** and the 15 GiB host hit a global OOM. The queue service peaked at
14.5 GiB, the kernel killed the test process and restarted the queue daemon;
shard 3/4 was stopped before it could repeat the failure. Two workers plus
per-process limits were not enough because the limit is not an aggregate cgroup
limit.

The one-worker, 8 GiB reruns completed:

- 2/4: 102 mutants; 30 caught, 0 missed, 30 unviable and 42 timeouts.
- 3/4: 100 mutants; 54 caught, 0 missed, 1 unviable and 45 timeouts.

Across all four shards the references file has **406 mutants: 205 caught, 0
missed, 59 unviable and 142 timeouts**. `cargo-mutants` exits with status 3
when it records timeouts; that is a completed campaign after the timeout list
is reviewed, not an infrastructure failure. The timeouts are scanner,
delimiter and bit-mask mutants that do not terminate within 60 seconds; no
finite test can catch them, and no product failure was demonstrated.

Across all four remote shards `src/parser.rs` completed with **695 mutants:
518 caught, 0 missed, 48 unviable and 129 timeouts**:

- 0/4: 174 mutants; 116 caught, 0 missed, 24 unviable and 34 timeouts.
- 1/4: 174 mutants; 136 caught, 0 missed, 18 unviable and 20 timeouts.
- 2/4: 174 mutants; 141 caught, 0 missed, 0 unviable and 33 timeouts.
- 3/4: 173 mutants; 125 caught, 0 missed, 6 unviable and 42 timeouts.

Remaining core and adapter shards also completed on the remote validation host
under the same one-worker / 8 GiB cap:

- `adotenv-process`: 9 mutants; 5 caught, 4 Windows-only misses (covered by Windows CI).
- `adotenv-config`: 10 mutants; 9 caught, 0 missed, 1 unviable.
- `adotenv-figment`: 10 mutants; 10 caught, 0 missed after adding `Provider::metadata` assertions.
- `src/limit.rs`: 9 mutants; 8 caught, 1 unviable, 0 missed, 0 timeouts after adding `Display`/`as_str` tests.
- `src/serde_support.rs`: 68 mutants; 36 caught, 32 unviable, 0 missed, 0 timeouts after adding the `size_hint` visitor test and verifying 3/3 caught.
- `src/parser/decode.rs` (0/2 and 1/2): 241 mutants; 151 caught, 66 missed, 13 unviable, 11 timeouts; targeted tests closed the genuine coverage gaps, confirmation shard 0/2 verified 93 caught, 8 missed (7 equivalent/defensive + 1 EOF bounds guard closed in follow-up) and 1 timeout, and confirmation shard 1/2 (120 mutants, `--timeout 120`) verified 94 caught, 1 unviable, 8 timeouts and 17 defensive/equivalent survivors (0 genuine gaps).
- `1.6.0` core additions (`--in-diff`, 0/3 through 2/3): 259 mutants; 196 caught, 17 missed, 44 unviable, 2 timeouts; added regression tests for all 12 genuine survivors and confirmed 35 caught, 1 unviable, 0 missed and 0 timeouts across the 36-mutant confirmation rerun (208 caught, 5 equivalent/defensive survivors, 44 unviable, 2 timeouts in total).
- `adotenv-lsp/src/lib.rs`: 146 mutants; 115 caught, 28 unviable, 0 timeouts, 3 equivalent survivors (0 genuine gaps) after adding 5 targeted regression assertions and 2-second transport test channel timeouts.
- `adotenv-lint` library (`src/lib.rs` + `src/config.rs`): 170 mutants; 130 caught, 39 unviable, 0 timeouts, 1 equivalent survivor (0 genuine gaps) after adding 30 targeted regression assertions.
- `adotenv-lsp/src/main.rs` (84 mutants) and `adotenv-lint/src/main.rs` (131 mutants): closed all 20 genuine gaps (11 LSP + 9 lint); confirmed 76 caught / 6 unviable / 1 timeout / 1 equivalent for `adotenv-lsp/src/main.rs` and 111 caught / 3 unviable / 2 timeouts / 10 Windows-only + 5 defensive/equivalent for `adotenv-lint/src/main.rs` (0 genuine gaps).
- `adotenv-cli/src/main.rs` (506 mutants across shards `0/2` and `1/2`): closed all 23 genuine gaps; 439 caught, 13 unviable, 9 infinite-loop timeouts, 19 Windows-only + 26 defensive/equivalent survivors (0 genuine gaps). Every published crate in the workspace has now completed sharded or full-file mutation runs.

Note on timeouts: a clean single-threaded `adotenv` package test pass takes
about 73-75 seconds on the reference host. Use `--timeout 120` (rather than
`--timeout 60`) for `adotenv` core shards so uncaught survivors report as
`MISSED` instead of timing out at 60 seconds. Optional full-crate companion
sweeps (`adotenv-lsp`, `adotenv-lint`, `adotenv-cli`) follow the same
one-shard-at-a-time idle-queue procedure.

If the prerequisites are ever removed, reprovision them before queueing.
If the remote host is unavailable, use GitHub Actions. Do not fall back to a
heavy local run.

To refresh the tracked source and queue a bounded workspace test (with
`ADOTENV_BENCH_HOST`, `ADOTENV_BENCH_ROOT`, `ADOTENV_BENCH_GROUP` and optional
`PUEUE_CONFIG_PATH` configured for your environment):

```console
tmp=$(mktemp -d ~/.tmp/adotenv-remote.XXXXXX)
git archive --format=tar.gz HEAD > "$tmp/src.tar.gz"
ssh -- "$ADOTENV_BENCH_HOST" "mkdir -p '$ADOTENV_BENCH_ROOT/adotenv-src' \
  && find '$ADOTENV_BENCH_ROOT/adotenv-src' -mindepth 1 -maxdepth 1 -exec rm -rf {} +"
ssh -- "$ADOTENV_BENCH_HOST" "tar -xzf - -C '$ADOTENV_BENCH_ROOT/adotenv-src'" < "$tmp/src.tar.gz"
rm -rf "$tmp"

ssh -- "$ADOTENV_BENCH_HOST" "${PUEUE_CONFIG_PATH:+PUEUE_CONFIG_PATH='$PUEUE_CONFIG_PATH' }pueue add \
  --group '${ADOTENV_BENCH_GROUP:-bench}' --working-directory '$ADOTENV_BENCH_ROOT/adotenv-src' \
  --label adotenv-tests --print-task-id -- \
  \"ulimit -v 8388608; \
    export HOME='$ADOTENV_BENCH_ROOT' CARGO_HOME='$ADOTENV_BENCH_ROOT/cargo' \
      PATH='$ADOTENV_BENCH_ROOT/bin:$ADOTENV_BENCH_ROOT/toolchain/bin:/usr/bin:/bin' \
      CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2; \
    exec cargo test --workspace --all-features --locked\""
```

Use the task id printed by `pueue add` to wait and collect the log:

```console
ssh -- "$ADOTENV_BENCH_HOST" "${PUEUE_CONFIG_PATH:+PUEUE_CONFIG_PATH='$PUEUE_CONFIG_PATH' }pueue wait <task>"
ssh -- "$ADOTENV_BENCH_HOST" "${PUEUE_CONFIG_PATH:+PUEUE_CONFIG_PATH='$PUEUE_CONFIG_PATH' }pueue log <task>"
```

For mutation testing, keep one worker and one test thread, and pass the
shell command as a single quoted argument so `pueue` does not split on `;`:

```console
ssh -- "$ADOTENV_BENCH_HOST" "${PUEUE_CONFIG_PATH:+PUEUE_CONFIG_PATH='$PUEUE_CONFIG_PATH' }pueue add \
  --group '${ADOTENV_BENCH_GROUP:-bench}' --working-directory '$ADOTENV_BENCH_ROOT/adotenv-src' \
  --label adotenv-mutants --print-task-id -- \
  \"ulimit -v 8388608; \
    export HOME='$ADOTENV_BENCH_ROOT' CARGO_HOME='$ADOTENV_BENCH_ROOT/cargo' \
      CARGO='$ADOTENV_BENCH_ROOT/toolchain/bin/cargo' \
      PATH='$ADOTENV_BENCH_ROOT/bin:$ADOTENV_BENCH_ROOT/toolchain/bin:/usr/bin:/bin' \
      CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1; \
    exec cargo mutants --workspace --all-features \
      --file src/references.rs --jobs 1 --timeout 120 \
      --gitignore true --baseline skip\""
```

The two `--jobs` flags are different: the first after `cargo mutants` is the
number of mutant workers, while `--jobs 2` in `.cargo/mutants.toml` is Cargo
build parallelism. Pass a non-login command string (not `bash -lc`): a
hardened queue service account may not read user `/home` profiles, and
`cargo mutants` needs `CARGO` set to the provisioned toolchain. `--in-place`
is acceptable only in the dedicated remote checkout with one mutant worker;
it avoids the per-worker copy but mutates the source tree.

The queue has one slot. Before submitting, run `pueue status` and wait if any
task from another project is running or queued. Submit one adotenv task at a
time, wait for it, then re-check the queue before submitting the next shard.
Do not queue several shards at once, do not run a mutation or test task at the
same time as a benchmark, and do not submit the same campaign twice. Remove
`mutants.out/` and remote scratch data after collecting results.

## Fallback and CI

The full test matrix, fuzzing, benchmarks, conformance and the sharded mutation
workflow run in GitHub Actions. The mutation workflow uses two workers and
takes a tracked-only checkout, so it does not copy conformance build
artifacts. If the remote host is unavailable or not provisioned, use CI rather
than the shared host.

Local cleanup after an interrupted campaign:

```console
rm -rf mutants.out mutants.out.old ~/.tmp/cargo-mutants-*
```
