# Changelog

All notable changes to this project are documented here. The format is based
on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Unreleased 0.3 literal `Dialect` configuration and `PORTABLE`, `NODE_DOTENV`,
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
