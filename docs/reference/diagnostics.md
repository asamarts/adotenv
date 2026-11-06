# Diagnostic codes

Every diagnostic has a stable code, a severity, a one-based line, a primary
byte span and an optional secondary span. The codes, severities and spans are
part of the stable API. Message text may change, so match on codes.

```rust
use adotenv::{DiagnosticCode, Severity};

let (_, diagnostics) = adotenv::parse("A=1\nA=2\n");
let duplicate = &diagnostics[0];

assert_eq!(duplicate.code(), DiagnosticCode::DuplicateKey);
assert_eq!(duplicate.code().as_str(), "dotenv.duplicate-key");
assert_eq!(duplicate.severity(), Severity::Warning);
assert_eq!(duplicate.line(), 2);
assert_eq!(duplicate.span(), 4..5);
assert_eq!(duplicate.secondary_span(), Some(0..1));
```

`Diagnostic` has `code()`, `severity()`, `line()`, `span()`,
`secondary_span()` and `message()`, and displays as `line N: message (code)`.
`DiagnosticCode` has `as_str()`, `severity()` and `message()`. The same codes
appear in the CLI's human, JSON and GitHub output.

`parse` returns the diagnostics ordered by the start of their spans. Spans are
byte ranges into the original source, byte-order mark included. Lines count
LF, CRLF and a lone CR as line ends in every dialect, as `LineIndex` and
`Entry::line` do. CLI human and GitHub columns on line 1 do not count a leading
byte-order mark; use `LineIndex::without_bom` for the same columns in the
library.

## Severities

- **Error** fails `parse_strict`, `parse_values` and loading, and makes
  `adotenv-cli check` exit `1`. A recovering `parse` keeps a malformed
  statement as an `Invalid` item. A declaration that breaks a dialect policy,
  such as `forbidden-bare-key` or `invalid-escape`, stays an entry with the
  diagnostic attached.
- **Warning** describes text that is read, but perhaps not as the author
  meant. It never fails a parse.
- **Info** records a neutral fact about the source.

## Codes

The **Item** column names the kind of item that carries the diagnostic, which
`Item::diagnostic()` returns. "none" means that no item carries it: the
diagnostic is only in the list that `parse` returns. "Portable syntax only"
means dialects whose `syntax` is `Syntax::Portable`: the Portable preset and
custom dialects built from it.

| Code | Severity | Item | Meaning |
|---|---|---|---|
| `dotenv.invalid-key` | Error | `Invalid` | A key is empty (Compose keys may be empty) or contains a character the dialect does not allow. Other statements that are not a declaration, such as `BAD KEY=value`, get this code too. |
| `dotenv.unterminated-quote` | Error | `Invalid` | A quoted value has no closing quote before the end of the input, or of the line when the dialect does not allow that quote to span lines. Only the line with the opening quote becomes the item. |
| `dotenv.trailing-text` | Error | `Invalid` | Text other than a comment follows a closing quote, as in `A="x"y`, in dialects with `QuotedTail::Error`, such as Portable and python-dotenv. |
| `dotenv.unknown-escape` | Warning | none | A double-quoted escape that the dialect does not define is kept as written, under `UnknownEscapes::Warn`, as in Portable. |
| `dotenv.duplicate-key` | Warning | none | A declaration that takes part in lookup repeats the key of an earlier one. Reported on each later such declaration; the secondary span is the key of the first. |
| `dotenv.bare-key` | Warning | none | A declaration has no `=` and no value, such as `DEBUG`. Portable syntax only. |
| `dotenv.non-posix-key` | Warning | none | A key is accepted but is not a POSIX shell variable name, such as `app.port`. Portable syntax only. |
| `dotenv.tight-hash` | Warning | none | A `#` in an unquoted value is literal text, not a comment, as in `A=a#b`. Portable starts a comment only at a `#` after whitespace. Portable syntax only. |
| `dotenv.backtick` | Warning | none | A backtick in an unquoted value is literal text, not a quote. Reported for each backtick. Portable syntax only. |
| `dotenv.bom` | Info | none | The source starts with a UTF-8 byte-order mark, which the dialect sets apart from the items (`BomPolicy::Strip`, used by every preset). A first line over the byte limit reports `dotenv.line-too-long` instead. |
| `dotenv.lone-cr` | Warning | none | A carriage return is not followed by a line feed. Portable ends a line there. Every lone CR is reported, including one inside a quoted value. Portable syntax with `LineBreaks::Universal` only. |
| `dotenv.forbidden-bare-key` | Error | `Entry` | A declaration has no `=` and the dialect forbids that (`BareKeys::Error`). |
| `dotenv.forbidden-bom` | Error | `Invalid` | The source starts with a byte-order mark and the dialect forbids that (`BomPolicy::Error`). The item holds only the three bytes of the mark. A first line over the byte limit reports `dotenv.line-too-long` instead. |
| `dotenv.invalid-escape` | Error | `Entry` | The dialect rejects a double-quoted escape it does not define (`UnknownEscapes::Error`). The value keeps the escape as written. |
| `dotenv.ignored-statement` | Warning | `Ignored` | A statement is kept in the source but ignored by the dialect: a malformed statement under `MalformedLines::Ignore`, or a statement with text after its closing quote under `QuotedTail::Skip`. |
| `dotenv.line-too-long` | Error | `Invalid` | A physical line is longer than the dialect's byte limit, counted without the final LF but with a CR before it. Docker allows 65,535 bytes; the other presets have no limit. |
| `dotenv.too-many-diagnostics` | Warning | none | More than 10,000 ordinary diagnostics were found. The list is truncated; the first error found after the budget is retained before this summary. |

In dialects that ignore malformed lines (`MalformedLines::Ignore`, used by the
Node preset), `dotenv.ignored-statement` replaces every error code above
except the four policy codes: `dotenv.forbidden-bare-key`,
`dotenv.forbidden-bom`, `dotenv.invalid-escape` and `dotenv.line-too-long`.

`DiagnosticCode` is non-exhaustive. A minor release can add codes; treat an
unknown code by its severity.
