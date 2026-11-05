# Format contract

adotenv and ayaml are independent parsers. This contract keeps their public surfaces
consistent without shared types, a shared trait, or a dependency between them. A format
crate offers the layers its grammar supports. Where it offers a layer, it uses these names
and rules. Conformance is checked by test data, not by a trait.

## Layers

| Layer | Name |
|---|---|
| Positions | `LineIndex::{new, to_position, to_offset, line_count}`, `ColumnEncoding::{Utf8, Scalar, Utf16}`, `Position`, `PositionError` |
| Syntax stream | `parse_events` |
| Lossless, recovering | `parse`, returning `(Document, Vec<Diagnostic>)` |
| Lossless, strict | `parse_strict`, returning `Result` |
| Native value | `parse_value` for one tree, `parse_values` for a flat map, `parse_documents` for several |
| Typed, with Serde | `from_str`, `from_slice`, `to_string`, `to_writer`, `from_value`, `to_value`. A crate that decodes from a parsed document may offer `Document::deserialize` instead, as adotenv does |
| Options | One function per operation: `<name>_with(input, options)` |
| Locate | The byte span of a value or an entry; encoding a scalar |
| Edit | `edit`, returning `Result<VerifiedEdits, EditError>`; `as_str`, `into_string`, `edits`, `into_edits` |
| Format | `format(&FormatOptions)` |
| Compare | `compare`, `portability` |
| Effects | Behind a feature or in a separate crate; never ambient |
| Tool | `<format>-cli` with `check`, `fmt`, `get`, `set`, `diff`, and `--format human\|json\|github` |

`parse` always means recovering. A crate without a recovering parser leaves the name unused.

## Positions

- Offsets are UTF-8 bytes in the original source, including a leading byte-order mark.
  Public spans convert to `Range<usize>`.
- Line and column are derived on demand. A hot path may keep a private forward cursor.
- Machine positions are zero-based `Position`. Display positions are one-based.
- Columns do not count a leading byte-order mark. adotenv keeps its documented behavior as
  the default through 1.x.
- Each format documents its own line endings. LF, CRLF and a lone CR for adotenv and ayaml.
- Conversions never clamp. A bad offset is an error.

## Diagnostics

- Every failure has a stable code, `<format>.<kebab-name>`. Match on codes, not message text.
- `Diagnostic` offers `code()`, `severity()`, `span()`, `secondary_span()`, `line()`,
  `message()`.
- `Severity` has `Error`, `Warning` and `Info`, is `non_exhaustive`, and is fixed per code in
  a parser. A lint crate may configure its own.
- A `Diagnostic` message holds no document values. Where a value helps, an explicit accessor
  offers it. An error type whose message can hold a value offers `masked()`; see Masking.

## Failures

- Parsing returns a `Diagnostic` or one `Error` type. Other operations have their own
  `non_exhaustive` error.
- Errors are `Send + Sync + 'static` and implement the standard error trait.

## Masking

- `mask(&value)` returns `Masked<&T>`, with a redacted `Debug`.
- An error that can hold a document value offers `masked()`, with redacted `Display` and
  `Debug`.
- Keys may be shown when the format treats them as structure.

## Edits

- `TextEdit { range: Range<usize>, replacement: String }`, with ranges into the original
  source.
- Edits are sorted by start and do not overlap. On equal starts the insertion comes first.
  Apply them in reverse slice order and do not re-sort.
- `VerifiedEdits` belongs to each format and has private fields. It returns text and edits and
  never writes.
- Verification reparses and compares meaning, which is specific to each format.

## Options and limits

- One options value per operation, with `const` presets, `with_*` builders and
  `non_exhaustive`.
- A reading profile is a value, not a Cargo feature, where practical.
- Limits are runtime values. A limit failure carries `kind`, `limit` and `observed`.
- A new crate limits by default. An existing crate must not start rejecting valid input in a
  minor release.

## Command-line tools

- Exit `0` for success, `1` for findings, `2` when the command could not complete. `3` may
  report an internal error.
- JSON output follows `schema/check-report.v1.json`. A format may add optional fields.
- GitHub annotation escaping and columns follow [Command-line tool](cli.md).
- Values reach output only where documented.

## Quality

`forbid(unsafe_code)`; docs.rs metadata with feature labels; a statement of the stable
surface; a security reference; SHA-pinned actions and `permissions: {}`; advisory and
license checks; semver checks; minimal versions; scheduled mutation testing; scaling tests
with generous bounds; a reviewed list of known deviations; Trusted Publishing.

## Test data

`tests/vectors/` holds plain files that a format crate vendors with a recorded hash:

- `positions.json`: source, offset, and expected position per column unit, or expected error.
  Covers a byte-order mark, CRLF, a lone CR, astral characters, NEL, LS and PS.
- `edit-order.json`: source, edits and expected output, including equal starts.
- `github-escaping.json`: input and escaped output.
- `schema/check-report.v1.json`: the JSON Schema of the check report.
