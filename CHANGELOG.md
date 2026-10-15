# Changelog

All notable changes to this project are documented here. The format is based
on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
  avoid quadratic collision scans and insertion-vector shifts in large batches.
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
