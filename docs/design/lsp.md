# Language server

`adotenv-lsp` is an optional companion crate for editor integration. It keeps
editor protocol work out of the parser and reuses the core document model,
spans, diagnostics, formatting and verified edits. Starting with `0.6.0`, it is
checked by `cargo semver-checks` in CI on its `0.x` line (tracking `lsp-types`
0.x).

## Operations

`adotenv-lsp` provides:

- diagnostics from the recovering parser, including related locations for
  duplicate keys;
- optional linter findings when the caller supplies an explicit
  `LintOptions` policy;
- completion of declared keys at a key position or after `$`/`${`, with
  triggers for `$` and `{`;
- definition from a key or a literal `$VAR`/`${VAR}` reference to the winning
  declaration;
- hover for a key or reference, with optional value masking;
- references across declarations and values, returning each reference's exact
  identifier (`reference.name`) span rather than an enclosing
  `${VAR:-default}` expression;
- document-symbol outlines of declared keys;
- workspace symbols across currently open documents;
- `textDocument/prepareRename` validating renameable declarations and
  references to declared keys with exact ranges and placeholders;
- rename of a key declaration and its literal references, selected from either
  a declaration or a reference, through
  `Document::rename_with_references_with_options` and its `VerifiedEdits`;
- `textDocument/formatting` backed by `Document::format`, honoring
  `LspOptions::format`, `LspOptions::lint`, LSP `FormattingOptions`
  (`trimTrailingWhitespace` and `insertFinalNewline`; `trimFinalNewlines` is
  not mapped because `adotenv` does not delete trailing blank lines separately
  from `BlankLines::Collapse`) and configured `ParseOptions` limits;
- `textDocument/codeAction` emitting verified `quickfix` (capped at 64
  per-entry quick fixes per request to keep response time linear on large
  files) and `source.fixAll` actions derived from `adotenv-lint` findings while
  preserving leading BOMs and parse limits.

Rename follows matching `$VAR`/`${VAR}` names. Definition, references,
`prepareRename` and rename use the parser's candidate list, including escaped
and single-quoted candidates that `Reference::is_eligible` would reject under
Portable rules. This is deliberate: editor operations work on the document
text, and a candidate can become active after a quote-style or dialect change.
Filtering would make rename miss occurrences that may later resolve. Clients
that need expansion semantics can apply `Reference::is_eligible` themselves.
When references exist, keys that cannot be written as variable-reference
names, such as keys containing `-`, fail with the core
`EditError::InvalidReferenceKey` instead of changing only the declaration. When
`key != new_key` and a matching reference occurs inside or after a `$(` command
substitution in the same entry, keys outside the core shell-safe allowlist fail
verification with `EditError::WouldChangeMeaning`. Rename parses through the
recovering parser so unrelated invalid lines do not block renaming a valid key
and are verified to remain unchanged.

## Coordinates, diagnostics, masking and environment

Coordinates default to UTF-16 (`PositionEncoding::Utf16`) per LSP 3.17, with
opt-in UTF-8 (`PositionEncoding::Utf8`) and UTF-32 (`PositionEncoding::Utf32`)
support negotiated from `general.positionEncodings` during `initialize`.
Incoming positions adopt LSP 3.17 §3.1.1 column clamping: on a valid line, a
`character` offset past the end of the line clamps to line end, while
out-of-bounds lines, inverted ranges, split UTF-16 surrogates and mid-character
UTF-8 byte offsets return `None`.

Parser diagnostics are always enabled. `LspOptions::with_dialect` selects the
parser dialect for every operation (including `BomPolicy::StripAll` multi-BOM
and `BomPolicy::Error` position mapping, full encoded-width BOM anchoring on
line 0, and monotonic snapping for diagnostic spans landing inside CRLF such
as `"\\\r\n"`); the stdio server takes it from `--dialect` (which overrides any
config `dialect`) or from the loaded lint configuration.
`LspOptions::with_parse_options` (and the stdio binary's `--max-input-bytes`,
`--max-lines`, `--max-items` and `--max-diagnostics` flags) applies opt-in
`ParseOptions` bounds to every document parse and to planned rename, format and
code-action outputs. Linter findings and quick fixes are an explicit policy:
`LspOptions::with_lint_options` accepts the linter's own `LintOptions` rule and
severity policy (and `LspOptions::without_lint_options` clears it). The default
`LspOptions` does not run the linter, so diagnostics do not silently depend on
a policy configuration.

The stdio server loads policy only from `--config <path>` or from
`--discover-lint-config`. Explicit `--config` is validated before the server
starts; `--discover-lint-config` searches ancestors from the first `file://`
entry in `InitializeParams::workspace_folders`, falling back to `root_uri` and
then the launch directory. It never discovers a config implicitly. Invalid
discovered configuration returns an error response to `initialize` and exits
non-zero. Configured ignore globs suppress published diagnostics and code
actions for matching `file://` document paths (non-`file://` URIs such as
`untitled:` are not suppressed); URI percent escapes are decoded and Windows
drive-letter leading slashes (`/C:/...`) are stripped before matching. Parser
diagnostics already report duplicate keys, so config-driven policy disables the
linter duplicate rule unless `[rules]` explicitly enables it. Embedding clients
can still construct `LspOptions` and supply `LintOptions` directly.

`LintOptions::new()` includes duplicate-key findings, which overlap the parser
diagnostics. Callers publishing both should disable the linter duplicate rule.

Hover values are visible by default. `LspOptions::with_mask_values(true)` or
the binary's `--mask-values` flag replaces them with `<redacted>`.

The server reads only text the editor sends. It does not read the process
environment, resolve references or run commands. A future completion source for
environment values must be explicit and opt-in.

## Library and transport

The library is transport-free and offers two layers:

- Stateful `ParsedDocument` and `Session` handles that parse each document
  and build its coordinate map once per open or incremental change, then serve
  every request without re-parsing.
- Twelve symmetric stateless functions (`diagnostics`, `completion`,
  `document_symbols`, `workspace_symbols`, `apply_change`, `definition`,
  `hover`, `references`, `prepare_rename`, `rename`, `formatting` and
  `code_actions`), each taking `&LspOptions`.

The stdio binary is gated behind the default-on `stdio` feature
(`[[bin]] name = "adotenv-lsp" required-features = ["stdio"]`), so library
embedders using `default-features = false` build only the library without the
binary or direct `serde`/`serde_json` dependencies.

The binary speaks LSP over standard input and standard output through a
synchronous bounded JSON-RPC 2.0 `Content-Length` transport with explicit
limits on header line length (8 KiB), header count (32) and payload size
(default 16 MiB, configurable with `--max-message-bytes`) enforced before
allocating payload buffers. It negotiates `positionEncoding`, advertises
`serverInfo` during `initialize` (advertising `codeActionProvider` only when a
lint policy is configured), rejects pre-initialize requests with
`ServerNotInitialized` (`-32002`), rejects duplicate `initialize` and
post-`shutdown` requests with `InvalidRequest` (`-32600`), uses incremental
document synchronization with full-document fallback, publishes versioned
diagnostics after `didOpen` and `didChange`, clears them on `didClose`, ignores
`didChange` for unopened documents without creating buffers, and exits with a
non-zero status if `exit` arrives before `shutdown`.

## Compatibility

`adotenv-lsp` builds on Rust 1.81 with zero `unsafe` code, matching `adotenv`,
`adotenv-process`, `adotenv-cli`, `adotenv-lint` and `adotenv-figment`. The
core crate remains Rust 1.81 compatible, `no_std` compatible and
dependency-free.

`adotenv-lsp` stays on its own `0.x` version line because its public API
exposes `lsp-types` 0.x directly; an upstream `lsp-types` minor release becomes
a `0.x` minor release of `adotenv-lsp`. Public API compatibility within `0.6.x`
is checked in CI with `cargo semver-checks`.
