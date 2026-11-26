# Test audit

This page records test coverage at the last release and the known gaps.
Coverage is a guide, not a release gate. How the tests are organized is
described in [Quality](quality.md).

## Coverage at 1.7.3

From CI run `45774141013` (`cargo llvm-cov --workspace --all-features`) on
2026-11-26 at release commit `c5c199f` (`v1.7.3`). The core crate
(`coverage-summary.txt`) covers 94.8 percent of regions (10,859 of 11,459),
97.2 percent of functions (1,131 of 1,163) and 95.5 percent of lines (10,328 of
10,815), up from 94.7 percent of regions at 1.7.2 (CI run `45751750161`, commit
`8c1c782`) and 94.2 percent of regions / 94.9 percent of lines at 1.6.0 (CI run
`45107927491`, commit `ca38775`). Across all seven published crates
(`lcov.info`, production source files excluding tests, benchmarks and `xtask`),
line coverage is 95.5 percent (16,018 of 16,773 lines). The earlier `1.2.0`
local workspace baseline (94.4 percent of regions and 94.8 percent of lines on
2026-11-04) predates `compact.rs`, `limit.rs`, `adotenv-lint` and `adotenv-lsp`.

### Core crate

| Scope | Regions | Lines |
|---|---:|---:|
| Core crate total | 94.8% | 95.5% |
| `compare.rs`, `limit.rs`, `parser/unicode.rs` | 100.0% | 100.0% |
| `compact.rs` | 99.6% | 99.5% |
| `parser/docker.rs` | 99.4% | 99.3% |
| `example.rs` | 99.1% | 98.8% |
| `structure.rs` | 98.5% | 99.0% |
| `parser/decode.rs` | 98.5% | 99.4% |
| `position.rs` | 98.1% | 98.2% |
| `portability.rs` | 97.8% | 97.5% |
| `diagnostic.rs` | 97.7% | 97.8% |
| `dialect.rs` | 97.6% | 97.6% |
| `parser.rs` | 97.5% | 97.0% |
| `model.rs` | 97.5% | 98.4% |
| `format.rs` | 97.4% | 97.0% |
| `sort.rs` | 96.5% | 96.1% |
| `encode.rs` | 96.1% | 97.4% |
| `parser/python.rs` | 95.5% | 94.5% |
| `edit.rs` | 95.4% | 97.1% |
| `references.rs` | 94.8% | 95.3% |
| `serde_support.rs` | 94.4% | 98.3% |
| `parser/compose.rs` | 94.2% | 94.0% |
| `parser/node.rs` | 94.0% | 97.0% |
| `load.rs` | 88.6% | 89.4% |
| `exec.rs` | 88.5% | 88.8% |
| `mask.rs` | 85.1% | 87.5% |
| `resolve.rs` | 84.7% | 89.6% |

### Published companion crates

Line coverage from the workspace `lcov.info` artifact (CI run `45774141013` at
release commit `c5c199f`, following `adotenv-config 0.12.1`,
`adotenv-figment 0.12.2`, `adotenv-lint 0.2.3`, `adotenv-lsp 0.5.1` and
`1.7.0`-`1.7.3`; production source files across all seven published crates,
excluding tests, benchmarks and `xtask`):

| Scope | Lines |
|---|---:|
| Published crates total (7 crates) | 95.5% |
| `adotenv-figment/src/lib.rs` | 100.0% |
| `adotenv-process/src/lib.rs` | 99.5% |
| `adotenv-lsp/src/lib.rs` | 99.0% |
| `adotenv-config/src/lib.rs` | 98.7% |
| `adotenv-lsp/src/main.rs` | 98.5% |
| `adotenv-lint/src/config.rs` | 98.3% |
| `adotenv-lint/src/lib.rs` | 96.0% |
| `adotenv-cli/src/main.rs` | 93.2% |
| `adotenv-lint/src/main.rs` | 89.1% |

`limit.rs`, `compare.rs`, `parser/unicode.rs` and `adotenv-figment/src/lib.rs`
are at 100.0 percent line coverage, `compact.rs` and
`adotenv-process/src/lib.rs` at 99.5 percent, `parser/decode.rs` at 99.4
percent, `adotenv-lsp/src/lib.rs` at 99.0 percent, `example.rs` at 98.8
percent, `adotenv-config/src/lib.rs` at 98.7 percent, `adotenv-lsp/src/main.rs`
at 98.5 percent, `model.rs` at 98.4 percent, `adotenv-lint/src/config.rs` at
98.3 percent, `encode.rs` at 97.4 percent, `edit.rs` at 97.1 percent,
`parser.rs` at 97.0 percent, `adotenv-lint/src/lib.rs` at 96.0 percent,
`references.rs` at 95.3 percent, `adotenv-cli/src/main.rs` at 93.2 percent,
`adotenv-lint/src/main.rs` at 89.1 percent and `mask.rs` at 87.5 percent.
Across all seven published crates, line coverage at 1.7.3 is 95.5 percent
(16,018 of 16,773 lines, CI run `45774141013` at `c5c199f`), matching 95.5
percent at 1.7.2 (16,008 of 16,763 lines, CI run `45751750161` at `8c1c782`),
compared with 96.0 percent at `9d852ec` (14,853 of 15,471 lines, before
pre-1.7.0 CLI and linter I/O and platform error-handling additions), 95.3
percent at `adotenv-lsp 0.5.0` (`94835f1`), 93.3 percent at 1.6.0 (`ca38775`)
and 92.0 percent at 1.5.4 (`8a50850`). `exec.rs`, `load.rs`, `resolve.rs`,
`adotenv-cli/src/main.rs` and `adotenv-lint/src/main.rs` retain defensive I/O
and Windows-only error branches (the Windows branches are exercised by the
Windows CI job rather than Linux coverage).


## What the tests cover

- **Parsing and limits:** every preset's grammar, recovery boundaries,
  diagnostics with their codes, severities and spans, positions, BOM and
  line-ending handling, long single-line inputs, and `ParseOptions` bounds
  (`max_input_bytes`, `max_lines`, `max_items` and `max_diagnostics`).
- **Compact retention:** `CompactValues` and `CompactEntries` parsing, lookup,
  provenance/span retention, duplicate policies and portability equivalence
  against full `Document` parsing.
- **Edits:** every `Edit` variant, `EditOptions` output-byte/line/parse limits,
  insert positions, key collision rules, batches, edit ordering, verification
  failures, `rename_with_references` / `rename_with_references_with_options`,
  and `$(...)` command-context allowlist and span verification.
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
- **Linter and LSP (`adotenv-lint`, `adotenv-lsp`):** rule configuration,
  ignore/walk filters, fix ordering and atomic writes, SARIF/JSON/human
  reports, hover, go-to-definition, completion, diagnostics, BOM/UTF-16 range
  mapping, `textDocument/rename`, and stdio transport limits.
- **Adapters:** `config` and Figment typed loading (including `CompactValues`
  and `CompactEntries`), and `adotenv-process` validation and compact helpers
  (`validate_compact` / `apply_compact`).
- **End-to-end regressions:** contextual loaded resolution, Compose and
  dotenv-expand operands, and `FinalNewline::Remove` idempotence, through the
  public API.
- **Encoding:** inverse witnesses for the presets and custom dialects,
  including the Node backtick-recovery regression and `QuotedTail::Append`
  normalized LF-to-CR and CR-padding line-cap regressions.
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

On 2026-11-23, four sequential GitHub Actions `fuzz.yml` workflow runs passed
against `1.6.0` release commit `eb2cef1`, each covering all seven targets
(`parser`, `edits`, `encoding`, `resolve`, `format`, `limits` and
`differential`, including the new `EditOptions`, `CompactEntries`,
`LookupIndex::unique` and `max_lines` invariants): `45118514973`,
`45119286866`, `45119980331` and `45120661154` (28 of 28 target jobs passed,
with zero crashes and zero timeouts).

On 2026-11-25 and 2026-11-26, seven-target `fuzz.yml` workflow runs validated
the `1.7.x` reference-rename, command-context allowlist and encoding updates
(with expanded `edits` oracle assertions covering mapped reference spans,
metadata, BOM preservation, item counts, untouched values and command-context
key allowlists):

- `45699785407` (`265a0c6`, `1.7.0` readiness): 7 of 7 targets passed.
- `45717314607` (`2e201ef`, PR #227, merged as `f3247a5`), `45729274669`
  (`3e87068`, PR #230, merged as `2e49fa3`) and `45740090801` (`a11391d`,
  PR #232): 7 of 7 targets passed on each run.
- `45748121425` (`8af893f`, PR #235): `encoding` caught a real inverse-search
  gap (`crash-708e9bd4e5a920419949f79e6dd3259765347025`) where
  `QuotedTail::Append` under `normalize_newlines` and `LineBreaks::Lf` did not
  map cooked LF to raw CR in appended tails past short prefixes. PR #237
  (`de944b8`) fixed `tail_starts`/`inverse` and added regression tests.
- `45750442018` (`de944b8`, pre-`1.7.2` release gate): 7 of 7 targets passed
  with zero crashes and zero timeouts.
- `45755858881` (`b445f37`, PR #238, merged as `5067df6`, post-`1.7.2` audit
  fixes for CR-padding `line_byte_limit` budgets and tightened `$(...)`
  command-context keys): 7 of 7 targets passed with zero crashes and zero
  timeouts.
- `45771294647` (`69df45f`, PR #241, merged as `eb9462d` ahead of `v1.7.3`
  `c5c199f`, validating the removal of the subsumed `has_command_ancestor`
  helper in `src/edit.rs` and `fuzz/fuzz_targets/edits.rs`): 7 of 7 targets
  passed with zero crashes and zero timeouts.

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
- Parser-dialect shard (`parser/compose`, `docker`, `node`, `python`,
  `unicode`): 299 mutants; 246 caught, 44 missed, 6 unviable, 3 timeouts. The
  survivors are scanner index/end arithmetic and recovery guards; alternate
  scan positions and the later parser checks preserve observable behavior.
- Formatting/sorting shard (`format`, `sort`): 298 mutants; 242 caught, 6
  missed, 16 unviable, 34 timeouts. The survivors are comment-trim comparisons
  and final-CR verification guards; the timeouts are nonterminating plan/index
  variants. No observable formatting or sorting failure was found.
- Core-model shard (`model`): 307 mutants; 193 caught, 4 missed, 76 unviable,
  34 timeouts. Added empty/non-empty lookup-index assertions after the shard;
  the residual survivors are `HashWinners` capacity/probe boundaries and a
  decoded-span boundary variant.
- Loading/resolution shard (`load`, `resolve`): 453 mutants; 282 caught, 76
  missed, 73 unviable, 22 timeouts. The survivors are offset/boundary and
  accounting guards in contextual resolution, synthetic keys, path reads and
  decoded-span arithmetic; no observable loading or resolution failure was
  found.
- References shard (`references`): the 2026-11-18 local run was cancelled
  after 41 outcomes when the shared host ran out of capacity. The complete
  remote validation rerun tested **406 mutants: 205 caught, 0 missed, 59
  unviable and 142 timeouts**. A two-worker run of 2/4 hit a global OOM after
  87 outcomes when a mutant test process reached 9.6 GiB RSS; the one-worker
  rerun completed. The timeouts are nonterminating scanner, delimiter and
  bit-mask mutations; no product failure was demonstrated. See
  [heavy validation](heavy-validation.md).
- Parser-core shard (`parser`): all four remote shards completed with zero
  missed survivors:
  - 0/4: 174 mutants; 116 caught, 0 missed, 24 unviable and 34 timeouts.
  - 1/4: 174 mutants; 136 caught, 0 missed, 18 unviable and 20 timeouts.
  - 2/4: 174 mutants; 141 caught, 0 missed, 0 unviable and 33 timeouts.
  - 3/4: 173 mutants; 125 caught, 0 missed, 6 unviable and 42 timeouts.
  Across all four shards `src/parser.rs` has **695 mutants: 518 caught, 0
  missed, 48 unviable and 129 timeouts**. The timeouts are nonterminating
  portable-value, physical-line, scanner-index, quote-closing and budget-loop
  mutations; no product failure was demonstrated.
- Parser-decode shard (`parser/decode`): both remote shards (`--timeout 120`)
  tested **241 mutants: 151 caught, 66 missed, 13 unviable and 11 timeouts**
  (0/2: 84 caught, 22 missed, 12 unviable, 3 timeouts; 1/2: 67 caught, 44
  missed, 1 unviable, 8 timeouts). Added targeted tests covering Compose octal
  escape boundaries, Node `U+2028` backtick/anchor/lone-quote unquoting,
  `SingleEscapes::QuoteAndBackslash` unknown escapes,
  `DoubleEscapes::SINGLE_QUOTE`, `DropBackslash` raw-span mapping, Node vs
  Portable second-backslash rescanning, Portable warning spans/gating, and
  `QuotedTail::Append` `decode_tail` boundaries. A targeted confirmation rerun
  of shard 0/2 (102 mutants) tested **93 caught, 8 missed and 1 timeout**,
  confirming every `NodeUnquote` and `selected_escape` survivor and 10 of 11
  `replace_octal` survivors are caught; follow-up tests then closed the
  remaining `replace_octal` EOF bounds guard (`end < body_end`) and the bare-LF
  unnormalized-line check in `decode`. Residual survivors are `needs_decode`
  fast-path skip conditions, 1-byte ASCII quote advance equivalence, `D`-guard
  equivalence and defensive `body.end`/`at == 0`/tail LF guards.
- Limit shard (`limit`): initial 60s run tested 9 mutants (6 caught, 1
  unviable, 2 timeouts in `Display::fmt` caused by a passing single-threaded
  package test run taking ~75s). Added `LimitKind::as_str` and `Display`
  assertions for `LimitKind` and `LimitExceeded`; the `--timeout 120` rerun
  tested **9 mutants: 8 caught, 1 unviable, 0 missed and 0 timeouts**.
- Serde shard (`serde_support`): `--timeout 120` run tested **68 mutants: 33
  caught, 3 missed, 32 unviable and 0 timeouts**. All three survivors were
  `EntriesAccess::size_hint` (`None`, `Some(0)`, `Some(1)`); added a custom
  `MapAccess` visitor test asserting `[Some(2), Some(1), Some(0)]`. The remote
  `size_hint` confirmation rerun verified **3 of 3 caught (0 missed)**,
  leaving `serde_support` at **36 caught, 32 unviable, 0 missed and 0
  timeouts**.
- Adapter crates (`adotenv-process`, `adotenv-config`, `adotenv-figment`):
  - `adotenv-process`: **9 mutants; 5 caught, 4 missed** (all four in
    `#[cfg(windows)]` validation branches exercised by Windows CI).
  - `adotenv-config`: **10 mutants; 9 caught, 0 missed, 1 unviable**.
  - `adotenv-figment`: initial run tested 10 mutants (9 caught, 1 missed in
    `Provider::metadata`). Added metadata assertions across all constructors;
    the rerun tested **10 mutants: 10 caught, 0 missed**.

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

## Focused mutation testing for 1.6.0 additions

A targeted `--in-diff` mutation pass evaluated all **259 mutants** across the
`1.6.0` core additions (`src/compact.rs`, `src/dialect.rs`, `src/edit.rs`,
`src/format.rs`, `src/limit.rs`, `src/mask.rs`, `src/model.rs`,
`src/parser.rs` and `src/sort.rs`) in three one-worker shards (`--timeout 120`):

- 0/3: 87 mutants; 68 caught, 5 missed, 13 unviable and 1 timeout.
- 1/3: 87 mutants; 60 caught, 7 missed, 20 unviable and 0 timeouts.
- 2/3: 85 mutants; 68 caught, 5 missed, 11 unviable and 1 timeout.

Across all three shards, the initial pass recorded **196 caught, 17 missed, 44
unviable and 2 timeouts**. Twelve of the 17 survivors were genuine test gaps:

- `CompactEntries::is_empty` on empty inputs;
- `check_operation_byte_budget` exact-boundary acceptance and pre-encoding
  rejection before `finish_sets`;
- `planned_exceeds_line_limit` lone-CR handling between `LineBreaks::Lf` and
  `LineBreaks::Universal`;
- `Document::last_retained_item` BOM-prelude guards when keeping a BOM-only
  comment or uncommenting a non-BOM comment before an `End` insert under
  `BomPolicy::Strip` and `BomPolicy::StripAll`;
- Portable fast-path bypass guards in `parse_values_compact_with_options` and
  `parse_entries_compact_with_options` for `Duplicates::First` and `max_items`.

After adding regression tests in `src/compact_tests.rs` and `src/edit_tests.rs`,
a targeted confirmation rerun of all six affected functions (**36 mutants**)
verified **35 caught, 1 unviable, 0 missed and 0 timeouts**, bringing the
`1.6.0` diff to **208 caught, 5 equivalent/defensive survivors, 44 unviable
and 2 timeouts**:

- Three survivors in `Document::verify_regions` are equivalent or defensive
  BOM-span guards (`after_bom > before_bom` inside `after_bom != before_bom`,
  plus defensive start/end invariants where a subsequent span check still
  refuses cross-region BOM absorption).
- One survivor in `tree_winners` (`slot.1 > 0` to `>=`) is equivalent because
  `or_insert((index, 0))` already initialized `slot.0` to `index`.
- One survivor (`parse_simple_portable_compact -> None`) bypasses a pure
  performance fast path whose fallback produces identical output.
- Both timeouts (`end *= BOM.len()` in `BomPolicy::stripped_prefix_len` and
  `at *= 1` in `exceeds_line_limit`) are nonterminating zero-multiplication
  loop mutations.

### `adotenv-lsp/src/lib.rs` sweep (`2026-11-23`)

A full-file remote sweep of `adotenv-lsp/src/lib.rs` (**146 mutants**, one
`--in-place` worker, 8 GiB cap, 60-second timeout) initially recorded 105
caught, 28 unviable, 5 timeouts (from unbounded channel waits in the in-process
transport test) and 8 missed mutants (5 genuine test gaps and 3 equivalent
mutants). Adding targeted regression tests in `adotenv-lsp/tests/lsp.rs` (line-1
key-start completion, zero-width `apply_change` insertion, line-1 BOM-free
editing, zero-width non-BOM diagnostic range, and BOM-prefixed limit diagnostic
range) plus 2-second `recv_timeout` bounds in `adotenv-lsp/src/main.rs` closed
all 5 genuine gaps and eliminated all 5 timeouts. The confirmation run verified
**115 caught, 28 unviable, 0 timeouts and 3 equivalent survivors** (`index + 1`
to `index * 1` in `completion_with_options` where `trim_start()` strips the
leading newline, and two `bom_chars > 0` to `>= 0` guards around `+ 0` / `- 0`
in `Mapped::offset` and `Mapped::position`).

### `adotenv-lint` library sweep (`2026-11-24`)

Full-file remote sweeps of `adotenv-lint/src/lib.rs` (96 mutants) and
`adotenv-lint/src/config.rs` (74 mutants) under one `--in-place` worker, 8 GiB
cap and 60-second timeout initially recorded 100 caught, 39 unviable, 0
timeouts and 31 missed mutants (30 genuine test gaps + 1 equivalent mutant).
Adding targeted unit tests in `adotenv-lint/src/lib.rs` (`Rule::index` slots,
`Finding::message`, `FixResult::edits`, `BatchResult::is_empty`,
`LintError::fmt`, standalone `apply_fixes` sort, `QuoteStyle::{Single,Unquoted}`,
all `whitespace_differs` branches, 3-entry `key_order_findings`, duplicate-key
skip and `line_of` > 1) and `adotenv-lint/src/config.rs` (`!explicit.key_order`
guard and `IgnoredPaths::is_ignored` negative case) closed all 30 genuine gaps.
The combined confirmation sweep (**170 mutants**) verified **130 caught, 39
unviable, 0 timeouts and 1 equivalent survivor** (`group.len() > 1` to `>= 1`
in `lint`, where `group.iter().skip(1)` already yields nothing for length 1).

### Companion binary sweeps (`adotenv-lsp/src/main.rs`, `adotenv-lint/src/main.rs`, `2026-11-24`)

- `adotenv-lsp/src/main.rs` (**84 mutants**): initially 65 caught, 6 unviable,
  1 infinite-loop timeout (`index -= 3` in `decoded_uri_path`) and 12 missed
  (11 genuine gaps + 1 bitwise-equivalent `|` vs `^` in `decoded_uri_path`).
  Adding assertions for all 8 `ServerCapabilities` fields, standalone
  `--config=`, and truncated/leading percent-escapes in `decoded_uri_path`
  closed all 11 genuine gaps (confirmed on the validation host: **76 caught,
  6 unviable, 1 timeout, 1 equivalent survivor, 0 genuine gaps**).
- `adotenv-lint/src/main.rs` (**131 mutants**): initially 104 caught, 3
  unviable, 1 infinite-loop timeout (`index -= 1` in `parse_args`) and 23
  missed (10 Windows-only `#[cfg(windows)]` / `#[cfg(not(unix))]` branches
  exercised by Windows CI, 4 defensive/race-injection branches in `write_fixed`
  / `fix_file`, and 9 genuine gaps). Tightening JSON array start checks,
  testing `--max-lines` at non-zero option index, multi-byte UTF-8 filename
  truncation, recursive `walk` directory filtering, `is_ignored_dir`, and
  `Severity::Warning` output helpers closed all genuine gaps (confirmed on the
  validation host: **111 caught, 3 unviable, 2 infinite-loop timeouts, 10
  Windows-only + 5 defensive/equivalent survivors, 0 genuine gaps**).
- `adotenv-cli/src/main.rs` (**506 mutants** across shards `0/2` and `1/2`):
  initially 416 caught, 13 unviable, 9 infinite-loop timeouts and 68 missed
  (19 Windows-only `#[cfg(windows)]` / `#[cfg(not(unix))]` branches, 26
  defensive/OS-ownership/fsync/equivalent guards, and 23 genuine gaps).
  Adding targeted regression tests in `adotenv-cli/tests/cli.rs` (human
  `check` summary counts for read errors/warnings/infos, BOM line-2 `--source`
  and non-tab caret width, `diff --format human`, `diff --`, 3-file matrix
  column alignment, `load`/`run` explicit mode and `load -- -mode`, Python
  quoted-key `render_portable` rejection, and all four `set` output-limit flag
  spellings with error-message checks) closed all 23 genuine gaps (bringing
  `adotenv-cli/src/main.rs` to **439 caught, 13 unviable, 9 timeouts, 19
  Windows-only + 26 defensive/equivalent survivors, 0 genuine gaps**).

### `1.7.x` diff mutation sweeps (`2026-11-25` to `2026-11-26`)

- **`v1.6.1..1.7.0` `--in-diff` sweep (commit `5d1feab`, PR #223):** evaluated
  **326 mutants** across `adotenv`, `adotenv-cli`, `adotenv-lint` and
  `adotenv-lsp` on the remote validation host (one worker, 8 GiB cap). Added
  targeted regression tests for `Document::example` with `quoted_comments(false)`
  on CRLF/CR inputs, `Entry::references` inside `QuotedTail::Append` tails,
  unclosed `${`/`$(` nesting depth boundaries, `rename_with_references`
  unspellable keys and exact output-cap errors, `adotenv-cli` `BomPolicy`
  offsets and exact `max_input_bytes` limits, `adotenv-lint` bounded file
  reads, root separator trimming and closed-stdout/write-error exits, and
  `adotenv-lsp` CommonMark inline-code padding boundaries, closing all genuine
  test gaps.
- **`v1.7.0..5067df6` core diff sweep (`src/edit.rs` and `src/encode.rs`,
  `2026-11-26`):** evaluated all **146 mutants** across the `1.7.1`, `1.7.2`
  and PR #238 reference-rename allowlist and `QuotedTail::Append` encoding
  changes (`--in-place`, 8 GiB cap, `--lib -- edit encode`). The initial pass
  recorded 95 caught, 47 missed, 3 unviable and 1 infinite-loop timeout
  (`planned_cursor *= 1` in `verify_reference_rename`). Every
  `is_safe_command_ref_key` and command-context allowlist mutant in
  `src/edit.rs` was caught; the 13 `src/edit.rs` survivors are redundant
  `has_command_ancestor` branches subsumed by `has_command_context` (since
  removed in `eb9462d`, PR #241, shipped in `v1.7.3`), disjoint
  `<` vs `<=` span-start equivalence, and defensive verification/overlap
  guards. In `src/encode.rs`, 11 of the 34 survivors exposed a genuine test gap
  where `append = false` shadowed the `QuotedTail::Append` CR-padding
  `line_byte_limit` test. Strengthening
  `append_tail_inverse_reserves_cr_padding_byte_under_line_byte_limit` in
  `src/encode_tests.rs` (forcing `QuotedTail::Append` via `\"\\` under
  `DoubleEscapes::DOUBLE_QUOTE` + `UnknownEscapes::Error`, plus exact CRLF and
  inline-comment boundaries) closed all 11 gaps. A 34-mutant confirmation rerun
  of the `src/encode.rs` CR-padding diff verified **23 caught, 2 unviable, 0
  timeouts and 9 equivalent/defensive survivors** (`* 1` vs `/ 1` ASCII quote
  width plus 8 defensive `Closing::None` literal-fallback raw-CR guards),
  bringing the 146-mutant `1.7.x` core sweep to **106 caught, 36
  equivalent/defensive survivors, 3 unviable, 1 timeout and 0 genuine gaps**
  (the remaining 14 `tail_raw_char`/`tail_starts` survivors are oracle-checked
  split heuristics guarded by `Context::verify`).

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
progress. Follow [heavy validation](heavy-validation.md) for the resource
requirements; mutation campaigns do not run on the shared development host.

## Docs site smoke

`.github/workflows/docs.yml` builds the mdBook site and runs the headless
Chromium smoke suite (`scripts/docs-smoke/smoke.mjs`) automatically on
docs-path pull requests, docs-path pushes to `main` and manual dispatches. It
verifies nested navigation, every discovered benchmark chart and result page, chart controls and compact-vs-map
views, table sorting and filtering, selector help popovers, and mobile
overflow and popover bounds (originally established on 2026-11-15 with
`mdbook` 0.5.4 at commit `c6ae7b6`).

## Known gaps

1. CLI branches for rare rename, permission and temporary-file failures. These
   need platform error injection and are deferred.
2. Coverage figures come from the CI `coverage` workflow artifact
   (`coverage-summary.txt` and `lcov.info`). CI uploads the artifact and prints
   the core summary, but does not enforce a threshold.
3. Mutation testing is focused/sharded rather than run as one unsharded whole-workspace
   campaign; the 1.5.4 baseline listed 5,502 workspace mutants, and the
   subsequent 1.6.0, 1.6.1 and 1.7.x per-crate and `--in-diff` confirmation
   shards are recorded above. Windows-only validation branches are exercised by
   the Windows CI job rather than Linux mutation runs.
4. Every published crate in the workspace (`adotenv` core, `adotenv-process`,
   `adotenv-config`, `adotenv-figment`, `adotenv-lsp`, `adotenv-lint`, and
   `adotenv-cli`) has completed sharded or full-file mutation runs on the
   dedicated validation host, including both confirmation shards (`0/2` and
   `1/2`) of `src/parser/decode.rs`. Shard `1/2` confirmation (120 mutants, `--timeout
   120`) verified **94 caught, 1 unviable, 8 infinite-loop timeouts and 17
   missed** (14 defensive single-line `decode_tail`/custom-dialect `\r` guards,
   1 custom `LineBreaks::Lf` + `normalize_newlines` boundary, and 2 equivalent
   mutants in `decode_escape`), with 0 remaining genuine product gaps.

