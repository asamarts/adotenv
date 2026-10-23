# Changelog

All notable changes to this project are documented here. The format is based
on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
  instead of 36.2 MB. Recorded an ecosystem comparison and ranked optimization
  backlog in [performance.md](docs/design/performance.md).

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
