# Architecture

Status: the lossless core/native edit model and 0.3 dialect
configuration/presets are implemented. Sections explicitly marked future are
designs, not additional shipped interfaces.

## Document model

The model is a flat, ordered list of items rather than a general syntax tree.
The selected grammar determines their boundaries: multiline values and quoted
keys can span physical lines, and Compose can split one physical line into
multiple statements.

```text
Document<'src>
  source: Cow<'src, str>
  item/entry metadata: vectors of spans and indexes (no self-references)
  bom: bool
  dialect: Dialect (exact selected configuration)

Item view
  kind: Blank | Comment | Entry | Invalid | Ignored
  span, line_ending, optional entry or primary diagnostic

Entry view
  snapshot-local id and item_id, quote, assignment state, optional decoded value

EntryTokens (byte ranges; optional syntax uses Option<Range<usize>>)
  indent  export  space_after_export  key  space_before_eq  eq  space_after_eq
  value  body  quoted_tail  trailing_space  inline_comment
```

Rules the model guarantees:

- **Lossless.** Printing the document reproduces any UTF-8 input byte for
  byte, including the BOM, comments, blank lines and malformed `.env` syntax.
  A property test asserts this.
- **Raw first.** Tokens keep text as written. Values borrow their body until
  unescaping, newline normalization or syntax deletion requires decoded text.
  `decoded_offset_to_raw` and `decoded_span_to_raw` map decoded UTF-8 boundaries
  back to original-source regions, including escapes and removed delimiters.
- **Assignment state.** Recognized entries expose Bare/Empty/Value separately
  from quoting. Bare `value()` is `None`; assigned empty is `Some("")`.
  Configurable bare policies affect effective lookup or errors without
  inheritance. Preset-specific grammar quirks remain explicit in
  [dialects.md](dialects.md), including Compose's EOF empty-key assignment.
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

Each selected dialect parses the retained source separately. `parse_with`,
`parse_owned_with` and `parse_strict_with` take an explicit `Dialect`;
`Document::dialect()` retains it and `reparse` returns a borrowed
reinterpretation. Existing entry points retain Portable defaults and no legacy
0.1 grammar is introduced.

Borrowed interpretations share text without copying it. Portability comparison
performs one linear parse per unique dialect and reports physical-line
differences from their ordered items; it does not require a neutral syntax
model, generic syntax tree or cross-dialect cache.

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

Duplicates remain ordered. Effective lookup filters `participates_in_lookup()`
and selects first or last by configuration; ignored bare keys do not erase an
earlier assigned value. `get_all()` and uniqueness checks retain all recognized
declarations. Portable keeps its last-bare-means-`None` behavior.

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
of enum order or wording. Errors include invalid keys, unterminated quotes,
quoted trailing text and explicit forbidden BOM/bare/escape/line-limit policies.
Warnings include duplicates, ignored statements and selected grammar's
portability diagnostics. A stripped, retained BOM prelude is informational.

Malformed statements become `Invalid` errors or `Ignored` warnings according to
configuration; primary diagnostics remain on items even when a strict/native
parse omits the side diagnostic vector. Explicit policy violations stay errors.
Multiline quotes follow the selected grammar rather than guessed author intent.

Portable unmatched quotes retain the opening physical line as invalid and resume
at the following line. Other grammars retain their own recovery boundaries and
literal fallback rules. The parser remains linear; repeated unbounded rescans
are not allowed.

`parse_strict` and `parse_strict_with` return the first error in source order.
Warning-only ignored statements do not fail strict parsing.

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
index; single lookups do not allocate one. Insertion separators use planned
surviving item kinds. Fields ending the preceding item belong before a
same-offset insertion; fields beginning the following item belong after it.
The original final-newline policy is retained unless a terminator is required
to represent a requested final bare declaration (Compose's EOF exception).

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

Comment generation follows the selected scanner's line breaks, so an internal
CR remains data in LF-only modes and comment/uncomment restoration stays exact.

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

An operation's edit batch is verified as a whole against one source snapshot;
individually valid edits do not waive final batch verification. Verification
always reparses using the document's exact configuration, including custom
overrides. A shared dialect-aware codec validates standalone LF/EOF tokens and
native source contexts. Value spelling is deferred until rename/export headers
are planned, and insertion spelling until its actual ending is known. Ending
fields account for generated separators. A selected entry may expose its own
blank/comment suffix as separate items, but cannot consume another declaration
or regroup an unselected item. Custom byte caps include the surrounding physical
line, including neighboring statements; same-line sets resolve against other
final values with bounded retries, so spelling is independent of operation order.
The complete output is always rechecked.

## Crate layout and platform

The repository is a Cargo workspace with the core package at the root
(`members = ["."]`). Fuzzing has an excluded, separate workspace. `adotenv`
has zero runtime dependencies, `#![no_std]` with `alloc`, and unconditional
`#![forbid(unsafe_code)]`. Default feature `std` can be disabled without losing
core parsing/editing/position APIs or `core::error::Error` implementations.
There is no I/O or environment/process integration under either mode.

The unsafe prohibition in `adotenv` is unconditional. A future opt-in,
std-only `adotenv-process` adapter will own the narrowly documented unsafe
function for applying validated values to the process environment. Future
file loading and map generation remain safe `adotenv` features, not current
core behavior. Later CLI/config/figment crates will join the workspace;
workspace crates will be versioned and released independently.

## Error types

0.2 fallible APIs use specific non-exhaustive `EditError` and `PositionError`
enums; strict parsing returns a `Diagnostic`. Each implements `Display` and
`core::error::Error` on MSRV 1.81. `ResolveError` covers 0.4 variable
resolution and command-callback failures; `LoadError` belongs to a future
loading milestone.
