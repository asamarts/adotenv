# Editing

Edits work on one parsed document and never modify it. Every edit method
returns `Result<VerifiedEdits, EditError>`. Before a result is returned, its
complete output is parsed again with the same dialect and checked: the
requested change happened, and unrelated declarations, comments, item kinds
and invalid statements did not change. If the check fails, the edit fails and
nothing is returned. To keep editing, parse the output again.

## Single edits

Apart from `insert` and `uncomment`, these methods select a declaration by
key. The key must be declared exactly once; otherwise they return
`EditError::Missing` or `EditError::Duplicate`.

| Method | Change |
|---|---|
| `set(key, value)` | Set the literal value, quoting and escaping it as the dialect needs |
| `remove(key)` | Remove the declaration, keeping the comments around it |
| `rename(key, new_key)` | Rename the key |
| `insert(key, value, position)` | Add a declaration of a key that is not declared yet; `None` adds a bare key |
| `comment(key)` | Comment out every physical line of the declaration |
| `uncomment(item)` | Remove the marker of a comment item, selected by item ID |
| `set_export(key, enabled)` | Add or remove the `export` prefix |

```rust
use adotenv::InsertPosition;

let doc = adotenv::parse_strict("# app\nA=1\nC=3\n").unwrap();

assert_eq!(doc.set("A", "one").unwrap().as_str(), "# app\nA=one\nC=3\n");
assert_eq!(doc.remove("C").unwrap().as_str(), "# app\nA=1\n");
assert_eq!(doc.set_export("A", true).unwrap().as_str(), "# app\nexport A=1\nC=3\n");

let inserted = doc.insert("B", Some("2"), InsertPosition::Sorted).unwrap();
assert_eq!(inserted.as_str(), "# app\nA=1\nB=2\nC=3\n");
```

Values are literal. `set("URL", "${HOST}/api")` writes text that reads back as
`${HOST}/api` in the same dialect, quoting or escaping it when needed. The
reference is written as typed and not escaped against expansion, so
`Document::resolve` can still expand it. A value that the dialect cannot
spell, such as a line break in Docker, fails with `EditError::Unencodable`.

Inserted lines use the line ending that is most common in the document, or LF
when it has none. At the end of a file without a final line ending, including
an empty file, the output still has no final line ending; a bare key added at
the end of a Compose file is the exception. In Compose, a declaration inserted
right after a quoted value with no line ending follows the closing quote on
the same line, as in `A="x"B=1`, because Compose reads text after a closing
quote as the next statement.

## Batches

`Document::edit` applies several operations to one document and verifies the
result once. Operations select targets by ID, so a batch can change one of
several duplicate declarations:

```rust
use adotenv::{Edit, InsertPosition};

let doc = adotenv::parse_strict("A=one\nA=two\n").unwrap();
let second = doc.get_all("A").nth(1).unwrap();

let verified = doc
    .edit(&[
        Edit::Set { entry: second.id(), value: "changed" },
        Edit::Insert { key: "B", value: None, position: InsertPosition::End },
    ])
    .unwrap();
assert_eq!(verified.as_str(), "A=one\nA=changed\nB\n");
```

| `Edit` | Change |
|---|---|
| `Set { entry, value }` | Set the value of one declaration |
| `Remove { item }` | Remove one item of any kind; the comments around it stay |
| `Rename { entry, key }` | Rename one declaration |
| `Insert { key, value, position }` | Add a declaration of a key that is not declared yet; `value: None` adds a bare key |
| `Comment { item }` | Prefix every physical line of one item with `# ` |
| `Uncomment { item }` | Remove one comment marker, and the following space if there is one, from a comment item |
| `Export { entry, enabled }` | Add or remove the `export` prefix |

`entry` is an entry ID and `item` an item ID of the document that runs the
batch; see [Items and entries](parsing.md#items-and-entries). Every ID and
position refers to the original document, not to the output of earlier
operations, and the batch either applies every operation or fails as a whole.
Restoring a commented-out multiline value takes one `Uncomment` per line in
one batch.

### Insert positions

| `InsertPosition` | Places the new declaration |
|---|---|
| `End` | After the last item |
| `AfterKey(key)` | After the only declaration of `key` |
| `AfterEntry(id)` | After one declaration, selected by entry ID |
| `Sorted` | Before the first declaration, in source order, whose key sorts after the new key, or at the end |

`Sorted` does not sort the file. In an unsorted file it still inserts before
the first greater key it finds.

### Key collisions

An inserted or renamed key must name exactly one declaration of the output.
The batch fails with `EditError::Duplicate` when the document keeps another
declaration of that key, or when another insertion, rename or uncommented line
in the batch declares it. Removing, commenting out or renaming away every
existing declaration of the key in the same batch frees it, so a batch can swap
two keys:

```rust
use adotenv::{Edit, EditError};

let doc = adotenv::parse_strict("A=1\nB=2\n").unwrap();
assert_eq!(doc.rename("A", "B"), Err(EditError::Duplicate(2)));

let swapped = doc
    .edit(&[
        Edit::Rename { entry: 0, key: "B" },
        Edit::Rename { entry: 1, key: "A" },
    ])
    .unwrap();
assert_eq!(swapped.as_str(), "B=1\nA=2\n");
```

Commented-out declarations are comments, so they never collide. Uncommenting
does not check collisions on its own: `uncomment` and `Edit::Uncomment` can
restore a declaration of a key that the document still declares.

## Errors

| `EditError` | Meaning |
|---|---|
| `Missing` | No declaration has the key. |
| `Duplicate(n)` | The key selects `n` declarations, or an insert or rename would leave `n` declarations of its key. |
| `InvalidTarget` | An ID is out of range or names the wrong kind of item. |
| `InvalidKey` | The dialect cannot spell the key. |
| `Unencodable` | The dialect cannot spell the value in this position. |
| `ConflictingEdits` | Operations in one batch overlap or change the same property twice. |
| `WouldChangeMeaning` | The output would change unrelated source or miss the requested change. |

## Applying text edits

`VerifiedEdits::as_str()` and `into_string()` give the complete output.
`edits()` and `into_edits()` give `TextEdit` values with a byte `range` in the
original source and a `replacement`, for editors and language servers that
apply changes themselves.

The edits are sorted by start offset and do not overlap. Two edits can start at
the same offset: an insertion, which has an empty range, then comes before the
replacement that starts there. Apply them in reverse order of the slice, from
the last to the first, so each range still addresses unchanged text. Do not
re-sort them: a sort by start offset alone can swap two edits that share a
start.

```rust
use adotenv::Edit;

let doc = adotenv::parse_strict("A=1\n").unwrap();
let verified = doc
    .edit(&[
        Edit::Export { entry: 0, enabled: true },
        Edit::Rename { entry: 0, key: "B" },
    ])
    .unwrap();

let mut text = doc.source().to_string();
for edit in verified.edits().iter().rev() {
    text.replace_range(edit.range.clone(), &edit.replacement);
}
assert_eq!(text, "export B=1\n");
assert_eq!(text, verified.as_str());
```

An editor that applies the edits to a buffer that may have changed since the
document was parsed must check the buffer version first.
[Formatting and sorting](../design/formatting.md) return edits in the same
order.

## Text helpers

`adotenv::set(text, key, value)` and `adotenv::remove(text, key)` parse `text`
with the recovering Portable parser, apply one edit and return the verified
output as a `String`. Unrelated invalid lines may remain, but their reading
must not change.

```rust
let text = "BAD LINE\nA=1\n";
assert_eq!(adotenv::set(text, "A", "2").unwrap(), "BAD LINE\nA=2\n");
```

## Encoding values

To write a value yourself, `encode_value_with(value, dialect)` returns one
literal token, or `EncodeError::Unrepresentable` when the dialect cannot spell
the value. The token is checked in both `A=<token>\n` and end-of-file
`A=<token>` contexts. It does not account for other neighboring syntax or for
consumers that expand references.

```rust
use adotenv::{encode_value_with, parse_strict_with, Dialect};

let value = "literal #\0\n";
let token = encode_value_with(value, Dialect::PYTHON_DOTENV).unwrap();
let source = format!("A={token}\n");
assert_eq!(
    parse_strict_with(&source, Dialect::PYTHON_DOTENV).unwrap().get("A"),
    Some(value)
);
assert!(encode_value_with("two\nlines", Dialect::DOCKER_RAW).is_err());
```

`encode_value(value)` is the Portable helper. It returns `None` for a value
with control characters other than tab, CR and LF.
