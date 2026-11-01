# Comparison and examples

## Compare documents

`Document::compare` reports the keys whose effective declarations differ
between two documents. Each document uses its own dialect for duplicates and
lookup participation, so a Node file can be compared with a Portable one.

```rust
use adotenv::{parse_strict, DifferenceKind};

let left = parse_strict("A=1\nB=\nC=3\n").unwrap();
let right = parse_strict("A=2\nB\nD=4\n").unwrap();
let comparison = left.compare(&right);
let changes: Vec<_> = comparison
    .differences()
    .iter()
    .map(|entry| (entry.key.as_str(), entry.kind))
    .collect();

assert_eq!(
    changes,
    [
        ("A", DifferenceKind::Value),
        ("B", DifferenceKind::State),
        ("C", DifferenceKind::OnlyLeft),
        ("D", DifferenceKind::OnlyRight),
    ]
);
```

`Comparison::differences()` returns `ComparisonEntry` values with `key`,
`left`, `right` and `kind`. `left` and `right` are `Option<ComparedValue>`,
with the assignment `state`, the decoded `value` and whether the declaration is
`exported`. Keys that agree are left out. `len()` and `is_empty()` count the
differences.

| `DifferenceKind` | The key |
|---|---|
| `OnlyLeft` | Has a declaration that takes part in lookup only in the left document |
| `OnlyRight` | Has a declaration that takes part in lookup only in the right document |
| `State` | Changes between a non-empty value, an empty value and a bare declaration |
| `Value` | Has a different decoded value |
| `Export` | Gains or loses the `export` prefix |

Each entry reports the first kind that applies, in that order, so a key whose
value and `export` prefix both differ reports `Value`. Entries list the left
document's keys in the order of their first declaration that takes part in
lookup, then the keys that take part in lookup only in the right document, in
the same order. A bare declaration that does not take part in lookup, as in
Node, Compose and Docker, counts as absent. Quoting, spacing, comments and
source order are not compared. Empty keys, which some dialects accept, are
compared too.

Comparison is literal. It does not expand values, read files, inspect the
environment or run commands. `adotenv-cli diff` prints the same comparison; see
[Comparison](cli.md#comparison).

## Generate an example

`Document::example` removes assigned values while preserving:

- whole-line comments and blank lines;
- keys, `export` prefixes and the spacing before each value;
- inline comments;
- line endings and a BOM;
- bare declarations, which stay bare.

Invalid statements, ignored statements and empty-key entries become `#`
comments that keep only their indentation and line ending. Value text from
those forms cannot leak into the output. Statements that shared one physical
line are put on separate lines. In Compose, text right after a closing quote is
the next statement, so `A="x"tail` gives `A=""` and a bare `tail` on the next
line.

```rust
let source = "# app\nA=1\nB=\"two\" # note\nBARE\nBAD KEY=secret\n";
assert_eq!(
    adotenv::parse(source).0.example(),
    "# app\nA=\nB= # note\nBARE\n#\n"
);
```

Where an empty value would change how the line reads, the value becomes `""`
instead of nothing. That happens when the separator is `:`, when the value
spans lines, when another statement follows on the same line, or when the
entry has an inline comment in a dialect that lets comments follow quotes
(Node, Python and Compose). Raw-value dialects such as Docker never get a
placeholder. In a custom dialect where an inline comment cannot follow an
empty value, the comment moves to its own line.

```rust
use adotenv::{parse_with, Dialect};

let (doc, _) = parse_with("A=1 # note\n", Dialect::NODE_DOTENV);
assert_eq!(doc.example(), "A=\"\" # note\n");
```

Comments are kept verbatim, including anything secret written in them. The
result is meant for examples and documentation, not for use as a runtime
configuration.
