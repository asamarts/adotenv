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
- the optional `adotenv-lsp` editor companion prototype.

## Current status

`adotenv`, `adotenv-process` and `adotenv-cli` 1.5.0 were published on
2026-11-12 with the compact values API. `adotenv-lint` 0.2.0 and
`adotenv-lsp` 0.4.0 were published on 2026-11-11. `adotenv-config` is at
0.12.0 and `adotenv-figment` at 0.12.1. A pre-release audit of 0.10.4 found
defects in behavior that 1.0 would freeze. They were fixed in 0.11.0, apart
from the rename collision rule, which shipped in 1.0.0. The fixes are listed in
[1.0-fix-round.md](docs/design/1.0-fix-round.md), and the 1.0 evidence was
collected again on the fixed code
([1.0-readiness.md](docs/design/1.0-readiness.md)). The 1.1 audit and its
fixes are in [1.1-audit.md](docs/design/1.1-audit.md). Version 1.2.0 was
released on 2026-11-05; version 1.3.0 was released on 2026-11-06. Version 1.4.0
was released on 2026-11-07 and adds the verified reference-rename primitive.
Version 1.4.1 was released on 2026-11-11 with the benchmark and hardening
cycle. Version 1.5.0 was released on 2026-11-12 with the compact values API.
The audit, release evidence and remaining companion work are recorded there.

## Near-term work

1. Keep the public API stable. Additive changes use non-exhaustive types and
   builders.
2. Work through the 1.1 audit in
   [1.1-audit.md](docs/design/1.1-audit.md). The 1.1.1 correctness fixes come
   first.
3. Work through the "After 1.0" list in
   [1.0-fix-round.md](docs/design/1.0-fix-round.md).
4. The optional `adotenv-lint` 0.1.0 policy companion was published on
   2026-11-06, followed by 0.1.1 through crates.io Trusted Publishing. The
   `adotenv-lsp` 0.1.0 was published on 2026-11-07, followed by 0.2.0,
   0.2.1 and 0.3.0 on 2026-11-08. The 0.3 line adds workspace symbols for open
   documents; linter 0.2.0 and LSP 0.4.0 were published on 2026-11-11 with
   shared configuration policy. See
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
8. Complete the [1.4.1 / linter 0.2.0 / LSP 0.4.0 plan](docs/design/1.4.1-plan.md):
   all three releases are published; retain the verified release evidence.
9. Maintain the compact values API. Compact parsing/loading and compact memory
   now have deterministic gates; decide separately whether to add a compact
   speed claim. The 1.4.1 map-based claim remains historical release evidence.

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
| [lsp.md](docs/design/lsp.md) | Language server prototype and limitations |
| [1.0-readiness.md](docs/design/1.0-readiness.md) | Release evidence checklist |
| [1.0-fix-round.md](docs/design/1.0-fix-round.md) | Pre-1.0 audit fixes and decisions |

Published site: <https://asamarts.github.io/adotenv/>
