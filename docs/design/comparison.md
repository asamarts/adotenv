# Comparison and examples

## Compare documents

`Document::compare` reports effective-key differences between two documents.
Each document uses its own dialect for duplicates and lookup participation.

Difference kinds:

- value changed;
- assignment state changed, such as assigned, empty or bare;
- key exists only on the left;
- key exists only on the right;
- export state changed.

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

Comparison is literal. It does not expand values, read files, inspect the
environment or run commands.

## Generate an example

`Document::example` replaces assigned values while preserving:

- comments and blank lines;
- keys, export prefixes and formatting;
- line endings and a BOM;
- the assignment state of bare declarations.

Malformed statements and empty-key entries become indentation-preserving
comments. Value text from those forms cannot leak into the output.

```rust
let source = "# app\nA=1\nB=\"two\" # note\nBARE\nBAD KEY=secret\n";
assert_eq!(
    adotenv::parse(source).0.example(),
    "# app\nA=\nB= # note\nBARE\n#\n"
);
```

Empty quoted values keep a placeholder when needed to keep an inline comment
attached. The result is intended for examples and documentation, not for use as
a runtime configuration.
