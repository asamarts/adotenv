# Benchmark methodology

## Claim boundary

The release claim is about the modes that have a direct competitor API:

- **values parsing:** `adotenv::parse_values` against each competitor's
  in-memory value parser;
- **file loading:** `adotenv::load::load_values` against each competitor's
  value-loading API;
- **memory:** the peak heap, retained heap and allocation count of those
  results.

It does **not** claim superiority for lossless documents, edits, formatting,
sorting, reference inspection or resolution. Those APIs retain exact source
spans, token structure or expansion state that the compared libraries do not
expose; no like-for-like competitor cell exists. Internal workloads are still
covered by the deterministic instruction/cycle gate and the memory gate.

The retained-heap result for 1.4.1 is a documented **no-loss** result, not a
strict win. `parse_values` returns `BTreeMap<String, String>`. At small sizes the
maps are byte-identical to the competitors; at larger sizes the map is already
at the compact-tree floor. A strict retained result requires a different public
values representation. The compact API now provides that representation for a
future minor release.

The compact evaluation uses `parse_values_compact` and `load_values_compact`
with the same fixtures and competitors. The 1.5.0 release run
records strict non-empty retained-heap wins, zero-byte ties on empty inputs,
and strict peak/allocation wins in all values/load cells. See
[RESULTS-1.5.0.md](RESULTS-1.5.0.md). It does not replace the 1.4.1 map-based
claim.

## Competitors

Pinned versions:

| Competitor | Version | Values API | Load API |
|---|---|---:|---:|
| `dotenvy` | 0.15.7 | yes | yes |
| `dotenv` | 0.15.0 | no | yes |
| `dotenvs` | 0.2.2 | yes | yes |
| `dotenv-parser` | 0.1.3 | yes | no |
| `dotenv-flow` | 0.16.2 | no | yes |

`dotenv-ng` 1.0.0 is deferred because its loader/CLI surface does not compare
without semantic ambiguity.

Only the conservative Portable shapes `plain`, `long-line` and `empty` are
used. Quoted/escaped, duplicate, malformed, multiline, CRLF/BOM, references and
realistic mixed inputs are reported as unsupported because the libraries
disagree on their semantics. See [SCENARIOS.md](SCENARIOS.md).

## Fixtures

Fixtures are generated deterministically from a size and shape. Sizes are 8,
128, 1,000, 10,000 and 100,000 declarations. Every memory row records the exact
SHA-256 of the generated source, and the scenario catalogue lists all comparable
fixture hashes. Speed benchmark names include the shape and size; the same
fixture generator and hashes apply.

## Speed

Speed is measured with Criterion in release mode, using the pinned competitor
adapters and output validation before timing. The published command is:

```console
cargo run -p adotenv-xtask -- queue-speed \
  --remote-root /var/lib/benchq/adotenv-bench \
  --out docs/benchmarks/speed.txt --sync
```

The raw file uses Criterion's `bencher` output:

```text
test competitor_values/adotenv/plain/128 ... bench:  ... ns/iter (+/- ...)
```

A median/estimate and its reported spread are the unit of comparison. The
release summary compares the adotenv line with the best competitor line for the
same mode, shape and size. Criterion wall time is publication evidence; it is
not a PR gate.

## Memory

`adotenv-bench`'s `memory` binary measures **one case in a fresh process**. A
counting global allocator records:

- `peak_heap_bytes`: maximum live heap above the pre-operation baseline;
- `retained_heap_bytes`: live heap while the result is held;
- `heap_after_drop_bytes`: live heap after dropping the result;
- `allocations`: allocation calls above the baseline;
- `rss_before_bytes`, `rss_peak_bytes`, `rss_after_bytes`: Linux RSS where
  available.

The operation is the public API under test (values, document, load or resolve).
The counting allocator makes heap and allocation results independent of host
wall-clock noise. The fixture and the empty baseline are measured in the same
process, so retained values are not inflated by fixture source storage that is
still owned by the test driver.

Memory publication runs use the release binary, not a debug build. The
`xtask memory` and `xtask queue` paths build release binaries automatically.

The source-level deterministic memory gate compares a fresh run with
`docs/benchmarks/memory.jsonl`:

- allocations must match exactly;
- peak and retained heap may grow by at most 5% or 64 bytes;
- a changed fixture SHA-256 fails the gate.

The baseline is updated only for an intentional, reviewed code change with a
fresh fingerprint. Do not update it merely to make a failing gate pass. CI runs
the same gate against `compact-memory.jsonl` with
`--implementation adotenv-compact`; empty compact cells are zero-byte ties and
allocation counts must still match exactly.

## Deterministic internal gate

`adotenv-bench/benches/det_parse.rs` runs four map-parsing cells, four compact
parsing cells and two compact-loading cells under Valgrind Callgrind through
gungraun. The gate compares a freshly built merge
base with the PR in the same run and fails on `Ir` above +2% or estimated
cycles above +5%. Branch counters are diagnostic only. This is the load-immune
CI gate; Criterion is not used for PR pass/fail.

`bench-smoke` exercises the deterministic harness and the memory gate on every
PR. `det-perf` runs the base/head instruction/cycle comparison.

## Host and provenance

Publication runs use `kbench`, the canonical benchmark host. The shared pueue
daemon is configured by `/etc/benchq/pueue.yml`; the group is `bench` with one
parallel slot. Work is serialized and no host-wide CPU, swap or other projects'
queue/CI settings are changed.

Every result directory includes a fingerprint sidecar with:

- OS and architecture;
- CPU model and CPU count;
- kernel;
- Rust compiler version;
- Criterion/hyperfine version where relevant;
- the `adotenv` git SHA used for the benchmark binary;
- one-minute load average at the run.

Publication checks the fingerprint matches the intended host and records the
load. A run on a noisy or different host is not comparable and must not be used
for a release claim.

## Variance policy

- Criterion reports an estimate and spread; do not compare single samples.
- Very small cells are dominated by fixed setup costs. They remain in the
  matrix and are reported as wins only when the measured ordering is strict.
- Memory heap and allocation counts are deterministic on the reference Linux
  host. RSS is advisory and may vary with the allocator and page cache.
- A retained-memory tie is recorded as a tie. It is not converted to a win by
  dropping the cell or changing the metric.
- Load average is recorded, not used to silently filter a run.
