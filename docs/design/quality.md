# Quality, testing and compatibility

Status: 0.2 quality harnesses are implemented. Unit/property and documentation
tests, Criterion benchmarks, public synthetic parser/edit fuzz seeds, and CI
definitions are part of the repository. This document distinguishes runnable
checks from recorded execution evidence. Dialect conformance and resolution
workloads are future 0.3/0.4 work.

## Permanent test expectations

Tests assert consumer-visible behavior, not implementation wiring or exact
message wording:

- Lossless raw items plus the optional BOM partition arbitrary UTF-8 source,
  including malformed statements, without gaps or overlaps.
- Spans are bounded, UTF-8 aligned and contained in the appropriate item.
- Portable syntax covers bare/empty/value states, duplicates, literal
  whitespace-sensitive hashes, known/unknown escapes, multiline grouping and
  unmatched-quote recovery. Diagnostics match code, severity and spans.
- `encode_value` returns a standalone token that reads back exactly when
  representable, without claiming contextual safety by itself.
- Verified edits produce the intended semantic change and leave unrelated
  entries, invalid items and diagnostic identities unchanged, allowing
  position shifts. Tests include adversarial regrouping of quotes and BOMs,
  explicit duplicate occurrences, whole-batch edits and selected comments.
- Position conversions round-trip wherever representable and reject invalid
  coordinates: interior UTF-8 bytes in scalar/UTF-16 conversions, split
  surrogate pairs, CRLF interiors, columns beyond content and out-of-range
  offsets. UTF-8 columns retain interior byte offsets. Empty input, Unicode,
  EOF, terminal line breaks, lone CR and BOM coordinates are included.
- README examples compile and run as documentation tests.

## Fuzz harnesses

The excluded `fuzz/` package has its own workspace and uses `cargo-fuzz` with
`libfuzzer-sys`. It calls only public core APIs; byte inputs are validated as
UTF-8 before parsing rather than decoded lossily.

- `parser`: exact source/print round trip, contiguous item coverage, bounded
  token and diagnostic spans, lexical token roles and complete trivia layout,
  borrowed/owned and strict/recovering agreement. An independent streaming
  decoder checks cooked values and exact sampled raw mappings, including both
  endpoints and invalid UTF-8 interiors. Position checks require success or
  the specific error according to the coordinate's representability.
- `edits`: native set/remove/rename/insert/comment/uncomment/export and batches,
  valid and invalid selectors, arbitrary values/keys and unrelated syntax
  errors. Successful results are independently replayed in the returned order;
  permitted raw ranges and required whole-item/marker replacements are derived
  from the requested operations. Ordered meanings include exact insertion
  positions. Unselected raw items, classifications and invalid diagnostic
  identities must survive. Multiline restoration also crosses explicitly
  removed comments. An independent conservative positive-case classifier also
  requires success for ordinary supported edits, standalone valid comment
  restoration and generated conflict-free batches. Malformed source and
  ambiguous preservation contexts are excluded from that positive domain;
  refusals remain allowed there. Every successful edit must satisfy the
  preservation and meaning postconditions.

Checked-in seeds are newly authored, public synthetic examples, with no real
credentials or private compatibility results. The edit input format is an
operation byte, selector byte, UTF-8 source, a NUL delimiter and UTF-8 value/key.
Its operation byte modulo eleven selects set, remove, rename, insert, comment,
uncomment, export, set/rename, comment restoration, restoration with a selected
comment removed, or EOF removal/insertion. Corpus mutation can generate
arbitrary invalid syntax. Fuzz output directories
and the separate fuzz lockfile are ignored; root package publishing excludes
this harness.

Scheduled weekly and manually dispatched CI jobs use pinned action revisions,
a fixed cargo-fuzz version, nightly sanitizers, a five-minute engine limit per
target, ten-second individual-input timeout, 2 GiB RSS limit, 64 KiB maximum
input, and a fifteen-minute job deadline. These bounds apply to that campaign,
not to public parser input sizes or a claim of sustained fuzz cleanliness.
A failed campaign reports its reproducer path and preserves available crash
reproducers as a CI artifact for fourteen days. Save and minimize the
reproducer before that expires.

The runner is pinned to cargo-fuzz 0.13.2. The initial actual GitHub campaign
exposed that 0.12.0's locked `rustix` 0.36.5 fails to compile on current nightly
because of obsolete compiler-internal attributes. Updating the runner, rather
than suppressing that failure or disabling fuzzing, keeps the gate executable.
Crash uploads use the Node 24 artifact action.

Run from the repository root after installing nightly and cargo-fuzz:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz --version 0.13.2 --locked
cargo +nightly fuzz run parser -- -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=65536
cargo +nightly fuzz run edits -- -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=65536
```

For an individual crash, use `cargo +nightly fuzz run <target> <reproducer>`.
Record the toolchain, runner version, target, limits, elapsed duration and
outcome when reporting an actual run. Do not label a bounded smoke campaign
as sustained fuzz coverage.

## Performance

`benches/core.rs` provides Criterion workloads for small (8 declarations),
typical (128) and large (65,536; multi-megabyte) synthetic files. Fixtures mix
literal and exported entries, comments, Unicode, escaped double quotes and
physical multiline values. Fixture generation and initial edit-document
parsing are outside timed loops:

- `parse/<size>` measures recovering parsing, including diagnostics and model
  construction/destruction.
- `native_edits/set/<size>` measures one verified native set operation.
- `native_edits/batch/<size>` measures a verified set/rename/export batch on
  distinct entries from one source snapshot.

Native edit costs include output generation and whole-result verification;
they are not presented as isolated range-splice costs. Throughput counts
source bytes, and Criterion reports timings and byte throughput. Large inputs
make scaling visible; no regression threshold is asserted without a recorded
baseline on a controlled machine. Resolution and multi-dialect benchmarking
remain future work.

```sh
cargo bench -p adotenv --bench core --locked -- --test  # smoke every workload
cargo bench -p adotenv --bench core --locked           # measurements
```

CI runs the smoke mode, not a performance comparison. For published results,
record the commit, `rustc -Vv`, target/CPU/OS, feature set, exact command and
Criterion's estimates/intervals. Results belong here or in release notes only
after execution; there are no invented performance numbers or fuzz durations.

## Platform, CI and safety

The root workspace contains `adotenv` at `.`. Core has no runtime dependencies;
Criterion and proptest are dev-only, and libfuzzer-sys is isolated in the fuzz
workspace. `std` is default-on but adds no I/O or environment/process behavior.
`--no-default-features` uses `no_std` with `alloc`; callers still supply their
own allocator on targets that require one.

`#![forbid(unsafe_code)]` is unconditional in the core. No core feature in 0.2
loads files, changes environment variables, executes commands or expands
values. Loading/expansion/command helpers remain later opt-in work. A future
process adapter, not core, will own any unsafe environment application API.

CI covers stable tests on Linux/macOS/Windows, README doctests, formatting,
Clippy and documentation warnings, packaging the explicit `adotenv` package,
benchmark smoke, Rust 1.81 library builds with and without defaults, and an
alloc-only build for `thumbv7em-none-eabi` where `std` is unavailable. MSRV jobs
build only the library because dev/fuzz dependencies can require newer Rust.
Release metadata and publishing select the package by name, not workspace
package order. Tag publication depends on the complete reusable CI workflow at
the same commit, including all platforms, MSRV, embedded, package and benchmark
gates. A manually dispatched Release runs only those checks and cannot publish.
Ordinary CI and the release's called CI have distinct concurrency groups so one
cannot cancel the other.

Representative local checks (after updating the root lockfile):

```sh
cargo fmt --all --check
cargo fmt --manifest-path fuzz/Cargo.toml --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p adotenv --lib --no-default-features --locked -- -D warnings
PROPTEST_CASES=2000 cargo test --workspace --locked
cargo test -p adotenv --no-default-features --locked
RUSTDOCFLAGS="-D warnings" cargo doc -p adotenv --no-deps --locked
cargo +1.81 build -p adotenv --lib --locked
cargo +1.81 build -p adotenv --lib --no-default-features --locked
rustup target add thumbv7em-none-eabi
cargo build -p adotenv --lib --no-default-features --target thumbv7em-none-eabi --locked
cargo package -p adotenv --locked
```

## Local execution evidence (2026-10-15)

Tested source: the unreleased 0.2 working tree based on
`d0cf81b9fc346c03c7665c39aa56471fa990a685`, not that unchanged commit.
Host: Linux 7.2.8-arch1-2, x86_64-unknown-linux-gnu, AMD Ryzen 9 3900X.
Stable compiler: rustc 1.99.0 (`b940084d7`, 2026-09-28), LLVM 23.1.1.

Executed successfully:

- 47 unit/property tests and four doctests with default features, using 2,000
  generated cases per property; all 51 also pass without default features.
- Strict all-target Clippy and alloc-only library Clippy; rustdoc with warnings
  denied; Rust 1.81.0 library builds with and without defaults.
- An alloc-only `thumbv7em-none-eabi` build, verified package construction, and
  all nine Criterion workloads in CI smoke mode.
- An external consumer built with `default-features = false`: 18,986 ordered
  operation pairs over recovering/BOM/mixed-line-ending/multiline fixtures,
  plus 41,371 exhaustive short Unicode/grammar inputs checked against an
  independent position/physical-line oracle. Native ranges were independently
  replayed and ordered entry meanings checked. Temporary consumers are not
  part of the package.
- GitHub workflow validation with actionlint 1.7.7.
- cargo-audit 0.22.2 found no advisory matches in the root lockfile (59 crate
  dependencies) or generated fuzz lockfile (12), against 1,329 loaded RustSec
  advisories. This is a dated dependency check, not a guarantee against unknown
  vulnerabilities.

Measured with default features and Criterion 0.5.1:

```sh
cargo +stable bench -p adotenv --bench core --locked -- --sample-size 10 --measurement-time 1 --warm-up-time 1 --noplot
```

Reported time estimate and interval:

| Workload | Small: 8 entries | Typical: 128 entries | Large: 65,536 entries |
|---|---|---|---|
| Parse | 2.206 µs [2.190–2.233] | 32.036 µs [31.829–32.489] | 43.321 ms [42.881–43.901] |
| Verified set | 3.300 µs [3.264–3.367] | 38.604 µs [38.039–39.073] | 56.849 ms [56.321–57.407] |
| Verified batch | 3.392 µs [3.378–3.412] | 37.974 µs [37.754–38.150] | 58.369 ms [57.928–58.848] |

These are short-run measurements on a shared host during other validation,
with reported outliers, not a controlled regression baseline. They include
model allocation/destruction and complete native-edit verification. No local
macOS/Windows execution is claimed; those platforms are configured in CI.

### Adversarial audit and scaling checks

Regression cases first failed, then passed for empty EOF uncommenting, multiline
restoration across explicitly removed comments, selected versus unrelated BOM
exposure, and insertion separators/final newlines against the surviving output.
The latter includes a selected comment becoming a BOM-only prelude.

Separate release-mode consumers exercised whole-batch renaming and interleaved
sorted insertions at 1,000, 2,000 and 4,000 original entries, checking the complete
ordered result. Original keys were `K{2*n:08}=x`; rename targets were
`R{n:08}` and inserted keys `K{2*n+1:08}` with value `new`. A mapping consumer
queried every decoded boundary across 4,000, 8,000 and 16,000 `\n` escapes,
asserting the exact original-source offset of each boundary.

Single-run elapsed measurements on the same shared host, with fixture creation
and initial parsing excluded:

| Operation | Size | Before audit fixes | After audit fixes |
|---|---:|---:|---:|
| Whole-batch rename | 4,000 entries | 161.469 ms | 3.326 ms |
| Sorted insertion batch | 4,000 insertions | 42.867 ms | 5.658 ms |
| Every decoded boundary | 16,000 escapes | 186.755 ms | 0.451 ms |

The fixes replace repeated original-key scans, pairwise collision checks,
front-of-vector insertion shifts and escape-segment rescans. These observations
are scaling smoke evidence, not statistically controlled regression thresholds.

### Bounded sanitizer campaigns

Local runner: cargo-fuzz 0.13.2, rustc 1.100.0-nightly (`67eda617e`,
2026-09-10), x86_64-unknown-linux-gnu, default features and AddressSanitizer.
The parser and native-edit campaigns completed normally with no crash, panic or
invariant failure. A fixed-count smoke additionally exercised the finalized
range/order oracle and in-place native insertion buffer:

| Target | Executions | Engine time |
|---|---:|---:|
| Parser | 1,805,672 | 301 s |
| Native edits, final EOF/BOM behavior | 1,698,495 | 301 s |
| Native edits, final oracle/buffer smoke | 200,000 | 19 s |

The timed campaigns used `-max_total_time=300`; the fixed-count smoke used
`-runs=200000`. Both used `-timeout=10 -rss_limit_mb=2048 -max_len=65536`.
Custom temporary directories received generated corpus and
crash output; checked-in synthetic seeds were additional read-only inputs.
The equivalent command shape is:

```sh
cargo +nightly fuzz run parser --target x86_64-unknown-linux-gnu /tmp/generated-parser-corpus fuzz/corpus/parser -- -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=65536 -artifact_prefix=/tmp/parser-artifacts/
cargo +nightly fuzz run edits --target x86_64-unknown-linux-gnu /tmp/generated-edit-corpus fuzz/corpus/edits -- -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=65536 -artifact_prefix=/tmp/edit-artifacts/
```

Create the temporary directories before running. These bounded local
campaigns are not a claim of sustained fuzz cleanliness or cross-dialect
conformance. Invalid UTF-8 inputs are skipped, not parsed lossily.

### Post-publication adversarial audit (2026-10-15)

Baseline: published 0.2.0 and release commit
`046b26ac7636b2c584ef278c9125b0235c78711f`. This pass found fuzz-oracle,
seed-dispatch and release-gating gaps, not a reproduced core defect; parser
and edit semantics are unchanged.

- Both the local core and the exact crates.io `=0.2.0` package passed an
  independent alloc-only consumer: 719,815 conflict-free two/four-operation
  batches over thirteen BOM, mixed-ending, multiline, trivia and EOF fixtures.
- Both passed all 1,112,064 Unicode scalar values for literal parsing, raw
  mappings, value/neighbor edits and independently computed all-byte
  UTF-8/scalar/UTF-16 positions.
- All 47 unit/property tests and four doctests passed in both feature modes
  with `PROPTEST_CASES=4000`; strict Clippy, rustdoc, both Rust 1.81 library
  builds, the embedded alloc-only build and all nine benchmark smoke workloads
  also passed. Root/fuzz advisory checks found no matches against 1,329 loaded
  advisories. Workflow syntax passed actionlint 1.7.7.
- Four temporary semantic mutation controls were accepted by the old fuzz
  oracles and rejected by the strengthened ones: blanket edit refusal,
  blanket position-conversion refusal, constant raw mappings and an invalid
  indent token covering the entire declaration. Their unmodified controls
  passed. Another eighteen positive fixtures exercised all eleven edit modes,
  all insertion anchors, no-op renaming, export removal and EOF/BOM restoration.

The strengthened harnesses also completed fresh AddressSanitizer campaigns with
the runner, toolchain, target and limits recorded above, using isolated generated
corpora and the checked-in seeds as read-only inputs. Neither campaign failed:

The final assignment-token completeness smoke used `-runs=200000` and additional
bare/empty/quoted declarations and trivia fixtures.

| Target | Executions | Engine time |
|---|---:|---:|
| Parser, strengthened oracle | 2,276,195 | 301 s |
| Native edits, positive-case oracle and corrected seeds | 3,706,810 | 301 s |
| Parser, final assignment-token completeness smoke | 200,000 | 14 s |

Temporary consumers and mutation controls are not shipped as permanent tests.
This evidence does not establish sustained fuzz cleanliness or conformance to
other dotenv implementations.

## Public upstream conformance corpus (0.3 preparation)

[`conformance/`](../../conformance/manifest.json) contains fresh synthetic UTF-8
inputs, executable upstream adapters, integrity locks, API/profile metadata and
recorded observations. It currently covers **67 cases across 20 profiles
(1,340 executed observations)**. It is an oracle for future dialect work, not a
claim that Portable 0.2 already matches those implementations.

| Implementation | Exact pin | Observation surfaces |
|---|---|---|
| Node `dotenv` | 18.0.8, npm SHA-512 lock | Default regex and opt-in fast parsing, string/buffer inputs, controlled loading with both override policies |
| `python-dotenv` | 1.2.4, wheel SHA-256 | Ordered bindings, literal stream/file values, resolution, literal/resolved loading with both override policies |
| Docker Compose | 5.6.0's compose-go 2.16.1 and Docker CLI 29.8.2 pins | Parser/resolution, project `.env`, actual service `env_file` default/raw loading |
| Docker CLI | 29.8.2, Go module checksums | Ordered raw env-file output, with no lookup or a controlled client lookup |

Reproduce on Linux with Node, Python (venv/pip) and Go installed:

```sh
python3 conformance/run.py --setup
# Only after reviewing intentional input, API or dependency changes:
python3 conformance/run.py --record
```

`--setup` installs locked dependencies with npm scripts disabled and forcibly
reinstalls the hash-checked Python wheel. Go modules are verified before every
build; local workspaces, overlays, build flags and persisted Go configuration
cannot alter the oracle. Normal execution compares every profile/case and
never updates goldens. Explicit recording first re-establishes those locked
dependencies, then executes the real APIs. Input, profile, manifest and
dependency-lock hashes bind its output. Required runtime/time/platform
provenance is validated but not compared to the current runtime. JSON types,
duplicate keys and nonstandard numeric constants are checked strictly.
The reusable CI matrix runs this checker, including before a tagged release.

Every API call uses an isolated controlled environment and explicit synthetic
input. No ambient lookup, file discovery, Docker daemon, container image,
shell evaluation or private corpus is used. Core Cargo dependencies and MSRV
are unchanged; the oracle harness is excluded from the published crate.
Literal parsing, resolution and loading have
separate profiles; Compose default parsing cannot disable interpolation and is
explicitly labeled resolution. `$`-free cases expose its literal syntax without
claiming a nonexistent interpolation-off API.

Only Python's pinned module-level `parse_stream` exposes ordered bindings and
syntax-error flags/starting lines. Node final maps expose neither declarations
nor syntax diagnostics. Docker raw output preserves ordered duplicates without
claiming a container-runtime winner. Unsupported items, spans and diagnostic
identities are not synthesized; human error prose is not pinned. Python file
versus stream newline behavior and actual loading exceptions/partial mutation
are separate observations.

### Validation evidence (2026-10-16)

The recorded corpus was replayed successfully using the runtimes in
`conformance/expected.json`. All **134** default/raw service observations were
also corroborated against the actual installed **Docker Compose 5.6.0 CLI**,
including error outcomes and explicit YAML-equivalent null overrides. The
black-box comparison removed only the CLI renderer's dollar doubling; no
containers or images were started or fetched. The lean raw registration glue
therefore has independent executable corroboration.

Five initial negative controls were rejected: a wrong value, a missing case,
a wrong service-role interpretation, a missing profile and stale input
provenance. Further controls reproduced then rejected boolean/numeric
conflation and missing execution provenance, and rejected malformed schema
types, manifest/profile data, duplicate keys and non-JSON constants. Hostile
Go workspace/overlay/toolchain/configuration variables did not alter a full
setup/check. A modified same-version Python module failed execution, and
`--setup` restored the hash-pinned wheel and matching observations. These
throwaway controls are not shipped as permanent tests.

The pinned Compose service loaders inherit a bare key from project lookup when
available and retain a preceding value when a bare duplicate has no lookup.
This conflicts with the service-file reference's bare-unset wording; see
[dialects.md](dialects.md). Explicit YAML `environment:` null remains a distinct
override. The corpus records the observed implementation instead of guessing
from either documentation or a generic parser helper.

Scope limits: UTF-8 inputs and Linux environment behavior; no invalid-byte
decoder oracle, multi-file cascade, daemon/container semantics or sustained-fuzz
claim. Dialect parsing/encoding/portability APIs still need implementation
against these observations. Public fuzz seeds remain separate from conformance.

## Strict and native verification resource fix (0.2.1, 2026-10-16)

The alint migration audit reproduced two allocation failures against 0.2.0
under a **512 MiB child-process address-space limit** on Linux: a single
16 MiB warning-dense line, and 4,000,000 bytes across 100,000 lone-CR-ended
warning-dense declarations. The LF equivalent of the latter completed.
`parse_strict` collected warning diagnostics it discarded, and lone-CR
merging allocated a second full diagnostic vector. Native edit verification
also requested diagnostic lists it did not use.

Strict parsing and internal native verification now share the same grammar
with diagnostic-list collection disabled. Duplicate-warning tries and
warning-only scans are omitted; malformed items still retain their error
records. Public `parse` and `parse_owned` retain all diagnostics.

The release-mode public-API smoke passed the eight synthetic strict cases
covering the original failures, 4 MiB warning density, 100,000 entries,
LF/CR combinations, long keys and escape maps under the same address-space
limit. Verified native set, free-function set and remove also passed on
3,999,971 bytes / 100,000 CR-ended declarations with a unique target and
unrelated duplicate warning-heavy keys. Exact output comparisons checked
that unrelated source survived unchanged.

Permanent warning-heavy BOM/multiline/duplicate tests cover every supported
line ending and first-error precedence. The arbitrary-UTF-8 property compares
the entire strict document or first error against recovering parsing; both
feature modes passed with 2,000 property cases. This is a bounded Linux
resource smoke, not a general memory ceiling or sustained-fuzz claim. Callers
still need byte and line/declaration budgets for retained lossless data.

## Supported Rust versions and semver

- MSRV is **1.81** for 0.2 (1.74 for 0.1), edition 2021. Raises happen only in
  minor releases and stay at least six months behind the latest stable Rust.
- Before 1.0, breaking changes belong in minor releases with explicit migration
  guidance in the changelog and README. 0.2 includes both Portable syntax and
  API changes; there is no hidden 0.1 compatibility grammar.
- After 1.0, the API follows semver. Stable diagnostic identities do not depend
  on enum order or message wording. Non-exhaustive enums allow new variants;
  callers retain a fallback arm.
- Future workspace crates will be versioned and released independently.
