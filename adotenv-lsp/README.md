# adotenv-lsp

An optional Language Server Protocol companion for `adotenv` documents.
Version 0.3.0 is published on crates.io as an unstable prototype; breaking
changes may occur within the 0.x line.

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
adotenv-lsp --mask-values
```

The binary speaks LSP over standard input and standard output. It accepts
incremental text changes and full-document replacements.

## Library

Linter findings are opt-in through `LspOptions::with_lint_options`. Parser
diagnostics remain enabled independently. The crate re-exports `LintOptions`,
`KeyCase`, `QuoteStyle`, `EqualsSpacing`, `Rule` and `Severity` so callers can
configure a policy without a separate direct dependency.

`LintOptions::new()` includes duplicate-key findings, which overlap parser
diagnostics. Disable `duplicate_keys` when publishing both. The stdio binary
does not currently expose linter flags; embedding clients enable linter policy
explicitly.

The operations are also available as functions over a source string and an
`lsp_types::Uri`:

```rust,ignore
use adotenv_lsp::{diagnostics, KeyCase, LintOptions, LspOptions};
use lsp_types::Uri;

let uri: Uri = "file:///tmp/.env".parse().unwrap();
let options = LspOptions::new().with_lint_options(
    LintOptions::new()
        .with_duplicate_keys(false)
        .with_key_case(KeyCase::Upper),
);
let diagnostics = diagnostics("A=1\nBAD KEY=2\n", &uri, &options);
```

Rename uses `Document::rename_with_references`, so the declaration and every
matching `$VAR`/`${VAR}` name change together. When references exist, keys that
cannot be written as variable references are rejected through
`EditError::InvalidReferenceKey`.

This crate requires Rust 1.85 because its LSP transport dependency uses the
2024 edition. The core `adotenv` crate remains `no_std`, dependency-free and
Rust 1.81 compatible.
