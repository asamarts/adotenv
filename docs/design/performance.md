# Performance

These are local reference measurements, not CI thresholds.

## Method

- Linux x86_64, AMD Ryzen 9 3900X.
- Release build with a fixed fixture generator.
- `cargo bench -p adotenv --bench core --locked -- --sample-size 20 --measurement-time 3`
- The host is shared. Treat wall-clock values as approximate and prefer
  instruction counts when comparing CPU work.
- Re-run on an idle host before making a performance decision.

## Core workloads

| Workload | 8 entries | 128 entries | 65,536 entries |
|---|---:|---:|---:|
| parse | 6.07 us | 87.4 us | 92.8 ms |
| lookup index build | 125 ns | 2.81 us | 15.6 ms |
| indexed get, 64 keys | 194 ns | 1.69 us | 1.62 us |
| portability, 4 presets | 46.2 us | 822 us | 843 ms |
| references | 513 ns | 8.91 us | 5.20 ms |
| resolve | 6.79 us | 133 us | 161 ms |
| sort | 3.12 us | 247 us | 436 ms |
| format normalize | 8.26 us | 150 us | 187 ms |
| native set | 8.57 us | 91.4 us | 80.3 ms |
| native batch | 7.12 us | 75.6 us | 106.6 ms |

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
| Default | 53.8 MB | 29.7 MB |
| `LoadOptions::full` | 63.9 MB | 35.6 MB |
| Values only | 13.9 MB | 12.6 MB |

The default keeps values, shared-file origins and raw tokens. Reports add entry,
override, repeat and skipped vectors. Values-only loading projects values while
parsing and retains no document or provenance.

## Diff and matrix

For three 65,536-entry files:

- streaming two-file diff with indexed comparison: about 73.3 MB peak and 0.6 s;
- matrix diff with compact incremental rows: about 103.0 MB peak and 0.8 s.

Matrix output retains one row per key because every column is needed. Two-file
diff streams candidates and buffers output until all candidates validate.

## Comparison with other parsers

The closest comparison is `adotenv::parse` followed by collecting effective
values. Other crates return maps or load into the environment directly.

Fixture: 65,536 compatible declarations, 1.8 MB source.

| Implementation | Instructions | Live memory | Peak |
|---|---:|---:|---:|
| adotenv parse | 272M | 23.6 MB | 32.0 MB |
| adotenv parse + map | 377M | 4.6 MB | 32.0 MB |
| dotenvy iterator + Vec | 373M | 5.0 MB | 15.7 MB |
| dotenv-compat parse | 307M | 9.5 MB | 18.6 MB |
| dotenv-ng input-only load | 406M | 7.9 MB | 20.4 MB |
| dotenv-parser map | 1,488M | 7.1 MB | 50.1 MB |

adotenv retains more syntax than the other parsers, including spans, items and
diagnostics. Values-only loading removes that overhead when it is not needed.

## Budget

- CI runs benchmark smoke only.
- Compare repeated runs on the same host and toolchain.
- A 1.5x deviation needs a repeat run and an explanation.
- A 2x deviation blocks a release until it is explained or fixed.
- Update this file when fixtures, compiler major version or methodology change.
