# Formatting and sorting

`Document::format` and `Document::sort` return a `FormatResult`: the verified
output and the text edits that produce it, against the original snapshot. The
complete output is parsed again with the same dialect before it is returned.
A change that would alter a declaration fails with
`FormatError::WouldChangeMeaning`.

`FormatResult` has `as_str()`, `into_string()`, `edits()`, `into_edits()` and
`is_empty()`, which is true when nothing changed. The edits follow the order
described in [Applying text edits](../guide/editing.md#applying-text-edits).

## Format

`format` changes trivia only:

```rust
use adotenv::{parse_strict, FormatOptions};

let source = "# app   \n\nA=1   \nB =  2 # note   \n";
let doc = parse_strict(source).unwrap();
let formatted = doc.format(&FormatOptions::normalize()).unwrap();
assert_eq!(formatted.as_str(), "# app\n\nA = 1\nB = 2 # note\n");
```

`FormatOptions::default()` changes nothing. Each field has a `with_*`
builder:

| Field | Choices |
|---|---|
| `equals_spacing` | `EqualsSpacing::Preserve`, `Spaces` (one space on each side) or `None` (no spaces) |
| `trim_trailing_whitespace` | Remove trailing spaces and tabs |
| `blank_lines` | `BlankLines::Preserve` or `Collapse` (each run becomes one blank line) |
| `quote_style` | `QuoteStyle::Preserve`, `Double`, `Single` or `Unquoted` |
| `final_newline` | `FinalNewline::Preserve`, `Ensure` or `Remove` |

Two presets cover common needs:

- `FormatOptions::normalize()`: spaces around `=`, trailing whitespace trimmed
  and a final line break ensured.
- `FormatOptions::normalize_groups()`: the same, plus blank-line runs
  collapsed. `adotenv-cli fmt` uses this profile.

### What formatting keeps

- Values never change. A quote change is applied only when the decoded value
  stays exactly the same, and empty assigned values keep their quote style.
- Invalid and ignored statements are never rewritten.
- Trimming never removes a line or touches whitespace inside a multiline
  value. A whitespace-only last line without a line ending stays as is unless
  `FinalNewline::Ensure` turns it into an empty blank line.
- In raw-value dialects such as Docker, entry lines stay untouched, because
  every byte after `=` is part of the value. Comments and blank lines are
  still formatted.
- Empty assignments that could become indentation, and boundaries between
  mixed lone-CR line endings, are preserved.

### Line endings

`FinalNewline::Ensure` adds an LF when the output does not end in a line
break, even in a CRLF file. It adds nothing when collapsing a trailing
blank-line run already leaves the output ending in a line break.
`FinalNewline::Remove` removes one final LF, CRLF or lone CR.

### Quote styles

`QuoteStyle::Double` and `Single` re-encode each value whose decoded text
survives in the new style, multiline values included. A multiline value can
then move onto one line with an escaped line break:

```rust
use adotenv::{parse_strict, FormatOptions, QuoteStyle};

let doc = parse_strict("A='x\ny'\n").unwrap();
let options = FormatOptions::default().with_quote_style(QuoteStyle::Double);
assert_eq!(doc.format(&options).unwrap().as_str(), "A=\"x\\ny\"\n");
```

`QuoteStyle::Unquoted` applies only when every other option is `Preserve`;
combined with other changes it is skipped.

## Sort

`Document::sort` orders declarations inside each blank-line group.

- Comments above the first entry stay as a group header.
- Comments above later entries move with those entries.
- Inline comments and multiline values move intact.
- Keys compare by bytes, so uppercase letters sort before lowercase ones:
  `b`, `B`, `a` sorts to `B`, `a`, `b`.
- When one key references another in the same group, the two keep their
  current relative order. A reference then sees the same declaration after
  sorting, so resolved values do not change under sequential or whole-file
  scope.
- References to keys in other groups do not constrain the order.
- Sorting never adds or removes a final line ending. When the file ends without
  one and its last declaration moves up, that declaration takes the line ending
  of the declaration that becomes last: `B=2\nA=1` sorts to `A=1\nB=2`.

These groups are left unchanged, byte for byte:

- groups with duplicate keys or empty keys;
- groups with invalid or ignored statements;
- groups with a self-reference or a reference cycle;
- groups where a declaration shares its physical line with the next statement.

```rust
let source = "# header\nC=3\nB=${A}\n# A note\nA=1\n# bottom\n";
let doc = adotenv::parse_strict(source).unwrap();
assert_eq!(
    doc.sort().unwrap().as_str(),
    "# header\nB=${A}\n# A note\nA=1\nC=3\n# bottom\n"
);
```

Moving `A=1` above `B=${A}` would change the value of `B` under sequential
resolution, so `B` stays above `A`.

Sorting does not format spacing, expand values, read files or inspect the
environment. Run `format` on the sorted output when both changes are wanted.
