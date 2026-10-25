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

Quote changes are accepted only when the decoded value remains exact. Empty
assigned values keep their quote style. Raw-value dialects, multiline bodies,
invalid statements, empty assignments that could become indentation and mixed
lone-CR boundaries are preserved.

## Sort

`Document::sort` orders declarations inside each blank-line group.

- Comments above the first entry stay as a group header.
- Comments above later entries move with those entries.
- Inline comments and multiline values move intact.
- References inside a group are ordered before their dependents.
- Duplicate keys, empty keys, malformed items and dependency cycles leave the
  group unchanged.

```rust
let source = "# header\nB=${A}\n# A note\nA=1\n# bottom\n";
let doc = adotenv::parse_strict(source).unwrap();
assert_eq!(
    doc.sort().unwrap().as_str(),
    "# header\n# A note\nA=1\nB=${A}\n# bottom\n"
);
```

Sorting does not format spacing, expand values, read files or inspect the
environment. Use `format` separately when both changes are wanted.
