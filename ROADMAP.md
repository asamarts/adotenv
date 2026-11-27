# Roadmap

adotenv aims to be a precise `.env` toolkit for Rust: literal parsing, lossless
source, verified edits and explicit optional loading.

## Current scope

The current release provides:

- parsing and recovery for five syntax presets;
- typed positions, diagnostics and source maps;
- verified edits, formatting, sorting, comparison and examples;
- literal reference inspection and opt-in resolution;
- optional file loading with values, origins and reports;
- optional serde, process, CLI and `config`/Figment adapters;
- the optional `adotenv-lint` policy companion;
- the optional `adotenv-lsp` language server and transport-free LSP library.

## Current status

`adotenv`, `adotenv-process` and `adotenv-cli` 1.7.3 were published on
2026-11-26 with `QuotedTail::Append` CR-padding `line_byte_limit` budget
accounting, tightened `$(...)` command-context key allowlist rules (rejecting
positional/leading-digit/dot-bounded keys and enforcing the allowlist after any
`$(` in the same entry), refreshed `1.7.x` test-audit and quality records, and
`adotenv-xtask`/release-script hardening. `adotenv`, `adotenv-process` and
`adotenv-cli` 1.7.2 were published on
2026-11-25 with a strict ASCII alphanumeric/`_`/`.` allowlist on renamed
references inside `$(...)` command substitutions, normalized LF-to-CR encoding
in `QuotedTail::Append` inverse search, generalized benchmark docs,
parameterized `adotenv-xtask` queue options, and permanent companion GitHub
Releases. `adotenv`, `adotenv-process` and `adotenv-cli` 1.7.1 were published on
2026-11-25 with exact reference-rename span and `$(...)` quote verification,
Compose empty-key unclosed `${` fixes, and benchmark/CI memory gate hardening
(including `adotenv-compact-entries`). `adotenv`, `adotenv-process` and
`adotenv-cli` 1.7.0 were published on
2026-11-25 with `Document::rename_with_references_with_options`,
`CompactValues` `PartialEq`/`Eq`, `adotenv-process::{validate_compact, apply_compact}`,
and the remaining post-1.0 and core/CLI audit fixes, alongside `adotenv-lint`
0.2.3 and `adotenv-lsp` 0.5.1 adopting `adotenv` 1.7.0 and companion audit
hardening. `adotenv`,
`adotenv-process` and `adotenv-cli` 1.6.1 were published on
2026-11-24 with `adotenv-cli` parse/output limit flags (`--max-input-bytes`,
`--max-lines`, `set --max-output-bytes`, `set --max-output-lines`) and
full-workspace mutation verification (with no core library behavior changes
relative to 1.6.0). `adotenv`, `adotenv-process` and `adotenv-cli` 1.6.0 were
published on 2026-11-23 with physical line limits (`ParseOptions::max_lines`,
`LimitKind::Lines`), multi-BOM stripping (`BomPolicy::StripAll`,
`Document::bom_span`), indexed uniqueness checks (`LookupIndex::unique`),
bounded verified edits (`EditOptions`, `EditError::Limit`,
`LimitKind::OutputBytes`, `Document::{edit,set,remove,insert,rename}_with_options`),
and bare-preserving compact entries (`CompactEntries`, `parse_entries_compact*`).
The previous published lockstep release was 1.5.4 on 2026-11-16 with the
diff-report schema and published schema URLs. `adotenv-lsp` 0.5.0 and
`adotenv-lint` 0.2.2 were published on 2026-11-23 with `adotenv` 1.6.0 parse
limits and expanded transport/config coverage (following `adotenv-lint` 0.2.1
and `adotenv-lsp` 0.4.2 on 2026-11-15). The previous
lockstep releases were 1.5.3 on 2026-11-16 with package-safe integration tests,
1.5.2 on 2026-11-15 with CLI help-text corrections, and 1.5.1 on 2026-11-15
with companion-tooling fixes; 1.5.0 on 2026-11-12 added the compact values API,
and `adotenv-lint` 0.2.0 / `adotenv-lsp` 0.4.0 were published on 2026-11-11.
`adotenv-config` is at 0.12.1 and `adotenv-figment` at 0.12.2 (both published on
2026-11-25 with `CompactValues` and `CompactEntries` constructors). A pre-release audit of 0.10.4 found
defects in behavior that 1.0 would freeze. They were fixed in 0.11.0, apart
from the rename collision rule, which shipped in 1.0.0. The fixes are listed in
[1.0-fix-round.md](docs/design/1.0-fix-round.md), and the 1.0 evidence was
collected again on the fixed code
([1.0-readiness.md](docs/design/1.0-readiness.md)). The 1.1 audit and its
fixes are in [1.1-audit.md](docs/design/1.1-audit.md). Version 1.2.0 was
released on 2026-11-04; version 1.3.0 was released on 2026-11-06. Version 1.4.0
was released on 2026-11-07 and adds the verified reference-rename primitive.
Version 1.4.1 was released on 2026-11-11 with the benchmark and hardening
cycle. Version 1.5.0 was released on 2026-11-12 with the compact values API;
version 1.5.1 followed on 2026-11-15 with companion-tooling fixes, 1.5.2
followed the same day with CLI help-text corrections, 1.5.3 on 2026-11-16 made
the published integration tests package-safe, and 1.5.4 added the diff-report
schema and published schema URLs. The audit, release evidence and remaining
companion work are recorded there.

## Near-term work

1. Keep the public API stable. Additive changes use non-exhaustive types and
   builders.
2. Keep the completed [1.1 audit](docs/design/1.1-audit.md) fixes and their
   regression coverage intact.
3. Keep the completed "After 1.0" fixes in
   [1.0-fix-round.md](docs/design/1.0-fix-round.md) and their regression
   coverage intact.
4. The optional `adotenv-lint` 0.1.0 policy companion was published on
   2026-11-06, followed by 0.1.1 through crates.io Trusted Publishing. The
   `adotenv-lsp` 0.1.0 was published on 2026-11-07, followed by 0.2.0,
   0.2.1 and 0.3.0 on 2026-11-08. The 0.3 line adds workspace symbols for open
   documents; linter 0.2.0 and LSP 0.4.0 were published on 2026-11-11 with
   shared configuration policy, followed by linter 0.2.1 and LSP 0.4.1 on
   2026-11-15, LSP 0.4.2 the same day, LSP 0.5.0 and linter 0.2.2 on
   2026-11-23 with `adotenv` 1.6.0 parse limits, and LSP 0.5.1 and linter 0.2.3
   on 2026-11-25 with `adotenv` 1.7.0 bounded reference renames and companion
   hardening. See
   [#14](https://github.com/asamarts/adotenv/issues/14).
5. Expand conformance cases only when a real upstream behavior is pinned and
   reproducible.
6. Keep the core dependency-free. New I/O, process or third-party behavior
   belongs in opt-in features or companion crates.
7. Follow the shared format contract in
   [format-contract.md](docs/design/format-contract.md), which adotenv and ayaml are
   adopting. The shared vectors and check-report schema live in `tests/vectors/`. The core
   stays dependency-free and keeps its own types; shared code, if any, is only for companion
   crates.
8. The [1.4.1 / linter 0.2.0 / LSP 0.4.0 plan](docs/design/1.4.1-plan.md) is
   complete; retain its verified release evidence as history.
9. Maintain the compact values API. Compact parsing/loading and compact memory
   now have deterministic gates, and compact speed evidence is published. The
   1.4.1 map-based claim remains historical release evidence.
10. Execute the [1.8.0 and LSP stabilization plan](docs/design/1.8.0-plan.md):
    graduate `adotenv-lsp` to a semver-checked `0.6.0` release
    ([#248](https://github.com/asamarts/adotenv/issues/248)), ship additive
    `1.8.0` core capabilities (`CompactEntries` loaders, bounded
    `format`/`sort`/edit helpers, `LineIndex::with_bom_policy`, and the
    remaining After-1.0 edge cases;
    [#249](https://github.com/asamarts/adotenv/issues/249),
    [#250](https://github.com/asamarts/adotenv/issues/250)), and follow up with
    `adotenv-lint 0.3.0` ([#251](https://github.com/asamarts/adotenv/issues/251)).

## 1.0 criteria

All met for 1.0.0:

- The workspace release gate passes.
- The conformance corpus passes for every preset.
- Four consecutive scheduled fuzz runs pass for all five targets.
- A manual performance run is recorded and reviewed.
- At least one external project confirms use of a published crate.
- CLI schemas and library contracts are documented and frozen.

## Non-goals

- Implicit environment inheritance.
- Variable expansion during parsing.
- Command execution during parsing.
- A language server in the core crate.
- A WebAssembly playground in the core crate.

## Documentation

| Document | Content |
|---|---|
| [getting-started.md](docs/guide/getting-started.md) | Installation and first steps |
| [parsing.md](docs/guide/parsing.md) | Parsing, lookup, raw tokens, positions |
| [editing.md](docs/guide/editing.md) | Verified edits, batches, encoding |
| [process.md](docs/guide/process.md) | Applying values to the process environment |
| [diagnostics.md](docs/reference/diagnostics.md) | Stable diagnostic codes |
| [security.md](docs/reference/security.md) | Untrusted input, secrets, commands, writes |
| [upgrading.md](docs/reference/upgrading.md) | Migration from 0.x |
| [architecture.md](docs/design/architecture.md) | Layers, document model, verification |
| [dialects.md](docs/design/dialects.md) | Presets, policies, encoding, portability |
| [expansion.md](docs/design/expansion.md) | References, resolution, commands |
| [formatting.md](docs/design/formatting.md) | Formatting and group sorting |
| [comparison.md](docs/design/comparison.md) | Comparison and example generation |
| [loading.md](docs/design/loading.md) | Cascades, provenance, values-only loading |
| [serde.md](docs/design/serde.md) | Typed deserialization |
| [cli.md](docs/design/cli.md) | Command line tools and machine output |
| [integrations.md](docs/design/integrations.md) | `config` and Figment adapters |
| [quality.md](docs/design/quality.md) | Tests, CI, fuzzing, conformance, MSRV |
| [test-audit.md](docs/design/test-audit.md) | Coverage and known test gaps |
| [performance.md](docs/design/performance.md) | Benchmarks, memory and budgets |
| [feature-decisions.md](docs/design/feature-decisions.md) | Adopt, defer and reject decisions |
| [format-contract.md](docs/design/format-contract.md) | Shared public shapes with ayaml |
| [lsp.md](docs/design/lsp.md) | Language server design, protocol and transport |
| [1.0-readiness.md](docs/design/1.0-readiness.md) | Release evidence checklist |
| [1.0-fix-round.md](docs/design/1.0-fix-round.md) | Pre-1.0 audit fixes and decisions |
| [1.1-audit.md](docs/design/1.1-audit.md) | 1.1 audit findings and fixes |
| [1.4.1-plan.md](docs/design/1.4.1-plan.md) | 1.4.1 benchmark and hardening plan |
| [1.5.0-plan.md](docs/design/1.5.0-plan.md) | 1.5.0 compact values plan |
| [1.6.0-plan.md](docs/design/1.6.0-plan.md) | 1.6.0 limits, edits and compact entries plan |
| [1.8.0-plan.md](docs/design/1.8.0-plan.md) | 1.8.0 core and LSP stabilization plan |
| [releases.md](docs/design/releases.md) | Release history and plan links |
| [heavy-validation.md](docs/design/heavy-validation.md) | Resource policy for heavy validation runs |

Published site: <https://asamarts.github.io/adotenv/>
