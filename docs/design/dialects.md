# Dialects

There is no single `.env` standard. Tools disagree about comments, quotes,
escapes, duplicate keys and keys without values. adotenv makes the reading
rules explicit: every document carries the `Dialect` it was read with.

## Presets

| Preset | Reads like | Pinned reference |
|---|---|---|
| `Dialect::PORTABLE` | A strict, literal default for files shared between tools | None |
| `Dialect::NODE_DOTENV` | The default parser of Node `dotenv` | `dotenv` 18.0.8 |
| `Dialect::PYTHON_DOTENV` | The stream parser of `python-dotenv`, without interpolation | `python-dotenv` 1.2.4 |
| `Dialect::COMPOSE` | Docker Compose env files, without interpolation | Compose 5.6.0, `compose-go` 2.16.1 |
| `Dialect::DOCKER_RAW` | The Docker CLI `--env-file` parser | Docker CLI 29.8.2 |

Presets choose parsing and literal lookup rules only. None enables
interpolation, inheritance from the process environment or file loading.
Compose bare-key roles stay the caller's job. The pinned references are the
versions that the [conformance corpus](quality.md#conformance) was recorded
against.

```rust
use adotenv::{parse_with, Dialect};

let source = "A=a#b\nB=`two words`\n";
let (node, _) = parse_with(source, Dialect::NODE_DOTENV);
assert_eq!(node.get("A"), Some("a"));
assert_eq!(node.get("B"), Some("two words"));

let (raw, _) = node.reparse(Dialect::DOCKER_RAW);
assert_eq!(raw.get("A"), Some("a#b"));
assert_eq!(raw.get("B"), Some("`two words`"));
```

## Portable rules

- Keys use ASCII letters, digits, `_`, `.` and `-`. A key that is not a POSIX
  shell name, such as `app.port`, is accepted with a warning. Other characters
  are invalid.
- An optional `export ` prefix and spaces or tabs around `=` are accepted.
- `#` starts a comment at the beginning of a line or after whitespace.
  `A=a#b` has value `a#b` and a warning. `A= # comment` is assigned empty.
- Single quotes are literal. Double quotes decode `\n`, `\r`, `\t`, `\\`, `\"`
  and `\$`. Unknown escape pairs stay literal with a warning.
- Single and double quoted values may span lines. Original line endings remain.
- After a closing quote, only whitespace and a `#` comment may follow.
  `A="x" # c` is fine; `A="x"#c` and `A="x"y` are errors.
- `KEY` is a bare declaration with no value. `KEY=` and `KEY=""` have an empty
  value. Bare declarations never read the environment.
- Duplicates are retained. The last declaration wins and a warning is emitted.
  A last bare declaration does not fall back to an earlier value.
- A leading BOM is retained as a separate prelude. LF, CRLF and lone CR are
  preserved. Lone CR produces a warning.
- Backticks are literal text, not quotes.
- Parsing never expands variables or runs commands.

## Preset behavior

**Node** supports backtick quotes, `KEY: value` separators (a colon followed by
whitespace), decoded newline normalization and its default regex fallback.
Malformed lines are ignored with a warning instead of failing. Bare keys are
excluded from lookup. `__proto__` is retained as source but excluded from
effective object lookup.

**Python** supports its named escape set, multiline single-quoted keys and
Python whitespace classes. Original CRLF bytes stay in text-stream values.

**Compose** recognizes Unicode letter and number categories, Compose whitespace
and octal escapes. Text after a quoted value can begin another statement on the
same line. Its end-of-file bare-key behavior is retained, and bare keys are
excluded from lookup. Dollar syntax stays literal.

**Docker** keeps values, hashes, quotes and internal CR bytes raw. Bare keys
are retained but excluded from lookup. Lines are limited to 65,535 bytes, and
only an ASCII space or tab invalidates a nonempty raw key.

## Custom dialects

Start from a preset and change single policies with `with_*` builders:

```rust
use adotenv::{parse_with, BareKeys, Dialect, InlineComments};

let dialect = Dialect::NODE_DOTENV
    .with_inline_comments(InlineComments::None)
    .with_bare_keys(BareKeys::Empty);

let (doc, _) = parse_with("A=one#two\nBARE\n", dialect);
assert_eq!(doc.get("A"), Some("one#two"));
assert_eq!(doc.get("BARE"), Some(""));
```

Every field of `Dialect` is public and has a builder of the same name:

| Field | Controls |
|---|---|
| `syntax` | Statement grammar: `Portable`, `Node`, `Python`, `Compose` or `Docker` |
| `inline_comments` | When `#` starts an inline comment |
| `double_escapes` | Which escapes double quotes decode; sets combine with `union` |
| `unknown_escapes` | Unknown escapes: keep, warn, drop the backslash or error |
| `single_escapes` | Escapes inside single quotes |
| `backticks` | Whether backticks quote values |
| `colon` | Whether `:` separates keys and values |
| `keys` | The key alphabet |
| `bare_keys` | Keys without `=`: keep, ignore, read as empty or error |
| `raw_values` | Keep every byte after the separator as the value |
| `line_breaks` | Whether a lone CR ends a line |
| `bom` | A leading byte-order mark: strip, keep as text or error |
| `duplicates` | Whether the first or last duplicate wins |
| `quoted_tail` | Text after a closing quote: error, ignore, append, fall back, skip or next statement |
| `multiline` | Which quote styles may span lines |
| `whitespace` | Which characters count as whitespace |
| `export` | Whether `export ` is recognized |
| `quoted_keys` | Whether single-quoted keys are allowed |
| `quoted_comments` | Whether a comment may follow a closing quote directly |
| `normalize_newlines` | Whether decoded CRLF and CR become LF |
| `malformed_lines` | Whether malformed statements are errors or ignored |
| `node_object_keys` | Whether `__proto__` is excluded from lookup |
| `line_byte_limit` | The longest physical line accepted, or `None` |

The variants of each policy are listed in the
[API reference](https://docs.rs/adotenv/latest/adotenv/struct.Dialect.html).

Each document keeps its exact configuration. `Document::reparse` rebuilds
items, entries and maps for a different dialect while reusing the source text.
Item and entry IDs belong to one reading only.

## Portability

`Document::portability(&[dialects])` reparses the retained source for each
unique configuration and reports lines whose meaning differs. The document's
own dialect is always the first reading.

```rust
use adotenv::Dialect;

let doc = adotenv::parse_strict("A=a#b\nB=1\n").unwrap();
let report = doc.portability(&[Dialect::NODE_DOTENV]);

assert_eq!(report.len(), 1);
assert_eq!(report.differences()[0].first_line, 1);
```

Each `PortabilityDifference` has the source span, its first and last line and
one `PortabilityReading` per dialect. A reading lists ordered parts:
declarations, blank lines, comments, invalid or ignored statements and
multiline continuations. Declaration parts include key, decoded value,
assignment state, export prefix, lookup participation, effective value and
duplicate winner. Quote style alone is not a difference.

The check is literal. It performs no expansion, lookup, inheritance or I/O.

## Encoding

`encode_value_with(value, dialect)` returns a literal token that reads back as
`value` in that dialect, or `EncodeError::Unrepresentable`. See
[Encoding values](../guide/editing.md#encoding-values).
