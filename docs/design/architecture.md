# Architecture

Status: the 0.2 lossless core and native edit model are implemented. Sections
explicitly marked future describe later milestones, not additional APIs
shipped by 0.2.

## Document model

`.env` is line oriented: almost every line is exactly one of a blank line, a
comment, an assignment, or something that does not parse. Only a quoted value
spanning several lines breaks that. So the model is a flat, ordered list of
items rather than a general syntax tree:

```text
Document<'src>
  source: Cow<'src, str>
  item/entry metadata: vectors of spans and indexes (no self-references)
  bom: bool

Item view
  kind: Blank | Comment | Entry | Invalid
  span, line_ending, optional entry or primary diagnostic

Entry view
  snapshot-local id and item_id, quote, assignment state, optional decoded value

EntryTokens (byte ranges; optional syntax uses Option<Range<usize>>)
  indent  export  space_after_export  key  space_before_eq  eq  space_after_eq
  value  body  trailing_space  inline_comment
```

Rules the model guarantees:

- **Lossless.** Printing the document reproduces any UTF-8 input byte for
  byte, including the BOM, comments, blank lines and malformed `.env` syntax.
  A property test asserts this.
- **Raw first.** Tokens keep the text as written. Literal values borrow their
  raw body; only actual double-quote unescaping allocates decoded text.
  `decoded_offset_to_raw` and `decoded_span_to_raw` map decoded UTF-8 byte
  boundaries back to original-source regions, including escape spellings.
- **Assignment state.** An entry is `KEY=value`, `KEY=` (empty), or `KEY`
  with no `=` (bare). `value()` is `None` for bare and `Some("")` for assigned
  empty values. `AssignmentState` distinguishes Bare/Empty/Value; parsing
  never performs an implicit environment lookup. Future dialect/loading
  policies may interpret bare syntax differently.
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

## Dialect-specific structure (future 0.3)

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

- **Entries** for the common case: `doc.get("K")`, `get_entry("K")`,
  `get_all("K")` (duplicates in order), `unique("K")` and per-entry `key()`,
  optional decoded `value()`, optional `raw_value()`, `quote()`, spans and
  `inline_comment()`.
- **Items and tokens** for formatters and editors: every blank line, comment,
  invalid statement and raw token span.

`entries()` and `items()` return exact-size double-ended view iterators, not
slices. `entry(id)` and `item(id)` return optional views for zero-based IDs
local to this exact snapshot. Views are borrowed from the document.

Duplicates are always retained and diagnosed. 0.2 lookups use the last
declaration; a last bare entry means `get()` is `None`, not an earlier value.

## Diagnostics

`parse` never fails. It returns the document and a list of diagnostics:

```rust
let text = "A=value\n";
let (doc, diagnostics) = adotenv::parse(text);
assert_eq!(doc.get("A"), Some("value"));
assert!(diagnostics.is_empty());
```

Each diagnostic has a stable code (an enum with documented meanings, never
renumbered), a severity, a primary span, an optional secondary span, and a
message. `DiagnosticCode::as_str()` is a stable machine identity, independent
of enum order or wording. Errors: invalid key, unterminated quote, quoted
trailing text. Warnings: unknown escape, duplicates, bare key, non-POSIX key,
tight hash, unquoted backticks, lone CR. A leading BOM is informational.

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
an error, never a clamped or rounded location. Scalar and UTF-16 conversions
reject interior UTF-8 bytes; UTF-8 columns retain byte-exact interior offsets.
UTF-16 also rejects split surrogate pairs. Text-edit ranges must be UTF-8
boundaries. `Position::one_based()` uses checked addition.
`LineIndex` treats LF, CRLF and lone CR as source line breaks,
independently of dialect syntax. EOF is valid; a final line break creates
a final empty line.
Line-ending starts map to the preceding content's end, while a position
inside a CRLF pair is unrepresentable. Reverse conversion rejects columns
beyond line content. Indexing counts the original source, BOM included.
Acceptance tests must cover exact round trips and rejection at these boundaries.

## Edits

An edit is data:

```rust
// Public shape; use adotenv::TextEdit in programs.
struct TextEdit { range: core::ops::Range<usize>, replacement: String }
```

`Document::edit(&[Edit])` produces a `VerifiedEdits` containing native ranges
and the complete verified output. `Edit` variants are Set, Remove, Rename,
Insert, Comment, Uncomment and Export. Entry selectors and item selectors
are separate, zero-based snapshot-local IDs; invalid IDs return an error.
`InsertPosition` supports End, AfterKey, AfterEntry and Sorted.
Sorted means before the first greater original key in source order, even when
the original entries are unsorted. Repeated key lookups share a batch-local
index; single lookups do not allocate one. Insertion separators are based on
surviving output, retaining the original final-newline policy.

Document convenience methods `set`, `remove`, `rename`, `insert`, `comment`,
`uncomment` and `set_export` use the same producer. Free `set` / `remove`
parse recovering Portable syntax and return `Result<String, EditError>`.
No legacy grammar adapter remains.

`VerifiedEdits::edits()` borrows text edits; `into_edits()` consumes them.
`as_str()` borrows the verified output; `into_string()` consumes it.

Key-only convenience edits require a unique matching entry and reject
duplicates. Native operations also accept an explicitly selected occurrence
from the same document snapshot, so callers can fix duplicates without
guessing or manually bypassing verification.

Comments are preserved by default. Callers may explicitly select comment
items/spans to remove with an entry in the same verified batch. Automatic
attachment-based shortcuts arrive with the 0.5 comment/group model, not
through guessed ownership in 0.2.

Uncommenting removes one marker and at most one following space. Empty EOF
comments may disappear, and adjacent selected comment regions may restore one
multiline statement across explicitly removed intervening items. An insertion
between those regions prevents them from merging. An explicitly uncommented
leading region may expose its own BOM; unrelated BOM exposure is still refused.

Every operation is verified before it returns: the edited text is parsed and
checked to read back the intended value, and all other entries are checked
to be unchanged. An edit that cannot be expressed exactly is an error, never
an approximation. Quoting preserves the existing style when exactly
representable and otherwise chooses an exact safe style.

Verified operations may edit explicitly identified valid targets even when
unrelated parts of the document are invalid. Re-parsing must preserve
unrelated raw items, literal states and diagnostic identities, allowing
expected position shifts. If an edit changes their interpretation or quote
grouping, reject it. Never infer a key target from invalid text; deliberate
repairs require an explicit item or span and defined postconditions.

Text edits refer to the exact source snapshot used to produce them.
External appliers are responsible for document-version checks.
When applying ranges externally, apply them from highest offset to lowest;
they do not refer to successively modified versions of the source.

An operation's edit batch is verified as a whole against one source
snapshot; individually valid edits do not waive final batch verification.

## Crate layout and platform

The repository is a Cargo workspace with the core package at the root
(`members = ["."]`). Fuzzing has an excluded, separate workspace. `adotenv`
has zero runtime dependencies, `#![no_std]` with `alloc`, and unconditional
`#![forbid(unsafe_code)]`. Default feature `std` can be disabled without losing
core parsing/editing/position APIs or `core::error::Error` implementations.
There is no I/O or environment/process integration in 0.2 under either mode.

The unsafe prohibition in `adotenv` is unconditional. A future opt-in,
std-only `adotenv-process` adapter will own the narrowly documented unsafe
function for applying validated values to the process environment. Future
file loading and map generation remain safe `adotenv` features, not current
core behavior. Later CLI/config/figment crates will join the workspace;
workspace crates will be versioned and released independently.

## Error types

0.2 fallible APIs use specific non-exhaustive `EditError` and `PositionError`
enums; strict parsing returns a `Diagnostic`. Each implements `Display` and
`core::error::Error` on MSRV 1.81. `ResolveError` and `LoadError` belong to
future expansion and loading milestones.
