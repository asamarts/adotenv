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

- Total: 92.65 percent regions and 92.78 percent lines.
- Strong areas: compare, diagnostics, structure, encoding, parser decode and
  editing.
- Lower areas:
  - `serde_support.rs`: 85.9 percent lines.
  - `resolve.rs`: 87.6 percent lines.
  - `load.rs`: 86.5 percent lines.
  - `format.rs`: 88.8 percent lines.
  - `references.rs`: 86.9 percent lines.
  - `parser/node.rs`: 89.8 percent lines.

## Completed expansion

- Serde covers every scalar shape, structural and value errors, duplicate and
  bare policies, records, non-string keys, loaded origins and stable display.
- Resolution covers every structured error, command errors, retained calls,
  limits and option builders.
- Docker covers raw values, comments, EOF, quoted keys, invalid boundaries,
  export and colon overrides, and scanner limits.
- Adapters cover typed cascade loading through `config` and Figment, plus
  process validation for NUL keys and values.
- Diagnostics cover every stable code, severity, message, span and secondary
  span.
- Dialect tests cover every `with_*` builder plus multiline and escape-set
  composition.
- Selected design examples are compile-checked from `tests/design_docs.rs`,
  including comparison, formatting, sorting and dialect builders.

## Remaining gaps

1. Loader error paths for invalid UTF-8, invalid modes, unreadable files and
   report-retention combinations.
2. Formatting and reference branch matrices that are still below the workspace
   average.
3. CLI error-path end-to-end cases for every command, output format and exit
   code.
4. Node and Compose parser branches that need either targeted regressions or
   scheduled fuzzing evidence.
5. A non-blocking coverage report and bounded mutation testing, tracked
   separately in issue #20.

## Suggested order

1. Loader and CLI error-path tests.
2. Formatting and reference branch tests.
3. Node and Compose regressions.
4. Coverage and mutation reporting.
