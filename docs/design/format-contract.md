# Format contract

adotenv and ayaml are independent parsers. This contract keeps their public surfaces
consistent without shared types, a shared trait, or a dependency between them. A format
crate offers the layers its grammar supports. Where it offers a layer, it uses these names
and rules. The target is conformance by shared test data, not by a trait.

## Status

This page defines a target, not current behavior. The shared vectors and check-report schema
are tracked in [#27](https://github.com/asamarts/adotenv/issues/27). Known gaps in adotenv
are [#29](https://github.com/asamarts/adotenv/issues/29) for limits and
[#30](https://github.com/asamarts/adotenv/issues/30) for minimal versions. Until #27 lands,
conformance is not enforced by CI.

## Layers

| Layer | Name |
|---|---|
| Positions | `LineIndex::{new, to_position, to_offset, line_count}`, `ColumnEncoding::{Utf8, Scalar, Utf16}`, `Position`, `PositionError` |
| Syntax stream | `parse_events` |
| Lossless, recovering | `parse`, returning `(Document, Vec<Diagnostic>)` |
| Lossless, strict | `parse_strict`, returning `Result` |
| Native value | `parse_value` for one tree, `parse_values` for a flat map, `parse_documents` for several |
| Typed, with Serde | `from_str`, `from_slice`, `to_string`, `to_writer`, `from_value`, `to_value`, where offered. A crate that decodes from a parsed document may offer `Document::deserialize` instead, as adotenv does |
| Options | One options value per operation, passed to a `*_with` entry point or an options-taking method |
| Locate | A byte span for a value or entry; `encode_value` for one scalar |
| Edit | `edit`, returning `Result<VerifiedEdits, EditError>`; `as_str`, `into_string`, `edits`, `into_edits` |
| Format | `format(&FormatOptions)` |
| Compare | `compare`, `portability` |
| Effects | Behind a feature or in a separate crate; never ambient |
| Tool | `<format>-cli` with at least `check`, `fmt`, `get`, `set` and `diff`; `check` accepts `--format human\|json\|github` |

`parse` always means recovering. A crate without a recovering parser leaves the name unused.

## Positions

- Offsets are UTF-8 bytes in the original source, including a leading byte-order mark.
  Public spans convert to `Range<usize>`.
- Line and column are derived on demand. A hot path may keep a private forward cursor.
- Machine positions are zero-based `Position`. Display positions are one-based.
- Editor and command-line columns do not count a leading byte-order mark. A library position
  API may keep its documented, byte-relative columns as the 1.x default and offer the no-mark
  form as an option; adotenv does this with `LineIndex::without_bom`. A 2.0 release should
  make the no-mark form the default.
- Each format documents its own line endings. LF, CRLF and a lone CR for adotenv and ayaml.
- Conversions never clamp, except offsets inside a hidden leading byte-order mark, which
  share the first visible column. A bad offset is an error.

## Diagnostics

- Every diagnostic has a stable code, `<format>.<kebab-name>`, such as `dotenv.invalid-key`.
  Match on codes, not message text.
- `Diagnostic` offers `code()`, `severity()`, `span()`, `secondary_span()`, `line()`,
  `message()`.
- `Severity` has `Error`, `Warning` and `Info`, is `non_exhaustive`, and is fixed per code in
  a parser. A lint crate may configure its own.
- A `Diagnostic` message holds no document values. Where a value helps, an explicit accessor
  offers it. An error type whose message can hold a value has a masked form; see Masking.

## Failures

- Parsing returns a `Diagnostic` or one `Error` type. Other operations have their own
  `non_exhaustive` error; those errors use kinds or variants, not diagnostic codes.
- Errors are `Send + Sync + 'static` and implement the standard error trait.

## Masking

- `mask(&value)` returns `Masked<&T>`, with a redacted `Debug`.
- An error that can hold a document value can be wrapped with `mask(&error)`, which redacts
  `Debug`. A dedicated `masked()` view, where offered, also redacts `Display`.
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
- JSON output follows the shared `tests/vectors/schema/check-report.v1.json` schema. A format
  may add optional fields.
- GitHub annotation escaping and columns follow [Command-line tool](cli.md), including the
  BOM rule above.
- Values reach output only where documented.

## Quality

Shared quality targets: `forbid(unsafe_code)`; docs.rs metadata with feature labels; a
statement of the stable surface; a security reference; SHA-pinned actions and
`permissions: {}`; advisory and license checks; semver checks; minimal versions; scheduled
mutation testing; scaling tests with generous bounds; a reviewed list of known deviations;
Trusted Publishing.

## Test data

The shared vectors and schema are tracked in #27. They will live under `tests/vectors/` as
plain files that a format crate vendors with a recorded hash:

- `positions.json`: source, offset, and expected position per column unit, or expected error.
  Covers a byte-order mark, CRLF, a lone CR, astral characters, and NEL, LS and PS as content.
- `edit-order.json`: source, edits and expected output, including equal starts.
- `github-escaping.json`: input and escaped output.
- `schema/check-report.v1.json`: the JSON Schema of the check report.
