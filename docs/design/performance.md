# Performance

These are reference measurements from one host, not CI thresholds. CI runs the
benchmarks once as a smoke test.

## Method

- Host: Intel Core i7-6700HQ, 4 cores and 8 threads at 2.6 GHz, 15 GiB RAM,
  Linux x86_64, rustc 1.97.0. The host is otherwise idle during measurements,
  and the benchmark process is pinned to one CPU.
- Release builds throughout.
- The core workloads come from `benches/core.rs`, which generates fixtures of
  8, 128 and 65,536 entries from four line shapes:

  ```console
  taskset -c 3 cargo bench -p adotenv --bench core --locked -- --sample-size 20 --measurement-time 2
  ```

- Instruction counts (callgrind), peak and retained RSS, binary size and
  startup time come from a separate harness that is not part of this
  repository. It uses a simple fixture of 65,536 declarations and records the
  fixture hashes, load average and compiler version with every run.

Wall time varies between runs by up to about 10% for 65,536 entries, even on
an idle host. Instruction counts are the stable CPU measure.

## Core workloads

Recorded for 1.0.0 on 2026-10-31, from commit `0a2e5c5`.

| Workload | 8 entries | 128 entries | 65,536 entries |
|---|---:|---:|---:|
| parse | 6.36 us | 102.5 us | 63.3 ms |
| lookup index build | 134.7 ns | 2.35 us | 3.44 ms |
| indexed get, 64 keys | 179.1 ns | 1.61 us | 1.71 us |
| portability, 4 presets | 61.9 us | 1.06 ms | 722 ms |
| references | 403.1 ns | 6.50 us | 4.62 ms |
| resolve | 4.18 us | 98.4 us | 70.2 ms |
| sort | 3.43 us | 294.0 us | 181.0 ms |
| format normalize | 8.38 us | 136.1 us | 77.3 ms |
| native set | 10.6 us | 104.5 us | 67.4 ms |
| native batch | 8.66 us | 105.3 us | 69.5 ms |

Compared with the 0.10.4 record from 2026-10-27, on the same host and compiler:

- resolve is about 42% faster;
- references are about 20% faster for small and typical documents, because
  values without `$` are not scanned;
- sort is 6% to 12% slower, because it keeps referencing pairs in source order;
- the other workloads are within run-to-run variation. Three runs of native set
  on 65,536 entries took 59.1 ms, 66.3 ms and 67.4 ms.

`Document::get` scans entries. For many lookups, build `Document::lookup_index`
once. The index starts small, grows with participating keys, and uses an
ordered fallback for collision-heavy input.

### 1.1.1 development-host check

Recorded on 2026-11-02 from release commit `bc0106e` on the development host,
an AMD Ryzen 9 3900X with 24 threads and rustc 1.97.1. The benchmark process
was pinned to one CPU. These values are a smoke check, not a new reference
record, and are not comparable with the reference-host table above:

```console
taskset -c 3 cargo bench -p adotenv --bench core --locked -- \
  --sample-size 10 --measurement-time 1
```

| Workload | 8 entries | 128 entries | 65,536 entries |
|---|---:|---:|---:|
| parse | 6.54 us | 90.8 us | 85.4 ms |
| lookup index build | 131.8 ns | 4.15 us | 8.77 ms |
| indexed get, 64 keys | 234.4 ns | 2.03 us | 2.49 us |
| portability, 4 presets | 54.6 us | 955 us | 878 ms |
| references | 332.5 ns | 7.62 us | 3.98 ms |
| resolve | 3.92 us | 77.2 us | 73.3 ms |
| sort | 3.52 us | 261 us | 226 ms |
| format normalize | 7.85 us | 145.9 us | 97.7 ms |
| native set | 10.2 us | 101.8 us | 74.1 ms |
| native batch | 7.50 us | 96.8 us | 75.7 ms |

### 1.2.0 development-host check

Recorded on 2026-11-04 from branch `prep-1.2.0` at `6fa1955` on the
development host, an AMD Ryzen 9 3900X with 24 threads and rustc 1.97.1. The
benchmark process was pinned to one CPU. This is a smoke check, not a new
reference record, and is not comparable with the reference-host table above:

```console
taskset -c 3 cargo bench -p adotenv --bench core --locked -- \
  --sample-size 10 --measurement-time 1
```

| Workload | 8 entries | 128 entries | 65,536 entries |
|---|---:|---:|---:|
| parse | 6.94 us | 112.6 us | 142.1 ms |
| lookup index build | 341 ns | 6.25 us | 35.6 ms |
| indexed get, 64 keys | 240 ns | 2.83 us | 8.51 us |
| portability, 4 presets | 41.3 us | 729 us | 651 ms |
| references | 427 ns | 6.47 us | 6.24 ms |
| resolve | 3.72 us | 105.7 us | 94.0 ms |
| sort | 3.29 us | 239 us | 204 ms |
| format normalize | 8.12 us | 315.6 us | 120.2 ms |
| native set | 7.88 us | 80.9 us | 56.5 ms |
| native batch | 6.93 us | 83.7 us | 49.5 ms |

The host was not isolated for this check; load average was around 1.9. The
values are useful for detecting regressions in the changed paths, but the
reference-host record above remains the last comparable release measurement.

CI runs the same benchmark once as a smoke test. The reference-host record
above remains the last comparable release measurement.

## Memory and instructions

Measured for 1.0.0 on 2026-10-31, from commit `0a2e5c5`, on the simple
65,536-declaration fixture (a 1.8 MB source), with 0.10.4 measured in the same
session for comparison. The separate memory and instruction harness was not
re-run for 1.1.1; the 1.1.1 changes touch loading, references, formatting edge
cases and the CLI, not the parser and edit hot loops. The 1.0.0 record remains
the last reference-host record:

| Profile | Instructions | Peak RSS | Retained RSS | 0.10.4 instructions | 0.10.4 retained |
|---|---:|---:|---:|---:|---:|
| parse | 211M | 26.6 MB | 26.6 MB | 207M | 26.5 MB |
| parse + values | 372M | 10.0 MB | 10.0 MB | 365M | 10.0 MB |
| parse + map | 280M | 34.7 MB | 16.2 MB | 276M | 16.2 MB |
| values-only load | 372M | 11.3 MB | 10.0 MB | 366M | 9.9 MB |
| default load | 469M | 47.8 MB | 28.0 MB | 465M | 27.8 MB |
| full-report load | 691M | 57.5 MB | 37.7 MB | 686M | 37.7 MB |

- **parse** keeps a lossless `Document`. Its memory is mostly item and token
  metadata.
- **parse + values** uses `parse_values`, which keeps no document.
- **parse + map** parses a document, collects the effective values into a map
  and drops the document.
- **default load** keeps one record per key for the value, its origin and its
  raw token.
- **full-report load** adds the entry, override, repeat and skipped reports.
- **values-only load** projects values while parsing and keeps no document or
  provenance.

## Budgets

A release is checked against these budgets on the reference host, with the
fixture above.

| Profile | Instructions | Peak RSS | Retained RSS |
|---|---:|---:|---:|
| parse | 260M | not budgeted | 29 MB |
| parse + values | 400M | 12 MB | 12 MB |
| parse + map | 335M | not budgeted | 17 MB |
| values-only load | 400M | 13 MB | 11.5 MB |
| default load | 560M | 50 MB | 30 MB |
| full-report load | 750M | 60 MB | 40 MB |

The parse-only binary has a 500 KB budget.

- A result above an instruction, memory or binary-size budget blocks a release
  until the regression is fixed or explicitly accepted.
- Wall time has no absolute budget. It is compared with the previous record
  from the same host and compiler: a 1.5x slowdown needs a repeat run and an
  explanation, and a 2x slowdown blocks the release until it is explained or
  fixed.
- Update this page when the fixtures, the compiler major version or the method
  change.

The parse + map retained budget was 16 MB until 1.0.0. Both 0.10.4 and 1.0.0
measure 16.2 MB on the reference host, where the earlier record was 14.2 MB, so
the budget was raised to 17 MB. The difference is memory that the allocator
keeps after the document is dropped while the map stays alive, not memory that
adotenv holds.

## CI regression gates

CI separates deterministic internal gates from publication wall-time evidence:

- `scripts/det-perf-gate.sh` builds the merge base and the PR in one session and
  runs `adotenv-bench/benches/det_parse.rs` under Valgrind Callgrind. The
  gungraun soft limits are `Ir` +2% and `EstimatedCycles` +5%; branch counters
  are diagnostic only. The comparison uses only instruction simulation, so
  hosted-runner load cannot fail it. The gate skips its first PR when the base
  predates `det_parse`, then gates subsequent changes.
- `adotenv-xtask gate` compares a fresh release memory JSONL against
  `docs/benchmarks/memory.jsonl` for every adotenv case. Allocation counts must
  match exactly; peak and retained heap may grow by at most 5% or 64 bytes.
- `bench-smoke` compiles and exercises the deterministic harness and runs the
  memory gate on every PR. The harness smoke is non-gating; the memory gate is
  gating. The deterministic `Ir`/cycle gate runs in `det-perf`.
- Competitor criterion and kbench wall-time results are advisory at PR time.
  Only the deterministic gates block a change.
- Update the memory baseline only with an intentional, reviewed code change and
  a fresh fingerprint. Do not update it to make a failing gate pass. The
  gungraun gate has no committed output; it always compares against the live
  merge base.

### 1.4.1 comparable baseline

The comparable matrix on `kbench` (five sizes, `plain`/`long-line`/`empty`,
values and load) is recorded in
[`docs/benchmarks/speed.md`](../benchmarks/speed.md) and
[`docs/benchmarks/memory.md`](../benchmarks/memory.md). The speed and memory snapshots share SHA `0649096`. The final release evidence
must still re-run both from the release-candidate SHA. Against the best
competitor in each cell:

- speed: **25/25 strict wins**;
- peak heap: **30/30 strict wins**;
- allocation count: **30/30 strict wins**;
- retained heap: ties in all 30 cells, with no losses.

The retained result cannot become a strict win without changing the public
`BTreeMap<String, String>` result type: both APIs return the same map at small
sizes, and larger cells are already at the compact-tree floor. For 1.4.1 this is
accepted as a no-loss result, not treated as a strict win; a compact values API
is a candidate for a later minor release.

## Earlier measurements

These were recorded for 0.10.4 on 2026-10-27, on the same host, and were not
repeated for 1.0. The 0.11 parser changes to blank lines in Node and Python
and to quoted tails in Compose affect the dialect rows.

### Fixtures

65,536 declarations, except the mixed fixture and the real-world corpus, which
repeat smaller inputs with unique key prefixes.

| Fixture | Workload | Wall | Instructions | Retained |
|---|---|---:|---:|---:|
| Mixed quoting, spacing, Unicode and comments | parse | 20.6 ms | 106M | 11.0 MB |
| Mixed quoting, spacing, Unicode and comments | parse + map | 27.6 ms | 153M | 5.8 MB |
| Pinned public configurations | parse | 9.4 ms | 63M | 4.2 MB |
| Pinned public configurations | parse + map | 11.2 ms | 78M | 2.4 MB |

### Dialects

| Dialect | Wall | Instructions | Retained |
|---|---:|---:|---:|
| Portable | 33.3 ms | 207M | 26.5 MB |
| Node | 60.6 ms | 423M | 14.3 MB |
| Python | 50.6 ms | 329M | 14.3 MB |
| Compose | 51.7 ms | 305M | 14.2 MB |
| Docker | 42.7 ms | 269M | 14.3 MB |

### Comparison

For three 65,536-entry files, measured on 2026-10-25:

- streaming two-file diff with indexed comparison: about 73.3 MB peak and
  0.6 s;
- matrix diff with compact incremental rows: about 103.0 MB peak and 0.8 s.

Matrix output keeps one row per key because every column is needed. Two-file
diff streams candidates and buffers output until all candidates validate.

### Build and startup

Clean release builds:

| Binary | Build | Stripped size | Startup |
|---|---:|---:|---:|
| parse-and-print demo | 46.6 s | 454 KB | 651 us |
| `adotenv-cli` | 16.8 s | 1,199 KB | 653 us |

| Library features | Build | Library |
|---|---:|---:|
| No default features | 11.9 s | 2,616 KB |
| Default (`std`) | 11.9 s | 2,616 KB |
| `load` | 13.2 s | 3,111 KB |
| `serde` | 14.8 s | 2,789 KB |
| `load` and `serde` | 16.2 s | 3,324 KB |
| `exec` | 12.1 s | 2,654 KB |

## Known gaps

- Building a map from mixed and real-world input pays for the ordered map and
  the retained lossless document. `parse_values` avoids both.
- The parse-only binary is larger than a values-only parser would need.

Both remain candidates for later optimization.
