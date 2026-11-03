# Parsing and lookup

A `Document` holds one source string and the items a dialect reads from it.
It never changes the source: printing a document returns the original bytes.

## Parse functions

| Function | Returns | On errors |
|---|---|---|
| `parse(source)` | `(Document, Vec<Diagnostic>)` | Keeps the bad text as an `Invalid` or `Ignored` item and reports it |
| `parse_strict(source)` | `Result<Document, Diagnostic>` | Returns the first error |
| `parse_owned(String)` | `(Document<'static>, Vec<Diagnostic>)` | Like `parse`, but the document owns the source |
| `parse_values(source)` | `Result<BTreeMap<String, String>, Diagnostic>` | Returns the first error |

Each has a `_with` variant that takes a [`Dialect`](../design/dialects.md):
`parse_with`, `parse_strict_with`, `parse_owned_with` and `parse_values_with`.
Without one, the Portable dialect is used.

`parse` never fails. It recovers at the smallest safe boundary, so one broken
line does not hide the rest of the file:

```rust
use adotenv::{DiagnosticCode, ItemKind};

let source = "BAD KEY=value\nGOOD=kept\n";
let (doc, diagnostics) = adotenv::parse(source);

assert_eq!(doc.get("GOOD"), Some("kept"));
assert_eq!(doc.items().next().unwrap().kind(), ItemKind::Invalid);
assert_eq!(diagnostics[0].code(), DiagnosticCode::InvalidKey);
assert_eq!(doc.source(), source);
```

`parse_strict` fails only on error diagnostics. Warnings and info diagnostics
do not fail it, and it does not return them. The codes are listed in
[Diagnostic codes](../reference/diagnostics.md).

`parse_values` is the fast path when only final values matter. It keeps no
items or spans, applies the dialect's duplicate and bare-key rules, and returns
the first error. The map, sorted by key, holds the same keys and values as
`effective_entries` below. It uses much less memory than a document.

`parse_owned` moves the source into the document. `Document::into_owned`
copies the source only when the document still borrows it. `Document::reparse`
reads the same source with another dialect.

## Items and entries

`Document::items()` yields every item in source order. Items cover the source
without gaps, apart from a leading byte-order mark:

| `ItemKind` | Item |
|---|---|
| `Blank` | An empty or whitespace-only line |
| `Comment` | A whole-line comment |
| `Entry` | A declaration: `KEY=value` or a bare `KEY` |
| `Invalid` | A statement with an error, kept as written |
| `Ignored` | A statement the dialect skips, with a warning |

`Document::entries()` yields the declarations, including duplicates and
declarations that do not take part in lookup. `Item` and `Entry` are borrowed
views. Their `id()` is a zero-based index into `items()` or `entries()` of this
document, and `Entry::item_id()` gives the item that holds an entry. IDs
belong to one document: a reparsed document, or one parsed from edited text,
has its own.

An `Item` has `kind()`, `span()`, `raw()`, `line_ending()`, `content_span()`,
`comment_text()` for comments, `diagnostic()` and `invalid_code()` for invalid
and ignored items and for entries that break a dialect policy, and `entry()` for
declarations.

Most items are one line, including its line ending. A multiline quoted value
makes one item of several lines, and some dialects, such as Compose, can read
several statements from one line. `line_ending()` is `Lf`, `CrLf`, `Cr` or
`None`. Portable, Node dotenv and python-dotenv end a line at a lone CR, and
Portable reports it as `dotenv.lone-cr`. Compose and Docker end lines only at
LF and keep a lone CR as text inside the line.

## Lookup

A dialect decides which declarations take part in lookup and which duplicate
wins. In Portable, every declaration takes part and the last one wins.

```rust
let doc = adotenv::parse_strict("A=1\nA=2\nB=3\n").unwrap();

assert_eq!(doc.get("A"), Some("2"));
assert_eq!(doc.get_entry("A").unwrap().line(), 2);
assert_eq!(doc.get_all("A").count(), 2);
assert!(doc.unique("A").is_err());

let winners: Vec<_> = doc
    .effective_entries()
    .map(|entry| (entry.key(), entry.value()))
    .collect();
assert_eq!(winners, [("A", "2"), ("B", "3")]);
```

| Method | Returns |
|---|---|
| `get(key)` | The effective value |
| `get_entry(key)` | The declaration that wins lookup |
| `get_all(key)` | Every declaration of the key, in source order |
| `unique(key)` | The only declaration, or `EditError::Missing` or `EditError::Duplicate` |
| `effective_entries()` | The winning declaration of each key that has an effective value, in source order |
| `lookup_index()` | A `LookupIndex` for many lookups |

`get` scans the entries. For many lookups, build `lookup_index()` once; it has
`get`, `get_entry`, `len` and `is_empty`. `effective_entries`, `parse_values`
and the index leave out empty keys, and `effective_entries` also leaves out a
key whose winning declaration is bare, unless the dialect reads bare keys as
empty. Compose still exposes an empty key through `get("")`.

### Bare and empty declarations

`KEY` is a bare declaration with no value. `KEY=` and `KEY=""` are empty
assignments. The two stay distinct:

```rust
use adotenv::AssignmentState;

let doc = adotenv::parse_strict("BARE\nEMPTY=\n").unwrap();

assert_eq!(doc.get_entry("BARE").unwrap().state(), AssignmentState::Bare);
assert_eq!(doc.get("BARE"), None);
assert_eq!(doc.get_entry("EMPTY").unwrap().state(), AssignmentState::Empty);
assert_eq!(doc.get("EMPTY"), Some(""));
```

`BareKeys::Empty` makes a bare declaration read as an empty value in lookup,
while the entry itself stays bare: its `state()` is `Bare` and its `value()` is
`None`. A bare declaration never reads the process environment.

## Raw tokens

An `Entry` gives the decoded value and the source tokens behind it:

```rust
let source = "export A=\"two words\" # note\n";
let doc = adotenv::parse_strict(source).unwrap();
let entry = doc.unique("A").unwrap();

assert!(entry.exported());
assert_eq!(&source[entry.key_span()], "A");
assert_eq!(entry.raw_value(), Some("\"two words\""));
assert_eq!(entry.value(), Some("two words"));
assert_eq!(entry.inline_comment(), Some("# note"));
assert_eq!(entry.decoded_span_to_raw(0..3).unwrap(), 10..13);
```

`Entry::tokens()` returns `EntryTokens`, with the byte range of every part of
the declaration: `indent`, `export`, `space_after_export`, `key`,
`space_before_eq`, `eq`, `space_after_eq`, `value`, `body`, `quoted_tail`,
`trailing_space` and `inline_comment`. `quote()` gives the quote style and
`state()` the assignment state. `decoded_offset_to_raw` and
`decoded_span_to_raw` map positions in the decoded value back to the source,
accounting for escapes.

## Spans and positions

Every span is a byte range into the original source, byte-order mark
included. Entry and diagnostic lines are one-based. They count LF, CRLF and a
lone CR as line ends in every dialect, the same way as `LineIndex`, so in
Compose and Docker a lone CR starts a new line number even though it does not
end a statement.

`LineIndex` converts between byte offsets and zero-based line and column
positions. The column encoding is explicit:

```rust
use adotenv::{ColumnEncoding, Position};

let doc = adotenv::parse_strict("A=é\n").unwrap();
let index = doc.line_index();
let position = index.to_position(2, ColumnEncoding::Scalar).unwrap();

assert_eq!(position, Position { line: 0, column: 2 });
assert_eq!(index.to_offset(position, ColumnEncoding::Scalar).unwrap(), 2);
assert_eq!(index.to_position(4, ColumnEncoding::Utf16).unwrap().column, 3);
```

`ColumnEncoding` is `Utf8` (bytes), `Scalar` (Unicode scalar values) or
`Utf16` (code units, as in the Language Server Protocol). Scalar columns are
not terminal cell widths. A position that is out of range, between CR and LF,
inside a UTF-8 sequence with `Scalar` or `Utf16` columns, or between the halves
of a surrogate pair returns a `PositionError`; positions are never rounded or
clamped. `Utf8` columns can address any byte.
`Position::one_based()` converts a position for display.

`Document::line_index()` builds a new index on each call, so keep it when
converting many positions. A leading byte-order mark is part of line 0: it
takes 3 `Utf8` columns, or 1 `Scalar` or `Utf16` column, before the first
character.

## Groups and comments

A group is a run of items between blank lines. These views help tools that
reason about layout:

- `Document::groups()` iterates the groups. Each `Group` has `items()`,
  `entries()`, `comments()`, `leading_comments()` and `span()`.
- `Document::header()` is the run of items before the first entry, if any.
  Unlike a group, it can contain blank lines.
- `Entry::leading_comments()` yields the whole-line comments directly above an
  entry. A blank line ends the attachment.

```rust
let doc = adotenv::parse_strict("# database\nDB=x\n\n# cache\nCACHE=y\n").unwrap();
let comments: Vec<_> = doc
    .get_entry("CACHE")
    .unwrap()
    .leading_comments()
    .filter_map(|item| item.comment_text())
    .collect();

assert_eq!(doc.groups().count(), 2);
assert_eq!(comments, [" cache"]);
```

## Untrusted input

Input must be UTF-8. A `Document` retains item and token metadata for the
whole source, so check the size of untrusted input before parsing it.
`Dialect::with_line_byte_limit` rejects physical lines longer than a limit with
`dotenv.line-too-long`; the Docker preset uses 65,535 bytes and the others have
no limit. See [Security and limits](../reference/security.md).
