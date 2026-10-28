# Test audit

This audit records current test coverage and the highest-value gaps. Coverage is
a guide, not a release gate.

## Inventory

- Core unit tests cover parsing, model, dialects, edits, encoding, formatting,
  sorting, comparison, structure, positions, references and resolution.
- Property tests cover arbitrary UTF-8, edits, encoding, comparison and
  `parse_values`.
- Public API integration tests cover values parsing, native edits and loading
  with origins and reports.
- CLI integration tests cover commands, schemas, stdin, recursion, exit codes
  and child execution.
- Adapter tests cover `adotenv-process`, `adotenv-config` and
  `adotenv-figment`.
- Conformance covers 80 pinned cases and 20 profiles.
- Six fuzz targets cover parser, edits, encoding, resolution, format and a local
  differential parser path.

## Coverage

Measured with `cargo llvm-cov --workspace --all-features --summary-only` after
the issue #23 expansion:

- Total: 95.09 percent regions and 95.45 percent lines.
- Strong areas: compare, diagnostics, parser decode, Docker, position,
  encoding, editing and serde.
- Module line coverage:
  - `position.rs`: 97.9 percent.
  - `encode.rs`: 97.5 percent.
  - `format.rs`: 95.2 percent.
  - `serde_support.rs`: 95.2 percent.
  - `parser/node.rs`: 95.2 percent.
  - `adotenv-cli`: 94.2 percent.
  - `load.rs`: 92.8 percent.
  - `references.rs`: 91.7 percent.
  - `parser/compose.rs`: 93.0 percent.
  - `resolve.rs`: 89.2 percent.

The resolution percentage includes many generic bounds, closing braces and
defensive mapping fallbacks that are not independently executable. The
behavioral error paths are covered; the metric remains below the workspace
average.

## Completed expansion

- Serde covers scalar, byte, byte-buffer, identifier and unit-struct shapes,
  structural and value errors, duplicate and bare policies, non-string keys,
  loaded origins and origins-disabled errors.
- Resolution covers every operator, every structured error, command failures,
  retained calls, limits, empty documents, bare-empty file bindings, caller
  fallback to file bindings, empty operator arguments and zero-size limits, and
  rule builders.
- Loading covers compact and full retention, origin accessors, compact values,
  resolved views, invalid modes, invalid UTF-8, read errors, report retention
  and stable error display.
- CLI integration covers usage errors, missing files, invalid documents,
  invalid modes, child startup failures, multi-file formatting, recursive
  stdin and file roots, absolute and empty excludes, symlink and non-regular
  roots, invalid baselines, matrix escaping and JSON control-character paths.
- Formatting covers every option builder, result views, conservative quote
  separation and idempotent normalization.
- References cover every rule builder, escaped containers, unclosed containers
  and empty-name containers.
- Docker covers raw values, comments, EOF, quoted keys, invalid boundaries,
  export and colon overrides, and scanner limits.
- Node covers export retry, colon spacing, cross-line quoted values, LS/PS
  preservation, quoted-key bounds and ignored malformed headers.
- Compose covers custom whitespace, export, EOF bare-key quirks, quoted keys
  and invalid boundaries.
- Adapters cover typed cascade loading through `config` and Figment, plus
  process validation for NUL keys and values.
- Diagnostics cover every stable code, severity, message, span and secondary
  span.
- Dialect tests cover every `with_*` builder plus multiline and escape-set
  composition.
- Selected design examples are compile-checked from `tests/design_docs.rs`,
  including comparison, formatting, sorting and dialect builders.

## Remaining gaps

1. Resolution metric lines that are generic bounds, closing braces or defensive
   mapping fallbacks. These were reviewed; no public API path reaches the
   defensive fallbacks without an invalid internal mapping.
2. CLI helper branches for rare rename, permission and temporary-file
   exhaustion failures. Testing these meaningfully requires platform error
   injection and is deferred.
3. A non-blocking coverage report and bounded mutation testing, tracked
   separately in issue #20.
4. Sustained scheduled fuzz evidence for the 1.0 gate.

## Suggested order

1. CLI filesystem-failure branch review.
2. Coverage and mutation reporting.
3. Complete the four scheduled fuzz runs and run the final 1.0 gate.
