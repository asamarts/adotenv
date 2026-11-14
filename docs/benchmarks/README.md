# Benchmark overview

This directory contains the comparable benchmark methodology, reproducible
commands, fixture catalogue, result schema and raw per-release evidence for
`adotenv`.

<div class="bench-callouts">
<a class="bench-callout" href="RESULTS-1.5.0.html"><strong>1.5.0 results</strong><span>Compact memory and speed evidence, with charts and sortable tables.</span></a>
<a class="bench-callout" href="compact-memory.html"><strong>Memory chart</strong><span>Compare all available libraries, compact vs map, or the best-competitor view.</span></a>
<a class="bench-callout" href="speed-1.5.0.html"><strong>Speed chart</strong><span>Compare values and load timings across every size and shape.</span></a>
<a class="bench-callout" href="METHODOLOGY.html"><strong>Methodology</strong><span>How the comparable matrix, gates and publication runs work.</span></a>
</div>

The current release evidence is the [1.5.0 result page](RESULTS-1.5.0.md). It
records strict non-empty retained-heap wins, with zero-byte ties on empty
inputs, strict peak and allocation wins in all values/load cells, and 25/25
speed wins against the best pinned competitor.

Charts can show **all available pinned libraries**, isolate **compact vs map**, or show
only the **best competitor** per cell. Compact is the opt-in `CompactValues`
API; map is the standard `BTreeMap`-returning API.

The [1.4.1 result page](RESULTS-1.4.1.md) remains historical evidence for the
map-based API and for the speed claim. It records a strict win in all 25
comparable speed cells, all 30 peak-heap cells and all 30 allocation-count
cells. Retained heap is a tie in all 30 map-based cells and a loss in none.

## Documents

- [Methodology](METHODOLOGY.md) -- what is measured, how, and what is excluded.
- [Running benchmarks](RUNNING.md) -- reproduce a cell locally or on `kbench`.
- [Scenario catalogue](SCENARIOS.md) -- fixtures, exact hashes and modes.
- [Result schema](SCHEMA.md) -- JSONL and bencher formats.
- [1.4.1 results](RESULTS-1.4.1.md) -- historical speed and map-based evidence.
- [1.5.0 results](RESULTS-1.5.0.md) -- current compact memory and speed evidence.

The rendered tables are also available directly:

- [1.4.1 map memory results](memory.md) and [raw JSONL](memory.jsonl)
- [1.5.0 compact memory results](compact-memory.md) and
  [raw JSONL](compact-memory.jsonl)
- [1.4.1 speed results](speed.md) and [raw bencher output](speed.txt)
- [1.5.0 compact speed results](speed-1.5.0.md) and
  [raw bencher output](speed-1.5.0.txt)
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
