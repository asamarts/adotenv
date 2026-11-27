# Release history

| Version | Date | Focus |
|---|---|---|
| 1.8.0 | 2026-11-27 | `CompactEntries` disk loaders and process helpers, bounded `format`/`sort`/edit `*_with_options` (`EditOptions::max_references`), `LineIndex::with_bom_policy`, Serde scalar map keys, Compose `example`/LF-format fixes, and `adotenv-cli` compact/bounded wiring |
| adotenv-lsp 0.6.0 | 2026-11-27 | Semver-checked API stabilization (`ParsedDocument`, `Session`, unified `&LspOptions`), bounded stdio transport, `stdio` feature, `prepareRename`/`formatting`/`codeAction`/`positionEncoding`, and Rust 1.81 MSRV |
| 1.7.3 | 2026-11-26 | `QuotedTail::Append` CR-padding `line_byte_limit` fix, tightened `$(...)` command-context key allowlist and post-`$(` enforcement, `test-audit`/`quality` refresh, and `xtask`/release-script hardening |
| 1.7.2 | 2026-11-25 | Strict `$(...)` command-reference rename allowlist, `QuotedTail::Append` LF-to-CR encode fix, generalized benchmark docs, parameterized `xtask`, and permanent companion GitHub Releases |
| 1.7.1 | 2026-11-25 | Reference-rename span and `$(...)` quote verification, Compose empty-key unclosed `${` fix, and benchmark/CI memory gate hardening |
| adotenv-lint 0.2.3 | 2026-11-25 | `adotenv` 1.7.0 adoption, multi-file error reporting, bounded reads, stdout error handling, and symlink/dedup hardening |
| adotenv-lsp 0.5.1 | 2026-11-25 | `adotenv` 1.7.0 `rename_with_references_with_options` bounds, Markdown hover escaping, BOM comment completion fix, and `serde_json` `"1"` |
| 1.7.0 | 2026-11-25 | `Document::rename_with_references_with_options`, `CompactValues` equality and process support, and post-1.0/audit fixes |
| adotenv-config 0.12.1 | 2026-11-25 | `CompactValues` and `CompactEntries` constructors (`adotenv` 1.6.1) |
| adotenv-figment 0.12.2 | 2026-11-25 | `CompactValues` and `CompactEntries` constructors (`adotenv` 1.6.1) |
| 1.6.1 | 2026-11-24 | CLI parse/output limit flags and full-workspace mutation verification; no core library behavior change |
| adotenv-lint 0.2.2 | 2026-11-23 | `adotenv` 1.6.0 adoption, CLI parse-limit flags and expanded coverage |
| adotenv-lsp 0.5.0 | 2026-11-23 | `adotenv` 1.6.0 parse limits, multi-BOM/keep mapping and transport tests |
| 1.6.0 | 2026-11-23 | Line limits, `BomPolicy::StripAll`, `LookupIndex::unique`, `EditOptions` and `CompactEntries` |
| 1.5.4 | 2026-11-16 | Diff-report schema and published schema URLs; no library behavior change |
| 1.5.3 | 2026-11-16 | Package-safe integration tests; no library behavior change |
| 1.5.2 | 2026-11-15 | CLI help-text corrections; no library behavior change |
| 1.5.1 | 2026-11-15 | CLI, companion-tooling and release-evidence fixes |
| adotenv-lint 0.2.1 | 2026-11-15 | Ignore, severity, whitespace and key-order fixes |
| adotenv-lsp 0.4.2 | 2026-11-15 | Dialect-aware completion context fixes |
| adotenv-lsp 0.4.1 | 2026-11-15 | Dialect, URI-ignore and editor behavior fixes |
| 1.5.0 | 2026-11-12 | Compact values API, compact memory and post-release speed evidence |
| 1.4.1 | 2026-11-11 | Competitive benchmarks, hardening and release provenance |
| 1.4.0 | 2026-11-07 | Verified reference-aware rename |
| 1.3.0 | 2026-11-06 | Linter and LSP additions |
| 1.2.0 | 2026-11-04 | Loading and adapter work |
| 1.1.2 | 2026-11-02 | Patch fixes |
| 1.1.1 | 2026-11-02 | Audit follow-up fixes |
| 1.1.0 | 2026-11-01 | Audit fixes |
| 1.0.1 | 2026-11-01 | First stable patch |
| 1.0.0 | 2026-10-31 | Stable API |

Current release evidence is on the [benchmark overview](../benchmarks/README.md).
Release plans and audit records:

- [1.8.0 and LSP stabilization plan](1.8.0-plan.md)
- [1.6.0 plan](1.6.0-plan.md)
- [1.5.0 plan](1.5.0-plan.md)
- [1.4.1 plan](1.4.1-plan.md)
- [1.0 readiness](1.0-readiness.md)
- [1.0 fix round](1.0-fix-round.md)
- [1.1 audit](1.1-audit.md)
