# Formatting and sorting

`Document::format` and `Document::sort` return a `FormatResult`: the verified
output and the text edits that produce it, with ranges into the original
source. The complete output is parsed again with the same dialect before it is
returned. A change that would make an item or a group read differently there
is left out, and the output is checked again. Both methods give up after a few
rounds, with `FormatError::WouldChangeMeaning`, so the work stays linear in the
size of the file; no known input gets that far.

`FormatResult` has `as_str()`, `into_string()`, `edits()`, `into_edits()` and
`is_empty()`, which is true when nothing changed. The edits follow the order
described in [Applying text edits](../guide/editing.md#applying-text-edits).

## Format

`format` changes layout only:

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
| `equals_spacing` | `EqualsSpacing::Preserve`, `Spaces` (one space on each side) or `None` (no spaces, but the space after the separator stays when an inline comment follows) |
| `trim_trailing_whitespace` | `true` removes trailing spaces and tabs |
| `blank_lines` | `BlankLines::Preserve` or `Collapse` (each run becomes one blank line) |
| `quote_style` | `QuoteStyle::Preserve`, `Double`, `Single` or `Unquoted` |
| `final_newline` | `FinalNewline::Preserve`, `Ensure` or `Remove` |

Two presets cover common needs:

- `FormatOptions::normalize()`: spaces around `=`, trailing whitespace trimmed
  and a final line break ensured.
- `FormatOptions::normalize_groups()`: the same, plus blank-line runs
  collapsed. `adotenv-cli fmt` uses this profile.

### What formatting keeps

- Decoded values never change. A quote change is applied only when the
  decoded value stays exactly the same. Empty values, bare declarations and
  empty keys keep their form.
- Separator spacing stays as it is for empty keys and, in Node dotenv, for
  `KEY: value` declarations, whose colon needs no space before it and
  whitespace after it. The space after `=` also stays when the value is empty
  and no inline comment follows, and when the value starts with `#`, as in
  `A=#x # note`, because changing it could change how the line reads.
- A change that reads differently once the whole output is parsed again is
  left out, with the other changes to the same item. For example, Compose
  reads a bare key on the last line of a file without a final line ending as
  the value of an empty key, so `FinalNewline::Ensure` adds nothing after it.
- Invalid and ignored statements are never rewritten.
- Trimming never removes a line or touches whitespace inside a multiline
  value. A whitespace-only last line without a line ending stays as is unless
  `FinalNewline::Ensure` turns it into an empty blank line.
- In raw-value dialects such as Docker, entry lines stay untouched, because
  every byte after `=` is part of the value. Comments and blank lines are
  still formatted.
- Trimming leaves alone lines that end with or follow a lone CR line ending,
  and an empty or bare declaration without an inline comment that is followed
  by an indented line, together with that line, so the next parse reads the
  same lines.

### Line endings

`FinalNewline::Ensure` adds an LF when the output ends with neither LF nor
CR, even in a CRLF file. It adds nothing to a document with no items, or when
collapsing a trailing blank-line run already leaves the output ending in a line
break. `FinalNewline::Remove` removes one final LF, CRLF or lone CR. It removes
nothing when the last item is a blank line, because that would remove the
blank line.

### Quote styles

`QuoteStyle::Double` and `Single` re-encode each value whose decoded text
survives in the new style, multiline values included. A multiline value can
then move onto one line with an escaped line break, or the other way around:

```rust
use adotenv::{parse_strict, FormatOptions, QuoteStyle};

let doc = parse_strict("A='x\ny'\n").unwrap();
let options = FormatOptions::default().with_quote_style(QuoteStyle::Double);
assert_eq!(doc.format(&options).unwrap().as_str(), "A=\"x\\ny\"\n");
```

`QuoteStyle::Unquoted` applies only when every other option keeps its
default (`Preserve` or `false`); combined with other changes it is skipped.
Raw-value dialects such as Docker are never re-quoted.

A value that contains `$` keeps its quoting, because quotes decide whether a
reference expands: with `ExpandOptions::portable()`, `A=${HOST}` expands
`${HOST}`, while `A='${HOST}'` keeps it as text. A quote change therefore never
changes resolved values.

## Sort

`Document::sort` orders declarations inside each blank-line group.

- Comments above the first entry stay as a group header.
- Comments above later entries move with those entries.
- Comments below the last entry stay at the bottom.
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
- groups where a declaration shares its physical line with the next statement;
- groups whose sorted text would read differently once the whole output is
  parsed again. In Compose, `G\nB=2` without a final line ending stays as it
  is, because `G` on the last line would read as the value of an empty key. A
  lone CR line ending moved in front of an LF would join it into a CRLF.

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
