# adotenv-lsp

An optional Language Server Protocol companion for `adotenv` documents.

It supports:

- diagnostics from the recovering parser;
- completion of declared keys and `$` references;
- definition lookup for keys and `$VAR`/`${VAR}` references;
- hover information;
- references across declarations and values;
- verified key rename when the key has no references.

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

The operations are also available as functions over a source string and an
`lsp_types::Uri`:

```rust,ignore
use adotenv_lsp::diagnostics;
use lsp_types::Uri;

let uri: Uri = "file:///tmp/.env".parse().unwrap();
let diagnostics = diagnostics("A=1\nBAD KEY=2\n", &uri);
```

Rename deliberately refuses a key that is referenced from a value, because the
current verified edit API cannot verify a reference rename. A later rule or LSP
release can add that operation after the edit layer has a verified reference
primitive.

This crate requires Rust 1.85 because its LSP transport dependency uses the
2024 edition. The core `adotenv` crate remains `no_std`, dependency-free and
Rust 1.81 compatible.
