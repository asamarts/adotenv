# Running benchmarks

## Prerequisites

- Linux x86_64 for the reference memory path.
- Rust stable for Criterion and the workspace.
- `valgrind` and the `gungraun-runner` matching the workspace `gungraun`
  version for deterministic benches.
- For publication, SSH access to `kbench` and use of the shared benchq pueue
  daemon. Never bypass the queue.

Set a private scratch directory on a shared host, for example
`~/.tmp/adotenv-bench`, rather than using `/tmp` for build output.

## Reproduce one memory cell

```console
mkdir -p ~/.tmp/adotenv-bench
cargo run --release -p adotenv-xtask -- memory \
  --out ~/.tmp/adotenv-bench/memory.jsonl \
  --sizes 128 --shapes plain --modes values
```

The JSONL row records the fixture SHA-256 and all memory metrics. A sibling
fingerprint is written next to the output. To run the full matrix locally:

```console
cargo run --release -p adotenv-xtask -- memory \
  --out ~/.tmp/adotenv-bench/memory.jsonl \
  --sizes 8,128,1k,10k,100k --shapes plain,long-line,empty \
  --modes values,load,document,resolve
```

`document` and `resolve` are internal adotenv cases; competitor load/values
cells are selected automatically by supported API.

`adotenv-compact` is an additional `values` and `load` implementation for the
compact-values evaluation. Run the separate compact matrix with `--compact`:

```console
cargo run --release -p adotenv-xtask -- queue \
  --remote-root /var/lib/benchq/adotenv-bench \
  --out docs/benchmarks/compact-memory.jsonl \
  --remote-out bench-compact-memory.jsonl \
  --sizes 8,128,1k,10k,100k --shapes plain,long-line,empty \
  --modes values,load --compact --sync
```

The local equivalent accepts the same flag and defaults to
`docs/benchmarks/compact-memory.jsonl`. The result is evaluation evidence for
a future minor release; the 1.4.1 matrix remains unchanged.

## Reproduce one speed cell

```console
cargo run --release -p adotenv-xtask -- speed \
  --out ~/.tmp/adotenv-bench/speed.txt \
  --bench competitors --filter 'plain/128'
```

The filter is a Criterion name substring. The result contains all supported
implementations for matching mode/shape/size cells. Do not use `--quick` for a
publication run: Criterion's quick bencher output omits benchmark names.

The full speed command is:

```console
cargo run --release -p adotenv-xtask -- speed \
  --out ~/.tmp/adotenv-bench/speed.txt --bench competitors
```

## Render and compare

```console
cargo run --release -p adotenv-xtask -- report \
  --input ~/.tmp/adotenv-bench/memory.jsonl \
  --out ~/.tmp/adotenv-bench/memory.md
cargo run --release -p adotenv-xtask -- compare \
  --baseline docs/benchmarks/memory.jsonl \
  --current ~/.tmp/adotenv-bench/memory.jsonl
```

`compare` reports deltas; the deterministic gate is separate:

```console
cargo run --release -p adotenv-xtask -- gate \
  --baseline docs/benchmarks/memory.jsonl \
  --current ~/.tmp/adotenv-bench/memory.jsonl
```

## Publication run on kbench

Build and queue both matrices from a clean commit. The remote root must be
writable by the benchq daemon and should be on its state filesystem:

```console
cargo run --release -p adotenv-xtask -- queue \
  --remote-root /var/lib/benchq/adotenv-bench \
  --out docs/benchmarks/memory.jsonl \
  --sizes 8,128,1k,10k,100k --shapes plain,long-line,empty \
  --modes values,load,document,resolve --sync

cargo run --release -p adotenv-xtask -- queue-speed \
  --remote-root /var/lib/benchq/adotenv-bench \
  --out docs/benchmarks/speed.txt --sync
```

The task is added to the shared `bench` group with one slot and waits for
completion before copying results and fingerprints back. Do not run a second
benchmark outside the queue. `--dry-run` shows the queued command without
touching the host.

The queue path builds release memory/xtask binaries locally, installs them into
benchq's state directory, and records the local git SHA through
`ADOTENV_GIT_SHA`; the remote fingerprint also records remote CPU/OS/kernel and
load.

## Deterministic gate

```console
bash scripts/det-perf-gate.sh
```

The script installs/checks `valgrind` and the exact `gungraun-runner` version,
builds the merge base and head with isolated target directories, runs the
`det_parse` map/compact parsing and compact-loading benches, and lets
gungraun's +2% Ir / +5% cycle soft limits fail the job. A deliberate duplicated parse is a useful manual check when changing the
gate.

## Publication checklist

1. Start from a clean commit.
2. Run both matrices through `kbench`/benchq.
3. Verify both fingerprint files describe the intended host and record load.
4. Render the Markdown reports.
5. Check all comparable cells and record tiers/tie/misses explicitly.
6. Update the per-version result page and link the raw JSONL, bencher output,
   rendered tables and fingerprints.
7. Do not tag until the evidence is reviewed.
