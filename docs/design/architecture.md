# Architecture

Status: planned for 0.2. 0.1 has a simpler model (a list of entries with
spans); this document describes where it is going.

## Document model

`.env` is line oriented: almost every line is exactly one of a blank line, a
comment, an assignment, or something that does not parse. Only a quoted value
spanning several lines breaks that. So the model is a flat, ordered list of
items rather than a general syntax tree:

```text
Document<'src> {
  source: Cow<'src, str>, items: Vec<Item>, bom: bool, final_newline: bool
}

Item
  Blank   { span, line_ending }
  Comment { span, text_span, line_ending }
  Entry   { span, line_ending, tokens }
  Invalid { span, line_ending, diagnostic }

Entry tokens (each is a span into the source; absent tokens are None)
  indent  export  key  space_before_eq  eq  space_after_eq
  value { quote: None | Single | Double, raw, body }
  trailing_space  inline_comment
```

Rules the model guarantees:

- **Lossless.** Printing the document reproduces any UTF-8 input byte for
  byte, including the BOM, comments, blank lines and malformed `.env` syntax.
  A property test asserts this.
- **Raw first.** Tokens keep the text as written. The decoded value
  (unquoted, unescaped) is computed from the raw value under the active
  dialect, together with a map from decoded offsets back to raw offsets, so a
  finding inside a decoded value (for example a secret) can be placed exactly.
- **Assignment state.** An entry is `KEY=value`, `KEY=` (empty), or `KEY`
  with no `=` (bare). Tools disagree about what a bare key means, so the model
  keeps it distinct. The selected dialect and explicit interpretation policy
  decide its meaning; parsing never performs an implicit environment lookup.
- **Multi-line values** are one entry whose span covers several lines.

Generic syntax-tree libraries were considered and rejected for this format:
they add a dependency and indirection that a line-based grammar with about a
dozen token kinds does not need.

## Input

The core accepts UTF-8 text (`&str`). Invalid `.env` syntax is retained with
diagnostics; invalid UTF-8 is outside the parser's input domain. File readers
must reject decoding errors rather than silently replacing bytes. The core
does not provide a lossy-decoding mode: replacement characters would change
the original bytes and invalidate source-relative offsets.

## Source ownership

Normal parsing borrows its `&str` input without copying the source. An
explicit owned entry point takes a `String` by move; converting a borrowed
document to an owned one copies the source only when requested.

Stored tokens are spans, not references into the document's own storage.
Reading derives views from the retained source, so borrowed and owned
documents use the same model without self-referential structures. Keys and
values that need no unescaping borrow source text; decoded values allocate
only when needed.

## Dialect-specific structure

A document's items, entries and diagnostics belong to the grammar used to
parse it. Dialects can change statement boundaries, not just decoded values:
for example, a multiline quoted assignment and a raw physical-line value
cannot share one universal entry grouping.

Each selected dialect therefore parses the retained source separately.
Borrowed interpretations share the same text without copying it. Portability
checking performs one linear parse per dialect; it does not require a second
neutral syntax model, a generic syntax tree or cross-dialect caches.

The 0.2 parser uses Portable syntax by default. Named presets and configurable
switches arrive in 0.3. The 0.2 migration therefore includes syntax changes
as well as the document API; there is no implicit 0.1 compatibility grammar.

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

An unparsable statement becomes an `Invalid` item; parsing resumes at its
next boundary. Multiline quotes follow the grammar: assignment-looking
lines remain value content when a matching closing quote exists.

If a quote remains unmatched at EOF, report the opening quote, retain only
its opening physical line as an `Invalid` item, and resume parsing at the
following line. Recovery never guesses author intent or drops source bytes.
The parser must remain linear; repeated unbounded rescans are not allowed.

`parse_strict` returns the first error-severity diagnostic as an error. It
provides a strict parsing surface under Portable rules, not a compatibility
shim for 0.1's grammar.

## Positions

Spans are byte ranges into the original text. `LineIndex` converts between
offsets and zero-based line/column positions, counting columns in UTF-8 bytes,
Unicode scalar values or UTF-16 code units. LSP defaults to UTF-16; callers
honor negotiated encodings. One-based display coordinates require explicit
conversion. Existing one-based line-number accessors remain documented as
such. Scalar columns are not terminal cell widths.

Conversions are checked: out-of-range or unrepresentable positions return
an error, never a clamped or rounded location. Unicode conversions reject
interior UTF-8 bytes and split UTF-16 surrogate pairs. Raw byte coordinates
remain source-exact; text edits additionally require UTF-8 character
boundaries. `LineIndex` treats LF, CRLF and lone CR as source line breaks,
independently of dialect syntax. EOF is valid; a final line break creates
a final empty line.
Line-ending starts map to the preceding content's end, while a position
inside a CRLF pair is unrepresentable. Reverse conversion rejects columns
beyond line content. Indexing counts the original source, BOM included.
Acceptance tests must cover exact round trips and rejection at these boundaries.

## Edits

An edit is data:

```rust
struct TextEdit { range: Range<usize>, replacement: String }
```

Edit operations take a document and return text edits: set a value, remove an
entry (optionally with explicitly selected comments), rename a key, insert an
entry (at the end, after a key, or in sorted position), comment or uncomment
an entry, toggle `export`. Callers can apply the edits themselves (a
linter's fix list, an editor's workspace edit) or ask adotenv to apply them.

Key-only convenience edits require a unique matching entry and reject
duplicates. Native operations also accept an explicitly selected occurrence
from the same document snapshot, so callers can fix duplicates without
guessing or manually bypassing verification.

Comments are preserved by default. Callers may explicitly select comment
items/spans to remove with an entry in the same verified batch. Automatic
attachment-based shortcuts arrive with the 0.5 comment/group model, not
through guessed ownership in 0.2.

Every operation is verified before it returns: the edited text is parsed and
checked to read back the intended value, and all other entries are checked
to be unchanged. An edit that cannot be expressed exactly is an error, never
an approximation. Quoting is chosen per value: the existing style is kept
when it can represent the new value, otherwise the minimal style that can.

Verified operations may edit explicitly identified valid targets even when
unrelated parts of the document are invalid. Re-parsing must preserve
unrelated raw items, literal states and diagnostic identities, allowing
expected position shifts. If an edit changes their interpretation or quote
grouping, reject it. Never infer a key target from invalid text; deliberate
repairs require an explicit item or span and defined postconditions.

Text edits refer to the exact source snapshot used to produce them.
External appliers are responsible for document-version checks.

An operation's edit batch is verified as a whole against one source
snapshot; individually valid edits do not waive final batch verification.

## Crate layout and platform

The repository becomes a Cargo workspace in 0.2. The `adotenv` crate is the
core: zero dependencies, `#![no_std]` with `alloc`, `#![forbid(unsafe_code)]`.
A default-on `std` feature enables std-only integration and is required by
the loader; `core::error::Error` implementations do not require it.

The unsafe prohibition in `adotenv` is unconditional, including its optional
features. A future opt-in std-only `adotenv-process` adapter owns the narrowly
documented `unsafe fn` for applying validated values to the process
environment. Loading files and returning maps remain `adotenv` features.
Other later crates (`adotenv-cli`, `adotenv-config`, `adotenv-figment`) share
the workspace. All workspace crates are versioned and released independently.

## Error types

Fallible operations return specific error enums (`EditError`, `ResolveError`,
`LoadError`), marked `#[non_exhaustive]`, each with a `Display` message and,
from Rust 1.81, a `core::error::Error` implementation.
