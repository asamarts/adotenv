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
| [strategy.md](docs/design/strategy.md) | Post-0.7 product/API/performance review and candidate priorities |
| [RELEASING.md](RELEASING.md) | Tag release, first-publish bootstrap and Trusted Publishing |

**Current status:** 0.2.1, 0.3.0, 0.4.0, 0.5.0, 0.6.0, 0.7.0 and 0.7.1 are
published. 0.3 implements explicit dialect configuration, all five syntax
presets, selected-dialect native verification, exact per-dialect encoding,
applicable pinned-upstream comparison and cross-dialect portability reporting.
0.4 implements literal reference inspection, opt-in variable resolution,
explicit command callbacks and an opt-in `exec` helper. 0.5 implements structure views, verified trivia formatting,
key comparison, safe example generation, blank-line collapsing, value quote
normalization and comment-aware dependency sorting. 0.6 implements file
cascade loading, serde deserialization and a separate workspace process
adapter. 0.7 publishes the workspace CLI, the process and tooling companions,
and the `config`/Figment integration adapters. Trusted Publishing is configured
for all five crates. 0.8 API work has started with
`LoadedEnvironment::deserialize`. Execution evidence is in the quality notes.

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

### After 0.7: candidate priorities

The post-0.7 strategy review proposes two candidate milestones before 1.0:

- **0.8:** a resolved/typed environment façade, shared effective-value
  extraction, API-freeze decisions and initial CLI machine output.
  `Document::effective_entries`, `LoadedEnvironment::deserialize`, CLI stdin
  and `check --json` are implemented in the unreleased tree.
- **0.9:** recursive discovery, diagnostic rendering, multi-file diffing and
  Windows write hardening.

These are proposals, not committed release dates. See
[strategy.md](docs/design/strategy.md) for the full analysis.

### 1.0: Stable API (future)

The core API is frozen and follows semver. Criteria:

- Every item through 0.9 shipped.
- Fuzzing clean for a sustained period, and the conformance corpus passing
  for every preset. A weekly/manual workflow runs bounded 300-second jobs for all
  five fuzz targets; a longer campaign is still required before 1.0.
- Complete API documentation with examples for each common task.
- At least one user outside this project.

### Later

- Semantic tokens and language-server support for editors.
- A WebAssembly playground.
- More dialect presets as users ask for them.

## Contributing to the roadmap

Open an issue to propose a feature or a dialect preset. A dialect preset
needs inputs and the expected output from the implementation it imitates.
