# Strategy: from strong core to best-in-class tooling

Status: post-0.7 review, 2026-10-20. This document is analysis and proposed
direction, not additional shipped API. The current release is 0.7.0.

## Positioning

adotenv should not compete with convenience loaders on implicit behavior. Its
differentiator is **literal, lossless, inspectable configuration work**:

- every byte retained and every token addressable;
- no implicit file, environment or command access;
- dialect-aware interpretation with pinned upstream observations;
- verified edits and formatting that preserve unrelated source;
- a small `no_std` core with optional application adapters.

That is already a strong foundation for linters, formatters, editors, migration
tools, secret scanners and CI checks. The remaining gap is not core correctness;
it is the distance between the strong document model and the everyday
application/tooling workflows built on top of it.

## Landscape

- **dotenvy** is the established convenience loader: parent-directory discovery,
  process mutation and a small API. adotenv should not copy its implicit
  behavior in the core, but should match its path from “file” to “ready
  configuration” with explicit, testable APIs.
- **dotenv-linter** shows the expected tooling surface: recursive discovery,
  targeted checks, fix/compare commands and CI integration. adotenv has the
  stronger lossless/verified-edit engine; its CLI needs the discovery and
  machine-output layer.
- **config** and **figment** are already the natural Rust integration targets.
  Their key semantics differ from literal `.env` spelling, so adapter
  documentation and typed output matter more than adding another adapter.

## What is already strong

- The flat item/entry model is simpler than a general syntax tree while still
  representing malformed input, multiline values and dialect-specific grouping.
- Span ownership, decoded-to-raw mapping, explicit dialects and strict/recovering
  parsing are unusually complete for this format.
- Verified native edits and trivia formatting are a genuine differentiator.
- Loading is all-or-nothing and does not mutate the process environment.
- Process mutation is isolated in an explicitly unsafe companion crate.
- `serde`, `config`, Figment and CLI integrations share the same literal rules.
- Property tests, conformance observations, fuzzing, MSRV checks and release
  automation are stronger than most small Rust format crates.

## Gaps and recommendations

### 1. Application-facing façade

Today an application must compose several surfaces manually:

```text
parse files / load_cascade
  -> LoadedEnvironment (literal values, origins)
  -> separate Document::resolve for references
  -> separate Document::deserialize for serde
  -> separate process::apply for environment mutation
```

There is no single, explicit object that represents the final resolved
environment for an application. This creates avoidable friction and makes it
too easy to skip origin/merge diagnostics.

Recommended direction:

- Add `LoadedEnvironment::resolve` (or a `ResolvedEnvironment`) that applies
  explicit `ExpandOptions` to the final merged values while retaining origins.
- Add `LoadedEnvironment::deserialize` behind `load + serde`, mapping final
  values to a Serde type with origins in errors.
- Keep mutation separate: resolved environments should still be applied only
  through `adotenv-process` or consumed as a map.
- Factor effective-value extraction into one core primitive so `load`, `serde`,
  `adotenv-config` and `adotenv-figment` cannot drift.
- Consider a small builder-style `LoadOptions` instead of positional
  directory/mode arguments once the API-freeze review begins.

### 2. Public API freeze and semver

The core API is large and several configuration types have public fields:

- `Dialect` has many independent switches.
- `FormatOptions`, `ExpandOptions`, `ReferenceRules` and similar policy structs
  are intended to grow.
- Adding a public field to a non-`non_exhaustive` struct is a breaking change.

Before 1.0, decide the policy deliberately:

- mark future-extensible option structs `#[non_exhaustive]` and provide
  `Default` plus `with_*` builders; or
- accept that adding fields is a minor-version event and state that explicitly.

Run `cargo-semver-checks` before every release. Use forced patch semantics while
pre-1.0 to catch accidental breakage, then review the intentional differences
against the changelog. The current release gate is documented in
[RELEASING.md](../../RELEASING.md).

### 3. Performance and scale

A provisional local sample run on the 65,536-entry fixtures (10 samples, 2-second
measurement window; not a controlled baseline) measured roughly:

- parse: ~136 ms
- portability across four presets: ~1.4 s
- sort: ~246 ms
- format: ~131 ms
- resolve: ~213 ms

These are usable for batch tooling, but they show why lookup indexing,
incremental workflows and stable regression budgets matter for an editor/LSP.
Do not treat these numbers as release thresholds; the benchmark policy in
[quality.md](quality.md#performance) governs comparison.

The parser is linear and allocation-conscious, but the public surface lacks a
lookup index. Repeated `Document::get` calls scan entries. That is fine for
editing a typical file; it can become quadratic for a language server or a
large migration tool.

Recommended direction:

- Add an optional caller-owned lookup index (or a `Document::lookup_index`)
  rather than putting hidden mutable caches inside `Document`.
- Establish benchmark baselines for parse, repeated lookup, format, sort,
  resolution and loading, with a documented machine and sample policy.
- Add a regression budget in CI only after stable numbers exist; do not turn
  noisy shared-runner measurements into false precision.
- Measure memory as well as time for multi-megabyte documents and large edit
  batches.

### 4. Tooling CLI

The CLI is intentionally small, but tool integration needs predictable machine
interfaces. Compared with mature `.env` linters, the main gaps are discovery,
machine-readable output and diagnostics presentation.

Recommended additions, in order:

1. `--stdin` and explicit file-list support for every read-only command.
2. `--recursive`, `--exclude` and glob/path discovery for `check`/`fmt`.
3. `--json` output for `check`, `diff`, `load` and `fmt --check`, with a
   versioned schema.
4. Source-snippet diagnostics with stable codes and exact spans, suitable for
   editors and CI annotations.
5. A multi-file `diff`/matrix mode for `.env`, `.env.local` and mode-specific
   cascades.
6. Optional `lint` rules (duplicates, casing, ordering, portability) kept
   separate from syntax diagnostics and configurable by code.
7. Safer Windows replacement where possible; keep the direct-write fallback
   documented until a platform-specific implementation is verified.

### 5. Integrations

The `config` and Figment adapters are a good start. Their key-normalization
differences are real and must stay explicit:

- `config` is case-insensitive and treats dots as paths;
- Figment preserves literal key spelling but path lookup still interprets dots.

Next steps:

- Add typed environment deserialization so users do not need a separate parse
  step.
- Track `config` 0.15 when the MSRV policy allows it; keep the 0.14 dependency
  justified by Rust 1.81 support.
- Document adapter version compatibility prominently on crates.io and docs.rs.

### 6. Diagnostics and errors

Diagnostics already carry stable codes, severities, primary/secondary spans and
one-based lines. The missing piece is presentation:

- a reusable renderer for source snippets, carets and secondary labels;
- structured serialization for CI and editors;
- a clear distinction between syntax errors, policy errors, portability
  warnings and application-level load/resolve failures.

Keep message wording non-contractual; make codes, spans, severity and JSON
schema the stable surface.

## Candidate milestones

### 0.8: Environment façade and API freeze preparation

- `LoadedEnvironment::resolve` with explicit options and origins.
- `LoadedEnvironment::deserialize` behind `load + serde`.
- Shared effective-value primitive used by core adapters.
- Public API inventory; decide `non_exhaustive`/builder policy for option types.
- Optional lookup index prototype and benchmark baseline.
- CLI `--stdin` and JSON diagnostics.

### 0.9: Tooling surface

- Recursive discovery and excludes.
- Multi-file diff/matrix.
- Diagnostic renderer and CI annotations.
- Windows replacement hardening.
- Typed/structured adapter output where the ecosystem supports it.
- `config` 0.15 evaluation if MSRV permits.

### 1.0: Stable contract

- Sustained fuzz campaign and accumulated weekly workflow evidence.
- At least one external production user and a documented migration story.
- Complete common-task examples for every public API family.
- Semver gate, benchmark budget and release checklist proven across a full
  cycle.
- Stable JSON/CLI contracts and documented compatibility policy.

## Non-goals

- Implicit environment mutation or parent-directory search in the core.
- Expansion or command execution during parsing.
- Guessing a value, key meaning or dialect when the source is ambiguous.
- Replacing a full shell parser; reference inspection remains explicit and
  bounded.
- Chasing every upstream dialect quirk without a recorded observation and a
  clear preset contract.

## Success measures

- A new user can go from a file to a typed, resolved application configuration
  without manually combining four APIs.
- A CI user can get stable JSON diagnostics and a nonzero exit status without
  parsing human text.
- Repeated lookups and a multi-megabyte edit batch stay within a documented
  time/memory budget.
- The public API can add a new dialect switch or formatting option without a
  breaking change, or the project explicitly accepts the semver cost.
- Weekly fuzzing, conformance and semver gates remain green across releases.
