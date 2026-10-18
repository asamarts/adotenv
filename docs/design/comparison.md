# Comparison and examples

Status: effective-key comparison and safe example generation are implemented in
the unreleased 0.5 tree. Neither operation expands values, consults the
environment, reads files or executes commands.

## Effective-key comparison

`Document::compare(&other)` returns a `Comparison` containing only changed
effective keys. It is intended for checking an environment file against an
example, a baseline or another configuration.

```text
Comparison
  differences() -> &[ComparisonEntry]
  len(), is_empty()

ComparisonEntry
  key: String
  left: Option<ComparedValue>
  right: Option<ComparedValue>
  kind: DifferenceKind

ComparedValue
  state: AssignmentState   Bare | Empty | Value
  value: Option<String>    decoded literal; None for Bare
  exported: bool

DifferenceKind
  OnlyLeft | OnlyRight | State | Value | Export
```

### Key selection

Each side contributes the keys of declarations that participate in lookup. A
key is read through that document's `get_entry`, so its own `Duplicates::First`
or `Duplicates::Last` policy decides which declaration is effective. A
nonparticipating declaration (for example an ignored bare key or Node's
`__proto__` object assignment) does not create an effective key.

Keys present on the left appear in left source order, followed by keys that
occur only on the right in right source order. Identical effective keys are
omitted. Because only effective declarations participate, a document with
duplicate keys compares its selected winner rather than every occurrence.

### Difference precedence

1. `OnlyLeft` / `OnlyRight` when one side has no effective declaration.
2. `State` when both exist but Bare, Empty and Value differ.
3. `Value` when state is equal but decoded values differ.
4. `Export` when state and value are equal but export prefixes differ.

Bare and empty are deliberately different: `KEY` is `Bare` with no value, while
`KEY=` is `Empty` with `Some("")`. An absent key is distinct from both. Quote
style alone is not a difference; decoded literal values are compared.

Comparison is allocation-owning and does not retain references to either
document. It is therefore convenient for reports and serialization. It performs
no verification or mutation.

## Safe example generation

`Document::example()` returns a new `String` with every assigned value removed:

- keys, export prefixes, spacing and inline comments remain;
- whole-line comments and blank lines remain byte-for-byte;
- line endings and a BOM remain unchanged;
- bare declarations remain bare;
- assigned values become `Empty` (`KEY=`, or `KEY=""` where a multiline,
  colon-separated, same-line or comment-sensitive grammar needs a placeholder);
- a Compose-style inline comment uses an empty quoted placeholder (`KEY=""`)
  because that grammar requires a value body before `#`; Compose's
  next-statement grammar then retains the comment as a following comment item,
  while other grammars keep it inline;
- invalid/ignored statements and entries with an empty key become
  indentation-preserving `#` comments.

The malformed-statement rule is a safety property, not formatting: a line the
parser cannot classify must not be copied into an example because it may contain
a secret. Replacing its content with `#` keeps its line structure and indentation
while discarding the raw statement. Whole-line comments are considered
intentional documentation and are kept, including comment text that resembles a
value.

Example generation is independent of `Document::format`: it intentionally
changes declaration values and item classifications, so it does not use the
trivia verifier. Tests and the format fuzz target reparse the generated text and
check entry keys, bare/empty states, inline comments and the absence of
malformed/ignored items.

Example:

```rust
use adotenv::parse;

let source = "# app\nA=1\nB=\"two\" # note\nBARE\nBAD KEY=secret\n";
let (doc, _) = parse(source);
assert_eq!(doc.example(), "# app\nA=\nB= # note\nBARE\n#\n");
```

Comparison:

```rust
use adotenv::{parse_strict, DifferenceKind};

let left = parse_strict("A=1\nB=\n").unwrap();
let right = parse_strict("A=2\nB\nC=3\n").unwrap();
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
        ("C", DifferenceKind::OnlyRight),
    ]
);
```

## Remaining 0.5 work

- Blank-line normalization with explicit group semantics.
- Quote-style rewriting that proves every decoded value identical.
- Sorting inside groups while keeping comments attached and respecting
  `${VAR}` dependencies.
- CLI surfaces for `fmt`, comparison and example generation (0.7).
