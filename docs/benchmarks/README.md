# Benchmark overview

This directory contains the comparable benchmark methodology, reproducible
commands, fixture catalogue, result schema and raw per-release evidence for
`adotenv`.

<div class="bench-callouts">
<a class="bench-callout" href="RESULTS-1.7.3.html"><strong>1.7.3 results</strong><span>Post-realloc-fix CompactValues, CompactEntries and map memory and speed evidence from tagged release v1.7.3.</span></a>
<a class="bench-callout" href="compact-memory-1.7.3.html"><strong>Memory chart</strong><span>Compare all available libraries, compact vs map, or the best-competitor view.</span></a>
<a class="bench-callout" href="speed-1.7.3.html"><strong>Speed chart</strong><span>Compare values and load timings across every size and shape.</span></a>
<a class="bench-callout" href="METHODOLOGY.html"><strong>Methodology</strong><span>How the comparable matrix, gates and publication runs work.</span></a>
</div>

The current published evidence is the [1.7.3 result page](RESULTS-1.7.3.md),
recorded from clean release tag `v1.7.3` (`c5c199f`). It records 20/20
non-empty strict speed wins (with 5/5 empty `load` losses from intentional
1.7.0 FIFO/permission file-opening checks) and 20/20 non-empty retained-heap
wins (10/10 empty ties and 30/30 peak/allocation wins, post-realloc fix) for
`CompactValues`, plus 10/10 strict speed wins and byte-for-byte identical
memory to `CompactValues` across all 15 `values` cells for bare-preserving
`CompactEntries`.

The [1.6.0 result page](RESULTS-1.6.0.md) remains historical evidence from
release-candidate commit `79860ac` (pre-realloc fix and pre-1.7.0 `load`
hardening). The [1.5.0 result page](RESULTS-1.5.0.md) remains historical
compact-values evidence (memory from `0a9fc92`, post-release speed from
`79937cc`).

Charts can show **all available pinned libraries**, isolate **compact vs map**, or show
only the **best competitor** per cell. Compact is the opt-in `CompactValues`
API, CompactEntries is the bare-preserving `CompactEntries` API, and map is the
standard `BTreeMap`-returning API.

The [1.4.1 result page](RESULTS-1.4.1.md) remains historical evidence for the
map-based API and for the speed claim. It records a strict win in all 25
comparable speed cells, all 30 peak-heap cells and all 30 allocation-count
cells. Retained heap is a tie in all 30 map-based cells and a loss in none.

## Documents

- [Methodology](METHODOLOGY.md) -- what is measured, how, and what is excluded.
- [Running benchmarks](RUNNING.md) -- reproduce a cell locally or on a dedicated reference host.
- [Scenario catalogue](SCENARIOS.md) -- fixtures, exact hashes and modes.
- [Result schema](SCHEMA.md) -- JSONL and bencher formats.
- [1.4.1 results](RESULTS-1.4.1.md) -- historical speed and map-based evidence.
- [1.5.0 results](RESULTS-1.5.0.md) -- historical compact-values memory and speed evidence.
- [1.6.0 results](RESULTS-1.6.0.md) -- historical 1.6.0 compact-values, compact-entries and map evidence.
- [1.7.3 results](RESULTS-1.7.3.md) -- current post-realloc-fix compact-values, compact-entries and map evidence.

The rendered tables are also available directly:

- [Current CI compact memory baseline](compact-memory.md) and
  [raw JSONL](compact-memory.jsonl) (180 cases: `adotenv`, `adotenv-compact`,
  `adotenv-compact-entries` and pinned competitors, post-realloc fix)
- [Current CI map memory baseline](memory.md) and
  [raw JSONL](memory.jsonl) (165 cases, post-realloc fix)
- [1.7.3 compact memory results](compact-memory-1.7.3.md) and
  [raw JSONL](compact-memory-1.7.3.jsonl)
- [1.7.3 speed results](speed-1.7.3.md) and
  [raw bencher output](speed-1.7.3.txt)
- [1.6.0 compact memory results](compact-memory-1.6.0.md) and
  [raw JSONL](compact-memory-1.6.0.jsonl)
- [1.6.0 speed results](speed-1.6.0.md) and
  [raw bencher output](speed-1.6.0.txt)
- [1.5.0 compact memory results](compact-memory-1.5.0.md) and
  [raw JSONL](compact-memory-1.5.0.jsonl)
- [1.5.0 compact speed results](speed-1.5.0.md) and
  [raw bencher output](speed-1.5.0.txt)
- [1.4.1 map memory results](memory-1.4.1.md) and
  [raw JSONL](memory-1.4.1.jsonl)
- [1.4.1 speed results](speed.md) and [raw bencher output](speed.txt)
- Fingerprints:
  [current compact memory](compact-memory.fingerprint.json),
  [current map memory](memory.fingerprint.json),
  [1.7.3 memory](compact-memory-1.7.3.fingerprint.json),
  [1.7.3 speed](speed-1.7.3.fingerprint.json),
  [1.6.0 memory](compact-memory-1.6.0.fingerprint.json),
  [1.6.0 speed](speed-1.6.0.fingerprint.json),
  [1.5.0 compact memory](compact-memory-1.5.0.fingerprint.json),
  [1.5.0 compact speed](speed-1.5.0.fingerprint.json),
  [1.4.1 memory](memory-1.4.1.fingerprint.json) and
  [1.4.1 speed](speed.fingerprint.json)

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
Publication runs use a dedicated quiet Linux reference host through a
single-slot serialized queue; do not run a publication benchmark concurrently
with another benchmark.

## Scope

The comparable claim covers Portable value parsing, file loading through each
library's value-loading API, and their peak/retained memory. Lossless
documents, edits, formatting, sorting, reference inspection and resolution are
internal workloads: competitors do not retain the same state, so comparing
them would not be like-for-like.

All fixture inputs are deterministic and content-addressed. See
[SCENARIOS.md](SCENARIOS.md) for the exact hashes and the supported matrix.
