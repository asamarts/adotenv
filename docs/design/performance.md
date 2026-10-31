# Performance

These are local reference measurements, not CI thresholds.

## Method

- Reference host: Intel Core i7-6700HQ, 4 cores and 8 threads, 2.6 GHz,
  Linux x86_64, rustc 1.97.0. The benchmark process was pinned to one CPU.
- Release build with a fixed fixture generator.
- `cargo bench -p adotenv --bench core --locked -- --sample-size 20 --measurement-time 2`
- The host was otherwise idle. Wall time is recorded; instruction counts are the
  stable CPU measure.

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
- the other workloads are within run-to-run variation, which is up to about
  10% for 65,536 entries. Three runs of native set on that size took 59.1 ms,
  66.3 ms and 67.4 ms.

`Document::get` scans entries. For many lookups, build `Document::lookup_index`
once. The index uses a compact open-addressed table with an ordered fallback for
collision-heavy input.

## Parser memory

One 65,536-entry document with a 1.8 MB source:

- live after parse: 23.6 MB;
- peak parse: 32.0 MB;
- parse plus collected map: 4.6 MB live after the document is dropped.

Retained document memory is dominated by item and token metadata. Values-only
loading avoids the document entirely.

## Loading memory

One 65,536-entry file on a release build:

| Profile | Peak | Retained |
|---|---:|---:|
| Default | 47.7 MB | 28.0 MB |
| `LoadOptions::full` | 57.4 MB | 37.7 MB |
| Values only | 11.2 MB | 10.0 MB |

The default keeps values, shared-file origins and raw tokens. Reports add entry,
override, repeat and skipped vectors. Values-only loading projects values while
parsing and retains no document or provenance.

The default profile now stores value, origin and raw-token data in one record
per key. This removes two per-key maps from the retained loader.

## Budgets

Budgets are checked on the reference host before a release, with the same
fixtures as the tables above.

| Profile | Instructions | Peak RSS | Retained RSS |
|---|---:|---:|---:|
| adotenv parse | 260M | not budgeted | 29 MB |
| adotenv parse + values | 400M | 12 MB | 12 MB |
| adotenv parse + map | 335M | not budgeted | 17 MB |
| values-only load | 400M | 13 MB | 11.5 MB |
| default load | 560M | 50 MB | 30 MB |
| full-report load | 750M | 60 MB | 40 MB |

Measured for 1.0.0 on 2026-10-31, from commit `0a2e5c5`, with 0.10.4 measured
in the same session for comparison:

| Profile | Instructions | Peak RSS | Retained RSS | 0.10.4 instructions | 0.10.4 retained |
|---|---:|---:|---:|---:|---:|
| adotenv parse | 211M | 26.6 MB | 26.6 MB | 207M | 26.5 MB |
| adotenv parse + values | 372M | 10.0 MB | 10.0 MB | 365M | 10.0 MB |
| adotenv parse + map | 280M | 34.7 MB | 16.2 MB | 276M | 16.2 MB |
| values-only load | 372M | 11.3 MB | 10.0 MB | 366M | 9.9 MB |
| default load | 469M | 47.8 MB | 28.0 MB | 465M | 27.8 MB |
| full-report load | 691M | 57.5 MB | 37.7 MB | 686M | 37.7 MB |

The parse + map retained budget was 16 MB. Both releases now measure 16.2 MB
on the reference host, where the earlier record was 14.2 MB, so the budget was
raised to 17 MB for 1.0.0. The difference is memory that the allocator keeps
after the document is dropped while the map stays alive, not memory that
adotenv holds.

The parse-only binary budget is 500 KB. Wall time is recorded but not enforced
because the shared host is too noisy for a hard threshold.

A release blocks above the instruction or memory budget until the regression is
fixed or explicitly accepted. A wall-time result between 1.5x and 2x requires a
repeat run and an explanation; above 2x blocks the release.

## Diff and matrix

For three 65,536-entry files:

- streaming two-file diff with indexed comparison: about 73.3 MB peak and 0.6 s;
- matrix diff with compact incremental rows: about 103.0 MB peak and 0.8 s.

Matrix output retains one row per key because every column is needed. Two-file
diff streams candidates and buffers output until all candidates validate.

## Budget

- CI runs benchmark smoke only.
- Compare repeated runs on the same host and toolchain.
- A 1.5x deviation needs a repeat run and an explanation.
- A 2x deviation blocks a release until it is explained or fixed.
- Update this file when fixtures, compiler major version or methodology change.
