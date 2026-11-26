# adotenv-lsp

An optional Language Server Protocol companion for `adotenv` documents.
Version 0.5.1 is an unstable prototype with explicit lint configuration,
configurable parser dialect, opt-in parse and rename output limits, and
multi-BOM support. Breaking changes may occur within the 0.x line.

It supports:

- diagnostics from the recovering parser;
- optional linter findings, including key-order, key-case, quote-style and
  whitespace rules;
- completion of declared keys and `$` references;
- definition lookup for keys and `$VAR`/`${VAR}` references;
- hover information;
- references across declarations and values;
- document-symbol outlines of declared keys;
- workspace symbols across open documents;
- verified key rename from a key or reference, including literal value references.

The server reads only text sent by the editor. It does not read the process
environment or resolve references. Hover values can be masked explicitly with
`LspOptions::with_mask_values(true)`.

## Run

```console
adotenv-lsp
adotenv-lsp --help
adotenv-lsp --version
adotenv-lsp --mask-values
adotenv-lsp --dialect python
adotenv-lsp --max-input-bytes 1048576 --max-lines 10000 --max-items 5000 --max-diagnostics 500
adotenv-lsp --config .adotenv-lint.toml
adotenv-lsp --discover-lint-config
```

The binary speaks LSP over standard input and standard output, reports
`serverInfo` during `initialize`, publishes document versions with diagnostics,
accepts incremental text changes and full-document replacements, and exits with
a non-zero status if `exit` arrives before `shutdown`.

Parser diagnostics are the default. `--dialect` selects the parser preset and
overrides any `dialect` set in a lint configuration file. Linter policy is
loaded only when `--config <path>` names a file or `--discover-lint-config`
explicitly opts into ancestor discovery from the launch directory; the two
cannot be combined. There is no implicit discovery.
Invalid configuration fails visibly before the server starts. Configured
ignore globs suppress published diagnostics for matching `file://` document
URIs. Because parser diagnostics already report duplicate keys, config-driven
lint policy disables the linter duplicate-key rule unless `[rules]` explicitly
enables it.

## Library

Linter findings are opt-in through `LspOptions::with_lint_options` (and can be
cleared with `LspOptions::without_lint_options`), `LspOptions::with_dialect`
selects the parser rules used by every operation, and
`LspOptions::with_parse_options` applies opt-in `ParseOptions` bounds (bytes,
lines, items and diagnostics) to every parse.
Parser diagnostics remain enabled independently. The crate re-exports
`BomPolicy`, `Dialect`, `EditError`, `ParseOptions`, `LimitKind`,
`LimitExceeded`, `LintOptions`, `KeyCase`, `QuoteStyle`, `EqualsSpacing`,
`Rule` and `Severity` so callers can configure a policy and inspect rename
errors without a separate direct dependency.

`LintOptions::new()` includes duplicate-key findings, which overlap parser
diagnostics. Disable `duplicate_keys` when publishing both. The stdio binary
exposes policy and the configured `dialect` through `--config`,
`--discover-lint-config` and `--dialect`, but not individual rule flags;
embedding clients enable linter policy explicitly.

The operations are also available as functions over a source string and an
`lsp_types::Uri`:

```rust
use adotenv_lsp::{diagnostics, KeyCase, LintOptions, LspOptions};
use lsp_types::Uri;

let uri: Uri = "file:///tmp/.env".parse().unwrap();
let options = LspOptions::new().with_lint_options(
    LintOptions::new()
        .with_duplicate_keys(false)
        .with_key_case(KeyCase::Upper),
);
let diagnostics = diagnostics("A=1\nBAD KEY=2\n", &uri, &options);
assert!(!diagnostics.is_empty());
```

Rename uses `Document::rename_with_references_with_options` on a recovering
parse, so the declaration and every matching `$VAR`/`${VAR}` name change
together under the configured parse limits while unrelated invalid lines remain
unchanged. Definition, references and rename follow every reference candidate
the parser reports, including escaped and single-quoted candidates that
Portable resolution would not expand. This keeps rename complete when a
quote-style or dialect change later makes a candidate active; clients that need
expansion semantics can apply `Reference::is_eligible`. When references exist,
keys that cannot be written as variable references are rejected through
`EditError::InvalidReferenceKey`, and renames of references inside or after a
`$(` command substitution whose old or new key falls outside the shell-safe
allowlist fail verification with `EditError::WouldChangeMeaning`.

See the [1.8.0 and LSP stabilization plan](https://github.com/asamarts/adotenv/blob/main/docs/design/1.8.0-plan.md)
and issue [#248](https://github.com/asamarts/adotenv/issues/248) for the
remaining `0.6.0` stabilization and semver-check graduation criteria.

This crate requires Rust 1.85 because its LSP transport dependency uses the
2024 edition. The core `adotenv` crate remains `no_std`, dependency-free and
Rust 1.81 compatible.
