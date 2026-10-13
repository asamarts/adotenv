# Architecture

Status: planned for 0.2. 0.1 has a simpler model (a list of entries with
spans); this document describes where it is going.

## Document model

`.env` is line oriented: almost every line is exactly one of a blank line, a
comment, an assignment, or something that does not parse. Only a quoted value
spanning several lines breaks that. So the model is a flat, ordered list of
items rather than a general syntax tree:

```text
Document { items: Vec<Item>, bom: bool, final_newline: bool }

Item
  Blank   { span, line_ending }
  Comment { span, text_span, line_ending }
  Entry   { span, line_ending, tokens }
  Invalid { span, line_ending, diagnostic }

Entry tokens (each is a raw slice plus a span; absent tokens are None)
  indent  export  key  space_before_eq  eq  space_after_eq
  value { quote: None | Single | Double, raw, body }
  trailing_space  inline_comment
```

Rules the model guarantees:

- **Lossless.** Concatenating every item's span text reproduces the input
  exactly, for any input, including invalid UTF-8 handled in a lossy mode and
  lines that don't parse. A property test asserts this.
- **Raw first.** Tokens keep the text as written. The decoded value
  (unquoted, unescaped) is computed from the raw value under the active
  dialect, together with a map from decoded offsets back to raw offsets, so a
  finding inside a decoded value (for example a secret) can be placed exactly.
- **Assignment state.** An entry is `KEY=value`, `KEY=` (empty), or `KEY`
  with no `=` (bare). Tools disagree about what a bare key means, so the model
  keeps it distinct and the dialect decides its meaning.
- **Multi-line values** are one entry whose span covers several lines.

Generic syntax-tree libraries were considered and rejected for this format:
they add a dependency and indirection that a line-based grammar with about a
dozen token kinds does not need.

## Reading

Two surfaces over one model:

- **Entries** for the common case: `doc.get("K")`, `doc.entries()`,
  `doc.get_all("K")` (duplicates in order), and per entry `key()`, `value()`
  (decoded), `raw_value()`, `quote()`, spans, `inline_comment()`.
- **Items and tokens** for formatters and editors: every blank line, comment
  and token with its span.

Lookups use the dialect's duplicate policy (last wins by default); duplicates
are always kept and reported.

## Diagnostics

`parse` never fails. It returns the document and a list of diagnostics:

```rust
let (doc, diagnostics) = adotenv::parse(text);
```

Each diagnostic has a stable code (an enum with documented meanings, never
renumbered), a severity, a primary span, an optional secondary span, and a
message. Examples: unterminated quote, invalid key character, text after a
closing quote, unknown escape, missing `=`, duplicate key, `#` without a
space before it, a lone carriage return.

An unparsable line becomes an `Invalid` item and parsing continues on the
next line. An unterminated quote is reported at the opening quote and its
entry ends at the end of that line, so it never swallows the rest of the file.

`parse_strict` returns the first error-severity diagnostic as an error, for
callers that want 0.1's behavior.

## Positions

Spans are byte ranges into the original text. `LineIndex` converts an offset
to a 0- or 1-based line and column, counting columns in UTF-8 bytes, Unicode
scalar values or UTF-16 code units, and converts back. Editors use UTF-16;
terminal tools use scalar values.

## Edits

An edit is data:

```rust
struct TextEdit { range: Range<usize>, replacement: String }
```

Edit operations take a document and return text edits: set a value, remove an
entry (optionally with the comments attached to it), rename a key, insert an
entry (at the end, after a key, or in sorted position), comment or uncomment
an entry, toggle `export`. Callers can apply the edits themselves (a
linter's fix list, an editor's workspace edit) or ask adotenv to apply them.

Every operation is verified before it returns: the edited text is parsed and
checked to read back the intended value, and all other entries are checked
to be unchanged. An edit that cannot be expressed exactly is an error, never
an approximation. Quoting is chosen per value: the existing style is kept
when it can represent the new value, otherwise the minimal style that can.

## Crate layout and platform

The repository becomes a Cargo workspace in 0.2. The `adotenv` crate is the
core: zero dependencies, `#![no_std]` with `alloc`, `#![forbid(unsafe_code)]`.
A default-on `std` feature adds `std::error::Error` implementations and is
required by the loader. Later crates (`adotenv-cli`, `adotenv-config`,
`adotenv-figment`) live in the same workspace and are versioned and released
independently.

## Error types

Fallible operations return specific error enums (`EditError`, `ResolveError`,
`LoadError`), marked `#[non_exhaustive]`, each with a `Display` message and,
from Rust 1.81, a `core::error::Error` implementation.
