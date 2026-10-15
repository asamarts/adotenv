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
| [quality.md](docs/design/quality.md) | Testing, fuzzing, conformance, benchmarks, MSRV and semver policy |

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
4. **Explicit about dialects.** `.env` has no standard, and common
   implementations read the same line differently. adotenv defines a portable
   default, can read the way other implementations do, and can point out lines
   whose meaning depends on which tool reads them.
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

### 0.2: Lossless core (breaking)

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
- `LineIndex`: offsets to line and column in UTF-8 and UTF-16.
- Edits as text edits: set, remove (optionally with explicitly selected
  comments), rename, insert (at end, after a key, or in sorted position),
  comment, uncomment and toggle `export`. Every edit verified by re-parsing.
- Key-only edits require unique keys; native operations also accept explicit
  occurrences and support verified edits around unrelated syntax errors.
  Automatic comment attachments remain part of 0.5.
- `#![no_std]` with `alloc`; MSRV 1.81; the repository becomes a Cargo
  workspace.
- Fuzzing and property tests for the round trip and for edits.

### 0.3: Dialects

- Configurable dialect switches and presets that read like Docker Compose,
  the Node `dotenv` package, `docker run --env-file` and `python-dotenv`,
  alongside the Portable default introduced in 0.2.
- Encoding per dialect, with an error for values a dialect cannot represent.
- A portability check: parse under several dialects and report every line
  whose meaning differs.
- A public conformance corpus with the expected result for each preset.

### 0.4: References and expansion

- `references()`: every `$VAR`, `${VAR}`, `${VAR:-default}`-style reference
  and `$(command)` with spans, operator and default; nothing resolved.
- `resolve()`: opt-in expansion against a lookup the caller provides, with a
  choice of operator set, lookup order and scope, unresolved-variable policy,
  structured `:?` errors, cycle detection, size and depth limits, and the
  origin of every resolved value.
- Command substitution through a source-aware caller-supplied callback only;
  an `exec::shell` callback for trusted files behind the `exec` feature,
  requiring explicit execution-directory/origin context.

### 0.5: Formatting and structure

- Comments attached to the entries they document (leading block, inline
  comment), a file header, and groups separated by blank lines.
- Formatter building blocks: spacing around `=`, trailing whitespace, blank
  lines, final newline, quoting changes that keep every value identical, and
  sorting within groups that keeps comments with their keys and respects
  `${VAR}` dependencies.
- Comparing files by key (missing, extra, empty versus unset), and generating
  an example file with values blanked and comments kept.

### 0.6: Loading

- Reading files and the `.env`, `.env.local`, `.env.{mode}` cascade, behind a
  `load` feature.
- Every value with its origin (file and span); a report of overridden,
  repeated and skipped keys.
- All files are parsed and validated before process-environment changes are
  applied. A broken file leaves the environment unchanged; side effects of
  explicitly supplied command callbacks cannot be rolled back.
- Applying to the process environment through an `unsafe fn` in a separate,
  opt-in std-only `adotenv-process` adapter; returning a map is the default.
- A `serde` feature: deserialize a document into a struct, with errors that
  point at the line.

### 0.7: Integrations and tools

- `adotenv-cli`: `get`, `set`, `fmt`, `check`, `diff` and `run`, with
  `--allow-commands` for trusted files.
- `adotenv-config` (a `config` crate source) and `adotenv-figment` (a figment
  provider).

### 1.0: Stable API

The core API is frozen and follows semver. Criteria:

- Every item through 0.7 shipped.
- Fuzzing clean for a sustained period, and the conformance corpus passing
  for every preset.
- Complete API documentation with examples for each common task.
- At least one user outside this project.

### Later

- Semantic tokens and language-server support for editors.
- A WebAssembly playground.
- More dialect presets as users ask for them.

## Contributing to the roadmap

Open an issue to propose a feature or a dialect preset. A dialect preset
needs inputs and the expected output from the implementation it imitates.
