# Architecture

adotenv separates syntax from meaning, and meaning from effects. Parsing
records what the source says. A dialect decides what it means. Anything that
touches the outside world, such as reading files, expanding references,
running commands or changing the process environment, is a separate,
explicit step.

## Layers

| Layer | Provides | Effects |
|---|---|---|
| Parser | Items, entries, token spans and diagnostics for one dialect | None |
| Model | Lookup, raw tokens, groups, positions | None |
| Views | Comparison, portability, examples, reference inspection | None |
| Changes | Verified edits, formatting and sorting | Returns new text; never writes |
| Resolution | Reference substitution from file bindings and a caller lookup | Calls the caller's closures only |
| `load` feature | Cascades and file lists | Reads files |
| `exec` feature | `exec::Shell` command callback | Runs commands |
| `adotenv-process` | `apply` | Writes the process environment (unsafe) |
| `adotenv-cli` | Commands for shells and CI | Reads and writes files, runs programs |

The core crate has no required dependencies (only the optional `serde`
feature adds one) and forbids unsafe code with every feature combination.
Without default features it builds for `no_std` with `alloc`; everything
except `load` and `exec` remains, `serde` included.

## Document model

`Document` borrows or owns one UTF-8 source string and stores the dialect,
the items in source order, the entries the dialect recognizes and the raw token
spans of each entry. Items cover the source without gaps or overlaps, apart
from a leading byte-order mark that the dialect strips, so joining their raw
text rebuilds the source after that mark. Printing a document writes the
original bytes, including the mark.

`Item` and `Entry` are borrowed views. Their IDs are zero-based indexes into
one document's items and entries, and they mean nothing in another reading of
the same source.

Diagnostics are returned by the parse functions, not stored in the document.
An invalid or ignored item keeps its primary diagnostic, and so does an entry
that breaks a dialect policy, such as a forbidden bare key.
`Item::diagnostic()` returns it. `Document::line_index()` builds a new
`LineIndex` on each call.

See [Parsing and lookup](../guide/parsing.md) for the API.

## Recovery

The recovering parser never fails. When a statement is malformed it records a
diagnostic, keeps the text as an `Invalid` item (an `Ignored` item in
dialects that ignore malformed lines) and resumes at the smallest safe
boundary, usually the next line. Tools can then report every problem in a
file and still edit the parts that are valid.

`parse_strict` runs the same grammar and returns the first error instead of
the document. `parse_values` runs the grammar without building items, spans or
diagnostic lists and keeps only the effective values.

Diagnostics have a stable code, a severity, a one-based line, a primary span
and an optional secondary span. Codes and spans are the contract; message text
may change. See [Diagnostic codes](../reference/diagnostics.md).

## Verified changes

Edits, formatting and sorting share one design: plan text replacements against
the original source, apply them to a copy, then parse the complete result
with the same dialect and compare it with the original. The comparison checks
that the requested change happened and that unrelated declarations, comments,
item kinds and invalid statements did not change. If the check fails, the
operation returns an error instead of text.

The result carries the replacements as `TextEdit` values sorted by start
offset, without overlaps, so editors can apply them to their own buffers. See
[Editing](../guide/editing.md#applying-text-edits) for the order rules.

Verification makes edits safe across dialects with unusual grammars: a value
that a dialect cannot spell in a given position fails with an error instead
of producing text that reads differently.

## Literal values and resolution

Values are always the decoded literal text. Reference inspection finds `$VAR`
and `$(command)` candidates with spans and quote context, but does not decide
whether they expand. Eligibility is a pure function of a candidate and
caller-selected rules, and resolution is a separate call that takes the
lookup closure and limits as arguments. See
[References and resolution](expansion.md).

## Crates

| Crate | Version line | Contains |
|---|---|---|
| `adotenv` | 1.x | The core and the optional `load`, `serde` and `exec` features |
| `adotenv-process` | 1.x | The only unsafe API: applying values to the process environment |
| `adotenv-cli` | 1.x | The command-line tool |
| `adotenv-config` | 0.x | A `config` source |
| `adotenv-figment` | 0.x | A Figment provider |

`adotenv`, `adotenv-process` and `adotenv-cli` release together with the same
version. The adapters follow their own 0.x line, so a new major version of
`config` or Figment is a minor release of the adapter. The reasons behind the
split are in [Feature decisions](feature-decisions.md).
