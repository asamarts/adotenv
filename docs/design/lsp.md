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
- document-symbol outlines of declared keys;
- workspace symbols across currently open documents;
- rename of a key declaration and its literal references, selected from either
  a declaration or a reference, through `Document::rename_with_references` and
  its `VerifiedEdits`.

Rename follows matching `$VAR`/`${VAR}` names. When references exist, keys
that cannot be written as variable-reference names, such as keys containing
`-`, fail with the core `EditError::InvalidReferenceKey` instead of changing
only the declaration.

## Diagnostics, masking and environment

Parser diagnostics are always enabled. Linter findings are an explicit policy:
`LspOptions::with_lint_options` accepts the linter's own `LintOptions` rule and
severity policy. The default `LspOptions` does not run the linter, so
diagnostics do not silently depend on a policy configuration.

The stdio server loads policy only from `--config <path>` or from
`--discover-lint-config`, which explicitly searches ancestors of the launch
directory. It never discovers a config implicitly. Invalid configuration is a
startup error rather than a silent fallback. Configured ignore globs suppress
published diagnostics for matching document paths. Parser diagnostics already
report duplicate keys, so config-driven policy disables the linter duplicate
rule unless `[rules]` explicitly enables it. Embedding clients can still
construct `LspOptions` and supply `LintOptions` directly.

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

The companion is a prototype. It does not yet offer code actions or completion
resolution.
