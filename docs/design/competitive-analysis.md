# Competitive analysis

This document compares adotenv with common Rust `.env` parsers and loaders. The
goal is to identify concrete work, not to rank libraries on a single number.

## Comparison set

- `adotenv` 0.10.1
- `dotenvy` 0.15.7
- `dotenv` 0.15.0
- `dotenv-compat` 0.4.0
- `dotenv-ng-core` 1.0.0
- `dotenv-parser` 0.1.3
- `dotenv-linter` 4.0.0 for CLI comparison

## Method

Benchmarks use a release build on Linux x86_64. The fixture has 65,536 simple
`KEY=value` entries and a 1.4 MB source. Each library reads the source before
measurement, parses it, and builds a map where the library supports one.

- Wall time: `hyperfine`, five process runs, multiple parses per run.
- Instructions: Valgrind callgrind, one parse, with a no-op baseline.
- Memory: `/proc/self/status` high-water mark and retained RSS, with a no-op
  baseline.
- Binary size: stripped release binary with thin LTO for a parse-and-print
  program.
- Dependencies and MSRV: crates.io metadata.

The harness is local. A committed, reproducible harness is tracked as a task.

## Results

### Parse and map build

Per parse on the 65,536-entry fixture:

| Implementation | Wall time | Instructions | Peak RSS | Retained RSS |
|---|---:|---:|---:|---:|
| adotenv parse | 57.3 ms | 236M | 27.0 MB | 27.0 MB |
| adotenv parse + map | 48.4 ms | 302M | 33.3 MB | 14.5 MB |
| dotenvy iterator + map | 87.9 ms | 356M | 19.2 MB | 11.7 MB |
| dotenv-compat parse | 64.1 ms | 267M | 20.4 MB | 14.9 MB |
| dotenv-ng input-only | 151.7 ms | 379M | 27.0 MB | 20.8 MB |
| dotenv-parser map | 154.3 ms | 1,241M | 25.7 MB | 10.8 MB |

Small-file parsing, in microseconds per parse:

| Implementation | 128 entries |
|---|---:|
| adotenv parse | 56.7 us |
| adotenv parse + map | 61.7 us |
| dotenvy iterator + map | 85.4 us |
| dotenv-compat parse | 70.4 us |
| dotenv-ng input-only | 117.1 us |
| dotenv-parser map | 252.5 us |

adotenv has the fastest wall time for map building in this fixture. It uses more
instructions than dotenv-compat because it retains spans, items and diagnostics.
Its retained document memory is higher for the same reason. The values-only
projection path is the correct comparison for applications that do not need
provenance.

### Footprint

| Crate | Required normal dependencies | MSRV | Parse-only stripped binary |
|---|---:|---:|---:|
| adotenv | 0 | 1.81 | 457 KB |
| dotenvy | 0 | 1.56 | 395 KB |
| dotenv | 0 | not declared | not measured |
| dotenv-compat | 4 | 1.85 | 376 KB |
| dotenv-ng-core | 0 | 1.74 | 414 KB |
| dotenv-parser | 2 | not declared | 460 KB |

adotenv has no required runtime dependencies and supports `no_std` with `alloc`.
Its parse-only binary is within 16 percent of the smallest measured binary.

## Capability matrix

| Capability | adotenv | dotenvy | dotenv | dotenv-compat | dotenv-ng | dotenv-parser |
|---|---|---|---|---|---|---|
| Literal parse | Yes | Yes | Yes | Yes | Yes | Yes |
| Map or iterator API | Both | Iterator | Loader | Map | Map | Map |
| Lossless source | Yes | No | No | No | No | No |
| Token spans and positions | Yes | No | No | No | No | No |
| Recovery diagnostics | Yes | Limited | No | Limited | Errors | Errors |
| Multiple syntax presets | 5 | 1 | 1 | Node | 1 | 1 |
| Verified edits and formatting | Yes | No | No | No | No | No |
| Sort, compare and examples | Yes | No | No | No | No | No |
| Reference inspection | Yes | No | No | No | No | No |
| Explicit resolution | Yes | No | No | No | Substitution | No |
| Cascade loading and provenance | Yes | Search/load | Search/load | Files | Multiple files | No |
| Values-only projection | Yes | No | No | No | No | No |
| Serde | Yes | No | No | No | No | No |
| CLI | Yes | No | No | No | No | No |
| `no_std` | Yes | No | No | No | No | No |

`dotenv-linter` is a separate CLI category. It provides lint rules and fixes but
does not expose adotenv's library model or loader provenance.

## Correctness

adotenv has a pinned conformance corpus for Node, Python, Compose and Docker
behavior. It also has parser, edit, encoding, resolution and formatting fuzz
targets.

The main gap is breadth, not absence. Add more pinned symbols and differential
checks against Node and Python, especially for malformed and multiline cases.
Keep every observation tied to a source identity and input hash.

## Documentation

adotenv has 20 public documents covering architecture, dialects, edits,
formatting, resolution, loading, serde, CLI, integrations, quality and
performance. Examples in the README are doctested.

Gaps:

- no dedicated documentation site;
- no guided tutorial or task-based examples;
- design examples are not compiled;
- no searchable API-to-task index;
- no published benchmark dashboard;
- `adotenv.github.io` is unavailable because there is no `adotenv` GitHub
  organization.

A project site can use `asamarts.github.io/adotenv` now, or a custom domain, or
an `adotenv` organization if that name can be created.

## API assessment

Strengths:

- one parse result supports inspection, edits, comparison and formatting;
- policy is explicit and composable;
- errors and diagnostics are structured;
- loading and process mutation stay opt-in;
- values, origins and reports are borrowed views.

Risks:

- the public surface is broad for new users;
- similar concepts use several types (`Document`, `Entry`, `Item`, views,
  `LoadedEnvironment`, `ResolvedEnvironment`);
- examples are spread across the README and design docs;
- the migration from parse to edit to load is not presented as a single guided
  path.

The next step is a task-based API audit, not a redesign.

## Priorities

1. Commit a reproducible comparison harness and keep these numbers current.
2. Build a documentation site with a tutorial, task guides and search.
3. Improve values-only performance and memory measurements against the same
   fixture.
4. Expand conformance and differential tests.
5. Audit common-task APIs and consolidate examples.
6. Close feature gaps only where they fit the literal, explicit design. File
   encryption, implicit environment search and ambient command execution are not
   automatic goals.
7. Complete the 1.0 evidence for fuzzing and external adoption.
