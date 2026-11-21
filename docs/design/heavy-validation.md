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

Do not override this with more than one mutation worker on kbench or any host
without an aggregate memory limit. If a local test is unavoidable, use a
temporary target directory, `CARGO_BUILD_JOBS=2`, `RUST_TEST_THREADS=1` and a
memory cap such as `ulimit -v 8388608` (8 GiB). Stop the run if memory or load
rises.

## kbench procedure

`kbench` currently has 8 CPUs, 15 GiB of RAM, no swap, a 442 GB disk, pueue
4.0.4 and a serialized `bench` group with one slot. Its `benchq` daemon runs
with `ProtectHome`, so it cannot use a toolchain under `/home`.

The test environment was provisioned on 2026-11-18:

- `/var/lib/benchq/toolchain` is a copy of Rust 1.97.0, readable by `benchq`;
- `/var/lib/benchq/bin/cargo-mutants` is `cargo-mutants` 27.1.0;
- `/var/lib/benchq/cargo` is the `CARGO_HOME`; dependencies were fetched with
  `cargo fetch --locked`;
- `/var/lib/benchq/adotenv-src` is a tracked-only source tree.

The first shard (0/4) used a 12 GiB address-space limit, two Cargo build jobs,
one test thread and two mutation workers. It tested 102 mutants: **68 caught,
0 missed, 12 unviable and 22 timeouts**. The host reported 2.6 GiB in use
immediately after the task.

The next shard (1/4) tested 102 mutants: **53 caught, 0 missed, 33 timeouts
and 16 unviable**. During shard 2/4, one mutant test process reached **9.6 GiB
RSS** and the 15 GiB host hit a global OOM. The benchq service peaked at
14.5 GiB, the kernel killed the test process and restarted pueued; shard 3/4
was stopped before it could repeat the failure. Two workers plus per-process
limits were not enough because the limit is not an aggregate cgroup limit.

The one-worker, 8 GiB reruns completed:

- 2/4: 102 mutants; 30 caught, 0 missed, 30 unviable and 42 timeouts.
- 3/4: 100 mutants; 54 caught, 0 missed, 1 unviable and 45 timeouts.

Across all four shards the references file has **406 mutants: 205 caught, 0
missed, 59 unviable and 142 timeouts**. `cargo-mutants` exits with status 3
when it records timeouts; that is a completed campaign after the timeout list
is reviewed, not an infrastructure failure. The timeouts are scanner,
delimiter and bit-mask mutants that do not terminate within 60 seconds; no
finite test can catch them, and no product failure was demonstrated.

The parser-core 0/4 shard tested 174 mutants: **116 caught, 0 missed, 24
unviable and 34 timeouts**. Parser-core 1/4 tested 174 mutants: **136 caught,
0 missed, 18 unviable and 20 timeouts**. Parser shards 2/4-3/4 remain.

If the prerequisites are ever removed, reprovision them before queueing.
If kbench is unavailable, use GitHub Actions. Do not fall back to a heavy
local run.

To refresh the tracked source and queue a bounded workspace test:

```console
tmp=$(mktemp -d ~/.tmp/adotenv-kbench.XXXXXX)
git archive --format=tar.gz HEAD > "$tmp/src.tar.gz"
ssh kbench 'sudo -n -u benchq sh -c "cd /var/lib/benchq \
  && mkdir -p adotenv-src \
  && find adotenv-src -mindepth 1 -maxdepth 1 -exec rm -rf {} +"'
ssh kbench 'sudo -n -u benchq sh -c "cd /var/lib/benchq \
  && tar -xzf - -C adotenv-src"' < "$tmp/src.tar.gz"
rm -rf "$tmp"

ssh kbench "PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml pueue add \
  --group bench --working-directory /var/lib/benchq/adotenv-src \
  --label adotenv-tests --print-task-id -- \
  env HOME=/var/lib/benchq CARGO_HOME=/var/lib/benchq/cargo \
    PATH=/var/lib/benchq/bin:/var/lib/benchq/toolchain/bin:/usr/bin:/bin \
    bash -c 'ulimit -v 8388608; \
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
  env HOME=/var/lib/benchq CARGO_HOME=/var/lib/benchq/cargo \
    CARGO=/var/lib/benchq/toolchain/bin/cargo \
    PATH=/var/lib/benchq/bin:/var/lib/benchq/toolchain/bin:/usr/bin:/bin \
    bash -c 'ulimit -v 8388608; \
      export CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1; \
      exec cargo mutants --workspace --all-features \
        --file src/references.rs --jobs 1 --timeout 60 \
        --gitignore true --baseline skip'"
```

The two `--jobs` flags are different: the first after `cargo mutants` is the
number of mutant workers, while `--jobs 2` in `.cargo/mutants.toml` is Cargo
build parallelism. Use `bash -c`, not `bash -lc`: the benchq account cannot
read the login shell's `/home` profile, and `cargo mutants` needs `CARGO` set
to the provisioned toolchain. `--in-place` is acceptable only in the dedicated
remote checkout with one mutant worker; it avoids the per-worker copy but
mutates the source tree.

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
artifacts. If kbench is unavailable or not provisioned, use CI rather than the
shared host.

Local cleanup after an interrupted campaign:

```console
rm -rf mutants.out mutants.out.old ~/.tmp/cargo-mutants-*
```
