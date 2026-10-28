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
the expansion below:

- Total: 94.17 percent regions and 94.42 percent lines.
- Strong areas: compare, diagnostics, encoding, parser decode, editing,
  formatting, references and parser presets.
- Lower areas:
  - `serde_support.rs`: 85.9 percent lines.
  - `resolve.rs`: 87.6 percent lines.
  - `position.rs`: 90.6 percent lines.
  - `adotenv-cli`: 90.5 percent lines.
  - `load.rs`: 92.8 percent lines.
  - `parser/compose.rs`: 93.0 percent lines.

## Completed expansion

- Serde covers every scalar shape, structural and value errors, duplicate and
  bare policies, records, non-string keys, loaded origins and stable display.
- Resolution covers every structured error, command errors, retained calls,
  limits and option builders.
- Loading covers compact and full retention, origin accessors, compact values,
  resolved views, invalid modes, invalid UTF-8, read errors, report retention
  and stable error display.
- CLI integration covers usage errors, missing files, invalid documents,
  invalid modes, child startup failures, multi-file formatting and every
  existing output contract.
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

1. Serde conversion and structural-error branches that remain below the
   workspace average.
2. Resolution branch combinations, especially command and nested-container
   error paths.
3. Position and encoding edge branches.
4. A non-blocking coverage report and bounded mutation testing, tracked
   separately in issue #20.
5. Sustained scheduled fuzz evidence for the 1.0 gate.

## Suggested order

1. Serde and resolution branch coverage.
2. Position and encoding edge branches.
3. Coverage and mutation reporting.
4. Complete the four scheduled fuzz runs and run the final 1.0 gate.
