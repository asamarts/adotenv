# Language server

`adotenv-lsp` is an optional companion crate for editor integration. It keeps
editor protocol work out of the parser and reuses the core document model,
spans, diagnostics and verified edits.

## Operations

The prototype offers:

- diagnostics from the recovering parser, including related locations for
  duplicate keys;
- optional linter findings when the caller supplies an explicit
  `LintOptions` policy;
- completion of declared keys at a key position or after `$`/`${`, with
  triggers for `$` and `{`;
- definition from a key or a literal `$VAR`/`${VAR}` reference to the winning
  declaration;
- hover for a key or reference, with optional value masking;
- references across declarations and values;
- rename of a key declaration through `Document::rename` and its
  `VerifiedEdits`.

Rename refuses a key that is referenced from a value. The core edit API cannot
yet verify a reference rename, so changing only the declaration would change
meaning. The refusal is explicit and tested.

## Diagnostics, masking and environment

Parser diagnostics are always enabled. Linter findings are an explicit policy:
`LspOptions::with_lint_options` accepts the linter's own `LintOptions` rule and
severity policy. The default `LspOptions` does not run the linter, so
diagnostics do not silently depend on a policy configuration. The stdio binary
does not expose linter flags yet; embedding clients opt in.

`LintOptions::new()` includes duplicate-key findings, which overlap the parser
diagnostics. Callers publishing both should disable the linter duplicate rule.

Hover values are visible by default. `LspOptions::with_mask_values(true)` or
the binary's `--mask-values` flag replaces them with `<redacted>`.

The server reads only text the editor sends. It does not read the process
environment, resolve references or run commands. A future completion source for
environment values must be explicit and opt-in.

## Transport

The binary is a synchronous LSP server over standard input and output. It
uses UTF-16 incremental document synchronization, accepts full-document
replacements, publishes diagnostics after `didOpen` and `didChange`, and clears
them on `didClose`.

The library functions are transport-free and accept an `lsp_types::Uri` plus
source text, so tests and other clients can use the operations directly.

## Compatibility

The LSP companion requires Rust 1.85 because its `lsp-server` transport uses the
2024 edition. The core crate remains Rust 1.81 compatible, `no_std` compatible
and dependency-free.

The companion is a prototype. It does not yet offer workspace symbols, code
actions, completion resolution or a verified reference rename.
