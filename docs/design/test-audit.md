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

Measured with `cargo llvm-cov --workspace --all-features --summary-only`:

- Total: about 90 percent region coverage and 90 percent line coverage.
- Strong areas: compare, property paths, structure, encoding, parser decode and
  editing.
- Lower areas:
  - `serde_support.rs`: about 55 percent lines.
  - `parser/docker.rs`: about 78 percent lines.
  - `resolve.rs`: about 80 percent lines.
  - `diagnostic.rs`: about 66 percent lines.
  - `dialect.rs`: about 27 percent lines, mostly trivial policy setters.

## Highest-value gaps

1. Serde error paths:
   - unsupported nested maps and sequences;
   - enum layouts;
   - invalid scalar conversion;
   - duplicate and bare declarations under every policy;
   - origin line reporting for loaded environments.
2. Resolution error paths:
   - each structured `ResolveError` variant;
   - command failure and call retention;
   - depth, size and required-variable limits;
   - caller-first versus file-first cycles.
3. Docker parser branches:
   - malformed raw lines;
   - trailing CR and EOF cases;
   - scanner byte limits and key boundaries.
4. Adapter end-to-end paths:
   - `load` plus `config` and Figment constructors;
   - process validation with repeated or unusual keys;
   - loaded environment to serde to process application.
5. Diagnostic accessors:
   - secondary spans;
   - severity and stable code accessors;
   - one-based line and message access.
6. Dialect builder matrix:
   - every `with_*` policy switch and reparse equivalence;
   - combinations that are easy to regress.
7. Public documentation examples:
   - compile-check selected design-doc snippets, not only the README.

## Other improvements

- Add a non-blocking coverage report to CI.
- Consider mutation testing for parser and edit policy branches.
- Add a daily or weekly code audit for `cargo audit`.
- Keep Windows path and write-fallback jobs in the required matrix.

## Suggested order

1. Serde and resolution error-path tests.
2. Adapter and cross-feature integration tests.
3. Docker parser regression cases.
4. Diagnostic and dialect accessor tests.
5. Coverage and mutation reporting.
