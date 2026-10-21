# Performance baseline

Status: local 0.8 development baseline recorded 2026-10-21. This is evidence,
not a CI performance gate.

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
| lookup index build | 320 ns | 10.2 µs | 38.4 ms |
| indexed get, 64 keys | 246 ns | 5.69 µs | 8.14 µs |
| portability (4 presets) | 46.2 µs | 822 µs | 843 ms |
| references | 513 ns | 8.91 µs | 5.20 ms |
| resolve | 6.79 µs | 133 µs | 161 ms |
| sort | 3.12 µs | 247 µs | 436 ms |
| format normalize | 8.26 µs | 150 µs | 187 ms |
| native set | 8.57 µs | 91.4 µs | 80.3 ms |
| native batch | 7.12 µs | 75.6 µs | 106.6 ms |

The 65,536-entry index lookup row measures 64 lookups per iteration, so the
effective per-lookup cost is on the order of hundreds of nanoseconds. The
existing `Document::get` path scans entries and is intentionally retained for
small documents and zero-allocation one-off calls.

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

## Interpretation

- Parsing is linear and fast for typical files; the 65,536-entry fixture is a
  deliberately large batch/tooling case.
- Portability is expensive because it reparses the source across multiple
  presets; use it for migration/audit workflows, not hot paths.
- Formatting and sorting are batch transformations; both include verification
  work and clone/allocate output.
- The lookup index is useful when many lookups are expected; building it for a
  single lookup is not.
