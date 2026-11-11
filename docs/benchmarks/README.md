# Benchmarks

This directory contains the comparable benchmark methodology, reproducible
commands, fixture catalogue, result schema and raw per-release evidence for
`adotenv`.

The current release evidence is the
[1.4.1 result page](RESULTS-1.4.1.md). It records a strict win in all 25
comparable speed cells, all 30 peak-heap cells and all 30 allocation-count
cells. Retained heap is a tie in all 30 cells and a loss in none; that is the
accepted 1.4.1 claim, not a strict retained-memory win.

## Documents

- [Methodology](METHODOLOGY.md) -- what is measured, how, and what is excluded.
- [Running benchmarks](RUNNING.md) -- reproduce a cell locally or on `kbench`.
- [Scenario catalogue](SCENARIOS.md) -- fixtures, exact hashes and modes.
- [Result schema](SCHEMA.md) -- JSONL and bencher formats.
- [1.4.1 results](RESULTS-1.4.1.md) -- current raw evidence and summary.

The rendered tables are also available directly:

- [Memory results](memory.md) and [raw JSONL](memory.jsonl)
- [Speed results](speed.md) and [raw bencher output](speed.txt)
- [Memory fingerprint](memory.fingerprint.json) and
  [speed fingerprint](speed.fingerprint.json)

## Reproduce one cell

```console
cargo run --release -p adotenv-xtask -- memory \
  --out ~/.tmp/adotenv-bench/one-memory.jsonl \
  --sizes 128 --shapes plain --modes values
cargo run --release -p adotenv-xtask -- speed \
  --out ~/.tmp/adotenv-bench/one-speed.txt --bench competitors --filter 'plain/128'
```

Use a private scratch directory such as `~/.tmp/adotenv-bench` rather than
`/tmp`. The raw result and fingerprint files are the source of truth.
Publication runs use `kbench` through the serialized benchq queue; do not run a
publication benchmark concurrently with another benchmark.

## Scope

The comparable claim covers Portable value parsing, file loading through each
library's value-loading API, and their peak/retained memory. Lossless
documents, edits, formatting, sorting, reference inspection and resolution are
internal workloads: competitors do not retain the same state, so comparing
them would not be like-for-like.

All fixture inputs are deterministic and content-addressed. See
[SCENARIOS.md](SCENARIOS.md) for the exact hashes and the supported matrix.
