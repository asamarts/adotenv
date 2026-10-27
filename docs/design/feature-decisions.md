# Feature decisions

Feature decisions use these rules:

- Keep parsing literal and free of implicit I/O.
- Keep the core free of required runtime dependencies.
- Preserve the MSRV and `no_std` build.
- Prefer a separate crate when a feature needs a protocol, crypto, editor
  integration or opinionated policy.
- Adopt compatibility behavior only when a pinned upstream oracle exists.

## Decisions

| Area | Decision | Reason |
|---|---|---|
| Values-only parsing | Adopt | Added `parse_values` and `parse_values_with`; no document metadata is retained. |
| Linter rules and fixes | Incubate as a companion crate | dotenv-linter has 14 opinionated checks. Lint policy does not belong in the parser core. |
| LSP and editor support | Incubate as a companion crate | adotenv already has spans, diagnostics and edits. An LSP needs editor protocol work, not parser changes. |
| Encryption and vault files | Reject for the core | Crypto, key handling and secret distribution are a different product. Keep adotenv usable as a literal parser. |
| Implicit file search | Reject | Loading is explicit by directory, mode or ordered file list. Hidden search changes behavior by working directory. |
| Environment inheritance | Reject | Bare keys stay bare. Callers provide lookup values explicitly. |
| Ambient command execution | Reject | Commands stay literal unless a caller supplies a callback or uses the opt-in exec helper. |
| Process mutation | Keep in `adotenv-process` | The core stays safe. Process mutation remains isolated and unsafe by design. |
| CLI process execution | Keep in `adotenv-cli run` | The CLI starts a child without mutating the parent environment. |
| More syntax presets | Conditional | Add a preset only with a pinned source, package identity and reproducible observations. |
| Streaming parser | Defer | `parse_values` covers the low-memory map path. Streaming needs a separate grammar and error model decision. |
| More adapters | Conditional | Add an adapter when it can consume documents or loaded environments without changing core policy. |

## Linter companion

dotenv-linter demonstrates demand for checks such as duplicate keys, unordered
keys, lowercase keys, quote style, whitespace, leading characters and schema
rules. adotenv should not add those policies to the parser.

A future `adotenv-lint` crate can use the existing document model and return
structured findings and verified edits. It should not require changes to core
parsing.

## LSP companion

Editor demand is visible in ecolog-lsp, which provides completion, definition,
hover, references, rename, diagnostics and value masking across many languages.

A future `adotenv-lsp` crate can reuse `Document` spans and `VerifiedEdits`.
It should keep value masking opt-in and avoid ambient environment lookup by
default.

## Syntax presets

New presets must include:

- a pinned implementation version and source identity;
- a reproducible oracle adapter;
- input and profile hashes;
- a clear statement of which behavior is literal and which requires loading or
  interpolation support.

Presets without a stable oracle remain rejected.

## Streaming

A streaming API would need to preserve source spans and recovery behavior, or
explicitly define a reduced stream mode. This is deferred until a real user
needs bounded-memory streaming beyond `parse_values` and the values-only loaders.
