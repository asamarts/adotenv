# Test audit

This page records test coverage at the last release and the known gaps.
Coverage is a guide, not a release gate. How the tests are organized is
described in [Quality](quality.md).

## Coverage at 1.2.0

From a local `cargo llvm-cov --workspace --all-features` run on 2026-11-04
from branch `prep-1.2.0` during the audit. The workspace total is 94.4 percent of
regions and 94.8 percent of lines. The core crate is 94.2 percent of regions
and 94.7 percent of lines.

| Scope | Regions | Lines |
|---|---:|---:|
| Workspace total | 94.4% | 94.8% |
| Core crate total | 94.2% | 94.7% |
| `adotenv-cli/src/main.rs` | 93.8% | 94.1% |
| `adotenv-config/src/lib.rs` | 98.9% | 98.1% |
| `adotenv-figment/src/lib.rs` | 98.9% | 96.1% |
| `adotenv-process/src/lib.rs` | 98.0% | 99.1% |
| `compare.rs`, `diagnostic.rs`, `parser/unicode.rs` | 100.0% | 100.0% |
| `parser/decode.rs` | 98.9% | 99.4% |
| `format.rs` | 98.2% | 97.6% |
| `portability.rs` | 98.7% | 99.2% |
| `position.rs` | 98.4% | 97.9% |
| `parser/docker.rs` | 99.6% | 99.3% |
| `structure.rs` | 99.1% | 99.0% |
| `sort.rs` | 97.9% | 96.7% |
| `dialect.rs` | 97.4% | 97.4% |
| `references.rs` | 92.9% | 93.0% |
| `parser.rs` | 96.0% | 95.8% |
| `edit.rs` | 94.7% | 96.1% |
| `model.rs` | 96.0% | 96.3% |
| `parser/node.rs` | 93.7% | 96.5% |
| `parser/python.rs` | 95.6% | 95.2% |
| `parser/compose.rs` | 93.5% | 93.0% |
| `example.rs` | 98.3% | 98.1% |
| `encode.rs` | 96.0% | 97.2% |
| `serde_support.rs` | 94.2% | 97.3% |
| `load.rs` | 91.5% | 90.2% |
| `exec.rs` | 92.8% | 89.7% |
| `mask.rs` | 82.2% | 83.5% |
| `resolve.rs` | 85.6% | 90.3% |

`mask.rs` improved from 58 percent after the structural mask assertions;
`exec.rs`, `load.rs` and `resolve.rs` retain defensive error and platform
branches. Their behavioral paths are covered by unit and integration tests.


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
- **Encoding:** inverse witnesses for the presets and custom dialects,
  including the Node backtick-recovery regression.
- **Conformance:** 89 pinned cases across 21 upstream profiles, including
  curated Compose resolution observations.
- **Documentation:** every Rust example on this site and in the README runs as
  a doctest.

## Fuzz smoke at 1.2.0

Five local bounded campaigns ran on 2026-11-04 from `prep-1.2.0`, one per
target, with a 10-second maximum and the same memory and timeout bounds as CI:

| Target | Executions | Result |
|---|---:|---|
| parser | 3,332 | clean |
| edits | 3,981 | clean |
| encoding | 5,245 | clean after harness fix |
| resolve | 2,549 | clean |
| format | 3,654 | clean |

These are development checks, not release evidence. The release rule remains
four successful workflow runs covering all five targets after the release
commit is merged.

The first encoding campaign found a harness assertion false positive: it
demanded a standalone encoding for a token whose custom `QuotedTail::Append`
line cap left no exact quoted spelling. The harness now skips that assertion
only when a conservative lower bound proves that even the shortest quoted and
escaped form cannot fit the cap. The artifact is retained locally; no library
panic or editing failure was involved.

## Release fuzz evidence

Four workflow fuzz runs passed on release commit `bf4b393`, each covering all
five targets: 41941036521, 41941908663, 41942723663 and 41943612879.

## Mutation testing at 1.2.0

A focused mutation campaign ran on the security and limit modules changed by
1.2.0: `src/mask.rs`, `src/exec.rs`, `src/example.rs`, `src/diagnostic.rs`,
`src/serde_support.rs` and `adotenv-process/src/lib.rs`.

```console
git diff --unified=0 main -- '*.rs' > ~/.tmp/adotenv-1.2.diff
cargo mutants --workspace --all-features --in-diff ~/.tmp/adotenv-1.2.diff \
  --file src/mask.rs --file src/exec.rs --file src/example.rs \
  --file src/diagnostic.rs --file src/serde_support.rs \
  --file adotenv-process/src/lib.rs --jobs 8 --timeout 60 --baseline skip
```

The first focused run tested 87 mutants: 60 caught, 9 missed, 10 unviable and
8 timed out. The survivors were concentrated in `Mask::fmt_masked` methods with
no structural assertion and in two Windows-only validation branches. Adding
structural assertions for the loaded, resolved and serde masks plus the loaded
environment types, then rerunning `src/mask.rs` and `src/serde_support.rs`,
tested 39 mutants with 33 caught, 6 unviable and 0 missed. The four remaining
survivors are the Windows-only case-collision and value-length branches in
`adotenv-process`; the Windows CI job exercises them, while a Linux mutation
run cannot. The eight timeouts are defensive command-cleanup, interrupted-read
and example-formatting paths; they are inconclusive rather than failures.

The full workspace mutation list is 4,319 mutants; running all of it is a
release-time campaign rather than a per-change gate. The focused run targets
the new security, masking, command-limit, diagnostic and Windows validation
code; the pre-1.0 full run remains the historical broad baseline.

## Known gaps

1. CLI branches for rare rename, permission and temporary-file failures. These
   need platform error injection and are deferred.
2. Coverage figures come from a local workspace run. CI uploads the lcov
   artifact and prints the workspace summary, but does not publish a threshold.
3. Mutation testing is focused rather than exhaustive; the full 4,319-mutant
   workspace campaign is deferred to release time. Windows-only validation
   branches are exercised by the Windows CI job but not by mutation runs on
   Linux.

