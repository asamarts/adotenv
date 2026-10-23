# Quality, testing and compatibility

Status: lossless-core quality harnesses through 0.7 are implemented:
unit/property/documentation tests, public synthetic fuzz seeds, Criterion
workloads, real pinned upstream replay and a public-API Rust probe.
This document distinguishes runnable checks from recorded execution evidence.
Standalone dialect encoding, same-line native batch budgets, cross-dialect
portability, literal reference inspection, opt-in variable resolution,
explicit command callbacks, the opt-in `exec` helper, file loading, serde,
the process adapter, CLI workflows and the config/Figment adapters are
covered.

## Permanent test expectations

Tests assert consumer-visible behavior, not implementation wiring or exact
message wording:

- Lossless raw items plus the optional BOM partition arbitrary UTF-8 source,
  including malformed statements, without gaps or overlaps.
- Spans are bounded, UTF-8 aligned and contained in the appropriate item.
- Portable syntax covers bare/empty/value states, duplicates, literal
  whitespace-sensitive hashes, known/unknown escapes, multiline grouping and
  unmatched-quote recovery. Diagnostics match code, severity and spans.
- `encode_value_with` must decode exactly under the selected configuration at
  LF and EOF. Known legal tokens are positive representability witnesses: an
  encoder refusing every value must fail. The legacy helper keeps its policy;
  neither API claims arbitrary contextual or interpolation safety.
- Verified edits produce the intended semantic change and leave unrelated
  entries, invalid items and diagnostic identities unchanged, allowing
  position shifts. Tests include adversarial regrouping of quotes and BOMs,
  explicit duplicate occurrences, whole-batch edits and selected comments.
- Position conversions round-trip wherever representable and reject invalid
  coordinates: interior UTF-8 bytes in scalar/UTF-16 conversions, split
  surrogate pairs, CRLF interiors, columns beyond content and out-of-range
  offsets. UTF-8 columns retain interior byte offsets. Empty input, Unicode,
  EOF, terminal line breaks, lone CR and BOM coordinates are included.
- Portability reports include the document's own configuration first, parse each
  unique dialect once, report semantic and grouping differences per physical
  line with valid UTF-8 spans, and are independent of request order. Lookup
  participation, effective bare values and duplicate winners are included;
  quote style alone is not a difference.
- Reference inspection reports variable and command candidates in source order
  with bounded UTF-8 spans, name/operator/argument/payload spans, quote and
  escape context, nesting containers and malformed-form metadata. Eligibility
  decisions are pure and account for containing forms.
- Variable resolution honors sequential/whole-file scope, caller/file
  precedence, all operators, unresolved policy and limits. Every successful
  segment concatenates to its entry value and carries bounded UTF-8 spans,
  direct file/lookup origins and nested file provenance. Required variables,
  cycles and exceeded limits are structured errors; caller values remain literal.
- Command callbacks are invoked only when rules enable commands and a callback
  is supplied. Raw command text and spans, source identity, literal output,
  call provenance and retained prior calls on failure are checked; no default
  execution path exists.
- The `exec` feature helper is constructed only with an explicit directory or
  source-directory origins. Tests check one-newline trimming, source directory
  selection, nonzero status and invalid-directory errors on Unix.
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
  The independent Portable oracle remains intact. All five presets and custom
  configurations additionally check source/token partitioning, decoded-map
  boundaries, strict/owned agreement, policy errors and scanner limits.
  Portability reports check valid ordered spans, physical-line bounds, reading
  order and determinism under reordered requests. Reference candidates check
  bounded UTF-8 spans, source ordering, operator/argument consistency, nesting
  containers and eligibility calls under every preset rule set.
- `format`: all five presets, structure views and four formatting policies
  (`normalize`, spacing-only, remove-and-trim, and spacing/trim/remove). Groups
  must partition nonblank items in order; headers and leading comments must be
  contiguous. Every successful formatting result must replay its sorted,
  nonoverlapping source edits, reparse to identical declarations and item
  structure, preserve malformed statement content and preserve comment text
  after trailing whitespace. Normal policies must be idempotent; final-newline
  removal is exempt because it removes one terminator at a time.
- `resolve`: variable resolution under four rule sets, both scopes, both lookup
  orders and keep/error unresolved policies. Successful outputs check segment
  concatenation, byte limits, source-bounded literal/reference/origin spans and
  nested provenance; errors may be any structured `ResolveError`. It also
  exercises the explicit command callback under both scopes and validates call
  spans, source identity and retained failure calls.
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
  Preset/custom set, remove, rename and export paths also check ordered meanings,
  authorized token ranges and unselected raw/diagnostic preservation. Preset
  comment/restoration checks preserve LF-only CR data and require ordinary
  Docker comments to succeed below the scanner cap.
- `encoding`: arbitrary UTF-8 must encode under Portable/Python/Compose; every
  successful preset/custom token must decode exactly at LF and EOF. Legal raw,
  single/double/backtick and appended-tail tokens independently require encoding
  of their cooked values. Configurations vary escape flags, unknown escapes,
  single escapes, tails, multiline forms, normalization, comments and byte caps.
  A five-byte policy header selects the grammar; payload bytes must be valid
  UTF-8, are bounded to 512 bytes independently of the engine's input limit, and
  are not lossily decoded. Legacy helper controls remain a separate compatibility
  assertion.

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
cargo +nightly fuzz run --target x86_64-unknown-linux-gnu parser -- -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=65536
cargo +nightly fuzz run --target x86_64-unknown-linux-gnu edits -- -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=65536
cargo +nightly fuzz run --target x86_64-unknown-linux-gnu encoding -- -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=65536
cargo +nightly fuzz run --target x86_64-unknown-linux-gnu resolve -- -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=65536
cargo +nightly fuzz run --target x86_64-unknown-linux-gnu format -- -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=65536
```

For an individual crash, use `cargo +nightly fuzz run <target> <reproducer>`.
Record the toolchain, runner version, target, limits, elapsed duration and
outcome when reporting an actual run. Do not label a bounded smoke campaign
as sustained fuzz coverage.

### 2026-10-22 longer campaign

A local ten-minute-per-target campaign ran on Linux x86_64 with nightly rustc
1.100.0 (`67eda617e`, 2026-09-10), cargo-fuzz 0.13.2, explicit
`--target x86_64-unknown-linux-gnu`, `-max_total_time=600 -timeout=10
-rss_limit_mb=2048 -max_len=65536`. All five targets exited 0; no crash or
sanitizer failure was observed. Peak RSS is the maximum reported by libFuzzer.

| Target | Executions | Engine seconds | Peak RSS (MiB) |
| --- | ---: | ---: | ---: |
| Parser | 302,729 | 601 | 529 |
| Native edits | 330,492 | 601 | 532 |
| Encoding | 301,552 | 601 | 501 |
| Resolve | 416,616 | 601 | 491 |
| Format | 98,425 | 601 | 544 |

Total: 1,449,814 executions. This is longer than the weekly five-minute CI
campaign, but it remains bounded evidence rather than a multi-week sustained
cleanliness claim. The updated explicit-GNU-target workflow was manually
dispatched as run
[39731568403](https://github.com/asamarts/adotenv/actions/runs/39731568403);
all five targets passed their 300-second jobs. The 1.0 sustained bar is four
consecutive scheduled weekly runs for all five targets after the last
fuzz-relevant change; manual dispatches do not count toward that period.

## Performance

`benches/core.rs` provides Criterion workloads for small (8 declarations),
typical (128) and large (65,536; multi-megabyte) synthetic files. Fixtures mix
literal and exported entries, comments, Unicode, escaped double quotes and
physical multiline values. Fixture generation and initial edit-document
parsing are outside timed loops:

- `parse/<size>` measures recovering parsing, including diagnostics and model
  construction/destruction.
- `encode_value_with/<dialect>/<size>` measures exact standalone encoding for
  all five presets at 32, 4,096 and 32,768-byte values.
- `native_edits/set/<size>` measures one verified native set operation.
- `native_edits/batch/<size>` measures a verified set/rename/export batch on
  distinct entries from one source snapshot.
- `native_edits_scaling/{set_all,set_rename_all}/<count>` measures whole-batch
  edits at 1,024, 4,096 and 16,384 entries.
- `portability/<size>` measures a report against the Node, Python, Compose and
  Docker presets, including the document's own Portable reading.
- `references/<size>` measures reference inspection across every entry in the
  parse fixture.
- `resolve/<size>` measures portable variable resolution with a caller lookup
  across every entry.

Native edit costs include output generation and whole-result verification;
they are not presented as isolated range-splice costs. Throughput counts
source bytes, and Criterion reports timings and byte throughput. Large inputs
make scaling visible; no regression threshold is asserted without a recorded
baseline on a controlled machine. Parsing fixtures use the Portable default;
portability and resolution have separate deterministic workloads.

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

`#![forbid(unsafe_code)]` is unconditional in the core. The `exec` and `load`
features are opt-in; there is no implicit file, environment, process or command
access. The separate process companion, not core, owns the unsafe environment
application API.

CI covers stable tests on Linux/macOS/Windows, README doctests, formatting,
Clippy and documentation warnings, packaging the root and companion packages,
benchmark smoke, Rust 1.81 library builds with and without defaults and with
the `exec`, `load` and alloc-only `serde` configurations, feature tests on all
platforms, and an alloc-only build for `thumbv7em-none-eabi` where `std` is
unavailable. MSRV jobs build the library configurations and the process, CLI
and integration companions; their dev/fuzz dependencies can still require
newer Rust.
Release metadata and publishing select packages by name in explicit dependency
order: core, then process/config/Figment, then CLI. The tag workflow waits for
each version in the crates.io sparse index before publishing dependents. Tag
publication depends on the complete reusable CI workflow at the same commit,
including all platforms, MSRV, embedded, package and benchmark gates. A manually
dispatched Release runs only those checks and cannot publish. Ordinary CI and
the release's called CI have distinct concurrency groups so one cannot cancel
the other.

Representative local checks (after updating the root lockfile):

```sh
cargo fmt --all --check
cargo fmt --manifest-path fuzz/Cargo.toml --all --check
cargo fmt --manifest-path conformance/rust/Cargo.toml --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p adotenv --features exec --all-targets --locked -- -D warnings
cargo clippy -p adotenv --lib --no-default-features --locked -- -D warnings
cargo clippy --manifest-path conformance/rust/Cargo.toml --all-targets --locked -- -D warnings
PROPTEST_CASES=2000 cargo test --workspace --locked
cargo test -p adotenv --features exec --locked
cargo test -p adotenv --no-default-features --locked
RUSTDOCFLAGS="-D warnings" cargo doc -p adotenv --no-deps --locked
cargo +1.81 build -p adotenv --lib --locked
cargo +1.81 build -p adotenv --lib --no-default-features --locked
rustup target add thumbv7em-none-eabi
cargo build -p adotenv --lib --no-default-features --target thumbv7em-none-eabi --locked
python3 conformance/check_adotenv.py --setup
cargo package -p adotenv --locked
```

## 0.3 dialect execution evidence (pre-release)

Tested source: the 0.3 implementation working tree based on
`3cae7045fc6f1905dbb5c25eb31119f36844ff5d`, not that unchanged commit.
Host/compiler: Linux x86_64, stable rustc 1.99.0 (`b940084d7`, LLVM 23.1.1).
These checks exercise literal syntax only, not expansion or loading.

- All 76 unit/property tests and six doctests pass with and without default
  features, with `PROPTEST_CASES=2000` in both runs. Strict all-target core and
  probe Clippy, alloc-only Clippy, formatting and warning-denied rustdoc pass.
- Rust 1.81.0 library builds pass with and without defaults, along with an
  alloc-only `thumbv7em-none-eabi` build, verified package construction and all
  nine Criterion smoke workloads.
  Smoke execution is not a new performance measurement.
- Real upstream replay matches all 1,420 recorded observations across 71 cases
  and 20 profiles. Selected-preset comparisons check 409 applicable observations
  and 798 fields; 20 fail-fast observations compare only the error flag, not a
  nonexistent recovered upstream map. The remaining profiles/cases are explicitly
  excluded, not counted as matching literal behavior.
- An isolated deterministic differential run (seed 30017) covers 9,719 synthetic
  grammar/Unicode sources: 57,358 applicable observations, 103,772 field
  comparisons and 10,944 fail-fast-error-only observations, all matching.
  Actual runtimes: Node v26.5.0/dotenv 18.0.8, Python 3.14.7/python-dotenv 1.2.4,
  Go 1.27.1-X:nodwarf5/compose-go 2.16.1 and Docker CLI 29.8.2.
- Negative controls reject malformed probe coverage/types, invented non-Docker
  ordered lines, a schema-valid wrong Node value and contradictory replay flags.
- A separate alloc-only public-API consumer runs under a 512 MiB address-space
  limit: all five presets, owned/strict/reparse, native batches, comment
  restoration, export, literal dollars, explicit policy errors, 100,000 ignored
  lines with a native edit, and Docker's 65,535/65,536-byte scanner boundary.
- Reproduced-before/passing-after regressions cover terminal-CR insertion,
  Node regex statement boundaries, Compose next-statement insertion, quoted-key
  rename, ignored tails/diagnostics, malformed octal mapping, unknown-escape
  newline normalization, tight-comment policy and whole-line scanner rejection.
- `cargo-audit` finds no advisory matches in the root, Rust probe or fuzz
  lockfiles against 1,334 loaded RustSec advisories. This is a dated advisory
  snapshot, not a claim that dependencies are vulnerability-free.
- Workflow syntax passes actionlint 1.7.7, built in an isolated temporary directory.

Bounded AddressSanitizer campaigns use cargo-fuzz 0.13.2, nightly rustc 1.100.0
(`67eda617e`, 2026-09-10), explicit `--target x86_64-unknown-linux-gnu`, isolated
temporary mutation corpora, `-runs=100000 -max_len=65536 -rss_limit_mb=2048
-timeout=10`. Both complete without sanitizer or invariant failures:

| Target | Executed inputs | Engine seconds | Peak RSS (MiB) |
| --- | ---: | ---: | ---: |
| Parser | 100,000 | 33 | 464 |
| Native edits, after final separator fixes | 100,000 | 32 | 478 |

The initial host-default musl build failed before fuzz execution because its
C++/sanitizer setup was unavailable. The runbook and scheduled workflow now
pass the GNU target explicitly so local and CI commands do not depend on host
defaults; the explicit GNU target is the actual successful campaign. These are bounded smoke campaigns, not sustained fuzz
coverage. No measured multi-dialect performance baseline is claimed.

### Delivered-dialect adversarial audit

The delivered `45943b0030dac263507e3440b32fcabe66569f6d` increment was
re-exercised before the encoder implementation:

- All 82 unit/property/documentation tests pass in both feature modes with
  `PROPTEST_CASES=2000`; locked real-runtime replay still matches all 1,420
  pinned observations.
- Exhaustive tokens of length zero through four over ten grammar characters
  produce 11,111 synthetic sources. Actual upstream comparison matches 66,666
  applicable observations and 129,288 fields; 4,044 fail-fast observations
  compare the error flag only. Existing role/profile exclusions remain visible.
- Seven reproduced-before/passing-after regressions repair LF-only comment
  restoration, planned-kind separators, prior-item field ordering, final bare
  Compose declarations, removed-statement separator ownership and whole-line
  rejection for plain Node items. The repaired increment passes all 90 tests
  in both feature modes with 2,000 generated cases per property.
- A separate alloc-only executable under a 512 MiB address-space limit exercises
  all five presets, the repaired batches/restoration/cap paths, exact CRLF
  output, literal dollars, a 100,000-ignored-line edit and Docker's scanner edge.
- Isolated ASan campaigns use the same runner, GNU target and limits above:

| Stage | Target | Executed inputs | Engine seconds | Peak RSS (MiB) |
| --- | --- | ---: | ---: | ---: |
| Delivered baseline | Parser | 100,000 | 41 | 470 |
| Delivered baseline | Native edits | 100,000 | 25 | 517 |
| After audit fixes | Parser | 100,000 | 34 | 464 |
| After audit fixes and restoration oracle | Native edits | 100,000 | 37 | 525 |

These are additional bounded campaigns, not sustained fuzz evidence.

### Dialect-encoder adversarial audit (initial pass)

The initial encoder working tree was validated after repairing the sanitizer
findings from that pass:

- All 100 unit/property tests and seven doctests pass in both feature modes,
  with `PROPTEST_CASES=2000`. Root, fuzz and conformance formatting; all-target
  core and conformance Clippy; alloc-only Clippy; warning-denied rustdoc; Rust
  1.81 builds in both modes; the alloc-only embedded target; package construction;
  and all nine Criterion smoke workloads pass.
- Pinned real-runtime replay still matches all 1,420 observations across 71
  cases and 20 profiles, including all 409 applicable literal observations and
  798 compared fields.
- An external public-API matrix checks 11,125 tokens under 238 preset/custom
  configurations. It recognizes 1,902,368 valid token observations, verifies
  1,222,163 unique standalone encodings at LF and EOF, and rechecks 1,206,428
  successful context-aware native sets with both neighboring entries retained.
- A sanitizer-discovered `LiteralFallback` witness exposed a real false-negative:
  some quote-prefixed values need an inline-comment marker to force exact
  unquoted fallback. Its minimized input now passes a permanent positive-inverse
  regression. A native-edit campaign separately exposed an oracle omission:
  Compose may split the selected entry's own CRLF suffix into a blank item.
  The oracle now excludes only blank/comment fragments inside the mapped selected
  region, and the minimized case is a checked-in seed.
- Final isolated AddressSanitizer campaigns use the same GNU target and bounds:

| Target | Executed inputs | Engine seconds | Peak RSS (MiB) |
| --- | ---: | ---: | ---: |
| Parser | 100,000 | 68 | 456 |
| Native edits | 100,000 | 89 | 504 |
| Encoding | 100,000 | 136 | 491 |

These are bounded smoke campaigns from that pass, not sustained fuzz evidence.

### Exact encoder and same-line batch completion (2026-10-17)

Tested source: commit
`348feb9c6b8a6e11b57b9db71898bddf489624da` (the exact encoder, native-batch,
iterator-annotation and test pass); only documentation follows it.
Host/compiler: Linux x86_64, stable rustc 1.99.0 (`b940084d7`, 2026-09-28),
LLVM 23.1.1; fuzz nightly rustc 1.100.0 (`67eda617e`, 2026-09-10), LLVM 23.1.1,
explicit `x86_64-unknown-linux-gnu`.

- All 112 unit/property tests and seven doctests pass in both feature modes
  with `PROPTEST_CASES=2000`. Root, fuzz and conformance formatting; all-target
  core and conformance-probe Clippy; alloc-only Clippy; warning-denied rustdoc;
  Rust 1.81 library builds in both modes; package construction; and all 30
  Criterion smoke workloads pass. The default stable 1.95 toolchain also passes
  the same Clippy gates after the iterator `#[must_use]` annotations.
- An external temporary public-API matrix checks 210 preset/custom
  configurations over 11,111 synthetic tokens of length zero through four over
  ten grammar characters. It recognizes 1,219,718 valid token observations and
  verifies every standalone encoding at LF and EOF, with no failures. The matrix
  exposed a Node-only first-line-cap false negative: when the cap cannot fit an
  opening quote on the header line, Node permits it after a line break. The
  encoder now returns that continuation-line token; a permanent regression
  covers the family.
- Three-set and combined set/rename/export batches on one capped physical line
  converge independently of request order. The five minimized inverse witnesses
  are checked-in seeds; the former crash reproducers execute cleanly. Eight
  older encoding seeds were migrated to the five-byte policy header so their
  original payloads remain intact.
- Pinned real-runtime replay still matches all 1,420 observations across 71
  cases and 20 profiles, including 409 applicable literal observations and 798
  compared fields.
- Final isolated AddressSanitizer campaigns use the same GNU target and bounds:

| Target | Executed inputs | Engine seconds | Peak RSS (MiB) |
| --- | ---: | ---: | ---: |
| Parser | 100,000 | 39 | 470 |
| Native edits | 100,000 | 99 | 499 |
| Encoding | 100,000 | 51 | 488 |

These are bounded final smoke campaigns, not sustained fuzz evidence.

### Cross-dialect portability (2026-10-17)

Tested source: commit
`cdafcc43bfe12d9c7ca9c18c7714c2a3ad893da8` (the portability module, tests,
parser-fuzz invariants, benchmark and documentation pass); only
release-documentation changes follow. Host and toolchains match the encoder
evidence above.

- All 123 unit/property tests and nine doctests pass in both feature modes with
  `PROPTEST_CASES=2000`. Root, fuzz and conformance formatting; all-target core
  and conformance-probe Clippy; alloc-only Clippy; warning-denied rustdoc; Rust
  1.81 builds in both modes; package construction; and all 33 Criterion smoke
  workloads pass.
- Eleven portability tests cover empty/repeated configurations, CRLF spans,
  value/comment differences, statement grouping, multiline continuation,
  BOM policy, bare lookup participation, `BareKeys::Empty`, duplicate winners,
  line ordering and an arbitrary-UTF-8 property. The property checks valid
  UTF-8 spans, one-based line bounds, reading order and request-order
  independence.
- The parser sanitizer target additionally checks portability report spans,
  line ordering, reading order and reordered-request determinism for five
  presets and a custom policy set. Its final bounded campaign executes 100,000
  inputs in 67 engine seconds at 492 MiB peak RSS.
- The encoding target now also constructs leading-newline token variants; its
  final bounded campaign executes 100,000 inputs in 71 engine seconds at 486 MiB
  peak RSS. Native-edit code is unchanged from the preceding campaign
  (100,000 inputs, 99 engine seconds, 499 MiB).
- Pinned real-runtime replay still matches all 1,420 observations across 71
  cases and 20 profiles, including 409 applicable literal observations and 798
  compared fields. Portability remains literal-only: no expansion, lookup,
  inheritance or I/O.

### Literal reference inspection (2026-10-17)

Tested source: the 0.4 reference working tree based on
`fc5a5024b034b2a05399a6913cb5b51b8cfa37cf` (the reference module plus delimiter
and escape hardening), not that unchanged commit. Host and toolchains match the
encoder evidence above.

- All 135 unit/property tests and ten doctests pass in both feature modes with
  `PROPTEST_CASES=2000`. Root, fuzz and conformance formatting; all-target core
  and conformance-probe Clippy; alloc-only Clippy; warning-denied rustdoc; Rust
  1.81 builds in both modes; package construction; and all 36 Criterion smoke
  workloads pass.
- Twelve reference tests cover short/braced variables, every operator form,
  nested defaults, commands and nested commands, mixed delimiter nesting,
  escaped literal blocks, malformed forms, quoted/escaped dollars, Compose
  `$$`, appended tails, bare/empty entries, eligibility and an arbitrary-UTF-8
  property. The property checks bounded UTF-8 spans, source ordering,
  operator/argument consistency, value containment and eligibility calls under
  every preset rule set.
- The parser sanitizer target checks reference candidate spans, ordering,
  container depth, operator/argument consistency and eligibility under every
  preset rule set for all five presets and custom policies. Its final bounded
  campaign executes 100,000 inputs in 139 engine seconds at 489 MiB peak RSS.
- Pinned real-runtime replay still matches all 1,420 observations. Reference
  inspection is literal-only: no lookup, resolution, I/O or command execution.

### Opt-in variable resolution and command callbacks (2026-10-18)

Tested source: commit
`3c3e358285fa3265b1e89f10db092d2ac42d63a1` (the resolution, command-callback
and exec-helper slice, tests, fuzz target, benchmark and documentation pass).
Host and toolchains match the encoder evidence above.

- All 159 default-feature unit/property tests and thirteen doctests pass in both
  feature modes with `PROPTEST_CASES=2000`; the `exec` feature adds three tests
  (162 total). Root, fuzz and conformance formatting; all-target
  core and conformance-probe Clippy; alloc-only Clippy; warning-denied rustdoc;
  Rust 1.81 builds in both modes; package construction; and all 39 Criterion
  smoke workloads pass.
- Twenty-four resolution tests cover sequential and whole-file scope,
  caller/file precedence, every operator, empty versus unset values, nested
  defaults, conditional arguments, required errors, cycles, depth and size
  limits, effective bare values, duplicates and nonparticipating declarations,
  caller-literal values, single-quote/escape/backtick eligibility, Compose
  `$$`, appended tails, command forms and nested file provenance. Command tests
  cover raw callback text/spans, source identity, literal output, conditional
  invocation, callback failures, retained prior calls and output-size limits.
  An arbitrary-UTF-8 property checks segment concatenation, value limits and
  bounded literal spans.
- The `resolve` sanitizer target checks four rule sets, both scopes, both
  lookup orders and keep/error unresolved policies, validating segment
  concatenation, byte limits, source-bounded literal/reference/origin spans and
  nested provenance. It also invokes the explicit command callback under both
  scopes and validates call spans, source identity and retained failures. Its
  final bounded campaign executes 100,000 inputs in 99 engine seconds at 474 MiB
  peak RSS.
- The `exec` feature adds three Unix tests for shell output trimming, source
  directory selection and failure reporting; its test run passes 162 unit tests
  and thirteen doctests. The feature builds on Linux, macOS and Windows
  (Unix-only tests run on Linux/macOS). Default-feature tests remain
  process-free.
- Pinned real-runtime replay still matches all 1,420 observations. Resolution
  uses only the caller lookup and callback: no environment, file or command
  access occurs implicitly. Caller values are literal; commands run only
  through the explicit callback or the explicit opt-in `exec` helper.

### Structure and formatting slice (0.5, released 2026-10-18)

Tested source: the 0.5 release working tree containing the structure views,
verified formatter, tests, fuzz target, benchmarks and documentation in this
commit. Host: Linux 7.2.8-arch1-2, x86_64-unknown-linux-gnu, AMD Ryzen 9 3900X.
Stable compiler: rustc 1.95.0 (`59807616e`, 2026-04-14); MSRV compiler: 1.81.0.
Fuzz nightly: rustc 1.100.0-nightly (`67eda617e`, 2026-09-10).

- With `PROPTEST_CASES=2000`, all 175 default-feature tests pass, the `exec`
  feature adds three tests (178 total), and all 16 doctests pass. The same 175
  unit/property tests and 16 doctests pass with `--no-default-features`.
- Ten formatting tests cover spacing, tabs, hash values, inline comments, raw
  values, multiline bodies, invalid lines, mixed CR boundaries, empty Compose
  keys, final-newline policies and edit replay/idempotence. Six structure tests
  cover blank-line grouping, header ranges, attached comments, multiline
  entries, BOM exclusion and an arbitrary-UTF-8 partitioning property.
- Root, fuzz and conformance formatting; all-target workspace Clippy;
  exec-feature Clippy; alloc-only Clippy; conformance-probe Clippy; and
  warning-denied rustdoc pass. Rust 1.81 builds the library with default
  features, with `exec`, and with no default features. The alloc-only
  `thumbv7em-none-eabi` build and the locked package build pass. The package
  contains 45 files (about 0.67 MiB, 0.16 MiB compressed).
- All 48 Criterion smoke workloads pass, including the six new structure and
  three normalize-format workloads.
- The `format` sanitizer target completes 99,589 inputs in 121 engine seconds
  with no crash, panic or invariant failure. It checks all five presets, four
  policies, group/header/comment attachment, sorted nonoverlapping edits, edit
  replay, declaration and item-structure preservation, comment/malformed
  content preservation and normal-policy idempotence. The initial bounded
  campaigns found and fixed three real formatter gaps: empty Compose keys
  accumulating indentation, empty-assignment whitespace becoming continuation
  indentation, and mixed CR/LF trimming merging physical lines. These were
  fixed with conservative skips and strengthened structural verification rather
  than suppressing the assertions.
- The recorded conformance snapshot was compared with
  `python3 conformance/check_adotenv.py --skip-upstream-replay`: 409 applicable
  observations and 798 field comparisons match across all five presets, with
  1,011 resolution/loading/fast-grammar exclusions and 20 fail-fast error-only
  cases. Upstream replay was not run for this slice; this is snapshot
  comparison, not a fresh upstream campaign.
- The parser, dialect, resolution, edit and encoding behavior is unchanged.
  Formatting is literal-only: no lookup, environment access, file I/O or
  command execution.

### Comparison and example slice (0.5, released 2026-10-18)

Tested source: the 0.5 release working tree containing effective-key
comparison, safe example generation and their tests, fuzz checks, benchmark and
documentation. Host and toolchains match the structure/formatting slice above.

- With `PROPTEST_CASES=2000`, all 194 default-feature tests pass, the `exec`
  feature adds three tests (197 total), and all 20 doctests pass. The same 194
  unit/property tests and 20 doctests pass with `--no-default-features`.
- Six comparison tests cover value/state/export/missing differences, empty
  versus absent, duplicate policy, nonparticipating keys, stable ordering and an
  arbitrary-UTF-8 symmetry property. Thirteen example tests cover comments and
  layout, multiline values, raw values, BOM, Compose empty keys, Compose
  inline/trailing comments, Python adjacent comments, Node colon values,
  custom Space comments, malformed-value redaction, bare/empty states and an
  arbitrary-UTF-8 key/state property.
- Root, fuzz and conformance formatting; all-target workspace Clippy;
  exec-feature Clippy; alloc-only Clippy; conformance-probe Clippy; and
  warning-denied rustdoc pass with Rust 1.99.0 stable. Rust 1.81 builds the
  library with default features, with `exec`, and with no default features. The
  alloc-only `thumbv7em-none-eabi` build and package verification pass. The
  package contains 50 files (about 0.69 MiB, 0.17 MiB compressed).
- All 51 Criterion smoke workloads pass, including three example-generation
  workloads.
- The `format` sanitizer target now also checks self-comparison, sorted edit
  replay, example reparsing, entry-key/state preservation, inline-comment
  retention, malformed-item removal and empty-key redaction. Its final bounded
  campaign executes 84,929 inputs in 181 engine seconds with no crash, panic or
  invariant failure. The campaign found and fixed three example-generation
  edge cases: Compose comments needing a placeholder, quoted trailing comments
  under Compose's next-statement grammar, and colon/multiline empty values
  absorbing following lines. Empty-key and malformed statements are commented
  rather than copied. A final 20,940-input smoke after the custom-dialect
  comment fix also passed without a failure.
- The recorded conformance snapshot still matches 409 applicable observations
  and 798 field comparisons with
  `python3 conformance/check_adotenv.py --skip-upstream-replay`. No upstream
  replay was run for this slice.
- Parser, dialect, resolution, edit, formatting and encoding behavior is
  unchanged. Comparison and example generation are literal-only and perform no
  lookup, environment access, file I/O or command execution.

### Blank-line grouping slice (0.5, released 2026-10-18)

Tested source: the 0.5 release working tree adding `BlankLines`,
`FormatOptions::normalize_groups` and collapse-aware verification. Host and
toolchains match the slices above.

- With `PROPTEST_CASES=2000`, all 198 default-feature tests pass, the `exec`
  feature adds three tests (201 total), and all 20 doctests pass. The same 198
  unit/property tests and 20 doctests pass with `--no-default-features`.
- Four blank-line tests cover inner/leading/trailing runs, exact preservation,
  a CR-blank collapse/trim regression, group separators, idempotence and edit
  replay. The formatter verifier compares
  every nonblank item exactly and rejects adjacent blank items in collapsed
  output.
- Root, fuzz and conformance formatting; workspace, exec, alloc-only and
  conformance-probe Clippy; warning-denied rustdoc; Rust 1.81 builds; the
  alloc-only Thumb build; and locked package verification pass.
- All 54 Criterion smoke workloads pass, including three collapse-format
  workloads.
- The `format` sanitizer target adds the five-policy matrix with blank collapse
  and blank-aware item comparison. Its final bounded campaign executes 64,144
  inputs in 121 engine seconds with no crash, panic or invariant failure. The
  campaign found and fixed a collapse/trim interaction where skipping a line
  after a lone CR made a second pass non-idempotent; trailing-whitespace planning
  now uses the post-collapse kept-item sequence.
- Parser, dialect, resolution, edit, comparison, example and encoding behavior
  is unchanged. No I/O, environment, expansion or command execution is added.

### Quote normalization slice (0.5, released 2026-10-18)

Tested source: the 0.5 release working tree adding `QuoteStyle` and
quote-preserving formatter verification. Host and toolchains match the slices
above.

- With `PROPTEST_CASES=2000`, all 202 default-feature tests pass, the `exec`
  feature adds three tests (205 total), and all 20 doctests pass. The same 202
  unit/property tests and 20 doctests pass with `--no-default-features`.
- Four quote tests cover double normalization with comments, single quotes for
  multiline values, unquoted preference with the encoder's fallback, raw-value
  skipping and decoded-value preservation. Empty assigned values intentionally
  retain their quote style.
- Root, fuzz and conformance formatting; workspace, exec, alloc-only and
  conformance-probe Clippy; warning-denied rustdoc; Rust 1.81 builds; the
  alloc-only Thumb build; and locked package verification pass.
- All 57 Criterion smoke workloads pass, including three double-quote format
  workloads.
- The `format` sanitizer target adds double, single and unquoted policies to its
  eight-policy matrix. Its bounded campaign executes 26,424 inputs in 121 engine
  seconds with no crash, panic or invariant failure. The campaign found and
  fixed an empty-value/after-`=` spacing interaction; empty assigned values are
  now left unchanged in this slice.
- No I/O, environment, expansion or command execution is added.

### Group sorting slice (0.5, released 2026-10-18)

Tested source: the 0.5 release working tree adding `Document::sort`,
comment-aware chunk planning, dependency ordering and sorting verification.

- With `PROPTEST_CASES=2000`, all 215 default-feature tests pass, the `exec`
  feature adds three tests (218 total), and all 22 doctests pass. The same 215
  unit/property tests and 22 doctests pass with `--no-default-features`.
- Thirteen sorting tests cover lexical order, group headers, leading and inline
  comment movement, trailing comments, transitive dependencies, ready-key tie
  ordering, cycles, duplicates, malformed groups, cross-group independence,
  multi-group plans and an arbitrary-UTF-8 preservation/idempotence property.
- Root, fuzz and conformance formatting; workspace, exec, alloc-only and
  conformance-probe Clippy; warning-denied rustdoc; Rust 1.81 builds; the
  alloc-only Thumb build; and locked package verification pass.
- All 60 Criterion smoke workloads pass, including three sorting workloads.
- The locked package contains 53 files (about 0.73 MiB, 0.18 MiB compressed).
- The `format` sanitizer target now checks sort output group counts, entry
  multisets, comment multisets and sort idempotence across all five presets. Its
  final bounded campaign executes 18,829 inputs in 61 engine seconds with no
  crash, panic or invariant failure. The campaign drove a plan-index/header
  regression fix for unsortable groups between sortable ones.
- No I/O, environment, expansion or command execution is added.

### File-loading slice (0.6, released 2026-10-19)

Tested source: the 0.6 release working tree adding the opt-in `load`
feature, cascade validation and origin/report structures.

- With `PROPTEST_CASES=2000`, all 226 default-feature + load tests pass, the
  full feature set (`exec` + `load` + process adapter) passes 229 tests and
  three process-adapter tests, and all 22 running doctests pass (two ignored
  filesystem examples). The no-default-features build still passes 215 tests.
- Eleven loading tests cover cascade precedence, missing files, explicit files,
  invalid syntax, invalid UTF-8, duplicate and bare reports, `BareKeys::Empty`,
  nonparticipating keys, mode traversal rejection, empty mode and exact origin
  spans/lines.
- Workspace, exec, load, alloc-only and conformance-probe Clippy pass with Rust
  1.99.0; warning-denied rustdoc passes with and without `load`. Rust 1.81
  builds pass with default, `exec`, `load` and no-default features. The
  alloc-only Thumb build passes. The locked package contains 56 files (about
  0.75 MiB, 0.18 MiB compressed).
- All 60 Criterion smoke workloads still pass. The load feature adds no default
  benchmark or runtime dependency to the core.
- The recorded conformance snapshot still matches 409 applicable observations
  and 798 field comparisons. No upstream replay was run for this slice.
- Loading reads and validates all participating files before returning a result;
  missing cascade files are skipped and errors return no partial environment.
  No process environment is mutated, and no expansion or command execution is
  performed.

### Serde slice (0.6, released 2026-10-19)

Tested source: the 0.6 release working tree adding the opt-in `serde`
feature and `Document::deserialize`.

- With `PROPTEST_CASES=2000`, all 236 all-feature tests pass, the process
  adapter adds three tests, and 23 doctests pass (three filesystem examples are
  intentionally ignored). Workspace-default tests remain green; the
  no-default-features + serde build passes 222 tests and 23 doctests.
- Seven serde tests cover literal structs/primitives, duplicate policy,
  optional fields, `BareKeys::Empty`, quoted decoding, missing/bare errors,
  line/key-aware type errors and unsupported nesting.
- Workspace, load, serde, alloc-only + serde and conformance-probe Clippy pass
  with Rust 1.99; warning-denied rustdoc passes for the workspace and for the
  serde/load feature docs. Rust 1.81 builds pass for default, load, serde, exec,
  no-default and the process crate. The alloc-only Thumb build passes.
- All 60 Criterion smoke workloads pass. The locked package contains 59 files
  (about 0.77 MiB, 0.19 MiB compressed).
- The conformance snapshot still matches 409 applicable observations and 798
  field comparisons.
- Deserialization performs no file, environment, expansion or command access.

### CLI slice (0.7, released 2026-10-19)

Tested source: the 0.7 release tree adding the zero-dependency
`adotenv-cli` workspace companion.

- Fourteen CLI tests cover `get`, `check`, `fmt` with writing/sorting
  and check-only mode, output-only formatting, `diff`, `example`, `load`, `set`,
  `run`, unsafe child environment rejection, dialect selection, help and version.
- The full workspace all-feature suite passes 236 `adotenv` tests, three process
  tests and fourteen CLI tests; 23 doctests pass with four filesystem examples
  ignored. The no-default core still passes 215 tests.
- Workspace, serde, alloc-only and conformance Clippy pass with Rust 1.99.
  Rust 1.81 builds pass for the CLI, process crate, core configurations and the
  alloc-only Thumb target. Warning-denied workspace rustdoc passes.
- All 60 Criterion smoke workloads pass.
- At this slice, the root package verified at 60 files (about 0.78 MiB); the
  CLI package at six files and the process package at five files.
- Conformance snapshot still matches 409 applicable observations and 798 field
  comparisons.
- The CLI uses only library APIs. `run` starts an environment-inheriting child
  without mutating the parent process environment; file command substitution is
  not enabled.

### Ecosystem integration slice (0.7, released 2026-10-19)

Tested source: the 0.7 release tree adding `adotenv-config` and
`adotenv-figment` workspace companions.

- Ten adapter tests (four default and one `load`-gated per crate) cover
  flat-value construction, native `config` path/case semantics, literal
  Figment key spelling, duplicate policy, bare/ignored declarations, quoted
  decoding, scalar coercion, string extraction and loaded environments.
- The all-feature workspace suite passes 236 `adotenv` tests, three process
  tests, fourteen CLI tests and five tests for each integration adapter; 23
  doctests pass with four filesystem examples ignored. The default workspace
  suite passes 226 `adotenv`, three process, fourteen CLI and four tests for
  each adapter, with 22 doctests and the same four ignored examples. The
  no-default core still passes 215 tests.
- Workspace, load-enabled adapter, serde, alloc-only and conformance Clippy pass
  with Rust 1.95 and Rust 1.99. Rust 1.81 builds pass for both integration
  adapters (default and `load`), CLI, process and core configurations.
  Warning-denied workspace rustdoc passes.
- All 60 Criterion smoke workloads pass.
- The root package verifies at 63 files (about 0.85 MiB). Companion packages
  fully verify once the core version exists on crates.io; before that, CI
  verifies their manifests and file lists with `cargo package --list`. Packages
  contain seven files for the CLI and six for process, config and Figment. The
  tag workflow performs the full verified publish in dependency order.
  Conformance snapshot still matches 409 applicable observations and 798 field
  comparisons.
- The release workflow passes `actionlint`. Its tag/version check, publication
  plan, bootstrap-token selection and sparse-index wait were exercised locally
  with simulated registry responses.
- 0.7.0 was published on 2026-10-19. The tag-triggered Release run
  [39317378941](https://github.com/asamarts/adotenv/actions/runs/39317378941)
  passed every quality and publish job. crates.io returns HTTP 200 for all five
  0.7.0 versions and docs.rs serves documentation for all five crates; GitHub
  Release v0.7.0 was the latest release at that point. A downstream crates.io
  project installed `adotenv-cli` 0.7.0 and consumed all four companion crates
  through the `config`, Figment and process APIs. Trusted Publishing is configured for all
  five crates; the retained bootstrap token is reserved for future first-time
  crates.

### Post-release audit (0.7.0; 2026-10-19)

- Refreshed `fuzz/Cargo.lock` from the stale 0.5.0 path dependency to 0.7.0.
- Bounded libFuzzer smoke against the release tree on
  `x86_64-unknown-linux-gnu`, using the existing corpus: parser 36,795
  executions, edits 22,922, encoding 29,196, resolve 47,859 and format 8,385.
  No crash, assertion failure or artifact was produced; generated corpus files
  were not committed.
- `cargo audit --deny warnings` scanned 77 locked dependencies with no
  vulnerability advisory.
- Full companion `cargo package` verification passes now that `adotenv` 0.7.0
  is on crates.io. CI now selects full package verification for published core
  versions and file-list verification for pre-release versions.
- CI now also covers the alloc-only + `serde` Clippy, test and MSRV paths. That
  configuration passes 222 tests and 23 doctests; no-default core remains 215
  tests and 22 doctests.
- The CLI now removes its temporary file when an atomic Unix write fails before
  replacement. Companion crates forbid unsafe code except for the explicitly
  unsafe process adapter, which now enforces `unsafe_op_in_unsafe_fn`.
- `cargo-semver-checks` 0.51.0 compared crates.io 0.6.0 with 0.7.0 under forced
  patch semantics: 228 of 229 checks pass. The only failure is the intentional
  `#[must_use]` addition on `Group::items` and `Entry::leading_comments`, which
  is covered by the 0.7.0 minor version. No breaking public API change was
  found; the later `LoadedEnvironment::len` addition is additive.
- The existing weekly/manual Fuzz workflow covers all five targets with
  bounded 300-second jobs and uploads crash artifacts. Manual validation run
  [39352940619](https://github.com/asamarts/adotenv/actions/runs/39352940619)
  passed parser, edits, encoding, resolve and format. It complements, but does
  not yet satisfy, the sustained pre-1.0 fuzzing criterion.
- Fixed a broken cross-document anchor in `formatting.md`; a local Markdown link
  scan reports no broken local files or anchors.
- Crates.io, docs.rs, GitHub Release and the installed CLI were rechecked after
  publication; all five trusted-publisher configurations are present.

### 0.7.1 release (2026-10-20)

- `cargo-semver-checks` 0.7.0 -> 0.7.1 under forced patch semantics with the
  `load` feature: 229 of 229 checks pass, 31 skipped. The additive
  `LoadedEnvironment::len` method is accepted.
- All-feature workspace: 236 core, 3 process, 14 CLI, 5 config and 5 Figment
  tests; 23 doctests pass with four ignored.
- Alloc-only + serde: 222 tests and 23 doctests; no-default core: 215 tests and
  22 doctests.
- Clippy passes on Rust 1.95 and 1.99; Rust 1.81 builds pass; warning-denied
  workspace rustdoc passes. Root package contains 62 files.
- Release run
  [39372957394](https://github.com/asamarts/adotenv/actions/runs/39372957394)
  passed all quality and publish jobs. The OIDC token step succeeded and all
  five crates were published through their configured Trusted Publishers; no
  bootstrap token was needed. crates.io returns HTTP 200 for all five 0.7.1
  versions, docs.rs serves all five, and GitHub Release v0.7.1 is Latest.
- A downstream crates.io project installed `adotenv-cli` 0.7.1 and consumed all
  four companion crates through the `config`, Figment and process APIs.

### 0.8.0 release (2026-10-21)

- Added `Document::effective_entries` and `EffectiveEntry`, the shared
  value-aware extraction now used by serde and both integration adapters.
- Added `LoadedEnvironment::deserialize` behind `load + serde`.
- Added `LoadedEnvironment::resolve_contextual` and `resolve_decoded` with
  origin-aware `ResolvedEnvironment` results.
- Added `Document::lookup_index` and `LookupIndex` for explicit repeated lookups.
- Added CLI `-`/`--stdin` source input and `check --json` versioned diagnostics.
- All-feature workspace: 242 core, 3 process, 17 CLI, 5 config and 5 Figment
  tests; 26 doctests pass with six ignored. Alloc-only + serde is 224 tests and
  25 doctests (six ignored); no-default core is 217 tests and 24 doctests
  (six ignored).
- Clippy passes for `load,serde` and the workspace; warning-denied docs and the
  package/manifest checks remain green. All 66 Criterion smoke workloads pass,
  including index construction and repeated indexed lookups. The controlled
  local 0.8 baseline and its no-CI-threshold policy are recorded in
  [performance.md](performance.md).
- `Dialect`, `FormatOptions`, `ExpandOptions` and `ReferenceRules` are
  non-exhaustive with `with_*` builders; external adapter tests, benches and fuzz
  targets were migrated to the builder style.
- Release run
  [39649753106](https://github.com/asamarts/adotenv/actions/runs/39649753106)
  passed all quality and publish jobs. OIDC minted successfully and all five
  0.8.0 crates published keylessly. crates.io returns HTTP 200, docs.rs serves
  all five crates, and GitHub Release v0.8.0 is Latest.
- A forced patch-semver check reports only the intentional non-exhaustive
  struct/enum changes. The normal 0.7.1 -> 0.8.0 check reports no semver update
  required. Report/result structs, `EntryTokens`, policy enums and `Multiline`
  follow the same hybrid policy; `TextEdit` and `Position` remain
  caller-constructible.

### 0.9.0 release (2026-10-21)

- Release run
  [39692412414](https://github.com/asamarts/adotenv/actions/runs/39692412414)
  passed the full quality gate and all publish jobs. All five 0.9.0 crates were
  published keylessly through Trusted Publishing/OIDC.
- crates.io returns HTTP 200 for all five 0.9.0 package versions and docs.rs
  serves all five 0.9.0 documentation roots. GitHub Release v0.9.0 is Latest.
- The normal 0.8.0 -> 0.9.0 semver check reports no update required; the core
  API is unchanged by this tooling-focused release.

### 0.9.1 release (2026-10-22)

- Release run
  [39707271205](https://github.com/asamarts/adotenv/actions/runs/39707271205)
  passed the full quality gate and all publish jobs. All five 0.9.1 crates were
  published keylessly through Trusted Publishing/OIDC.
- crates.io returns HTTP 200 for all five 0.9.1 package versions and docs.rs
  serves all five documentation roots. GitHub Release v0.9.1 is Latest.
- The forced patch-semver check against 0.9.0 reports no update required; the
  core API is unchanged.

### 0.9.1 development slice (2026-10-22)

- Fixed recursive discovery to keep discovered paths as `PathBuf` values until
  display, so non-UTF-8 `.env` filenames are opened correctly. Text and JSON
  rendering remain explicitly lossy for such paths.
- Fixed relative excludes with a leading `./` so `--exclude ./sub` matches
  `sub` under each discovery root.
- Added regression coverage for default current-directory discovery, multiple
  roots, empty recursive results, leading-`./` excludes, non-UTF-8 discovery,
  tab-expanded human carets, GitHub warning/notice severities, two-file and
  export-aware matrices, diff bare/empty states, and Windows backup-swap
  failure restoration.
- All-feature workspace: 242 core, 3 process, 34 CLI, 6 config and 6 Figment
  tests; 30 doctests pass with six ignored. Alloc-only + serde is 224 tests and
  29 doctests (six ignored); no-default core is 217 tests and 28 doctests
  (six ignored). README examples now cover lookup indexing, effective values,
  raw tokens and decoded spans, common edit operations including uncomment,
  and checked editor positions.
- Completed a ten-minute-per-target fuzz campaign with 1,449,814 executions and
  no crashes; the weekly bounded workflow remains the sustained evidence path.

### 0.9.6 release (2026-10-23)

- Release run
  [39894080644](https://github.com/asamarts/adotenv/actions/runs/39894080644)
  passed the full quality gate and all publish jobs. All five 0.9.6 crates were
  published keylessly through Trusted Publishing/OIDC.
- crates.io returns HTTP 200 for all five 0.9.6 package versions and docs.rs
  serves all five documentation roots. GitHub Release v0.9.6 is Latest.
- The forced patch-semver check against 0.9.5 reports no breaking API change;
  this release adds the values-only loading API.

### Post-0.9.5 values-only loading (2026-10-23)

- Added `LoadedValues`, `load_cascade_values`, `load_cascade_values_with`,
  `load_values` and `LoadedEnvironment::into_values`. The compact path keeps
  strict file/dialect/duplicate/override semantics but drops origins, raw tokens
  and reports.
- Measured 65,536-entry loading at 81.3 MB peak / 60.5 MB retained RSS for the
  full loader versus 38.8 MB / 13.5 MB for values-only loading.
- Added cascade, explicit-file, dialect, bare-key and error-parity tests for the
  values-only path. All-feature workspace: 244 core, 3 process, 38 CLI, 6
  config and 6 Figment tests; 30 doctests pass with six ignored.

### 0.9.5 release (2026-10-22)

- Release run
  [39889882157](https://github.com/asamarts/adotenv/actions/runs/39889882157)
  passed the full quality gate and all publish jobs. All five 0.9.5 crates were
  published keylessly through Trusted Publishing/OIDC.
- crates.io returns HTTP 200 for all five 0.9.5 package versions and docs.rs
  serves all five documentation roots. GitHub Release v0.9.5 is Latest.
- The forced patch-semver check against 0.9.4 reports no update required; this
  release updates the internal metadata layout and test harness.

### 0.9.4 release (2026-10-22)

- Release run
  [39846156775](https://github.com/asamarts/adotenv/actions/runs/39846156775)
  passed the full quality gate and all publish jobs. All five 0.9.4 crates were
  published keylessly through Trusted Publishing/OIDC.
- crates.io returns HTTP 200 for all five 0.9.4 package versions and docs.rs
  serves all five documentation roots. GitHub Release v0.9.4 is Latest.
- The forced patch-semver check against 0.9.3 reports no update required; this
  release adds the retained-memory reduction, package-content fix and ecosystem
  performance audit.

### Post-0.9.3 performance audit (2026-10-22)

- Audited parser memory and ecosystem performance. The 65,536-entry fixture
  retained 36.2 MB before the fix and 23.6 MB after shrinking vector slack and
  compacting internal metadata; parse-only peak falls from 40.4 MB to 32.0 MB,
  and parse+map peak from 43.4 MB to 32.0 MB. Instruction-count comparisons and
  memory methodology are recorded in [performance.md](performance.md).
- Prototyped a sorted-vector `LookupIndex`: large-index build improved from
  21.5 ms to 8.9 ms and memory was lower, but batched gets regressed from
  7.1 µs to 16.9 µs for 64 keys. It was not adopted because repeated gets are
  the index's purpose; the hybrid/lazy-cache option is in the backlog.
- Added a CI check that `MIGRATION.md` and `docs/design/1.0-readiness.md` are in
  the core package after discovering the migration guide was excluded from the
  0.9.3 tarball.
- All-feature workspace: 242 core, 3 process, 38 CLI, 6 config and 6 Figment
  tests; 30 doctests pass with six ignored. Callgrind comparison and local
  benchmark numbers are exploratory and are not a new CI threshold.

### 0.9.3 release (2026-10-22)

- Release run
  [39831995654](https://github.com/asamarts/adotenv/actions/runs/39831995654)
  passed the full quality gate and all publish jobs. All five 0.9.3 crates were
  published keylessly through Trusted Publishing/OIDC.
- crates.io returns HTTP 200 for all five 0.9.3 package versions and docs.rs
  serves all five documentation roots. GitHub Release v0.9.3 is Latest.
- The forced patch-semver check against 0.9.2 reports no update required; this
  release contains the migration and readiness documentation only.

### Post-0.9.2 readiness documentation (2026-10-22)

- Added [MIGRATION.md](../../MIGRATION.md) and
  [1.0-readiness.md](1.0-readiness.md). The checklist separately tracks
  completed API, conformance, performance and contract evidence from the still
  pending four-week scheduled-fuzz accumulation and external-project
  requirement. Both acceptance bars are defined in the checklist.

### 0.9.2 release (2026-10-22)

- Release run
  [39814645082](https://github.com/asamarts/adotenv/actions/runs/39814645082)
  passed the full quality gate and all publish jobs. All five 0.9.2 crates were
  published keylessly through Trusted Publishing/OIDC.
- crates.io returns HTTP 200 for all five 0.9.2 package versions and docs.rs
  serves all five documentation roots. GitHub Release v0.9.2 is Latest.
- The forced patch-semver check against 0.9.1 reports no update required; this
  release contains documentation, contract-test and policy changes only.

### Post-0.9.1 contract slice (2026-10-22)

- Froze the `check --json` and `diff --format json` version-1 wire formats with
  exact-output tests, including `--json` alias equivalence. Froze the GitHub
  warning annotation shape and the exit-code contract, and documented the
  compatibility rules, ordering guarantees and the boundary between stable
  machine output and human/matrix output.
- All-feature workspace: 242 core, 3 process, 38 CLI, 6 config and 6 Figment
  tests; 30 doctests pass with six ignored. Alloc-only + serde is 224 tests and
  29 doctests (six ignored); no-default core is 217 tests and 28 doctests
  (six ignored).

### Post-0.9.1 performance slice (2026-10-22)

- Recorded a manual 1.0 performance budget in
  [performance.md](performance.md). A fresh local run was discarded rather than
  recorded because the shared 24-thread host had a load average near 96; the
  controlled 0.8 baseline remains the reference until an idle-host re-record.
- The policy keeps CI benchmark smoke only, requires idle-host comparisons,
  repeats the 1.5x–2x range and blocks above 2x until explained.

### 0.9.0 development slice (2026-10-21)

- Added recursive `.env` discovery and repeatable excludes to `check` and
  `fmt`; discovery skips `.git`, `target`, `node_modules` and symlinks.
- Added `check --format human|json|github`, one-based columns and summaries in
  human output, GitHub Actions annotations, and secret-safe `--source` as an
  explicit human-only opt-in. `--json` remains a JSON alias and the versioned
  JSON schema is unchanged.
- Added baseline-first multi-file `diff` with unchanged two-file human output,
  a matrix table and versioned JSON output.
- Hardened Windows replacement with a same-directory temporary file, a direct
  replace attempt and a backup-swap fallback. It restores the old file on
  failure and cleans up on success, but the fallback is explicitly not
  crash-atomic.
- Evaluated typed adapter usage and `config` 0.15. `config` typed extraction
  relies on its native scalar coercion and is covered by a test; Figment is
  documented and tested as requiring string fields and explicit conversion.
  `config` remains at 0.14 because 0.15.0 changes key-normalization behavior
  and the current 0.15 line requires Rust 1.88/edition 2024.
- All-feature workspace: 242 core, 3 process, 26 CLI, 6 config and 6 Figment
  tests; 26 doctests pass with six ignored. Alloc-only + serde is 224 tests and
  25 doctests (six ignored); no-default core is 217 tests and 24 doctests
  (six ignored).
- Clippy, rustdoc, MSRV, package and 66 Criterion smoke workloads remain green.
  The CLI tests cover env-name filtering, default ignored directories,
  excludes, short flags, symlink non-following, recursive check/fmt behavior,
  human/JSON/GitHub rendering, workflow escaping, columns, summaries, the
  source opt-in boundary, baseline-first multi-file diff, matrix output,
  diff JSON, replacement-file cleanup, config typed coercion and Figment's
  explicit-conversion boundary. The runbook now passes the GNU fuzz target
  explicitly, matching the scheduled workflow and avoiding host musl defaults.

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
cargo +nightly fuzz run resolve --target x86_64-unknown-linux-gnu /tmp/generated-resolve-corpus fuzz/corpus/resolve -- -max_total_time=300 -timeout=10 -rss_limit_mb=2048 -max_len=65536 -artifact_prefix=/tmp/resolve-artifacts/
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
recorded observations. It currently covers **71 cases across 20 profiles
(1,420 executed observations)**. Applicable literal profiles also compare against
the unreleased Rust presets; Portable is not asserted equivalent to an upstream.

| Implementation | Exact pin | Observation surfaces |
|---|---|---|
| Node `dotenv` | 18.0.8, npm SHA-512 lock | Default regex and opt-in fast parsing, string/buffer inputs, controlled loading with both override policies |
| `python-dotenv` | 1.2.4, wheel SHA-256 | Ordered bindings, literal stream/file values, resolution, literal/resolved loading with both override policies |
| Docker Compose | 5.6.0's compose-go 2.16.1 and Docker CLI 29.8.2 pins | Parser/resolution, project `.env`, actual service `env_file` default/raw loading |
| Docker CLI | 29.8.2, Go module checksums | Ordered raw env-file output, with no lookup or a controlled client lookup |

Reproduce on Linux with Node, Python (venv/pip) and Go installed:

```sh
python3 conformance/check_adotenv.py --setup
# Only after reviewing intentional input, API or dependency changes:
python3 conformance/run.py --record
```

`--setup` installs locked dependencies with npm scripts disabled and forcibly
reinstalls the hash-checked Python wheel. Go modules are verified before every
build; local workspaces, overlays, build flags and persisted Go configuration
cannot alter the oracle. Normal execution compares every profile/case and
never updates goldens. Explicit recording first re-establishes those locked
dependencies, then executes the real APIs. Exact input, profile, manifest and
dependency-lock snapshots are hashed before setup and execution; changed files
abort recording and comparison. Declared, locked and executed artifact identities
must agree. Every profile's observation fields and JSON types are validated before
recording. Required runtime/time/platform provenance is retained; compiler/runtime
versions need not equal the recording host. Duplicate JSON keys and nonstandard
numeric constants are rejected.
The reusable CI matrix replays every upstream profile, then builds the locked
Rust probe and compares applicable literal observations, including before a tag.
The probe's isolated workspace uses harness-only `serde_json`; it adds no core
runtime dependency. `--skip-upstream-replay` is explicitly local iteration only;
it still validates recorded provenance, artifacts, types and input hashes.

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

The next adversarial pass reproduced three recorder defects in a disposable
copy using the actual adapters: declaring Node 18.0.9 while executing locked
18.0.8, recording and subsequently accepting an empty observation, and binding
old observations to a fixture changed during execution. All three are now
rejected without replacing the golden. Wrong executed package identity and six
wrongly typed profile payloads are also rejected. A valid temporary recording
and its complete 1,340-observation replay still pass after a fresh locked setup.

The accompanying isolated alint audit fixed original-BOM byte accounting,
repeated-BOM native range offsets and pre-verification generated-output budgets.
Actual `check`, `fix`, `check` scenarios under a 512 MiB address-space limit
preserved repeated BOM bytes and CRLF, rejected over-budget raw/generated
documents without modifying files, reduced a nearly 100,000-line quoted value,
and fixed a 3,999,966-byte lone-CR warning-dense document. Conservative output
size bounds can decline a near-limit edit even when a shorter encoding would
fit. The primary alint checkout and its concurrent changes were not modified.

The pinned Compose service loaders inherit a bare key from project lookup when
available and retain a preceding value when a bare duplicate has no lookup.
This conflicts with the service-file reference's bare-unset wording; see
[dialects.md](dialects.md). Explicit YAML `environment:` null remains a distinct
override. The corpus records the observed implementation instead of guessing
from either documentation or a generic parser helper.

Scope limits: UTF-8 inputs and Linux environment behavior; no invalid-byte
decoder oracle, multi-file cascade, daemon/container semantics or sustained-fuzz
claim. The implemented literal comparison excludes resolution/loading profiles,
Node's opt-in fast parser and Python's file newline conversion. Compose excludes
dollar-containing sources or supplied lookup; fail-fast errors compare only
error presence, not partial recovering maps. Encoding/portability remain next.
Public fuzz seeds remain separate from conformance.

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

- MSRV is **1.81** for 0.2 through 0.7 (1.74 for 0.1), edition 2021. Raises
  happen only in minor releases and stay at least six months behind the latest
  stable Rust.
- Before 1.0, breaking changes belong in minor releases with explicit migration
  guidance in the changelog and README. 0.2 includes both Portable syntax and
  API changes; there is no hidden 0.1 compatibility grammar.
- After 1.0, the API follows semver. Stable diagnostic identities do not depend
  on enum order or message wording. Non-exhaustive enums allow new variants;
  callers retain a fallback arm.
- Workspace companions share the workspace release version and were published
  after core. Trusted Publishing is configured for all five crates; the retained
  bootstrap token is reserved for future first-time crates.
