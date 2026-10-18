# Group sorting

Status: comment-aware dependency sorting is implemented in the unreleased 0.5
tree as `Document::sort`. It is separate from `Document::format`, because
sorting changes source order rather than only trivia.

## Contract

`Document::sort()` returns a verified `FormatResult` with one replacement edit
per changed blank-line group. Trivia formatting is not applied implicitly;
callers can format the sorted text separately.

For each group, entries are ordered by key unless a value creates a dependency.
A dependency is any variable reference in the entry value whose name is also a
key in the same group. The referenced key is placed before the dependent key.
Multiple ready entries are chosen in lexical key order, so output is deterministic.

Dependencies on keys in other groups, on the caller lookup, or on keys that do
not exist are not ordering constraints. Command references do not create
dependencies. References are scanned conservatively, including nested or
quote/escape contexts; a dependency may therefore constrain sorting even where a
particular expansion policy would treat it literally.

## Comment attachment

- Comments before the first entry of a group are a fixed group header.
- Comments immediately before a later entry move with that entry.
- Trailing comments after the last entry stay at the bottom of the group.
- Inline comments and multiline value text are part of the moved entry.

This avoids guessed ownership while keeping the common leading-comment and
trailing-comment layouts stable.

## Groups left unchanged

A group is not sorted when it:

- has fewer than two entries;
- contains malformed or ignored items;
- has an empty key;
- has duplicate keys; or
- has a dependency cycle.

Other groups can still sort. This makes the operation deterministic and
failure-safe rather than partially sorting an ambiguous group.

## Verification

The output is reparsed and checked group-by-group:

1. BOM and group count match.
2. Each changed group’s raw text equals the planned reordered text.
3. Entry keys appear in the planned order and entry meanings (value, state,
   quote, export, lookup participation and inline comment) are preserved.
4. The comment text multiset is unchanged within each group.
5. Groups without a plan remain byte-for-byte unchanged.

Edit ranges refer to the original source snapshot and can be replayed in order.
The generated text is also reparsed before it is returned.

## Example

```rust
use adotenv::parse_strict;

let source = "# header\nB=${A}\n# A note\nA=1\n# bottom\n";
let doc = parse_strict(source).unwrap();
let sorted = doc.sort().unwrap();
assert_eq!(
    sorted.as_str(),
    "# header\n# A note\nA=1\nB=${A}\n# bottom\n"
);
```

## Remaining 0.5 work

None for the planned formatter primitives. Release preparation, CLI surfacing
and sustained fuzzing remain.
