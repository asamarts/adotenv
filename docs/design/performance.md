# Performance baseline

Status: local 0.8 development baseline recorded 2026-10-21; manual 1.0
performance budget recorded 2026-10-22. This is release evidence, not a
shared-runner CI gate.

## Method

- Host: Linux x86_64, AMD Ryzen 9 3900X, 12 cores / 24 threads.
- Toolchain: rustc 1.95.0 (2026-04-14).
- Benchmark: `cargo bench -p adotenv --bench core --locked -- \
  --sample-size 20 --measurement-time 3 --save-baseline 0.8-local`
- Fixtures: synthetic 8, 128 and 65,536-entry documents with comments,
  exports, Unicode, escapes and multiline values.
- The table reports Criterion midpoints. The 95% intervals are in the local
  Criterion output and are not committed.
- The machine was not CPU-pinned and was not dedicated to benchmarking. Treat
  these numbers as an order-of-magnitude baseline, not a release guarantee.

## Representative results

| Workload | 8 entries | 128 entries | 65,536 entries |
|---|---:|---:|---:|
| parse | 6.07 µs | 87.4 µs | 92.8 ms |
| lookup index build | 125 ns | 2.81 µs | 15.6 ms |
| indexed get, 64 keys | 194 ns | 1.69 µs | 1.62 µs |
| portability (4 presets) | 46.2 µs | 822 µs | 843 ms |
| references | 513 ns | 8.91 µs | 5.20 ms |
| resolve | 6.79 µs | 133 µs | 161 ms |
| sort | 3.12 µs | 247 µs | 436 ms |
| format normalize | 8.26 µs | 150 µs | 187 ms |
| native set | 8.57 µs | 91.4 µs | 80.3 ms |
| native batch | 7.12 µs | 75.6 µs | 106.6 ms |

The lookup-index rows were refreshed on 2026-10-23 after the 0.9.9 open-addressed
representation; the remaining rows retain the 0.8 baseline. The 65,536-entry
index lookup row measures 64 lookups per iteration. The existing
`Document::get` path scans entries and is intentionally retained for small
documents and zero-allocation one-off calls.

## Policy

- CI runs the **smoke** form of every Criterion workload; it does not enforce a
  performance threshold.
- A regression gate should wait for a stable dedicated runner or a controlled
  measurement environment. Shared-runner noise would otherwise create false
  failures.
- Re-record this baseline when the fixture generator, compiler major version or
  benchmark methodology changes, and note the new host/toolchain explicitly.
- Memory measurements remain a separate follow-up; the context resolver now
  retains raw winning tokens in `LoadedEnvironment`, which should be included.

## Manual 1.0 budget

The core implementation is unchanged since the 0.8 baseline, so the recorded
midpoints remain the reference. A 2026-10-22 attempt to refresh them was
discarded: the 24-thread host had a load average near 96, and several
representative workloads were several times slower. No noisy numbers were
recorded as a new baseline.

The manual release budget is:

- Re-run the documented benchmark command on an otherwise idle host with the
  same fixture generator, compiler, target and measurement settings.
- Compare representative small, typical and 65,536-entry midpoints against the
  recorded baseline. A result up to `1.5x` is accepted as normal local
  variance; `1.5x` to `2x` requires a repeat run and an explanation; above `2x`
  blocks the release until the regression is fixed or explicitly accepted.
- Prefer comparing ratios and the full Criterion interval over single point
  estimates. Do not compare runs from different hosts or load conditions.
- Update this file and the baseline together when the fixture generator,
  compiler major version, target, or methodology changes.
- Keep CI to benchmark smoke execution. Shared-runner timing is not used as a
  pass/fail threshold; a dedicated runner can replace this policy later.

The release checklist records the host, toolchain, load, command, baseline
name, representative midpoints and any accepted deviation.

## Ecosystem comparison (2026-10-22)

This is a local, not apples-to-apples comparison. `adotenv::parse` returns a
lossless document with token spans and edit/diagnostic support; the other
libraries return a key/value map or load directly into the process environment.
The closest `adotenv` comparison is `parse` followed by `effective_entries()`
collection.

### Method

- Linux x86_64, AMD Ryzen 9 3900X; shared host with load average around 43–53,
  so wall-clock times were noisy. Instruction counts are reproducible and are
  the primary CPU comparison below.
- Release builds. Fixture: 128 or 65,536 compatible `KEY=value`, quoted,
  exported and single-line comment entries (1.8 MB for the large case).
- Instruction counts include startup and fixture generation; the separately
  measured fixture baseline (465,574 instructions for 100 small iterations;
  59,801,078 for the large run) was subtracted from each parser total.
- Memory was measured with a counting global allocator in the same release
  binary. Live is after the returned value is retained; peak includes temporary
  parser allocations. Source text was allocated before counters were reset.
- Versions: `dotenvy` 0.15.7, `dotenv-compat` 0.4.0, `dotenv-ng-core` 1.0.0,
  `dotenv-parser` 0.1.3. `dotenv` 0.15.0 has no pure reader-iterator API and was
  excluded rather than comparing file I/O/process mutation.

### CPU work

| Implementation | 128 entries | 65,536 entries |
|---|---:|---:|
| adotenv parse | 471k instr | 272M instr |
| adotenv parse + map | 634k | 377M |
| dotenvy iterator + Vec | 669k | 373M |
| dotenv-compat parse | 555k | 307M |
| dotenv-ng input-only load | 719k | 406M |
| dotenv-parser map | 2.52M | 1,488M |

On the least-contended samples, the 65,536-entry wall times were approximately
51–55 ms for adotenv parse, 61–80 ms for dotenv-compat, 86–110 ms for dotenvy,
140–178 ms for dotenv-ng and 225–233 ms for dotenv-parser. The shared host made
wall-clock comparisons unreliable; instruction counts show adotenv's parser is
the cheapest of this set, while map collection brings it close to dotenvy.

### Memory

| Implementation | Live after parse | Peak |
|---|---:|---:|
| adotenv parse (after compacting) | 23.6 MB | 32.0 MB |
| adotenv parse + map | 4.6 MB | 32.0 MB |
| dotenvy iterator + Vec | 5.0 MB | 15.7 MB |
| dotenv-compat map | 9.5 MB | 18.6 MB |
| dotenv-ng map | 7.9 MB | 20.4 MB |
| dotenv-parser map | 7.1 MB | 50.1 MB |

Before the retained-capacity fix, adotenv parse retained 36.2 MB; shrinking
geometric-growth slack reduced that to 28.3 MB. Boxing rare diagnostics and
decoding maps and using a sentinel entry index then reduced it to 23.6 MB. The
layout is now `EntryData` 296 bytes, `ItemData` 64 bytes and the public
`EntryTokens` 240 bytes; 65,536 entries account for 23.6 MB of metadata plus the
borrowed 1.8 MB source. `EntryTokens` is now the dominant remaining cost and
cannot be compacted without a public field-type change. The map-comparison row
shows that converting to values drops live memory to the same range as dotenvy,
but peak memory still includes the document until it is dropped.

### Lightweight footprint

- Core normal dependencies: none; `std` default, `alloc`-only `no_std` available.
- CLI normal dependencies: only `adotenv`; release binary is about 1.1 MB
  (896 KiB text, 23 KiB data), dynamically linked only to libc/libgcc.
- Root package: 65 files, about 0.9 MiB uncompressed / 0.23 MiB compressed
  after including the new documentation. The larger size relative to
  dotenvy/dotenv is documentation, benchmarks and tests, not runtime
  dependencies.
- `dotenv-parser` pulls in pest/stacker/cc (and a proc-macro build);
  `dotenv-compat` pulls in AES-GCM, URL/ICU and platform dependencies.

### Loader retention (0.9.7)

`LoadOptions::new` is the default profile: values, shared-file origins and raw
winning tokens, with reports disabled. `LoadOptions::full` enables entries,
overrides, repeats and skipped declarations. Values and report views borrow
interned `Arc<str>` keys/values and the shared file table. For the 65,536-entry
fixture (one file, measured 2026-10-25 on a release build):

| Profile | Peak RSS | Retained RSS |
|---|---:|---:|
| default (`LoadOptions::new`) | 53.8 MB | 29.7 MB |
| `LoadOptions::full` | 63.9 MB | 35.6 MB |
| `load_values` | 13.9 MB | 12.6 MB |

The 0.9.19 comparison points were 63.6 MB / 37.1 MB for the full default,
16.2 MB retained for the separate compact values-plus-origins loader, and
13.6 MB for values-only loading. The unified default is below the 30 MB target;
the extra memory over the old compact API is the retained raw-token context
and the final origin map.

The full-loader merge reads each document once for reports instead of scanning
the whole document for every key. The previous per-key scans made the
65,536-entry case effectively quadratic; the single-pass merge removes that
cost. The loader still keeps one lossless document while each file is parsed
and uses a temporary occurrence map for reports. Winning raw tokens are stored
in one contiguous source buffer with ranges. Final origins use a compact
file-index/span/line record and one shared path per file. In full-report mode,
entry records share keys and values with the final value map, so overridden
final values are not duplicated. Reports are off by default, so applications
that only use values, origins or contextual resolution do not pay for report
vectors.
Values-only loading uses the shared parser's projection mode, so it retains
neither the document nor report metadata; its peak is close to the final map
size.

### Streaming and indexed diff (0.9.10/0.9.11)

Human and JSON diff parse the baseline once and each candidate incrementally,
buffering output until every candidate validates. Matrix output still retains
all documents because it needs every column at once. Streaming alone reduced
peak RSS from 95.6 MB to 67.4 MB, but `Document::compare` still scanned the
document for every key. 0.9.11 uses the explicit `LookupIndex` for comparison,
with a direct fallback for dialect-specific empty keys. On the same three-file
65,536-entry fixture, peak RSS is about 73.3 MB and the complete diff runs in
about 0.6 seconds instead of a quadratic scan; output remains identical. The
reduction over the retained-document path grows with the number of candidates;
these are local measurements, not CI thresholds.

Matrix mode now extends a compact row table one candidate at a time instead of
retaining every document. On the same three-file 65,536-entry fixture, peak RSS
falls from 119.6 MB to 103.0 MB and runtime from 141 seconds to 0.8 seconds,
with identical 10,289,460-byte table output.

## Performance follow-ups

Ranked by expected impact and risk:

1. **Retained parser slack and compact metadata (implemented).** `parse` shrinks
   `items` and `entries` before returning, boxes rare diagnostics, uses a sentinel
   entry index and boxes decoded/mapping storage. Retained memory falls from
   36.2 MB to 23.6 MB; parse+map peak falls from 43.4 MB to 32.0 MB. Callgrind
   shows no measurable instruction increase from the shrink step.
2. **Further internal metadata (evaluated 2026-10-25).** `ItemData` is 64 bytes,
   `EntryData` 296 bytes and the public `EntryTokens` 240 bytes; `EntryData` is
   dominated by `EntryTokens`. Remaining non-breaking padding/integer packing
   saves little, while changing public token field types requires a major version
   or a separate compact mode. Defer.
3. **Values-only loading and projection (implemented).** `load_values` and
   `load_cascade_values` run the shared parser with a projection output, retaining
   only final strings. Peak/retained memory for the 65,536-entry fixture is 13.6
   MB; the full loader remains the path that retains the lossless document.
4. **LoadedEnvironment residency (implemented).** `LoadOptions` can drop raw
   winning tokens, report vectors and/or the final origin map. The 0.10 default
   retains values, shared origins and raw tokens with reports disabled;
   values-only loading remains the lowest-memory option.
5. **Full-loader report merge (implemented).** Replace per-key document scans
   with one occurrence pass while preserving duplicate, bare, empty-key and
   override reporting.
6. **LookupIndex representation (implemented).** Replaced the `BTreeMap` with a
   compact open-addressed table. For 65,536 entries, build falls from 38.4 ms
   to 15.6 ms and a 64-key batch falls from 8.14 µs to 1.62 µs; retained RSS is
   about 30.9 MB versus 31.5 MB. Bounded probing falls back to the ordered map
   for collision-heavy inputs.
7. **Streaming multi-file diff (implemented).** Human/JSON diff parses the
   baseline and each candidate incrementally, buffering output to preserve the
   no-partial-output contract. Matrix mode now extends compact rows incrementally
   instead of retaining every document.
8. **Comparison lookup (implemented).** `Document::compare` uses `LookupIndex`
   for nonempty keys and a direct lookup only for dialect-specific empty keys,
   removing the per-key document scan.

9. **Unified 0.10 provenance (implemented).** The 1.0 review chose one interned
   `LoadedEnvironment` over two provenance types. The default now retains
   values, shared-file origins and raw tokens with reports opt-in, retaining
   29.7 MB at 53.8 MB peak on the 65,536-entry fixture. `LoadOptions::full`
   retains reports at 35.6 MB / 63.9 MB peak, and values-only loading retains
   12.6 MB / 13.9 MB peak. The decisions and public shape are documented in
   [compact-provenance.md](compact-provenance.md).

## Interpretation

- Parsing is linear and fast for typical files; the 65,536-entry fixture is a
  deliberately large batch/tooling case.
- Portability is expensive because it reparses the source across multiple
  presets; use it for migration/audit workflows, not hot paths.
- Formatting and sorting are batch transformations; both include verification
  work and clone/allocate output.
- The lookup index is useful when many lookups are expected; building it for a
  single lookup is not.
