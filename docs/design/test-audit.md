# Test audit

This page records test coverage at the last release and the known gaps.
Coverage is a guide, not a release gate. How the tests are organized is
described in [Quality](quality.md).

## Coverage at 1.1.1

From a local `cargo llvm-cov --workspace --all-features` run on 2026-11-02
from release commit `bc0106e`. CI uploads the same lcov data as an artifact.
The workspace total is 95.14 percent of regions and 95.42 percent of lines.
The core crate is 94.96 percent of regions and 95.27 percent of lines.

| Scope | Regions | Lines |
|---|---:|---:|
| Workspace total | 95.1% | 95.4% |
| Core crate total | 95.0% | 95.3% |
| `adotenv-cli/src/main.rs` | 94.3% | 94.4% |
| `adotenv-config/src/lib.rs` | 98.6% | 97.7% |
| `adotenv-figment/src/lib.rs` | 98.9% | 95.8% |
| `adotenv-process/src/lib.rs` | 97.7% | 99.0% |
| `compare.rs`, `diagnostic.rs` | 100.0% | 100.0% |
| `parser/unicode.rs` | 100.0% | 100.0% |
| `parser/decode.rs` | 99.8% | 100.0% |
| `structure.rs` | 99.1% | 99.0% |
| `portability.rs` | 98.7% | 99.2% |
| `position.rs` | 98.4% | 97.9% |
| `format.rs` | 98.0% | 97.3% |
| `sort.rs` | 97.9% | 96.7% |
| `dialect.rs` | 97.4% | 97.4% |
| `exec.rs` | 97.2% | 97.2% |
| `example.rs` | 96.8% | 93.4% |
| `parser.rs` | 96.6% | 96.1% |
| `encode.rs` | 96.3% | 97.2% |
| `model.rs` | 96.0% | 96.4% |
| `references.rs` | 95.6% | 96.1% |
| `parser/python.rs` | 95.6% | 95.2% |
| `parser/docker.rs` | 99.6% | 99.3% |
| `edit.rs` | 94.7% | 96.1% |
| `parser/node.rs` | 94.4% | 97.0% |
| `parser/compose.rs` | 93.5% | 93.0% |
| `serde_support.rs` | 92.9% | 95.2% |
| `load.rs` | 91.7% | 90.4% |
| `resolve.rs` | 85.5% | 89.9% |

The resolution figure includes generic bounds, closing braces and defensive
mapping fallbacks that cannot be reached independently. Its behavioral error
paths are covered. `load.rs` and `references.rs` lost some percentage points
to the new contextual-resolution and operand-grammar code; the new branches
are exercised by unit and integration tests, including the failure cases.

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
- **End-to-end regressions:** contextual loaded resolution, Compose and
  dotenv-expand operands, and `FinalNewline::Remove` idempotence, through the
  public API.
- **Conformance:** 80 pinned cases across 20 upstream profiles.
- **Documentation:** every Rust example on this site and in the README runs as
  a doctest.

## Known gaps

1. CLI branches for rare rename, permission and temporary-file failures. These
   need platform error injection and are deferred.
2. Coverage figures come from a local workspace run. CI uploads the lcov
   artifact and prints the workspace summary, but does not publish a threshold.
3. Mutation testing last ran before the 1.0 changes to edit verification. The
   surviving mutants then were triaged as defensive branches; the next run
   should confirm that for the 1.1 code.
