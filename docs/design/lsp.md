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
  a declaration or a reference, through
  `Document::rename_with_references_with_options` and its `VerifiedEdits`.

Rename follows matching `$VAR`/`${VAR}` names. Definition, references and
rename use the parser's candidate list, including escaped and single-quoted
candidates that `Reference::is_eligible` would reject under Portable rules.
This is deliberate: editor operations work on the document text, and a
candidate can become active after a quote-style or dialect change. Filtering
would make rename miss occurrences that may later resolve. Clients that need
expansion semantics can apply `Reference::is_eligible` themselves.
When references exist, keys that cannot be written as variable-reference
names, such as keys containing `-`, fail with the core
`EditError::InvalidReferenceKey` instead of changing only the declaration. When
`key != new_key` and a matching reference occurs inside or after a `$(` command
substitution in the same entry, keys outside the core shell-safe allowlist fail
verification with `EditError::WouldChangeMeaning`. Rename parses through the
recovering parser so unrelated invalid lines do not block renaming a valid key
and are verified to remain unchanged.

## Diagnostics, masking and environment

Parser diagnostics are always enabled. `LspOptions::with_dialect` selects the
parser dialect for every operation (including `BomPolicy::StripAll` multi-BOM
and `BomPolicy::Error` position mapping, plus monotonic CRLF end-of-line mapping
under `LineBreaks::Lf` dialects); the stdio server takes it from `--dialect`
(which overrides any config `dialect`) or from the loaded lint configuration.
`LspOptions::with_parse_options` (and the stdio binary's `--max-input-bytes`,
`--max-lines`, `--max-items` and `--max-diagnostics` flags) applies opt-in
`ParseOptions` bounds to every document parse and to planned rename output.
Linter findings are an explicit policy: `LspOptions::with_lint_options` accepts
the linter's own `LintOptions` rule and severity policy (and
`LspOptions::without_lint_options` clears it). The default `LspOptions` does not
run the linter, so diagnostics do not silently depend on a policy configuration.

The stdio server loads policy only from `--config <path>` or from
`--discover-lint-config`, which explicitly searches ancestors of the launch
directory. It never discovers a config implicitly. Invalid configuration is a
startup error rather than a silent fallback. Configured ignore globs suppress
published diagnostics for matching `file://` document paths (non-`file://` URIs
such as `untitled:` are not suppressed); URI percent escapes are decoded and
Windows drive-letter leading slashes (`/C:/...`) are stripped before matching.
Parser diagnostics already report duplicate keys, so config-driven policy
disables the linter duplicate rule unless `[rules]` explicitly enables it.
Embedding clients can still construct `LspOptions` and supply `LintOptions`
directly.

`LintOptions::new()` includes duplicate-key findings, which overlap the parser
diagnostics. Callers publishing both should disable the linter duplicate rule.

Hover values are visible by default. `LspOptions::with_mask_values(true)` or
the binary's `--mask-values` flag replaces them with `<redacted>`.

The server reads only text the editor sends. It does not read the process
environment, resolve references or run commands. A future completion source for
environment values must be explicit and opt-in.

## Transport

The binary is a synchronous LSP server over standard input and output. It
advertises `serverInfo` during `initialize`, uses UTF-16 incremental document
synchronization, accepts full-document replacements, publishes versioned
diagnostics after `didOpen` and `didChange`, clears them on `didClose`, ignores
`didChange` for unopened documents without creating buffers, and exits with a
non-zero status if `exit` arrives before `shutdown`.

The library functions are transport-free and take source text; operations
that produce locations also take an `lsp_types::Uri`, so tests and other
clients can use the operations directly. `LspOptions::with_dialect` selects the
parser dialect for every operation.

## Compatibility

The LSP companion requires Rust 1.85 because its `lsp-server` transport uses the
2024 edition. The core crate remains Rust 1.81 compatible, `no_std` compatible
and dependency-free.

The companion is still an unstable `0.5.x` prototype while its API and
transport framing are finalized for `0.6.0`. It does not yet offer
`prepareRename`, document formatting, code actions or completion resolution; see
the [1.8.0 and LSP stabilization plan](1.8.0-plan.md) and tracking issue
[#248](https://github.com/asamarts/adotenv/issues/248) for the stabilization
checklist.
