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
- optional serde, process, CLI and `config`/Figment adapters.

## Current status

The project is at 0.10.4. The 1.0 evidence set is complete: release gates,
conformance, performance, four controlled fuzz runs and the external alint
adopter. What remains is the final release gate, the 1.0 tag and publication
verification.

## Near-term work

1. Run the final 1.0 release gate, tag 1.0 and verify crates.io, docs.rs and the
   GitHub Release.
2. Keep the public API stable. Additive changes use non-exhaustive types and
   builders.
3. Decide whether the differential parser fuzz target joins the recurring suite
   in #22.
4. Start the optional linter and LSP prototypes in #14 after 1.0.
5. Expand conformance cases only when a real upstream behavior is pinned and
   reproducible.
6. Keep the core dependency-free. New I/O, process or third-party behavior
   belongs in opt-in features or companion crates.

## 1.0 criteria

- The workspace release gate passes.
- The conformance corpus passes for every preset.
- Four consecutive controlled fuzz runs pass for all five targets.
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
| [architecture.md](docs/design/architecture.md) | Document model, spans, diagnostics, edits |
| [dialects.md](docs/design/dialects.md) | Presets, policies, encoding, portability |
| [expansion.md](docs/design/expansion.md) | References, resolution, commands |
| [formatting.md](docs/design/formatting.md) | Formatting and group sorting |
| [comparison.md](docs/design/comparison.md) | Comparison and example generation |
| [loading.md](docs/design/loading.md) | Cascades, provenance, values-only loading |
| [serde.md](docs/design/serde.md) | Typed deserialization |
| [cli.md](docs/design/cli.md) | Command line tools and machine output |
| [integrations.md](docs/design/integrations.md) | `config` and Figment adapters |
| [quality.md](docs/design/quality.md) | Tests, fuzzing, conformance, MSRV |
| [test-audit.md](docs/design/test-audit.md) | Coverage and integration test gaps |
| [performance.md](docs/design/performance.md) | Benchmarks and memory |
| [benchmark-plan.md](docs/design/benchmark-plan.md) | Benchmark program and acceptance criteria |
| [competitive-analysis.md](docs/design/competitive-analysis.md) | Competitor metrics and gaps |
| [benchmark-dashboard.md](docs/design/benchmark-dashboard.md) | Reference results and engineering metrics |
| [feature-decisions.md](docs/design/feature-decisions.md) | Adopt, defer and reject decisions |
| [1.0-readiness.md](docs/design/1.0-readiness.md) | Release evidence checklist |

Published site: <https://asamarts.github.io/adotenv/>
