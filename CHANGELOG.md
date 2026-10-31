# Changelog

All notable changes to this project are documented here. The format is based
on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The 1.0 fix round. See [1.0-fix-round.md](docs/design/1.0-fix-round.md).

### Security

- `adotenv-cli`: `fmt --write` and `set` create their temporary file with
  owner-only permissions before writing any content.
- Resolution enforces a total output budget, `ExpandOptions::max_output_bytes`
  (64 MiB in every preset), across resolved values and provenance. A small file
  of doubling references can no longer allocate unbounded memory.
- `LoadedEnvironment::resolve_decoded` resolves each loaded key exactly as
  loaded. A key that the synthetic source cannot represent is an error naming
  that key, never a different key.

### Added

- `ExpandOptions::max_output_bytes`, `ExpandOptions::with_max_output_bytes` and
  `ResolveError::OutputTooLarge`.

### Changed

- **Breaking:** `Edit::Insert` and `Document::insert` reject a key that the
  document already declares or that another edit in the batch creates, with
  `EditError::Duplicate`. Removing, commenting out or renaming away the existing
  declarations frees the key.
- **Breaking:** `sort` keeps two keys connected by a one-way reference in their
  source order, so resolved values never change. Other keys still sort by name.
- **Breaking:** in the Node and Python presets, each blank or whitespace-only
  line is its own `Blank` item instead of leading text of the next item. Groups,
  leading comments, removal, sorting, blank-line collapsing and portability
  checks now agree with the other presets. Parsed values do not change.
- **Breaking:** in the Compose preset, and in dialects with
  `QuotedTail::NextStatement`, a quoted entry's item includes the trailing
  whitespace, inline comment and line ending when no other statement follows on
  that line. `Entry::inline_comment` returns that comment. Parsed values do not
  change.
- **Breaking:** `LoadedEnvironment::resolve_contextual` requires whole-file
  scope, like `resolve_decoded`. Winning declarations come from different
  files, so no single order defines sequential bindings.
- **Breaking (CLI):** every read or write failure exits with `2`, including in
  `check` and `fmt`. Multi-file `check` and `fmt` exit with the highest code
  across files.
- **Breaking (CLI):** `run` exits with `128 + N` when the child is killed by
  signal `N`, and with `255` for a status outside `0` to `255`.
- **Breaking (CLI):** `load` prints Portable assignments that parse back to the
  same values. It exits with `2` before printing anything when a key or value
  has no Portable spelling.
- CLI: `fmt --write` and `set` on a symbolic link replace the link target and
  keep the link, and keep the file's owner and group where the user may set
  them.
- `adotenv-config` and `adotenv-figment` are versioned separately from the
  core. `adotenv-config` uses `config` 0.15, whose keys keep their case, and
  requires Rust 1.88.

### Fixed

- Parsing stays linear when many statements share one physical line in Compose
  and Node, including with a line byte limit.
- Resolution and reference scanning stay linear for many references in one
  value, long `$` runs and deep nesting.
- `LoadError::InvalidMode` describes every rejected mode name.
- Diagnostics for an unterminated quoted key count line breaks in the leading
  whitespace.
- `sort` handles a last group without a final line ending.
- Trimming keeps a whitespace-only last line that has no line ending, and
  `FinalNewline::Ensure` no longer adds a second line ending after collapsed
  blank lines.
- Batches that set entries and insert after them run in linear time.
- CLI: failed output writes and closed pipes exit with `2` instead of
  panicking, and arguments that are not UTF-8 are usage errors.

### Documentation

- `DoubleEscapes` rustdoc shows single backslashes.
- `ExpandOptions::compose` and `ReferenceRules::COMPOSE` describe reference
  rules, not Docker Compose interpolation.
- `loading.md` states the mode-name rules as implemented.
- Text edits apply in reverse order of the returned slice. Edits that start at
  the same offset are ordered for that.
- `cli.md` documents every JSON field, and a test keeps it in sync.
- The CLI docs describe what `run` passes to the child and which commands print
  values.
- The README notes that `Debug` output includes values.
- Restored this changelog.

### Breaking changes and migration

Parsed values and Portable item boundaries are unchanged. Resolution returns
the same values unless it exceeds the new output budget. Code that moves from
0.10 needs to check these points:

- `insert` of a key the document already declares now fails with
  `EditError::Duplicate`. Use `Edit::Set` to change the existing declaration,
  or remove or rename it in the same batch.
- `sort` output can differ from 0.10 where one key references another in the
  same group. Re-run formatting checks that compare against stored output.
- Code that walks `Document::items` for Node, Python or Compose files sees
  blank lines as separate `Blank` items and trailing comments of quoted Compose
  entries as inline comments.
- Pass `ExpandScope::WholeFile` to `LoadedEnvironment::resolve_contextual`.
- A resolution whose output exceeds 64 MiB fails with
  `ResolveError::OutputTooLarge`. Raise the limit with
  `ExpandOptions::with_max_output_bytes` where large outputs are expected.
- CLI scripts: a read or write failure exits with `2` in every command, and
  `run` reports a child killed by a signal as `128 + N`. `load` prints Portable
  assignments, quoted where needed, instead of raw `KEY=value` lines.
- `adotenv-config` requires `config` 0.15 and Rust 1.88, and `config` 0.15 keys
  are case-sensitive: read `A.B` with the case it has in the file. Depend on the
  adapters by their own version, which no longer follows the core version after
  0.11.

## [0.10.4] - 2026-10-27

### Performance

- Added a parser fast path for documents without duplicate keys.

### Fixed

- Duplicate detection for empty-key Compose declarations.

## [0.10.3] - 2026-10-27

### Added

- `parse_values` and `parse_values_with` for values-only parsing without
  retaining a lossless document.

## [0.10.2] - 2026-10-26

### Performance

- The loader stores value, origin and raw-token data in one record per key.
  Default retained memory on the 65,536-entry fixture fell to 27.8 MB and
  full-report memory to 37.6 MB.
- Added reproducible benchmarks, loader profiles and enforced performance
  budgets.

## [0.10.1] - 2026-10-26

### Changed

- Rewrote the README, design documents and crate READMEs for the current API.
  No API changes.

## [0.10.0] - 2026-10-25

### Changed

- Unified the full and compact provenance APIs into one interned
  `LoadedEnvironment`. Values, shared-file origins and raw winning tokens are
  retained by default; entries, overrides, repeats and skipped declarations are
  opt-in through `LoadOptions::full` or `.with_reports(true)`.
- `LoadedEnvironment::values()` now returns a borrowed iterator, and
  `entries()`/`overrides()`/`repeated()`/`skipped()` return borrowed report
  iterators. `LoadedEntry`, `LoadOrigin`, `LoadOverride`, `LoadRepeated` and
  `LoadSkipped` expose private data through accessors.
- Keys and values use `Arc<str>` sharing, and origins use a compact file table.
- Added `LoadOptions::full`; `LoadOptions::new` is now the lean default profile.
- Resolution errors expose `LoadOriginOwned` with accessors.
- Updated the CLI, process, `config` and Figment consumers for borrowed value
  iteration.

### Removed

- `CompactLoadedEnvironment`, `CompactLoadOrigin`, `load_cascade_compact`,
  `load_cascade_compact_with`, `load_files_compact` and
  `LoadedEnvironment::into_compact` are removed. Their values-plus-origins
  profile is the default unified `LoadedEnvironment` profile.

### Performance

- On the 65,536-entry fixture the default loader retains 29.7 MB at 53.8 MB
  peak RSS; `LoadOptions::full` retains 35.6 MB at 63.9 MB peak; values-only
  loading retains 12.6 MB at 13.9 MB peak. The 0.9.19 comparison points were
  37.1 MB / 63.6 MB full and 16.2 MB compact.

## [0.9.19] - 2026-10-25

### Added

- Additive `CompactLoadedEnvironment` and `CompactLoadOrigin` APIs, with direct
  `load_cascade_compact`, `load_cascade_compact_with` and `load_files_compact`
  loaders plus `LoadedEnvironment::into_compact`. They retain values and
  shared-file origins without reports or raw context.
- Compact values expose borrowed `get`/`iter` access and `into_values` for an
  explicitly owned map.

### Performance

- The 65,536-entry compact loader retains 16.2 MB at 41.3 MB peak RSS, versus
  37.1 MB / 63.6 MB for the full default loader and 13.6 MB for values-only
  loading.

## [0.9.18] - 2026-10-24

### Performance

- Overridden report values now use a sparse index map instead of an
  `Option<String>` slot on every compact entry record. The 65,536-entry full
  loader falls from 65.5 MB peak / 38.6 MB retained RSS to 63.6 MB / 37.1 MB;
  with raw tokens disabled it falls to 53.4 MB / 26.3 MB.

## [0.9.17] - 2026-10-24

### Performance

- Compact report entries now reference a shared file table instead of cloning a
  path for every entry; full `LoadOrigin` values are materialized only for the
  public origin/entry APIs. The 65,536-entry full loader falls from 69.8 MB
  peak / 42.7 MB retained RSS to 65.5 MB / 38.6 MB; with raw tokens disabled it
  falls to 54.7 MB / 27.9 MB.

## [0.9.16] - 2026-10-24

### Performance

- Report entries are stored compactly and materialized lazily on the first
  `entries()` call. Final values are no longer duplicated eagerly, while
  overridden-entry values remain available. The 65,536-entry full loader falls
  from 71.8 MB peak / 44.7 MB retained RSS to 69.8 MB / 42.7 MB; with raw tokens
  disabled it falls to 59.1 MB / 31.9 MB.
- Added coverage for lazy materialization of overridden values and origins.

## [0.9.15] - 2026-10-24

### Performance

- Winning raw tokens are stored in one contiguous source buffer with ranges
  instead of two owned strings plus map overhead per key. The 65,536-entry full
  loader falls from 76.5 MB peak / 49.3 MB retained RSS to 71.8 MB / 44.7 MB;
  with origins and reports disabled it falls to 49.6 MB / 24.3 MB.
- Added a final source-buffer shrink after loading.

## [0.9.14] - 2026-10-24

### Performance

- When reports are retained, the default full loader no longer stores a duplicate
  final-origin map; `origin()` derives from the final matching entry through a
  lazy lookup cache. The 65,536-entry fixture falls from 83.3 MB peak / 56.6 MB
  retained RSS to 76.5 MB / 49.3 MB. With raw tokens disabled it falls from
  68.3 MB / 41.3 MB to 61.1 MB / 34.0 MB.
- Added coverage for repeated origin lookup through the lazy cache.

## [0.9.13] - 2026-10-24

### Added

- `LoadOptions::with_origins(false)` omits the final origin map and cross-file
  override reports. Remaining reports carry their own origins; combine with
  `with_reports(false)` for the lowest full-loader residency.

### Performance

- With raw tokens retained, disabling origins and reports lowers the
  65,536-entry full loader from 83.3 MB peak / 56.6 MB retained RSS to 54.3 MB /
  28.9 MB. Also disabling raw tokens reaches 39.0 MB / 13.6 MB.

## [0.9.12] - 2026-10-24

### Performance

- Matrix `diff` now extends a compact row table one candidate at a time instead
  of retaining every document. On three identical 65,536-entry files, peak RSS
  falls from 119.6 MB to 103.0 MB and runtime from 141 seconds to 0.8 seconds,
  with byte-identical output.
- Matrix mode continues to buffer the complete table before printing, preserving
  the no-partial-output contract.

## [0.9.11] - 2026-10-24

### Performance

- `Document::compare` now uses the explicit `LookupIndex` for nonempty keys and
  a direct fallback only for dialect-specific empty keys. This removes the
  per-key document scan that made large CLI diffs quadratic. On three identical
  65,536-entry files, the streaming diff completes in about 0.6 seconds with
  about 73.3 MB peak RSS, versus 95.6 MB before streaming; output is identical.
- Added a Compose empty-key comparison regression test.

## [0.9.10] - 2026-10-23

### Performance

- Human and JSON `diff` now parse the baseline once and candidates
  incrementally, buffering output until all candidates validate. Matrix diff
  still retains the complete document set. On three identical 65,536-entry
  files, peak RSS falls from 95.6 MB to 67.4 MB with byte-identical output.
- Added regression coverage ensuring a late candidate read or parse failure
  emits no partial human or JSON output.

## [0.9.9] - 2026-10-23

### Performance

- Reworked `LookupIndex` from a `BTreeMap` to a compact open-addressed table
  with bounded-probe fallback to the ordered map. For the 65,536-entry fixture,
  large-index construction falls from 38.4 ms to 15.6 ms and a 64-key lookup
  batch falls from 8.14 µs to 1.62 µs; retained RSS is about 30.9 MB versus
  31.5 MB.
- Added a differential property test that compares index lookups and winner
  identities against `Document::get`/`get_entry` across all presets and
  duplicate/bare policies. Added a forced tree-fallback test.

## [0.9.8] - 2026-10-23

### Performance

- Values-only loading now uses a projection mode in the shared parser. It
  applies the same grammar, dialect, duplicate, bare-key and error semantics
  while retaining only the final value map. The 65,536-entry fixture drops from
  38.8 MB peak / 13.6 MB retained RSS to 13.6 MB / 13.6 MB.
- Added differential property tests comparing projected values and first-error
  codes against the full parser across all five presets and duplicate/bare-key
  policies, plus explicit cascade, dialect and invalid-source regressions.

## [0.9.7] - 2026-10-23

### Added

- `LoadOptions`, `load_cascade_with_options` and `load_files_with_options` for
  full-loader retention control. Values and origins are always retained; raw
  winning tokens and report vectors can be dropped independently.
- `LoadedEnvironment::retains_raw_tokens` reports whether contextual resolution
  is available. `resolve_contextual` returns a clear error when raw tokens were
  dropped; `resolve_decoded` remains available.

### Performance

- Replaced per-key `Document::get_entry`/`get_all` scans in the full-loader merge
  with a single occurrence pass, removing effectively quadratic behavior on
  large files. Retaining raw tokens and reports now uses 83.3 MB peak / 56.6 MB
  retained RSS for the 65,536-entry fixture; disabling both falls to 51.7 MB /
  26.9 MB, and values-only loading remains 38.8 MB / 13.6 MB.

## [0.9.6] - 2026-10-23

### Added

- `load_cascade_values`, `load_cascade_values_with`, `load_values` and
  `LoadedValues` for values-only application loading, plus
  `LoadedEnvironment::into_values`. The compact path keeps strict read, dialect,
  duplicate and override semantics while dropping origins, raw tokens and
  reports.

### Performance

- Values-only loading reduces the 65,536-entry fixture from 81.3 MB peak /
  60.5 MB retained RSS to 38.8 MB peak / 13.5 MB retained.

## [0.9.5] - 2026-10-23

### Performance

- Compact internal document metadata: boxed rare diagnostics, sentinel entry
  indices, boxed decoded/mapping storage and geometric-slack shrinking. The
  65,536-entry fixture retains 23.6 MB instead of 36.2 MB; peak falls from
  40.4 MB to 32.0 MB.

### Fixed

- Fixed a flaky stdin test helper that could leave the child pipe open.

## [0.9.4] - 2026-10-22

### Performance

- Reduced retained parser memory by shrinking geometric-growth slack in the
  document item and entry vectors; the 65,536-entry fixture retains 28.3 MB
  instead of 36.2 MB. Recorded a ranked optimization backlog in
  [performance.md](docs/design/performance.md).

### Fixed

- Include `MIGRATION.md` in the published core package and verify required
  packaged documentation in CI.

## [0.9.3] - 2026-10-22

### Documentation

- Added [MIGRATION.md](MIGRATION.md) with the 0.9-to-1.0 compatibility path,
  older migration links and an adoption checklist.
- Added [1.0-readiness.md](docs/design/1.0-readiness.md) with explicit evidence
  status for API freeze, fuzz, conformance, performance, contracts and the
  still-pending external-user requirement, including the four-week fuzz bar and
  external-adopter definition.

## [0.9.2] - 2026-10-22

### Tests

- Froze the versioned `check` and `diff` JSON wire formats, GitHub annotation
  shape, JSON alias behavior and exit-code contract with exact-output
  regression tests.

### Documentation

- Documented machine-output compatibility rules, exit codes and the distinction
  between stable JSON/GitHub contracts and human-readable output.
- Recorded the manual 1.0 performance budget: idle-host comparison against the
  controlled baseline, repeat the 1.5x–2x range, and block above 2x until
  explained. CI remains benchmark smoke only.

## [0.9.1] - 2026-10-22

### Fixed

- Recursive discovery now reads `.env` files whose filesystem names are not
  valid UTF-8 instead of converting the path lossily before opening it. Human
  and JSON output still display such paths lossily.
- `--exclude ./path` now normalizes the leading `./` and matches the intended
  relative path.

### Tests

- Added coverage for default current-directory discovery, multiple roots,
  empty recursive results, leading-`./` excludes, non-UTF-8 discovery paths,
  tab-expanded source carets, GitHub warning/notice annotations, two-file and
  export-aware matrices, diff bare/empty JSON states, and Windows backup-swap
  failure restoration.

## [0.9.0] - 2026-10-21

### Added

- CLI recursive `.env` discovery and repeatable `--exclude` paths for `check`
  and `fmt`. Discovery skips `.git`, `target`, `node_modules` and symlinks.
- `check --format human|json|github`, with `--json` retained as a JSON alias.
  Human output adds columns and a summary; GitHub output emits workflow
  annotations. `--source` adds source lines and carets only as an explicit
  human-output opt-in because values may be secret.
- Baseline-first multi-file `diff` with `--format human|matrix|json`; the first
  file is the baseline, and `--json` remains an alias for JSON output.
- Windows replacement hardening: writes use a same-directory temporary file,
  attempt a direct replace, and fall back to a backup swap that restores the old
  file on failure and cleans up on success. The fallback remains best-effort
  rather than crash-atomic.
- Documented and tested typed adapter usage: `config` uses its native scalar
  coercion, while Figment requires string fields and explicit conversion.
  Retained `config` 0.14 after evaluating 0.15 key semantics and MSRV.

## [0.8.0] - 2026-10-21

### Added

- `Document::effective_entries` yields the winning declaration and effective
  value for each key, honoring duplicate and bare-key policy.
- `LoadedEnvironment::deserialize` maps final loaded values to a Serde type
  when both `load` and `serde` are enabled, preserving origin lines in value
  conversion errors.
- `LoadedEnvironment::resolve_decoded` resolves final decoded values under an
  explicit decoded-text policy and returns `ResolvedEnvironment` with origins.
- `LoadedEnvironment::resolve_contextual` reconstructs winning raw key/value
  tokens so quote, escape and dialect context survive cascade resolution.
- `Document::lookup_index` builds a borrowed `LookupIndex` for repeated lookups,
  honoring duplicate selection and bare-key policy without mutating the document.
- CLI commands that read source accept `-` or `--stdin` for standard input.
  Writes and `set` reject standard input; multiple stdin inputs are rejected.
- `check --json` emits a versioned machine-readable diagnostic document with
  paths, severity, stable codes, messages and exact spans.

### Changed

- `Dialect`, `FormatOptions`, `ExpandOptions` and `ReferenceRules` are now
  `#[non_exhaustive]` and expose `with_*` builder methods. External struct
  literals must be replaced by a preset/default plus builders or field mutation.
  This is a deliberate pre-1.0 API-freeze change. Library-produced report
  structs and entry tokens are also non-exhaustive; caller-built value types
  such as `TextEdit` and `Position` remain constructible with literals.
  Dialect/format policy enums and `Multiline` are non-exhaustive as well;
  `Multiline` exposes `with_single`, `with_double` and `with_backtick`.

## [0.7.1] - 2026-10-20

### Added

- `LoadedEnvironment::len` reports the number of effective values.

### Fixed

- The CLI removes its temporary file if an atomic Unix write fails before
  replacement.

## [0.7.0] - 2026-10-19

### Added

- `adotenv-config` workspace adapter exposing effective literal values as a
  `config` source, and `adotenv-figment` exposing them as a Figment provider.
  Both companions offer an optional `load` feature for loaded environments.
- `adotenv-process`, `adotenv-cli`, `adotenv-config` and `adotenv-figment` are
  published as 0.7.0 companions. The release workflow publishes all workspace
  crates in dependency order; first-time crates.io publication uses a temporary
  bootstrap token.
- Zero-dependency `adotenv-cli` workspace companion with `get`, `check`, `fmt`
  (including `--write` and `--sort`), `diff`, `example`, `load`, `set` and
  `run`. `run` starts a child with loaded values without mutating the parent
  process environment; child environment values are validated and command
  substitution from files is not enabled. A global `--dialect` option selects
  all library presets for CLI commands; `fmt --check` reports drift without
  writing, and Unix writes use atomic same-directory replacement.

## [0.6.0] - 2026-10-19

### Added

- Opt-in std-only `load` feature with `load_cascade`, `load_cascade_with` and
  `load_files`. The standard cascade is `.env`, `.env.local`, `.env.{mode}` and
  `.env.{mode}.local`, with later files overriding earlier files by key.
- `LoadedEnvironment` exposes final literal values and effective origins
  (path, source span and one-based line), every loaded declaration, cross-file
  overrides, per-file repeated keys and skipped bare/nonparticipating/empty-key
  declarations.
- All participating files are read, UTF-8 decoded, parsed and validated before
  any result is returned. Missing cascade files are skipped; an existing file
  with an I/O, decoding, syntax or policy error invalidates the complete load.
  No process environment is changed.
- Separate `adotenv-process` adapter with `validate` and an explicit
  `unsafe apply`. Every key/value is validated before the first mutation; only
  loaded keys are set, unrelated process variables are not cleared, and changes
  cannot be rolled back.
- Opt-in `serde` feature with `Document::deserialize`, `DeserializeError` and
  line/key-aware value conversion. Effective literal declarations map to Serde
  structs and scalar primitives; duplicate policy, lookup participation and
  `BareKeys::Empty` are honored.

### Notes

- `adotenv-process` was a workspace companion crate at the 0.6 release and had
  not yet been published to crates.io. It depends on the 0.6 loading feature;
  0.7 publishes it.

## [0.5.0] - 2026-10-18

### Added

- `Document::groups`, `Group` and `Groups` expose blank-line-separated runs of
  nonblank items without copying or normalizing source. `Document::header`
  exposes the structural prefix before the first declaration. Group views
  provide `items`, `entries`, `comments`, `leading_comments` and `span`.
- `Entry::leading_comments` returns contiguous whole-line comments immediately
  above a declaration; a blank line breaks attachment. Inline comments remain
  available through `Entry::inline_comment`.
- `Document::format`, `FormatOptions`, `EqualsSpacing`, `BlankLines`,
  `QuoteStyle`, `FinalNewline`, `FormatResult` and `FormatError` provide
  verified trivia-only formatting. Spacing around assignment separators,
  trailing spaces/tabs, blank-line grouping, value quote preference and
  final-newline policy are explicit; `FormatOptions::normalize` selects a
  conservative common policy, and `normalize_groups` additionally collapses
  blank-line runs. Quote changes are accepted only through the context-aware
  encoder and only when the decoded value survives; empty assigned values keep
  their existing quote style.
- Formatting edits are source-relative `TextEdit` values and the formatted text
  is always reparsed with the selected dialect before being returned. The
  verifier checks declarations, quotes, states, exports, lookup participation,
  comment text, item kinds and malformed statements. Raw values, multiline
  bodies, invalid/ignored statements, empty assignments whose spacing could
  become indentation and mixed lone-CR boundaries are preserved.
- `Document::compare`, `Comparison`, `ComparisonEntry`, `ComparedValue` and
  `DifferenceKind` compare effective keys between two documents. Missing keys,
  bare/empty/assigned states, decoded values and export differences are
  distinct; duplicate policy and lookup participation follow each document.
- `Document::example` generates a value-free example while retaining comments,
  blank lines, keys, exports, formatting, line endings and BOM. Bare keys remain
  bare, and malformed/ignored statements become indentation-preserving comments
  so their raw text cannot leak values.
- `Document::sort` orders declarations by key within blank-line-separated
  groups. Group headers and trailing comments stay in place; leading/inline
  comments and multiline entries move with their declarations. `${VAR}`
  dependencies are ordered before dependents. Duplicate/empty keys, malformed
  groups and dependency cycles are left unchanged, and the result is reparsed
  and verified.
- A dedicated `format` fuzz target, structure/format/comparison/example/sort
  property tests, Criterion workloads and checked-in synthetic corpus seeds.

### Notes

- `FormatOptions` gained the `blank_lines` and `quote_style` fields; external
  struct literals that do not use `..FormatOptions::default()` must add
  `BlankLines::Preserve` and `QuoteStyle::Preserve`.
- No default I/O, environment, expansion or command execution is introduced.

## [0.4.0] - 2026-10-18

### Added

- `Entry::references` reports raw `$VAR`, `${VAR:-default}` and `$(command)`
  candidates with original-source spans, quote/escape context, operators,
  nested containers and malformed-form metadata. `ReferenceRules` and
  `OperatorSet` provide pure eligibility decisions without lookup, expansion,
  I/O or command execution. Appended quoted tails are inspected as unquoted
  context; Compose `$$` escaping and quote-specific escape rules are retained.
- `Document::resolve`, `ExpandOptions`, `ResolvedDocument`, `ResolvedEntry`,
  `ResolvedSegment`, `ResolvedSegmentKind`, `ValueOrigin` and `ResolveError`
  for opt-in variable resolution with an explicit caller lookup. Sequential or
  whole-file scope, caller/file precedence, every operator, unresolved policy,
  required-variable errors, reachable cycle detection, depth/size limits and
  file/lookup/nested provenance are supported. Caller values remain literal;
  no environment, file or command access occurs by default.
- `Document::resolve_with_commands`, `CommandContext`, `CommandError`,
  `CommandCall`, `CommandOutcome` and `ResolveFailure` for explicit command
  substitution. Commands run only through the caller callback when rules enable
  them; raw text/spans and optional source identity are supplied. Outputs are
  literal, every call is recorded, and failures retain earlier calls.
- Optional `exec` feature with `exec::Shell`: explicit working directory or
  source-directory origins, platform-shell execution of exactly the supplied
  text, one trailing newline trimmed, and spawn/status/UTF-8 errors returned as
  `CommandError`. It is never enabled by default and never inherits the host
  working directory.

### Breaking changes and migration

- 0.4 is additive for existing parsing, editing and 0.3 APIs. Resolution is
  opt-in and the `exec` feature is disabled by default; no default process,
  environment or file access is introduced.
- Command forms remain literal unless `resolve_with_commands` receives a
  callback and the selected rules enable commands. Caller lookup values are
  literal and are not re-expanded.

## [0.3.0] - 2026-10-17

### Added

- 0.3 literal `Dialect` configuration and `PORTABLE`, `NODE_DOTENV`,
  `PYTHON_DOTENV`, `COMPOSE` and `DOCKER_RAW` presets. `parse_with`,
  `parse_owned_with`, `parse_strict_with`, retained configuration and borrowed
  `Document::reparse` preserve Portable defaults without enabling expansion,
  inheritance or loading.
- Typed syntax/escape/lookup/error policies; backtick quotes, Python quoted keys,
  ignored-statement metadata, appended-tail source maps and Docker scanner
  boundaries. Native edits verify with the exact selected configuration.
- Public Rust probe and CI comparisons against applicable real pinned upstream
  profiles. Four Compose EOF/Unicode regressions extend the corpus to 71 cases
  and 1,420 actual observations; resolution/loading exclusions are explicit.
- Property and sanitizer fuzz invariants across five presets and custom policies,
  retaining independent Portable semantic and positive-edit oracles.
- Exact `encode_value_with` and non-exhaustive `EncodeError`, with a shared
  context-aware codec for native set/insert. Independent escape switches,
  overlapping substitutions, appended tails, literal-fallback comment padding,
  raw terminal CR and scanner budgets preserve literal values. The original
  helper keeps its control policy; the new API and native edits accept every
  control the selected grammar represents.
- `Document::portability` and the `PortabilityReport`/`PortabilityDifference`/
  `PortabilityReading`/`PortabilityPart`/`PortabilityDeclaration` API. The
  document's own configuration is included first, each unique dialect parses
  once, and differing declarations, values, states, export prefixes, lookup
  participation, effective bare values, duplicate winners and statement grouping
  are reported per physical line. Quote style alone is not a difference; no
  expansion or lookup occurs.
- Positive inverse properties and a dedicated encoding sanitizer target/CI job.
- Criterion workloads for `encode_value_with` across all five presets and for
  large native set/set-rename batches, plus minimized regression seeds for the
  bounded inverse findings below.

### Fixed

- Reject conformance recordings with inconsistent declared/locked/executed
  package identities or malformed profile observations. Snapshot and hash exact
  corpus, profile, manifest and lock bytes before setup/execution, rejecting
  changed inputs before golden recording or comparison.
- Preserve ignored-line diagnostic metadata during native verification; repair
  quoted-key style changes, tight bare comments, ignored quote tails and generated
  LF-mode ending selection and insertion separators at terminal CR, Node regex
  and Compose next-statement boundaries. Map every boundary after malformed
  Compose octal deletion, keep newline normalization independent of unknown-escape
  handling, enforce tight-comment policy and consume whole rejected scanner lines.
- Exclude the isolated Rust probe's generated build directory from version control.
- Keep LF-only comment restoration exact, order preceding-item fields before
  same-offset insertion groups, and use planned item kinds and required bare
  terminators when inserting. Reject complete oversized physical lines even
  when a Node comment ends early at a Unicode regex anchor.
- Complete the bounded literal inverse for Node outer-unquote/literal fallback,
  escaped terminal closers, normalized-CR overlap and custom append scanner
  boundaries. Five minimized sanitizer witnesses are checked-in seed corpus
  entries, and the former crash reproducers execute cleanly.
- Permit a Node standalone token to move its opening quote past a line break
  when a first-line byte cap cannot fit it. The grammar treats that break as
  trivia between `=` and the value; this removes a false `Unrepresentable`
  result for legal continuation-line encodings.
- Resolve same-line native set values against other final set values under a
  line-byte cap, retrying until the batch converges. Renamed/exported headers
  participate in the same budget, and capped next-statement insertion adds the
  physical separator it requires. Batch acceptance no longer depends on request
  order, and the budget interaction is covered by a three-set regression.

### Breaking changes and migration

- Public `EntryTokens` gained the `quoted_tail` field. Code constructing it must
  add `quoted_tail: None` when absent; parsed documents populate it.
- Portable parsing and the existing entry/edit defaults are unchanged. New
  `Dialect`, `parse_with`, `encode_value_with` and `Document::portability`
  surfaces are additive.

## [0.2.1] - 2026-10-16

### Added

- Public version-pinned conformance inputs, executable real-upstream adapters,
  integrity locks and 1,340 observations across 67 cases and 20 profiles.
  Node dotenv 18.0.8, python-dotenv 1.2.4, Compose 5.6.0's role-specific APIs
  and Docker CLI 29.8.2 keep literal parsing, resolution and loading distinct.
- Reproducible Linux corpus checking in reusable CI, with explicit golden
  recording, controlled lookup, profile/input/lock provenance and actual
  Compose CLI corroboration. Dialect presets remain future APIs.

### Fixed

- Avoid materializing discarded diagnostic lists, duplicate-warning key tries
  and CR diagnostic merge buffers in strict parsing and native edit verification.
  Error records remain on invalid items; recovering diagnostics, lossless
  documents and strict first-error behavior are unchanged.
- Require successful native edits for independently classified safe fuzz inputs,
  rather than accepting a regression that rejects every request.
- Check parser token roles and complete trivia layout, independently decoded
  values and exact sampled raw mappings, and valid/error position domains.
- Correct the nine original named edit seeds for the eleven-operation dispatch.
- Gate tag publication on the complete reusable, same-commit CI matrix.
  Manual Release dispatch runs those checks without publishing; its concurrency
  group is separate from ordinary CI.

## [0.2.0] - 2026-10-15

### Added

- A lossless `Document` retaining borrowed or owned UTF-8 source, raw token
  spans, blank/comment/entry/invalid items, BOM and exact line endings.
- Recovering `parse`, owned `parse_owned`, and first-error `parse_strict`;
  stable diagnostic codes, severities and secondary locations.
- Multiline single/double-quoted values; distinct bare, empty and value states;
  decoded-value boundaries and ranges mapped back to original source bytes.
- Checked `LineIndex` conversions for UTF-8, scalar and UTF-16 positions,
  including explicit one-based display conversion.
- Native snapshot-local set, remove, rename, insert, comment, uncomment and
  export operations, with whole-batch verification and `VerifiedEdits` exposing
  both text edits and the verified output. Explicit occurrence selectors can
  edit duplicates; unrelated invalid syntax is preserved.
- An alloc-only `no_std` core behind `default-features = false`, with zero
  runtime dependencies and unconditional prohibition of unsafe code.
- A root Cargo workspace, Criterion parsing/native-edit workloads, parser/edit
  fuzz targets with public synthetic seeds, and scheduled bounded fuzz CI.
  These are runnable harnesses, not benchmark results or a sustained-fuzz claim.
- CI checks for Rust 1.81, an embedded no-std target, both feature modes,
  packaging and benchmark smoke; publishing selects `adotenv` explicitly.

### Audit fixes

- Preserve empty EOF uncommenting, explicit multiline restoration across removed
  comments, and final-newline policy against the planned surviving output.
- Permit an explicitly uncommented region to expose its own BOM, while still
  rejecting reinterpretation of unrelated source.
- Use logarithmic decoded-boundary lookup and indexed repeated key lookups;
  avoid quadratic collision scans and insertion-vector shifts in large batches,
  reusing the existing edit buffer in place.
- Independently constrain fuzz edits to requested raw targets, assert exact
  insertion positions, and preserve unselected items and invalid diagnostics.
- Cover the excluded fuzz workspace in dependency updates.
- Replace the nightly-incompatible cargo-fuzz 0.12.0 runner with the exercised
  0.13.2 pin, and pin crash-artifact uploads to the supported Node 24 action.

### Breaking changes and migration

- MSRV increases from 1.74 to **1.81**; the package version becomes **0.2.0**.
- `parse(&str)` returns `(Document, Vec<Diagnostic>)`, not `Result`.
  Use `parse_strict` for the first error-severity diagnostic; warnings do not
  fail strict parsing. Match stable codes rather than old errors or messages.
- `entries()` is a double-ended exact-size iterator of borrowed views, not a
  slice. Use `entry(id)` or iterator selection instead of slice indexing.
  Item/entry IDs are local to one source snapshot.
- `Entry::value()`, `raw_value()` and `value_span()` are optional: bare `KEY`
  is retained with no assigned value. Assigned empty values remain `Some("")`.
  Last-wins lookup does not skip a last bare declaration.
- Portable keys are nonempty ASCII letters, digits, `_`, `.` and `-`.
  Other characters are errors; non-POSIX spellings are accepted with warnings.
- Whitespace-sensitive hash comments include an empty assignment such as
  `A= # comment`. A tight hash in `A=a#b` remains literal with a warning.
- Double quotes additionally decode `\$`; unknown escapes remain literal
  with warnings. Single quotes remain fully literal.
- Non-comment text after a closing quote is now an error instead of being
  ignored. Both quote styles support physical multiline values; matching
  quotes can therefore group lines that 0.1 treated separately. Unmatched
  quotes recover after their opening physical line.
- Document-native edits return `VerifiedEdits`. Free `set` and `remove` still
  return strings but use the new recovering Portable parser and native
  verification; no legacy grammar shim remains.
- Core error enums are specific, non-exhaustive and implement
  `core::error::Error` without requiring `std`.

Full examples and migration guidance are in [README.md](README.md). Dialect
presets, expansion, formatting, loading and integrations remain future work,
not features of 0.2.

## [0.1.1] - 2026-10-15

### Added

- `EditError::WouldChangeMeaning`: refuse an edit that would invalidate or
  reinterpret the remaining assignments.

### Fixed

- Verify removals by re-parsing and comparing the remaining ordered
  assignments and BOM state. Deleting the first line can no longer expose an
  embedded BOM and silently rename or drop another key.
- Build `set`'s double-quoted fallback only when the first encoding fails
  contextual readback, without repeating an already quoted candidate.
- Clarify that entry spans and `encode_value` alone do not guarantee a safe
  contextual edit; use the verified `set` and `remove` operations.
- Describe encoder failures without claiming that every `.env` reader rejects
  the value, and explicitly document 0.1's permissive quoted-tail behavior.

## [0.1.0] - 2026-10-13

First release, extracted from [alint](https://github.com/asamarts/alint)'s
`.env` support.

### Added

- `parse`: literal `.env` parsing into a `Document` of entries with line
  numbers and byte spans for each line, key and value.
- `encode_value`: write a value as `.env` syntax that parses back exactly.
- `set` and `remove`: edit one assignment in place, keeping every other byte,
  and refuse a missing or duplicated key.
