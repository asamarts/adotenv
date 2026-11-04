# Dialects

There is no single `.env` standard. Tools disagree about comments, quotes,
escapes, duplicate keys and keys without values. adotenv makes the reading
rules explicit: every document carries the `Dialect` it was read with.

## Presets

| Preset | Reads like | Pinned version |
|---|---|---|
| `Dialect::PORTABLE` | A strict, literal default for files shared between tools | None |
| `Dialect::NODE_DOTENV` | The default parser of Node `dotenv` | `dotenv` 18.0.8 |
| `Dialect::PYTHON_DOTENV` | The stream parser of `python-dotenv`, without interpolation | `python-dotenv` 1.2.4 |
| `Dialect::COMPOSE` | Docker Compose env files, without interpolation | Compose 5.6.0, `compose-go` 2.16.1 |
| `Dialect::DOCKER_RAW` | The Docker CLI `--env-file` parser | Docker CLI 29.8.2 |

Presets choose parsing and literal lookup rules only. None expands
references, reads the process environment or loads files, so a bare key never
takes a value from the environment, even where Docker or Compose would. The
pinned versions are the upstream releases that the
[conformance corpus](quality.md#conformance) was recorded against.

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
  `A=a#b` has the value `a#b` and a warning. `A= # comment` has an empty
  value.
- Single quotes are literal. Double quotes decode `\n`, `\r`, `\t`, `\\`, `\"`
  and `\$`. Unknown escape pairs stay literal with a warning.
- Single and double quoted values may span lines. Original line endings remain.
- After a closing quote, only whitespace and a `#` comment may follow.
  `A="x" # c` is fine; `A="x"#c` and `A="x"y` are errors.
- `KEY` is a bare key: it takes part in lookup with no value, with a warning.
  `KEY=` and `KEY=""` have an empty value. Bare keys never read the
  environment.
- Duplicates are kept. The last declaration wins and gets a warning. A bare
  key that comes last does not fall back to an earlier value.
- A leading byte-order mark is kept apart from the items, with an
  informational diagnostic. If the first line exceeds `line_byte_limit`, the
  `LineTooLong` error takes precedence and the BOM diagnostic is not reported.
  LF, CRLF and a lone CR end lines and are kept as written. A lone CR gets a
  warning.
- Backticks are literal text, not quotes. In an unquoted value they get a
  warning.
- Parsing never expands variables or runs commands.

## Preset behavior

**Node** starts a comment at any `#` in an unquoted value, so `A=a#b` reads
`a`, and a comment may follow a closing quote directly. Backticks quote values,
and a colon followed by whitespace works like `=`, as in `KEY: value`. Double
quotes decode only `\n` and `\r`; other pairs stay as written. Text after a
closing quote makes the whole value literal: `A="x"y` reads `"x"y`. CRLF and
CR inside decoded values become LF. Malformed lines are ignored with a warning
instead of failing. Bare keys and the key `__proto__` stay in the document but
take no part in lookup.

**Python** double quotes decode `\n`, `\r`, `\t`, `\\`, `\"`, `\'`, `\a`,
`\b`, `\f` and `\v`, but not `\$`; other pairs stay as written. Single quotes
decode `\'` and `\\`. A comment may follow a closing quote directly, and any
`#` right after a bare key starts a comment. Keys may contain any character
except whitespace, `=` and `#`, and may be single-quoted. Whitespace follows
Python's definition. `export # note` is a comment line. CRLF inside quoted
values stays as written.

**Compose** starts a comment only at a `#` after an ASCII space, and not at
the start of a value: `A=a\t#b` and `A= #b` keep the hash. Any colon separates
a key from its value. Keys may use Unicode letters and numbers, `_`, `.`, `-`,
`[` and `]`, and may be empty, as in `=value`. Double quotes also decode `\a`,
`\b`, `\f`, `\v` and the octal escape `\0` followed by exactly three octal
digits, so `\0101` reads `A`; single quotes decode `\'`. Text after a closing
quote starts another statement on the same line, and only LF ends a line. Bare
keys take no part in lookup, but a bare key on a last line without a line
ending reads as a value with an empty key, as Compose does. `$VAR` stays
literal.

**Docker** keeps every byte after `=` as the value: quotes, `#`, backslashes,
spaces and CR bytes inside the line. A key is everything before `=`; it may not
be empty or contain an ASCII space or tab. There is no `export` prefix and no
multiline value, and only LF ends a line. Bare keys stay in the document but
take no part in lookup. Lines are limited to 65,535 bytes.

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

Each document keeps the dialect it was read with. `Document::reparse` reads
the same source text again with another dialect and builds new items and
entries. Item and entry IDs belong to one reading only.

## Portability

`Document::portability(&[dialects])` parses the document's source again with
each distinct dialect and reports the lines that they read differently. The
document's own dialect is always the first reading.

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
duplicate winner. Quote style alone is not a difference: `A=x` and `A="x"`
read the same. Lines are split at LF, CRLF and a lone CR, whatever the
dialects' line break rules, and a multiline item shows as a declaration on its
first line and continuations on the others.

The check is literal. It expands nothing and reads no environment or file.

## Encoding

`encode_value_with(value, dialect)` returns a literal token that reads back as
`value` in that dialect, or `EncodeError::Unrepresentable`. `encode_value` is
the Portable helper; it returns `None` for a value that contains a control
character other than tab, CR and LF. See
[Encoding values](../guide/editing.md#encoding-values).
