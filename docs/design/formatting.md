# Structure and formatting

Status: the 0.5 structure views and trivia-only verified formatter are
implemented in the unreleased tree. Comparison and example generation are
documented in [comparison.md](comparison.md); quoting changes, blank-line
normalization and sorting remain future work.

The formatter is deliberately smaller than a pretty-printer. It changes only
syntax trivia that can be selected explicitly, then proves that every
declaration and structural boundary survived under the document's exact
dialect. It never guesses ownership, rewrites values, loads files or expands
references.

## Structure views

The document model remains a flat ordered list of `Item`s. Structure is a set of
borrowed views, not a second syntax tree:

```text
Group<'doc>
  document + item range
  items()             exact-size, double-ended
  entries()           declarations in the run
  comments()          whole-line comments in the run
  leading_comments()  contiguous comments before the first entry
  span()              first item start .. last item end
```

`Document::groups()` yields maximal runs of nonblank items in source order.
Blank items are separators and belong to no group. Empty documents produce no
groups. A file header is `Document::header()`, the item range before the first
`Entry`; it may contain comments and blank lines. A header is `None` when the
first item is already an entry or the document has no items. The BOM prelude is
never part of a view.

`Entry::leading_comments()` walks backward from an entry through immediately
preceding `Comment` items. It stops at a blank line, another entry or a
malformed statement. Inline comments are token data on the entry
(`inline_comment`, `Entry::inline_comment`) and are not repeated by
`leading_comments`.

Views use zero-based snapshot-local item IDs. They are valid only for the
document snapshot and selected dialect that produced them. A group never
contains a blank item, and groups partition exactly the nonblank items; tests
and the format fuzz target assert both invariants.

No automatic edit shortcut removes or moves attached comments yet. The views are
inspection primitives; explicit item selectors remain the only way to remove a
comment.

## Formatting model

An edit is still `TextEdit { range, replacement }` into the original source.
`Document::format(&FormatOptions)` builds a sorted, nonoverlapping batch,
applies it, reparses the complete output with `parse_with` and the exact retained
dialect, and only then returns:

```text
FormatResult
  edits()       borrowed source-relative TextEdits
  into_edits()  consuming form of the same batch
  as_str()      verified output
  into_string() consuming verified output
  is_empty()    no bytes changed
```

`FormatError::WouldChangeMeaning` means the requested batch could not be
verified. It is non-exhaustive so future safety checks can be added.

### Options

| Option | Values | Contract |
|---|---|---|
| `equals_spacing` | `Preserve`, `Spaces`, `None` | Normalize one ASCII space before/after applicable assignment separators, remove it, or leave it alone. |
| `trim_trailing_whitespace` | `bool` | Remove trailing spaces/tabs from whole-line comments, blank lines and the final line of entries. |
| `final_newline` | `Preserve`, `Ensure`, `Remove` | Leave EOF bytes, add one LF when missing, or remove one terminator from a nonblank final item. |

`FormatOptions::default()` preserves everything. `FormatOptions::normalize()`
uses `Spaces`, trimming and `Ensure`; it is idempotent.

`FinalNewline::Remove` intentionally removes only one terminator. A file ending
in a blank line keeps that blank line: removing its terminator would change
group structure rather than only the final newline. Empty documents and
BOM-only documents are unchanged by `Ensure`.

### What is never rewritten

- Raw-value dialects (`raw_values`): leading/trailing spaces are value data.
- Multiline value bodies: only the final line's trailing trivia can be trimmed.
- Invalid and ignored statements: no trivia edits are planned for them.
- Empty assignments (`KEY=` / a bare key) followed by an indented line: trailing
  spaces can become the next line's indentation in continuation-capable
  grammars, so the assignment and that next item are protected.
- Empty Compose keys: adding spacing before `=` would change indentation.
- Values beginning with `#` where whitespace would turn a literal value into a
  comment: the after-`=` edit is skipped.
- After an empty assignment: spacing is not normalized because it may be
  structural indentation; inline comments are exempt.
- Lone-CR line endings and mixed CR/LF boundaries: trimming is skipped when it
  could expose or merge a different physical line ending.
- A comment's separating space before `#` is never removed; only trailing
  spaces after the comment text are trimmed.
- Trailing spaces inside a quoted value are never touched.

These are safety rules, not silent normalizations. Conservative skipping keeps
the result valid and idempotent; an unverifiable batch is an error.

## Verification

`same_meaning` compares the original and reparsed documents:

1. BOM presence.
2. Item count and ordered `ItemKind` values, so blank/comment/declaration
   boundaries cannot merge or split.
3. Comment text after trailing spaces/tabs, so comments may be trimmed but not
   edited.
4. Exact content (excluding the line ending) of invalid and ignored statements.
5. Every entry's key, decoded value, assignment state, quote style, export
   prefix and lookup participation.

The edited batch is checked for sorted, nonoverlapping, valid UTF-8 ranges before
application. The output is then reparsed; verification does not rely on the
formatter's own assumptions about token boundaries.

The fuzz target additionally checks that edits replay to the returned text and
that normal policies are idempotent. `FinalNewline::Remove` is excluded from
the idempotence assertion because removing one of several final line breaks is
intentionally a one-terminator operation; it still preserves declarations and
item structure.

## Examples

```rust
use adotenv::{parse_strict, FormatOptions};

let doc = parse_strict("# app   \n\nA=1   \nB =  2 # note   \n").unwrap();
let formatted = doc.format(&FormatOptions::normalize()).unwrap();
assert_eq!(formatted.as_str(), "# app\n\nA = 1\nB = 2 # note\n");
```

Spacing without trimming:

```rust
use adotenv::{parse_strict, EqualsSpacing, FinalNewline, FormatOptions};

let doc = parse_strict("A=1\nB =  2\n").unwrap();
let formatted = doc
    .format(&FormatOptions {
        equals_spacing: EqualsSpacing::Spaces,
        trim_trailing_whitespace: false,
        final_newline: FinalNewline::Preserve,
    })
    .unwrap();
assert_eq!(formatted.as_str(), "A = 1\nB = 2\n");
```

## Future 0.5 work

- Quoting changes that prove every decoded value identical.
- Blank-line normalization with explicit group semantics.
- Sorting inside groups, keeping attached comments and respecting `${VAR}`
  dependencies.
- A CLI formatter and comparison/example command surfaces once the library
  primitives are stable.

Evidence for this slice is recorded in
[quality.md](quality.md#structure-and-formatting-slice-05-unreleased).
