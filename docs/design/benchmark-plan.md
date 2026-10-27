# Benchmark program

The goal is to measure adotenv against the main Rust `.env` parsers with
reproducible evidence and to keep adotenv at the front of the comparison on
material metrics.

## Quiet host

Use `kbench` through SSH for the reference run.

- Intel Core i7-6700HQ, 4 cores and 8 threads, 2.6 GHz
- 15 GiB RAM
- Linux x86_64
- rustc and cargo from the stable toolchain
- hyperfine and valgrind installed
- Record load average, hostname, rustc version and fixture hashes with every run

Run on an otherwise idle host. Do not use shared-runner wall time as a release
threshold.

## Workstreams

### 1. Harness and reproducibility

- [x] Standalone comparison crate with pinned competitor versions.
- [x] Simple 128-entry and 65,536-entry fixtures.
- [x] Mixed fixture with quoting, spacing, Unicode, comments and assigned values.
- [x] Wall time, CPU time, callgrind instructions, peak RSS, retained RSS and
  stripped binary size.
- [x] One-command report generation and optional document update.
- [x] Budget check for selected profiles.
- [x] Record fixture hashes and host metadata in the JSON result.
- [x] Add a real-world corpus from representative public `.env` files.
- [x] Add a per-dialect adotenv matrix.

### 2. Comparable output

- [x] Report native-map and values-only paths separately.
- [ ] Record hash-map versus ordered-map costs.
- [ ] Add stream and loader profiles where the competitor exposes them.
- [x] Exclude or clearly label non-comparable APIs.

### 3. Engineering metrics

- [ ] Measure clean release build time for each parser demo.
- [ ] Record binary size per feature set.
- [ ] Record dependency count, MSRV and build-script requirements.
- [ ] Record edit, format, sort and compare throughput for adotenv-only
  workloads where no competitor equivalent exists.

### 4. Optimization

- [ ] Parse throughput.
- [ ] Values-only throughput and memory.
- [ ] Map build throughput and memory.
- [ ] Default loader and full-report loader memory.
- [ ] Parse-only binary size.
- [ ] Startup time for the CLI.

### 5. Reporting

- [x] Generate a benchmark page for the documentation site.
- [x] Include method, host, versions, raw JSON links and budgets.
- [ ] Record repeated runs and minimum, median and standard deviation.
- [ ] Keep historical results out of the current docs; store raw artifacts
  separately.

### 6. Release gates

- [x] Normal CI runs benchmark smoke only.
- [x] Manual budget check exists.
- [x] Run the complete comparison on kbench before a performance claim or a 1.0
  release.
- [ ] Compare against the previous accepted baseline and require an explanation
  for a 1.5x to 2x regression.
- [ ] Block above 2x until fixed or explicitly accepted.

## Current reference run

The current reference run uses the simple 128-entry and 65,536-entry
fixtures, the mixed fixture, CPU time, callgrind instructions, RSS and
parse-only binary sizes. The comparison crate remains outside the published
package and the normal workspace.

Remaining work before the benchmark program is complete:

- clean build-time measurements;
- compile-time and feature-size matrix;
- startup, edit, format and compare throughput;
- CPU-cycle fields in the committed result schema;
- previous-baseline comparison and regression owner assignment;
- dedicated dashboard page with the summary and raw JSON links.

## Acceptance for 1.0

- The comparison covers simple, mixed and real-world fixtures.
- All compared parsers use the same input and output definition.
- Wall time, CPU time, instructions, RSS and binary size are recorded.
- Results are reproducible with one command on kbench.
- The docs site publishes the current results and method.
- Performance budgets pass.
- Any remaining gap is documented with a reason.
