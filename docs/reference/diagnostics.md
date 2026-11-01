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

| Code | Severity | Meaning |
|---|---|---|
| `dotenv.invalid-key` | Error | A key is empty or contains a character outside the dialect's key alphabet. |
| `dotenv.unterminated-quote` | Error | A quoted value has no closing quote before the end of the input, or of the line when the dialect does not allow that quote to span lines. |
| `dotenv.trailing-text` | Error | Text other than a comment follows a closing quote, in dialects with `QuotedTail::Error`. |
| `dotenv.unknown-escape` | Warning | A double-quoted escape that the dialect does not define is kept literally. Portable reports it. |
| `dotenv.duplicate-key` | Warning | An earlier declaration that takes part in lookup has the same key. The secondary span points at the key of the first such declaration. |
| `dotenv.bare-key` | Warning | A declaration has no `=` and no value. Portable syntax only. |
| `dotenv.non-posix-key` | Warning | A key is accepted but is not a POSIX shell variable name, such as `app.port`. Portable syntax only. |
| `dotenv.tight-hash` | Warning | A `#` directly after an unquoted value is literal text, not a comment, as in `A=a#b`. Portable only. |
| `dotenv.backtick` | Warning | A backtick is literal text, not a quote. Portable only. |
| `dotenv.bom` | Info | A UTF-8 byte-order mark precedes the source, with `BomPolicy::Strip`. |
| `dotenv.lone-cr` | Warning | A carriage return without a line feed is kept as a line ending. Portable only. |
| `dotenv.forbidden-bare-key` | Error | The dialect forbids key-only declarations (`BareKeys::Error`). |
| `dotenv.forbidden-bom` | Error | The dialect forbids a leading byte-order mark (`BomPolicy::Error`). |
| `dotenv.invalid-escape` | Error | The dialect rejects an unknown escape (`UnknownEscapes::Error`). |
| `dotenv.ignored-statement` | Warning | A statement is kept in the source but ignored by the dialect. |
| `dotenv.line-too-long` | Error | A physical line exceeds the dialect's byte limit. Docker uses 65,535 bytes. |

In dialects that ignore malformed lines (`MalformedLines::Ignore`, used by the
Node preset), `dotenv.ignored-statement` replaces every error code above
except the four policy codes: `dotenv.forbidden-bare-key`,
`dotenv.forbidden-bom`, `dotenv.invalid-escape` and `dotenv.line-too-long`.

`DiagnosticCode` is non-exhaustive. A minor release can add codes; treat an
unknown code by its severity.
