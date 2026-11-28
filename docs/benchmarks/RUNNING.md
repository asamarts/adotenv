# Running benchmarks

## Prerequisites

- Linux x86_64 for the reference memory path.
- Rust stable for Criterion and the workspace.
- `valgrind` and the `gungraun-runner` matching the workspace `gungraun`
  version for deterministic benches.
- For publication, SSH access to a dedicated quiet Linux reference host with a
  single-slot serialized `pueue` queue. Never bypass the queue.

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
compact-values evaluation, and `adotenv-compact-entries` adds `values`
measurements for bare-preserving `CompactEntries`. Run the compact matrix with
`--compact`:

```console
cargo run --release -p adotenv-xtask -- queue \
  --host "$ADOTENV_BENCH_HOST" \
  --remote-root "$ADOTENV_BENCH_ROOT" \
  --out docs/benchmarks/compact-memory.jsonl \
  --remote-out bench-compact-memory.jsonl \
  --sizes 8,128,1k,10k,100k --shapes plain,long-line,empty \
  --modes values,load --compact --sync
```

The local equivalent accepts the same flag and defaults to
`docs/benchmarks/compact-memory.jsonl`. The 1.8.0 publication run records both
`adotenv-compact` and `adotenv-compact-entries` in
`docs/benchmarks/compact-memory-1.8.0.jsonl` and
`docs/benchmarks/speed-1.8.0.txt`; the 1.7.3, 1.6.0, 1.5.0 and 1.4.1 matrices
remain as historical evidence.

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

For the 1.5.0 compact speed evidence, use
`--out docs/benchmarks/speed-1.5.0.txt --remote-out bench-speed-1.5.0.txt` with
`queue-speed` and `--bench competitors`.

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

cargo run --release -p adotenv-xtask -- gate \
  --baseline docs/benchmarks/compact-memory.jsonl \
  --current ~/.tmp/adotenv-bench/compact-memory.jsonl \
  --implementation adotenv-compact

cargo run --release -p adotenv-xtask -- gate \
  --baseline docs/benchmarks/compact-memory.jsonl \
  --current ~/.tmp/adotenv-bench/compact-memory.jsonl \
  --implementation adotenv-compact-entries
```

## Baseline maintenance

The committed memory baselines are `memory.jsonl` for the map API and
`compact-memory.jsonl` for `adotenv-compact` and `adotenv-compact-entries`.
Normal PRs do not update either baseline. When an intentional allocation change
lands, regenerate both matrices from the same commit, update their rendered
Markdown and fingerprints, and run:

```console
cargo run --release -p adotenv-xtask -- gate \
  --baseline docs/benchmarks/memory.jsonl \
  --current ~/.tmp/adotenv-bench/memory.jsonl

cargo run --release -p adotenv-xtask -- gate \
  --baseline docs/benchmarks/compact-memory.jsonl \
  --current ~/.tmp/adotenv-bench/compact-memory.jsonl \
  --implementation adotenv-compact

cargo run --release -p adotenv-xtask -- gate \
  --baseline docs/benchmarks/compact-memory.jsonl \
  --current ~/.tmp/adotenv-bench/compact-memory.jsonl \
  --implementation adotenv-compact-entries
```

Publication speed files, including `speed.txt`, `speed-1.5.0.txt`,
`speed-1.6.0.txt`, `speed-1.7.3.txt` and `speed-1.8.0.txt`, are evidence and
are not PR gates.

## Publication run on a reference host

Build and queue both matrices from a clean commit. Set `ADOTENV_BENCH_HOST`
(or pass `--host`), `ADOTENV_BENCH_ROOT` (an absolute, non-root, non-home
working directory writable by the queue service), and optional
`PUEUE_CONFIG_PATH`, `ADOTENV_BENCH_GROUP` (default `bench`) and
`ADOTENV_BENCH_USER`:

```console
cargo run --release -p adotenv-xtask -- queue \
  --host "$ADOTENV_BENCH_HOST" \
  --remote-root "$ADOTENV_BENCH_ROOT" \
  --out docs/benchmarks/memory.jsonl \
  --sizes 8,128,1k,10k,100k --shapes plain,long-line,empty \
  --modes values,load,document,resolve --sync

cargo run --release -p adotenv-xtask -- queue-speed \
  --host "$ADOTENV_BENCH_HOST" \
  --remote-root "$ADOTENV_BENCH_ROOT" \
  --out docs/benchmarks/speed.txt --sync
```

The task is added to the single-slot serialized queue (`bench` group by
default) and waits for completion before copying results and fingerprints back.
Do not run a second benchmark outside the queue. `--dry-run` shows the queued
command without touching the host.

The queue path builds release memory/xtask binaries locally, installs them into
the configured remote working directory, and records the local git SHA through
`ADOTENV_GIT_SHA`; the remote fingerprint also records remote CPU/OS/kernel and
load.

## Deterministic gate

```console
bash scripts/det-perf-gate.sh
```

The script installs/checks `valgrind` and the exact `gungraun-runner` version,
builds the merge base and head with isolated target directories, runs the
`det_parse` map, compact values, compact entries and compact-loading benches,
and lets gungraun's +2% Ir / +5% cycle soft limits fail the job. A deliberate
duplicated parse is a useful manual check when changing the gate.

## Publication checklist

1. Start from a clean commit.
2. Run both matrices on the dedicated reference host through its single-slot
   serialized queue.
3. Verify both fingerprint files describe the intended host and record load.
4. Render the Markdown reports.
5. Check all comparable cells and record tiers/tie/misses explicitly.
6. Update the per-version result page and link the raw JSONL, bencher output,
   rendered tables and fingerprints.
7. Do not tag until the evidence is reviewed.
