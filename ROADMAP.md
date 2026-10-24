# adotenv roadmap

adotenv aims to be the most complete, correct and pleasant `.env` library for
Rust: first for tools that read, check, format and rewrite `.env` files, and
then, built on the same core, for applications that load them.

This file states the goals, the principles every feature follows, and the
milestones to 1.0. Design detail lives in [`docs/design/`](docs/design/):

| Document | Covers |
|---|---|
| [architecture.md](docs/design/architecture.md) | The lossless document model, diagnostics, edits, `no_std` |
| [dialects.md](docs/design/dialects.md) | The default syntax, dialect switches and presets, the portability check |
| [expansion.md](docs/design/expansion.md) | `${VAR}` references, opt-in resolution, command substitution |
| [formatting.md](docs/design/formatting.md) | Comment/group structure and verified trivia formatting |
| [comparison.md](docs/design/comparison.md) | Effective-key comparison and safe example generation |
| [sorting.md](docs/design/sorting.md) | Comment-aware, dependency-respecting group sorting |
| [loading.md](docs/design/loading.md) | Opt-in file cascade loading, validation and origins |
| [serde.md](docs/design/serde.md) | Opt-in typed deserialization with line-aware errors |
| [cli.md](docs/design/cli.md) | Zero-dependency workspace command-line tools |
| [integrations.md](docs/design/integrations.md) | `config` and `figment` companion adapters |
| [quality.md](docs/design/quality.md) | Testing, fuzzing, conformance, benchmarks, MSRV and semver policy |
| [performance.md](docs/design/performance.md) | Local 0.8 benchmark methodology and baseline |
| [strategy.md](docs/design/strategy.md) | Post-0.7 product/API/performance review and candidate priorities |
| [RELEASING.md](RELEASING.md) | Tag release, first-publish bootstrap and Trusted Publishing |
| [1.0-readiness.md](docs/design/1.0-readiness.md) | Evidence checklist for the 1.0 milestone |
| [MIGRATION.md](MIGRATION.md) | User-facing migration and adoption guidance |

**Current status:** 0.2.1, 0.3.0, 0.4.0, 0.5.0, 0.6.0, 0.7.0, 0.7.1, 0.8.0,
0.9.0, 0.9.1, 0.9.2, 0.9.3, 0.9.4, 0.9.5, 0.9.6, 0.9.7, 0.9.8, 0.9.9, 0.9.10, 0.9.11, 0.9.12, 0.9.13, 0.9.14, 0.9.15 and 0.9.16 are published. 0.3 implements explicit dialect configuration, all five syntax
presets, selected-dialect native verification, exact per-dialect encoding,
applicable pinned-upstream comparison and cross-dialect portability reporting.
0.4 implements literal reference inspection, opt-in variable resolution,
explicit command callbacks and an opt-in `exec` helper. 0.5 implements structure views, verified trivia formatting,
key comparison, safe example generation, blank-line collapsing, value quote
normalization and comment-aware dependency sorting. 0.6 implements file
cascade loading, serde deserialization and a separate workspace process
adapter. 0.7 publishes the workspace CLI, the process and tooling companions,
and the `config`/Figment integration adapters. Trusted Publishing is configured
for all five crates. 0.8 adds effective-entry extraction, loaded deserialization,
decoded/contextual loaded resolution, lookup indexing, CLI stdin/JSON output and
the API-freeze policy. 0.9 adds recursive discovery and excludes, diagnostic
rendering and CI annotations, baseline-first multi-file diff/matrix output,
Windows replacement hardening, and adapter typed-usage evaluation. Execution
evidence is in the quality notes.

## Goals

1. **Lossless.** Parsing then printing reproduces any UTF-8 input byte for
   byte, including comments, blank lines, spacing, quoting, line endings, a
   BOM and lines that don't parse. Invalid encoding is rejected at the
   caller's decoding boundary, never silently replaced.
2. **Never silently wrong.** No value is guessed, truncated, expanded or
   dropped without the caller being told. Every recovery produces a diagnostic
   with an exact position.
3. **Precise.** Every token has a byte span. Position conversions use explicit
   UTF-8, scalar or UTF-16 units and reject coordinates that are out of range
   or unrepresentable in the requested encoding, never silently clamp them.
4. **Explicit about syntax.** `.env` has no standard. Portable remains the
   default; explicit presets interpret retained text under other grammars.
   Cross-dialect portability reporting remains the next 0.3 task.
5. **Safe by default.** No I/O, no environment access and no variable
   expansion unless asked for. Commands in a file are never run unless the
   caller supplies a way to run them. The core forbids unsafe code; changing
   the process environment is only offered as an `unsafe fn` in a separate,
   opt-in process adapter.
6. **Edits you can trust.** Every edit changes only what it targets and is
   checked by parsing the result before it is returned. Edits are available as
   text edits (range plus replacement) for linters and editors, or applied.
7. **Fast and small.** Linear parsing per selected dialect, no dependencies
   in the core, `no_std` with `alloc`, and published benchmark numbers.
8. **Intuitive.** The common cases are one call (`parse`, `get`, `set`); the
   detail is there when you need it.

## Principles

- Each selected dialect parses retained source into raw tokens; meaning
  (unescaping and expansion) is computed from them, so no information is lost
  by reading. Different dialects may group the same source differently.
- A parse never fails. It returns a document and a list of diagnostics; strict
  callers turn diagnostics into errors.
- Policy belongs to callers. adotenv reports what a file contains and what is
  ambiguous; deciding that a key must be uppercase or a value must be a URL is
  the job of a linter or validator built on it.
- Features that need I/O, the process environment or third-party crates are
  opt-in Cargo features or separate crates, never part of the core.

## Milestones

### 0.1 (released 2026-10-13)

Literal parsing into ordered entries with key, value and line spans;
`encode_value`; verified `set` and `remove`. Extracted from
[alint](https://github.com/asamarts/alint).

### 0.2: Lossless core (released 2026-10-15; breaking)

- Portable default syntax, including explicit diagnostics for ambiguous or
  invalid input. Syntax changes are part of the 0.2 migration, not a second
  default-syntax cutover in 0.3.
- Document model of items (`Blank`, `Comment`, `Entry`, `Invalid`) with every
  token's raw text and span; byte-identical round trip for any UTF-8 input.
- Error-recovering parse returning the document plus diagnostics with stable
  codes; `parse_strict` for callers that want the first error.
- Multi-line quoted values; bare `KEY` lines as a distinct state from `KEY=`
  and `KEY=""`.
- Spacing, comments, line endings, BOM and final-newline state in the model.
- `LineIndex`: checked offsets/positions in UTF-8, scalar and UTF-16 units;
  explicit one-based display conversion and decoded-value source mapping.
- Edits as text edits and a verified output: set, explicit item removal,
  rename, insert (end, after a key/entry, or sorted), comment, uncomment and
  toggle `export`. Every complete batch is verified by re-parsing.
- Key-only edits require unique keys; native operations also accept explicit
  occurrences and support verified edits around unrelated syntax errors.
  Automatic comment attachments remain part of 0.5.
- `#![no_std]` with `alloc`; MSRV 1.81; the repository becomes a Cargo
  workspace.
- Property tests and runnable parser/edit fuzz harnesses with synthetic public
  seeds; scheduled bounded fuzz CI and Criterion parse/native-edit workloads.
  Workload availability alone is not evidence of a completed campaign or
  measured performance.

### 0.3: Dialects (released 2026-10-17)

- **Implemented:** configurable `Dialect` and `parse_with`, `parse_owned_with`,
  `parse_strict_with`; retained configuration and borrowed `Document::reparse`.
- **Implemented:** `PORTABLE`, `NODE_DOTENV`, `PYTHON_DOTENV`, `COMPOSE` and
  `DOCKER_RAW` syntax presets. Native edits verify with the selected configuration.
  No preset enables expansion, inheritance or loading.
- **Implemented:** real, artifact-pinned upstream observation replay plus Rust
  comparison for applicable literal profiles. Resolution/loading, Node's opt-in
  fast grammar and Python's file-newline conversion are explicitly excluded.
  See [execution evidence](docs/design/quality.md#public-upstream-conformance-corpus-03-preparation).
- **Implemented:** `encode_value_with` and explicit `EncodeError`, shared with
  context-aware native set/insert. Preset/custom escape policies, raw CR padding,
  quoted tails, controls and byte budgets retain exact values. Same-line set
  batches resolve against other final values and retry under a byte cap;
  renamed/exported headers share that budget and capped next-statement insertion
  adds its required separator. The original Portable `encode_value` helper keeps
  its published control-character policy.
- **Implemented:** `Document::portability` reparses the retained text once per
  unique configuration, includes the document's own reading first, and reports
  semantic and grouping differences per physical line with ordered, inspectable
  parts. Quote style alone is not a difference; no expansion or lookup occurs.
- **Released:** 2026-10-17 after the complete quality gate.

### 0.4: References and expansion (released 2026-10-18)

- **Implemented:** `Entry::references()` for `$VAR`, `${VAR}`, every listed
  operator form and `$(command)`, with source spans, quote/escape context,
  nested containers, malformed-form metadata and appended-tail handling.
  `ReferenceRules` and `OperatorSet` provide pure eligibility decisions without
  lookup, expansion or execution.
- **Implemented:** `Document::resolve` for variable references with an explicit
  caller lookup closure, sequential or whole-file scope, caller/file precedence,
  all operators, unresolved policy, structured `:?` errors, cycle detection,
  depth/size limits and file/lookup/nested provenance. Caller values are literal.
- **Implemented:** `resolve_with_commands` with a source-aware caller callback,
  raw command text/spans, source identity, literal outputs, `CommandCall`
  provenance and `ResolveFailure` retention of prior calls. No default command
  execution exists.
- **Implemented:** an opt-in std-only `exec::Shell` helper behind the `exec`
  feature, requiring an explicit working directory or source-directory origins,
  with nonzero-status, spawn and UTF-8 output errors.
- **Released:** 2026-10-18 after the complete quality gate.

### 0.5: Formatting and structure (released 2026-10-18)

- **Implemented:** `Document::groups` for blank-line-separated item runs,
  `Document::header` for the structural prefix before the first declaration,
  and `Entry::leading_comments` for contiguous attached whole-line comments.
  Inline comments remain on tokens; blank lines break attachment.
- **Implemented:** verified trivia formatting through `Document::format`,
  `FormatOptions`, `EqualsSpacing`, `QuoteStyle`, `FinalNewline`,
  `FormatResult` and `FormatError`. Spacing around assignment separators,
  trailing spaces/tabs, value quote preference and final-newline policy are
  explicit. `BlankLines::Collapse` reduces runs of blank items to one group
  separator. The complete output is reparsed under the exact dialect and
  checked for declaration, comment and item-structure preservation. Raw values,
  multiline bodies, invalid/ignored statements, empty assigned values,
  ambiguous empty assignments and mixed lone-CR boundaries are left untouched.
- **Implemented:** `Document::compare` for effective-key comparison. Missing,
  bare, empty and assigned states, decoded values and export differences are
  distinct; duplicate policy and lookup participation follow each document.
- **Implemented:** `Document::example` removes assigned values while retaining
  comments, blank lines, keys, exports, formatting, line endings and BOM. Bare
  declarations stay bare; malformed and ignored statements become comments so
  their text cannot leak values.
- **Implemented:** `Document::sort` orders entries within each blank-line group,
  keeps leading/inline comments attached, preserves group headers/trailing
  comments, and places `${VAR}` dependencies before dependents. Duplicate/empty
  keys, malformed groups and dependency cycles are left unchanged.
- **Released:** 2026-10-18 after the complete quality gate.

### 0.6: Loading (released 2026-10-19)

- **Implemented:** opt-in `load` feature reading `.env`, `.env.local`,
  `.env.{mode}` and `.env.{mode}.local` from an explicit directory, plus
  explicit ordered `load_files`.
- **Implemented:** `LoadedEnvironment` values, file/span/line origins,
  cross-file overrides, per-file repeated keys, and skipped bare,
  nonparticipating or empty-key declarations.
- **Implemented:** all participating files are read, decoded, parsed and
  validated before any result is returned. Missing cascade files are skipped;
  malformed existing files invalidate the complete load. No process environment
  is changed.
- **Implemented:** separate workspace `adotenv-process` adapter with
  prevalidation and an explicit unsafe `apply`; returning a map remains the
  default. At the 0.6 release it was not yet published; 0.7 publishes it.
- **Implemented:** opt-in `serde` feature with `Document::deserialize`, typed
  literal values, dialect-aware key selection and line/key-aware conversion errors.
- **Released:** 2026-10-19 after the complete quality gate.

### 0.7: Integrations and tools (released 2026-10-19)

- **Implemented:** zero-dependency workspace `adotenv-cli` with `get`, `check`,
  `fmt`, `diff`, `example`, `load`, `set`, `run`, global `--dialect` selection,
  `fmt --check` and atomic Unix writes.
- **Implemented:** `run` with an environment-inheriting child process and no
  parent mutation.
- **Implemented:** `adotenv-config` and `adotenv-figment` workspace adapters,
  each with an optional `load` feature for loaded environments.
- **Released:** 2026-10-19 after the complete quality gate.
- **Patch:** 0.7.1 on 2026-10-20 adds `LoadedEnvironment::len` and cleans up
  atomic CLI temporary files on write failure.
- **Configured:** Trusted Publishing for all five crates. The bootstrap token is
  intentionally retained as a fallback for future first-time crates. See
  [RELEASING.md](RELEASING.md).

### 0.8: Environment façade and API freeze (released 2026-10-21)

- `Document::effective_entries` and `EffectiveEntry`.
- `LoadedEnvironment::deserialize`.
- `LoadedEnvironment::resolve_contextual` and `resolve_decoded`.
- `Document::lookup_index` and `LookupIndex`.
- CLI stdin and `check --json`.
- Non-exhaustive option/data/enum API-freeze policy with builders.
- Local performance baseline; no CI performance threshold yet.
- **Released:** 2026-10-21 after the complete quality gate.

### 0.9: Tooling surface (released 2026-10-21)

- Recursive discovery and excludes.
  - **Implemented:** `check --recursive`, `fmt --recursive`, repeatable
    `--exclude`, default ignored directories and symlink skipping.
- Diagnostic rendering and CI annotations.
  - **Implemented:** human line/column output and summaries, JSON v1 preserved,
    GitHub Actions annotations, and secret-safe `--source` opt-in.
- Multi-file diff/matrix.
  - **Implemented:** baseline-first multi-file comparison, matrix table, JSON
    output and unchanged two-file human output.
- Windows replacement hardening.
  - **Implemented:** same-directory temporary file, direct-replace attempt and
    backup-swap fallback with failure restoration; no unsafe or new dependencies.
- Typed/structured adapter output where the ecosystem supports it.
  - **Evaluated:** `config` typed extraction uses its native scalar coercion;
    Figment requires string fields and explicit conversion. Both paths are
    documented and tested without adding a second coercion layer.
- `config` 0.15 evaluation if MSRV permits.
  - **Evaluated:** retained 0.14. 0.15.0 changes key-normalization behavior,
    and the current 0.15 line requires Rust 1.88/edition 2024.
- **Released:** 2026-10-21 after the complete quality gate.

See [strategy.md](docs/design/strategy.md) for the full analysis.

### 0.9.7: Loader retention (released 2026-10-23)

- **Implemented:** `LoadOptions`, `load_cascade_with_options` and
  `load_files_with_options`; the full loader can retain values and origins while
  dropping raw winning tokens, reports, or both. `resolve_contextual` reports an
  error when raw tokens were dropped; `resolve_decoded` remains available.
- **Performance:** replaced per-key document scans in the full-loader merge with
  a single occurrence pass, removing effectively quadratic behavior on large
  files. The 65,536-entry fixture ranges from 83.3 MB peak / 56.6 MB retained for
  the full default loader to 38.8 MB / 13.6 MB for values-only loading.
- **Released:** 2026-10-23 after the complete quality gate.

### 0.9.8: Values projection (released 2026-10-23)

- **Implemented:** values-only loading runs the shared parser in projection
  mode, retaining only effective strings instead of a lossless document. The
  65,536-entry fixture falls from 38.8 MB peak / 13.6 MB retained RSS to 13.6
  MB / 13.6 MB with unchanged grammar, dialect, duplicate, bare-key and error
  semantics.
- **Validated:** differential property tests compare projected values and first
  error codes against the full parser across all presets and duplicate policies.
- **Released:** 2026-10-23 after the complete quality gate.

### 0.9.9: Compact lookup index (released 2026-10-23)

- **Implemented:** replaced the `LookupIndex` `BTreeMap` with a compact
  open-addressed table and a bounded-probe fallback to the ordered map. The
  65,536-entry build falls from 38.4 ms to 15.6 ms and a 64-key lookup batch
  from 8.14 µs to 1.62 µs, with slightly lower retained RSS.
- **Preserved:** Compose's EOF empty-key declaration remains directly
  addressable through `Document::get`/`get_entry`, while `LookupIndex` excludes
  empty keys by contract.
- **Released:** 2026-10-23 after the complete quality gate.

### 0.9.10: Streaming diff (released 2026-10-23)

- **Implemented:** human and JSON diff parse the baseline once and candidates
  incrementally, buffering output until all candidates validate. Matrix diff
  still retains all documents. Three 65,536-entry files fall from 95.6 MB to
  67.4 MB peak RSS with identical output.
- **Released:** 2026-10-23 after the complete quality gate.

### 0.9.11: Indexed comparison (released 2026-10-24)

- **Implemented:** `Document::compare` uses the explicit lookup index for
  nonempty keys and a direct fallback for dialect-specific empty keys. Large
  CLI diffs are no longer quadratic; three 65,536-entry files complete in about
  0.6 seconds with about 73.3 MB peak RSS while retaining the streaming-output
  contract.
- **Released:** 2026-10-24 after the complete quality gate.

### 0.9.12: Compact matrix diff (released 2026-10-24)

- **Implemented:** matrix diff extends a compact row table one candidate at a
  time instead of retaining every document. Three 65,536-entry files fall from
  119.6 MB / 141 s to 103.0 MB / 0.8 s with identical table output.
- **Released:** 2026-10-24 after the complete quality gate.

### 0.9.13: Origin retention (released 2026-10-24)

- **Implemented:** `LoadOptions::with_origins(false)` omits the final origin map
  and override reports while leaving other selected reports intact. Combined
  with `with_reports(false)`, the 65,536-entry full loader falls from 83.3 MB /
  56.6 MB peak/retained to 54.3 MB / 28.9 MB with raw tokens, or 39.0 MB / 13.6
  MB with raw tokens disabled.
- **Released:** 2026-10-24 after the complete quality gate.

### 0.9.14: Default origin residency (released 2026-10-24)

- **Implemented:** the default full loader derives final origins from retained
  entries through a lazy cache instead of storing a duplicate origin map. The
  65,536-entry fixture falls from 83.3 MB / 56.6 MB peak/retained to 76.5 MB /
  49.3 MB; with raw tokens disabled, 61.1 MB / 34.0 MB.
- **Released:** 2026-10-24 after the complete quality gate.

### 0.9.15: Compact raw tokens (released 2026-10-24)

- **Implemented:** winning raw tokens are stored in one contiguous source buffer
  with ranges instead of two owned strings plus map overhead per key. The
  65,536-entry full loader falls from 76.5 MB / 49.3 MB peak/retained to 71.8 MB
  / 44.7 MB; with origins and reports disabled it falls to 49.6 MB / 24.3 MB.
- **Released:** 2026-10-24 after the complete quality gate.

### 0.9.16: Lazy report entries (released 2026-10-24)

- **Implemented:** report entries are stored compactly and materialized lazily
  on the first `entries()` call. Final values are no longer duplicated eagerly;
  overridden-entry values and origins remain available. The 65,536-entry full
  loader falls from 71.8 MB / 44.7 MB peak/retained to 69.8 MB / 42.7 MB; with
  raw tokens disabled it falls to 59.1 MB / 31.9 MB.
- **Released:** 2026-10-24 after the complete quality gate.

### 1.0: Stable API (future)

The core API is frozen and follows semver. The live evidence checklist is
[1.0-readiness.md](docs/design/1.0-readiness.md). Criteria:

- Every item through 0.9 shipped.
- Fuzzing clean for a sustained period, and the conformance corpus passing
  for every preset. A weekly/manual workflow runs bounded 300-second jobs for all
  five fuzz targets. The 0.9.8 projection-parser refactor reset the evidence
  clock; a fresh manual 60-second-per-target campaign completed 250,705
  executions with no crashes. Four consecutive scheduled weekly runs for all
  five targets are required after the 2026-10-23 change; weekly evidence must
  continue accruing.
- A manual performance budget is recorded in [performance.md](docs/design/performance.md):
  compare an idle-host run against the controlled baseline, repeat the 1.5x–2x
  range, and block above 2x until explained. CI remains benchmark smoke only.
- Complete API documentation with examples for each common task.
- At least one external project using a published adotenv crate, with
  maintainer confirmation, version/features used and an integration or migration
  note; public attribution is optional.

### Later

- Semantic tokens and language-server support for editors.
- A WebAssembly playground.
- More dialect presets as users ask for them.

## Contributing to the roadmap

Open an issue to propose a feature or a dialect preset. A dialect preset
needs inputs and the expected output from the implementation it imitates.
