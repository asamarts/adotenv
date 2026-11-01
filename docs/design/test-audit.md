# Test audit

This page records test coverage at the last release and the known gaps.
Coverage is a guide, not a release gate. How the tests are organized is
described in [Quality](quality.md).

## Coverage at 1.0.0

From the CI coverage report of the 1.0.0 release commit `c695b8d`, produced by
`cargo llvm-cov --workspace --all-features`. The summary lists the core
crate's modules:

| Scope | Regions | Lines |
|---|---:|---:|
| Core crate total | 95.5% | 96.0% |
| `compare.rs`, `diagnostic.rs` | 100.0% | 100.0% |
| `parser/decode.rs` | 99.8% | 100.0% |
| `structure.rs` | 99.1% | 99.0% |
| `portability.rs` | 98.7% | 99.2% |
| `position.rs` | 98.4% | 97.9% |
| `format.rs` | 98.3% | 97.6% |
| `references.rs` | 98.3% | 98.4% |
| `sort.rs` | 97.6% | 96.0% |
| `parser.rs` | 97.1% | 96.4% |
| `encode.rs` | 96.5% | 97.5% |
| `edit.rs` | 94.7% | 96.1% |
| `load.rs` | 94.0% | 93.2% |
| `serde_support.rs` | 92.9% | 95.2% |
| `resolve.rs` | 85.6% | 90.6% |

The resolution figure includes generic bounds, closing braces and defensive
mapping fallbacks that cannot be reached independently. Its behavioral error
paths are covered.

## What the tests cover

- **Parsing:** every preset's grammar, recovery boundaries, diagnostics with
  their codes, severities and spans, positions, BOM and line-ending handling,
  and long single-line inputs.
- **Edits:** every `Edit` variant, insert positions, key collision rules,
  batches, edit ordering and verification failures.
- **Formatting and sorting:** every option, quote changes, idempotent
  normalization, reference-preserving order and files without a final line
  ending.
- **References and resolution:** every operator, rule builder and structured
  error, command callbacks, limits including the total output budget, and
  linear time and memory for deep nesting and long `$` runs.
- **Loading:** compact and full retention, origins, reports, values-only
  loading, both resolution methods, mode names and every error.
- **Serde:** scalar, option, unit, newtype and byte shapes, structural and
  value errors, duplicate and bare policies, and loaded origins.
- **CLI:** every command, the JSON schemas, GitHub output, standard input,
  recursion and excludes, symbolic links, file permissions, exit codes and
  child processes.
- **Adapters:** `config` and Figment typed loading, and `adotenv-process`
  validation.
- **Conformance:** 80 pinned cases across 20 upstream profiles.
- **Documentation:** every Rust example on this site and in the README runs as
  a doctest.

## Known gaps

1. CLI branches for rare rename, permission and temporary-file failures. These
   need platform error injection and are deferred.
2. The coverage summary in CI lists the core crate's modules only, so the CLI
   and the adapters have no published coverage figure.
3. Mutation testing last ran before the 1.0 changes to edit verification. The
   surviving mutants then were triaged as defensive branches; the next run
   should confirm that for the 1.0 code.
