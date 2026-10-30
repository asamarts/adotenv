# Benchmark program

The goal is reproducible performance evidence for adotenv: parse, values-only,
map building, loading, editing and formatting throughput, memory and binary
size, with budgets that block regressions.

## Quiet host

Use a dedicated reference host for release measurements.

- Intel Core i7-6700HQ, 4 cores and 8 threads, 2.6 GHz
- 15 GiB RAM
- Linux x86_64
- rustc and cargo from the stable toolchain
- hyperfine and valgrind installed
- Record load average, rustc version and fixture hashes with every run

Run on an otherwise idle host. Do not use shared-runner wall time as a release
threshold.

## Workstreams

### 1. Harness and reproducibility

- [x] Simple 128-entry and 65,536-entry fixtures.
- [x] Mixed fixture with quoting, spacing, Unicode, comments and assigned values.
- [x] Wall time, CPU time, callgrind instructions, peak RSS, retained RSS and
  stripped binary size.
- [x] Budget check for selected profiles.
- [x] Record fixture hashes and host metadata with every result.
- [x] Add a real-world corpus from representative public `.env` files.
- [x] Add a per-dialect matrix.

### 2. Output profiles

- [x] Report native-map and values-only paths separately.
- [x] Record the cost of ordered maps as an API tradeoff.
- [x] Add default, values-only and full-report loader profiles.

### 3. Engineering metrics

- [x] Measure clean release build time.
- [x] Record library size per feature set.
- [x] Record edit, format, sort and compare throughput.

### 4. Optimization

- [x] Parse throughput.
- [x] Values-only throughput and memory.
- [x] Map build throughput and memory.
- [x] Default loader and full-report loader memory.
- [x] Parse-only binary size.
- [x] Startup time for the CLI.

### 5. Reporting

- [x] Publish a benchmark page on the documentation site.
- [x] Include method, host and budgets.
- [x] Record repeated runs and the median.
- [x] Keep historical results out of the current docs.

### 6. Release gates

- [x] Normal CI runs benchmark smoke only.
- [x] Manual budget check exists.
- [x] Run the complete reference measurement before a performance claim or a
  1.0 release.
- [x] Compare against the previous accepted baseline and require an explanation
  for a 1.5x to 2x regression.
- [x] Block above 2x until fixed or explicitly accepted.

## Current reference run

The current reference run uses the simple 128-entry and 65,536-entry
fixtures, the mixed fixture, the real-world corpus, CPU time, callgrind
instructions, RSS and parse-only binary size. Results are on the
[benchmark dashboard](benchmark-dashboard.md).

Two performance gaps remain accepted for now:

- mixed and real-world map building pays for the ordered map and the retained
  lossless document;
- the parse-only binary is larger than a values-only parser would need.

Both remain candidates for later optimization.

## Acceptance for 1.0

- The measurement covers simple, mixed and real-world fixtures.
- Wall time, CPU time, instructions, RSS and binary size are recorded.
- Results are reproducible on the reference host.
- The docs site publishes the current results and method.
- Performance budgets pass.
- Any remaining gap is documented with a reason.
