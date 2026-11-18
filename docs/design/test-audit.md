# Test audit

This page records test coverage at the last release and the known gaps.
Coverage is a guide, not a release gate. How the tests are organized is
described in [Quality](quality.md).

## Coverage at 1.2.0 (historical)

The current release is 1.5.4; the figures below are the last published
coverage audit and are retained as a historical baseline. From a local
`cargo llvm-cov --workspace --all-features` run on 2026-11-04 from branch
`prep-1.2.0` during the audit. The workspace total is 94.4 percent of
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
four successful workflow runs covering all targets in the current matrix
after the release commit is merged.

The first encoding campaign found a harness assertion false positive: it
demanded a standalone encoding for a token whose custom `QuotedTail::Append`
line cap left no exact quoted spelling. The harness now skips that assertion
only when a conservative lower bound proves that even the shortest quoted and
escaped form cannot fit the cap. The artifact is retained locally; no library
panic or editing failure was involved.

## Release fuzz evidence

Four workflow fuzz runs passed on release commit `bf4b393`, each covering the
then-current five targets: 41941036521, 41941908663, 41942723663 and
41943612879. The current matrix has seven targets, including `limits` and the
`differential` target.

On 2026-11-15, a local campaign ran all seven targets for five minutes each
against release commit `41b7e3d` (`v1.5.2`). All targets exited clean, with no
crash or timeout artifacts:

| Target | Executions |
|---|---:|
| `parser` | 91,939 |
| `edits` | 249,288 |
| `encoding` | 284,724 |
| `resolve` | 69,887 |
| `format` | 41,088 |
| `limits` | 6,605,509 |
| `differential` | 2,245,169 |
| **Total** | **9,587,604** |

On 2026-11-16 and 2026-11-17, an extended local campaign ran all seven targets
for ten minutes each against `98823a6` (`v1.5.4`). The differential target was
rerun after an interrupted process; the completed run is counted below. All
targets exited clean, with no crash or timeout artifacts:

| Target | Executions |
|---|---:|
| `parser` | 8,706 |
| `edits` | 416,031 |
| `encoding` | 689,355 |
| `resolve` | 13,651 |
| `format` | 29,653 |
| `limits` | 11,357,816 |
| `differential` | 2,583,867 |
| **Total** | **15,099,079** |

The differential target compares Portable value parsing with the pinned
in-memory competitors on the conservative `KEY=value` grammar and runs
malformed/recovery inputs through parser invariants where upstream semantics
differ.

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

## Mutation testing at 1.4.1

The focused campaign was rerun on the current parser/edit/format verification
surface:

```console
cargo mutants --file src/parser.rs --file src/edit.rs --file src/format.rs \
  --re 'in (merge_lone_cr|valid_key_in|verify_entry_region|same_meaning)' \
  --timeout 90 --jobs 4 --baseline skip
```

The result was **72 mutants: 60 caught, 12 equivalent survivors, 0 unviable and
0 timeouts**. The survivors are classified as follows:

- All 11 survivors in `verify_entry_region` are in its defensive suffix loop.
  Every current caller builds the region so its end equals the parsed item's
  end; the loop and its comparisons therefore cannot be reached through the
  public edit API. They remain as protection for future Compose quote forms.
- The remaining survivor is the `carriage_returns.len() < limit` boundary in
  `merge_lone_cr`. An extra carriage return admitted by the `<=` variant is
  omitted again by the later output-budget pass, so the externally observable
  diagnostics and counts are unchanged.

The campaign added direct budget/omitted-error tests for `merge_lone_cr` and a
Compose comment-suffix edit regression. The new tests make the previously
surviving budget/severity mutations fail; the equivalent survivors are recorded
here rather than hidden by changing the verification logic.

## Focused mutation testing at 1.5.4

The recent LSP completion and CLI option/discovery code was mutation-tested on
top of the extended fuzz campaign.

```console
cargo mutants --package adotenv-lsp --file adotenv-lsp/src/lib.rs \
  --re 'in (completion_with_options|inline_comment_starts|CompletionContext|name_at|reference_at)' \
  --jobs 4 --timeout 60 --baseline skip
cargo mutants --package adotenv-cli --file adotenv-cli/src/main.rs \
  --re 'in (extract_dialect|strip_options_terminator|parse_check_args|parse_fmt_args|expand_paths)' \
  --jobs 4 --timeout 120 --baseline skip
```

The LSP campaign tested 45 mutants: 43 caught, 2 equivalent misses, 0
timeouts. Both survivors are `index + 1` line-start variants whose newline is
removed by `trim_start`, so completion behavior is unchanged. The CLI campaign
tested 58 mutants: 50 caught, 1 equivalent miss, 7 timeouts. The equivalent
miss is the `set` positional guard, which cannot change a valid invocation;
the timeouts are index-increment mutants that fail to advance their loops.
The test additions cover diagnostic fields, completion metadata, underscore
references, escaped quotes, and CLI `set`/`fmt` argument boundaries.

## Full mutation campaign at 1.5.4

A single full-workspace process evaluated 883 of 5,502 mutants before the
background runner stopped: 599 caught, 154 missed, 14 timeouts and 116
unviable. The remaining work was split into file shards:

- `src/dialect.rs`, `src/compare.rs`, `src/compact.rs`: 187 mutants; 145
  caught, 8 missed, 31 unviable, 3 timeouts. Added preset-field, escape-flag,
  comparison-state and compact debug/merge tests. The compact rerun then
  tested 76 mutants with 67 caught, 5 missed, 1 unviable and 3 timeouts; the
  survivors are equivalent or allocation-size-only variants.
- `src/encode.rs`: 86 mutants; 45 caught, 41 missed. Added exact double-escape
  spelling coverage. The remaining misses are in canonical encoding,
  fallback and defensive verification paths; alternate valid spellings often
  preserve behavior.
- `src/edit.rs`: 74 mutants; 49 caught, 25 missed. The survivors are in
  internal lookup, insertion planning and verification branches that valid
  public edits do not reach.
- Safety/structure shard (`mask`, `exec`, `portability`, `diagnostic`,
  `example`, `position`, `structure`): 272 mutants; 180 caught, 17 missed, 54
  unviable, 21 timeouts. Added masked-debug, non-empty portability and
  group-comment assertions. The `mask` rerun tested 41 mutants with 36 caught
  and 5 unviable; the remaining 13 survivors are example-formatting,
  defensive IO and boundary-check variants.

The remaining survivors are classified as:

- **Encode canonical/fallback alternatives:** another quoted, raw or
  literal-fallback spelling round-trips to the same value; the verifier
  rejects any spelling that changes meaning.
- **Encode defensive predicates:** `exact_native`, `appended_body_end`,
  `verify_with_cr_padding` and the `escape` arms for `$`/`'` guard malformed
  internal states or spellings that the conventional encoder never emits.
- **Edit lookup/planning/verification:** `KeyLookup`, `Desired`, `ItemPlan`,
  `verify_entry_region`, `rewrite_span` and related branches protect the
  verified-edit path. Public edit tests cover their observable outcomes; the
  defensive `verify_entry_region` suffix loop was classified in the 1.4.1
  campaign.

No product failure was demonstrated by the survivors. Triage is complete at
the policy level: the residual encode/edit/safety survivors are retained as
documented equivalents or defensive paths, and no product change is
justified. The full campaign can be resumed in shards as new code changes
arrive.

## Mutation policy

A surviving mutant is a test-coverage question, not automatically a product
bug. Each survivor is classified as:

- observable behavior that needs a regression test;
- equivalent behavior caused by an alternate spelling, fallback or redundant
  guard;
- defensive code that valid public inputs cannot reach.

Product behavior is not changed solely to improve a mutation score. A
survivor is excluded from gate expectations only after a written rationale
and, where possible, a positive witness test. Full-workspace campaigns are
run in file or package shards so one long process cannot invalidate all
progress.

## Docs site smoke

On 2026-11-15, `mdbook` 0.5.4 built the site from commit `c6ae7b6`, and the
headless-Chromium docs smoke passed. It covered nested navigation, every
discovered benchmark chart and result page, chart controls and compact-vs-map
views, table sorting and filtering, selector help popovers, and mobile
overflow and popover bounds.

## Known gaps

1. CLI branches for rare rename, permission and temporary-file failures. These
   need platform error injection and are deferred.
2. Coverage figures come from a local workspace run. CI uploads the lcov
   artifact and prints the workspace summary, but does not publish a threshold.
3. Mutation testing is focused rather than exhaustive; the current workspace
   list is 5,502 mutants, and the partial full campaign plus file shards are
   recorded above. Windows-only validation branches are exercised by the
   Windows CI job but not by mutation runs on Linux.

