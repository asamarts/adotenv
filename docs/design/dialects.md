# Dialects

There is no single `.env` standard. adotenv makes the reading rules explicit.

## Presets

| Preset | Syntax |
|---|---|
| `Dialect::PORTABLE` | Strict, literal default for mixed tooling |
| `Dialect::NODE_DOTENV` | Node dotenv default regex parser |
| `Dialect::PYTHON_DOTENV` | Python dotenv literal text-stream parser |
| `Dialect::COMPOSE` | Compose env-file statement syntax without interpolation |
| `Dialect::DOCKER_RAW` | Docker CLI raw key/value parser |

Presets choose parsing rules only. None enables interpolation, inheritance or
file loading. Compose bare-key roles and lookup remain the caller's job.

## Portable rules

- Keys use ASCII letters, digits, `_`, `.` and `-`. A non-POSIX key is accepted
  with a warning. Other characters are invalid.
- An optional `export ` prefix and spaces or tabs around `=` are accepted.
- `#` starts a comment at the beginning of a line or after whitespace.
  `A=a#b` has value `a#b` and a warning. `A= # comment` is assigned empty.
- Single quotes are literal. Double quotes decode `\n`, `\r`, `\t`, `\\`, `\"`
  and `\$`. Unknown escape pairs stay literal with a warning.
- Single and double quoted values may span lines. Original line endings remain.
- Text after a closing quote is an error unless it is a comment.
- `KEY` is a bare declaration with no value. `KEY=` and `KEY=""` have an empty
  value. Bare declarations never read the environment.
- Duplicates are retained. The last declaration wins and a warning is emitted.
  A last bare declaration does not fall back to an earlier value.
- A leading BOM is retained as a separate prelude. LF, CRLF and lone CR are
  preserved. Lone CR produces a warning.
- Backticks are literal text, not quotes.
- Parsing never expands variables or runs commands.

## Custom policies

Start from a preset and use builders:

```rust
use adotenv::{BareKeys, Dialect, InlineComments};

let dialect = Dialect::NODE_DOTENV
    .with_inline_comments(InlineComments::None)
    .with_bare_keys(BareKeys::Ignore);
```

Policy groups cover:

- statement grammar and key characters;
- comments, quoting, escapes and raw values;
- quoted-key support and multiline values;
- bare declarations and duplicate lookup;
- BOM handling, line breaks and whitespace classes;
- quoted tails, malformed lines, Node object-key rules and line-byte limits.

Each document keeps its exact configuration. `Document::reparse` rebuilds items,
entries and maps for a different dialect while reusing the source text. Snapshot
IDs belong to one interpretation only.

## Preset behavior

Node supports backticks, colon separators after whitespace, decoded newline
normalization and its default regex fallback. `__proto__` is retained as source
but excluded from effective object lookup.

Python supports its named escape set, multiline single-quoted keys and Python
whitespace classes. Original CRLF bytes stay in text-stream values.

Compose recognizes Unicode letter and number categories, Compose whitespace and
octal escapes. Text after a quoted value can begin another statement. Its EOF
bare-key behavior is retained. Dollar syntax stays literal.

Docker keeps values, hashes, quotes and internal CR bytes raw. Bare keys are
retained but excluded from lookup. The scanner enforces the upstream line-byte
limit and only ASCII space or tab invalidates a nonempty raw key.

## Encoding

`encode_value_with(value, dialect)` returns one literal token or
`EncodeError::Unrepresentable`. Success is checked in both `A=<token>\n` and
EOF `A=<token>` contexts. The token does not account for arbitrary neighboring
syntax or for consumers that enable interpolation.

`encode_value(value)` is the Portable helper. It rejects controls other than
CR, LF and tab.

## Portability

`Document::portability(&[dialects])` reparses the retained source for each unique
configuration and reports lines whose meaning differs. The document's own
dialect is always the first reading.

Each reading exposes ordered parts: declarations, blank lines, comments, invalid
or ignored statements and multiline continuations. Declaration parts include
key, decoded value, assignment state, export prefix, lookup participation,
effective value and duplicate winner. Quote style alone is not a difference.

The check is literal. It performs no expansion, lookup, inheritance or I/O.
