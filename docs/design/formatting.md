# Formatting and sorting

Both operations return verified edits against the original snapshot.

## Format

`Document::format` changes selected trivia only:

- spacing around assignment separators;
- trailing spaces and tabs;
- blank-line runs;
- quote style when it can be changed without changing the value;
- the final newline.

```rust
use adotenv::{parse_strict, FormatOptions};

let source = "# app   \n\nA=1   \nB =  2 # note   \n";
let doc = parse_strict(source).unwrap();
let formatted = doc.format(&FormatOptions::normalize()).unwrap();
assert_eq!(formatted.as_str(), "# app\n\nA = 1\nB = 2 # note\n");
```

The complete output is parsed again with the same dialect. Formatting fails if it
changes declarations, comment attachment, item kinds or malformed statements.

Options include `EqualsSpacing`, `BlankLines`, `QuoteStyle`, `FinalNewline` and
`FormatOptions::normalize_groups`.

Trimming never removes a line. A whitespace-only last line without a line
ending stays as is unless `FinalNewline::Ensure` turns it into an empty blank
line. `FinalNewline::Ensure` adds nothing when collapsing a trailing blank-line
run already leaves the output ending in a line break.

Quote changes are accepted only when the decoded value remains exact. Empty
assigned values keep their quote style. Raw-value dialects, multiline bodies,
invalid statements, empty assignments that could become indentation and mixed
lone-CR boundaries are preserved.

## Sort

`Document::sort` orders declarations inside each blank-line group.

- Comments above the first entry stay as a group header.
- Comments above later entries move with those entries.
- Inline comments and multiline values move intact.
- When one key references another in the same group, the two keep their
  current relative order. Other keys are ordered by key. A reference sees the
  same declaration after sorting, so resolved values do not change under
  sequential or whole-file scope.
- References to keys in other groups do not constrain the order.
- Duplicate keys, empty keys, malformed items, self-references and reference
  cycles leave the group unchanged. So does a declaration that shares its
  physical line with the next statement.
- Sorting never adds or removes a final line ending. When the file ends without
  one and its last declaration moves up, that declaration takes the line ending
  of the declaration that becomes last: `B=2\nA=1` sorts to `A=1\nB=2`.

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
environment. Use `format` separately when both changes are wanted.
