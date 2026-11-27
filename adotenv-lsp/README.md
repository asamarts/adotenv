# adotenv-lsp

An optional Language Server Protocol companion for `adotenv` documents.
Version 0.6.0 graduates `adotenv-lsp` into a semver-checked `0.x` line with a
symmetric transport-free library API, stateful `ParsedDocument` and
`Session` handles, a bounded JSON-RPC 2.0 stdio transport gated behind the
default-on `stdio` feature, and Rust 1.81 MSRV parity with `adotenv`.

It supports:

- diagnostics from the recovering parser;
- optional linter findings, including key-order, key-case, quote-style and
  whitespace rules;
- completion of declared keys and `$`/`${` references;
- definition lookup for keys and `$VAR`/`${VAR}` references;
- hover information with opt-in value masking;
- references across declarations and values (returning exact identifier spans);
- document-symbol outlines of declared keys;
- workspace symbols across open documents;
- `textDocument/prepareRename` and verified key rename from a key or reference,
  including literal value references;
- `textDocument/formatting` backed by `Document::format`;
- `textDocument/codeAction` quick fixes and `source.fixAll` backed by
  `adotenv-lint` verified fixes;
- LSP 3.17 `positionEncoding` negotiation (`utf-16`, `utf-8`, `utf-32`) and
  end-of-line column clamping.

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
adotenv-lsp --max-message-bytes 16777216
adotenv-lsp --config .adotenv-lint.toml
adotenv-lsp --discover-lint-config
```

The binary speaks LSP over standard input and standard output using a bounded
synchronous JSON-RPC 2.0 `Content-Length` transport (default 16 MiB message
limit, 8 KiB header-line limit, 32-header cap). It negotiates
`positionEncoding`, reports `serverInfo` during `initialize`, publishes
document versions with diagnostics, accepts incremental text changes (with LSP
3.17 column clamping) and full-document replacements, caches parsed documents
in a `Session`, and exits with a non-zero status if `exit` arrives before
`shutdown`.

Parser diagnostics are the default. `--dialect` selects the parser preset and
overrides any `dialect` set in a lint configuration file. Linter policy is
loaded only when `--config <path>` names a file or `--discover-lint-config`
explicitly opts into ancestor discovery from the client's first `file://`
workspace folder (falling back to `rootUri` and then the launch directory);
the two cannot be combined. There is no implicit discovery. Invalid explicit
configuration fails before the server starts, and invalid discovered
configuration fails `initialize` with an error response. Configured ignore
globs suppress published diagnostics and code actions for matching `file://`
document URIs. Because parser diagnostics already report duplicate keys,
config-driven lint policy disables the linter duplicate-key rule unless
`[rules]` explicitly enables it.

## Library

Disable default features (`default-features = false`) when embedding the
transport-free library without building the stdio binary or depending on
`serde`/`serde_json` directly.

Linter findings and quick fixes are opt-in through
`LspOptions::with_lint_options` (and can be cleared with
`LspOptions::without_lint_options`), `LspOptions::with_format_options` sets an
explicit formatting base policy, `LspOptions::with_dialect` selects the parser
rules used by every operation, `LspOptions::with_position_encoding` selects
UTF-16, UTF-8 or UTF-32 coordinates, and `LspOptions::with_parse_options`
applies opt-in `ParseOptions` bounds (bytes, lines, items and diagnostics) to
every parse and edit output. Parser diagnostics remain enabled independently.
The crate re-exports `BlankLines`, `BomPolicy`, `Dialect`, `Document`,
`EditError`, `EqualsSpacing`, `FinalNewline`, `FormatError`, `FormatOptions`,
`ParseOptions`, `LimitKind`, `LimitExceeded`, `LintOptions`, `KeyCase`,
`QuoteStyle`, `Rule` and `Severity`.

`LintOptions::new()` includes duplicate-key findings, which overlap parser
diagnostics. Disable `duplicate_keys` when publishing both.

Every stateless operation takes `&LspOptions` consistently, and
`ParsedDocument` / `Session` retain parsed documents and coordinate maps across
repeated requests:

```rust
use adotenv_lsp::{
    diagnostics, KeyCase, LintOptions, LspOptions, ParsedDocument, Session,
};
use lsp_types::{Position, Uri};

let uri: Uri = "file:///tmp/.env".parse().unwrap();
let options = LspOptions::new().with_lint_options(
    LintOptions::new()
        .with_duplicate_keys(false)
        .with_key_case(KeyCase::Upper),
);
let diags = diagnostics("A=1\nBAD KEY=2\n", &uri, &options);
assert!(!diags.is_empty());

let doc = ParsedDocument::parse("HOST=localhost\nURL=${HOST}\n", options);
assert!(doc.prepare_rename(Position::new(1, 7)).is_some());

let mut session = Session::new(options);
session.open(uri.clone(), Some(1), "HOST=localhost\n");
assert_eq!(session.workspace_symbols("host").len(), 1);
```

Rename uses `Document::rename_with_references_with_options` on a recovering
parse, so the declaration and every matching `$VAR`/`${VAR}` name change
together under the configured parse limits while unrelated invalid lines remain
unchanged. Definition, references, `prepareRename` and rename follow every
reference candidate the parser reports, including escaped and single-quoted
candidates that Portable resolution would not expand. This keeps rename
complete when a quote-style or dialect change later makes a candidate active;
clients that need expansion semantics can apply `Reference::is_eligible`. When
references exist, keys that cannot be written as variable references are
rejected through `EditError::InvalidReferenceKey`, and renames of references
inside or after a `$(` command substitution whose old or new key falls outside
the shell-safe allowlist fail verification with
`EditError::WouldChangeMeaning`.

This crate builds on Rust 1.81, matching `adotenv`, `adotenv-process`,
`adotenv-cli`, `adotenv-lint` and `adotenv-figment`. The core `adotenv` crate
remains `no_std`, dependency-free and Rust 1.81 compatible.
