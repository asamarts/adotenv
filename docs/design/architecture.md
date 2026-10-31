# Architecture

adotenv separates syntax from meaning.

## Document model

`Document` borrows or owns one UTF-8 source string and a dialect. It stores:

- items in source order, including comments, blank lines and invalid text;
- entries that the selected dialect recognizes;
- raw token spans for keys, separators, values and comments;
- diagnostics produced during parsing;
- a line index built on demand.

`Item` covers comments, blank lines, declarations and invalid or ignored
statements. `Entry` is a declaration view. Both are zero-based snapshot IDs.

The parser never edits source text. Printing a document returns the original
bytes unchanged.

## Recovering and strict parsing

`parse` and `parse_owned` always return a document plus diagnostics. They recover
at the smallest safe boundary and preserve the source that caused a problem.
`parse_values` and `parse_values_with` are the values-only path. They use the
projection parser, discard warnings and return the first error.

`parse_strict` returns the first error. Warnings and informational diagnostics do
not fail a strict parse.

Diagnostics contain a stable code, severity, message, primary span and optional
secondary labels. Message text may change; codes and spans are the stable
contract.

## Meaning and lookup

A dialect decides which text counts as a declaration and how it is decoded.
Lookup follows the dialect's duplicate policy and participation rules.

- `get` returns the effective value.
- `get_entry` returns the effective declaration.
- `get_all` returns every occurrence.
- `unique` rejects duplicate keys.
- `effective_entries` yields one winning declaration per effective key.
- `lookup_index` builds an explicit index for repeated lookups.

Bare declarations are distinct from empty assignments. `KEY` has no value,
`KEY=` has an empty value. `BareKeys::Empty` changes effective lookup without
changing the declaration's syntax state.

## Spans and positions

Source spans are byte ranges into the original input. Token helpers return raw
ranges. Decoded value offsets can be mapped back to raw ranges when escapes are
present.

`LineIndex` converts between offsets and line/column positions. Encodings are
explicit: UTF-8 bytes, Unicode scalars or UTF-16 code units. Invalid positions
return an error. They are never rounded or clamped.

Entry and diagnostic lines are one-based. `Position` is zero-based.

## Verified edits

Edits are built as text replacements against the original snapshot. Before an
edit is returned, the complete result is parsed with the same dialect and checked
for changes to unrelated declarations, comments, item kinds and invalid
statements. If a change cannot be verified, the edit fails.

`VerifiedEdits` exposes borrowed text edits, the verified output and optional
metadata. The edits are sorted by start offset and do not overlap. When an
insertion and a replacement start at the same offset, the insertion comes
first. Apply the edits in reverse order of the returned slice, from the last
edit to the first. A sort by start offset alone can swap edits that share a
start, so clients must not re-sort them. Formatting and sorting results follow
the same order.

`Edit::Insert` adds a declaration of a new key. It fails with
`EditError::Duplicate` when the verified output would declare that key more
than once: the document already declares it (duplicates and bare declarations
included), or another insertion, rename or uncommented line in the batch
declares it. Removing, commenting out or renaming every existing declaration in
the same batch frees the key. Commented-out declarations are comments and never
collide. `Edit::Rename` rejects a new key that the document already declares.

## Features and safety

The core has no runtime dependencies and no I/O. `load` enables filesystem
loading. `serde` enables typed deserialization. `exec` enables a command runner
with an explicit working directory.

The core forbids unsafe code. Process environment mutation lives in the separate
`adotenv-process` crate and is an unsafe API because the environment is global.

Disable default features for `no_std` with `alloc`. The parser, model, edits,
formatting, comparison and sorting remain available.
