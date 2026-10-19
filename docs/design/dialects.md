# Dialects

Status: Portable remains the default. Explicit `Dialect` configuration, all five
presets, exact per-dialect encoding, selected-dialect native verification and
cross-dialect portability reporting are implemented in 0.3, released
2026-10-17.

There is no `.env` standard. Implementations agree on the simple cases
(`KEY=value`, `#` comment lines) and differ on many others. For example, the
same line can produce different values depending on which tool reads it:

| Input | Readings seen across common implementations |
|---|---|
| `A=a#b` | `a#b`, or `a` (the `#` starts a comment) |
| `A="a\tb"` | a tab character, a literal `\t`, `atb`, or an error |
| `A="x"y` | `x`, `xy`, `"x"y`, an error, or the line skipped |
| `A` (no `=`) | skipped, empty, unset, taken from the environment, or an error |
| `A='${B}'` | literal `${B}`, or expanded |
| a UTF-8 BOM before the first key | stripped, part of the key name, or an error |

Docker Compose's specification is the closest thing to a written standard.
adotenv therefore makes the reading rules explicit and selectable.

## Selecting a grammar

Each parse retains the original source and raw tokens (see
[architecture.md](architecture.md)), but its entries and diagnostics belong
to the selected dialect. Changing dialect reparses the retained text:
multiline, raw-value and line-break rules can change statement boundaries.
Borrowed interpretations share the source without copying it.

`parse_with(source, dialect)`, `parse_owned_with(source, dialect)` and
`parse_strict_with(source, dialect)` take a copyable `Dialect`. Existing entry
points use `Dialect::PORTABLE`. `Document::dialect()` retains the exact
configuration, including overrides; `Document::reparse(dialect)` borrows its
original source and returns a new document and diagnostics. Snapshot-local IDs
belong only to one interpretation.

Start from a preset with a struct update, for example
`Dialect { inline_comments: InlineComments::None, ..Dialect::NODE_DOTENV }`.
Typed policies control recognition, decoding and effective lookup:

| Policy | Available choices |
|---|---|
| `syntax` | Portable, Node, Python, Compose or Docker statement grammar |
| `inline_comments` | configured whitespace, literal ASCII space, any hash, none, or Python's whitespace-values/tight-bare split |
| `double_escapes` | composable `DoubleEscapes` flags, including named preset sets |
| `unknown_escapes` | preserve, warn, drop backslash, or error |
| `single_escapes` | none, apostrophe only, or apostrophe and backslash |
| `backticks` | quoted values or literal text |
| `colon` | disabled, followed by whitespace, or any colon separator |
| `keys`, `quoted_keys` | POSIX, Portable, frozen Unicode letter/number, non-whitespace, Python, Compose or Docker; optional single-quoted keys |
| `bare_keys` | preserve `None`, exclude from lookup, effective empty string, or error |
| `raw_values` | retain every byte after the separator rather than process quotes/trivia |
| `line_breaks` | universal CR/LF/CRLF, or LF-only scanning |
| `bom` | separate retained prelude, ordinary input, or explicit error |
| `duplicates` | first or last participating declaration wins |
| `quoted_tail` | error, ignore, append, literal fallback, skip, or next statement |
| `multiline` | independent single-, double- and backtick-quote permissions |
| `whitespace` | ASCII, Unicode, ECMAScript, Python or Compose classification |
| other syntax | export prefix, tight quoted comments, decoded newline normalization |
| failure/compatibility | malformed-line error/ignore, Node object-key semantics, optional line-byte cap |

No policy performs inheritance. Bare syntax keeps `Entry::value() == None` and
`AssignmentState::Bare`, even when `get()` returns an effective empty string.
`entries()`, `get_all()` and uniqueness checks retain every recognized
declaration; `participates_in_lookup()` explains lookup exclusions. Ignored
malformed statements retain source and a primary `IgnoredStatement` warning.
Explicit forbidden BOM/bare/escape/line-limit policies remain errors even when
ordinary malformed syntax is ignored.

Key token ranges include single-quote wrappers; `key()` and `key_span()` expose
the inner name. Backtick quotes use `Quote::Backtick`. `quoted_tail` tokens retain
ignored/appended noncomment text; an appended tail is included in the semantic
value and `value_span()`. Decoded maps address original UTF-8 bytes, including
normalized CRLF and removed syntax.

## Portable (implemented 0.2 default)

Portable is strict where implementations disagree and literal everywhere:

- `export ` prefix accepted; spaces and tabs allowed around `=`.
- Nonempty keys: ASCII letters, digits, `_`, `.` and `-`; other characters
  (including Unicode letters) are invalid. A key that is not a POSIX shell
  name gets a warning rather than being rejected.
- `#` starts a comment at the start of a line, or after whitespace. A `#`
  directly after value text is part of the value, with a portability
  diagnostic.
  `A= # comment` is assigned empty; `A=a#b` retains `a#b`.
- Single quotes are fully literal and may span lines.
- Double quotes decode `\n`, `\r`, `\t`, `\\`, `\"` and `\$`, may span lines,
  and keep any other backslash pair as written with a diagnostic.
- Backticks are literal, not quotes; unquoted backticks get diagnostics.
- Non-comment text after a closing quote is an error diagnostic; 0.1's
  permissive discarded quoted tails are not retained as a compatibility mode.
- A bare `KEY` is retained with `value() == None` and a warning. `KEY=` and
  `KEY=""` have `Some("")`, a distinct assigned-empty state. No inheritance
  or environment access occurs during parsing.
- Duplicates: last declaration wins, with a warning. A last bare declaration
  does not fall back to an earlier assigned value.
- An unmatched quote is diagnosed at EOF; its opening physical line is kept
  as invalid and parsing resumes at the following line. A matching quote
  follows the grammar even when its value contains assignment-looking lines.
- A leading BOM is retained as a separate source prelude, excluded from key
  spelling, with an informational diagnostic. Lone carriage returns are
  retained as line endings and get warnings.
- No expansion: `$` is literal after any documented escape decoding.
  Reference inspection and resolution are the opt-in 0.4 interfaces (see
  [expansion.md](expansion.md)).

## Presets (0.3)

| Constant | Literal compatibility surface |
|---|---|
| `Dialect::PORTABLE` | The unchanged default above |
| `Dialect::NODE_DOTENV` | npm dotenv 18.0.8's default regex parser, not its opt-in fast parser |
| `Dialect::PYTHON_DOTENV` | python-dotenv 1.2.4's literal text-stream parser, not file newline conversion |
| `Dialect::COMPOSE` | compose-go 2.16.1 statement syntax and escape projection, without interpolation/inheritance |
| `Dialect::DOCKER_RAW` | Docker CLI 29.8.2's ordered raw kvfile parser, with no lookup |

The [executable comparison](quality.md#public-upstream-conformance-corpus-03-preparation)
replays the real pinned APIs before probing Rust. Node/Python compare literal
maps and Python error presence; Docker compares ordered output and failure.
Compose's real parser always resolves, so only dollar-free, empty-lookup inputs
are literal comparison oracles. Fail-fast Compose/Docker errors compare error
presence, not recovering partial maps. Unsupported source spans, item ordering
or diagnostic identities are not invented as upstream observations.

Compatibility quirks are deliberate:

- Node normalizes decoded CRLF/CR, supports backticks and colon-plus-whitespace,
  preserves its overlapping newline replacements and regex quote fallback,
  and excludes `__proto__` from effective object lookup while retaining source.
- Python permits multiline single-quoted key names and its full named escape
  set. Original CRLF bytes remain in text-stream values. Physical source line
  positions count CRLF once; they do not reproduce upstream's malformed-reader
  CRLF line-number inflation.
- Compose recognizes frozen Unicode 17 letter/number categories, its exact
  horizontal whitespace and octal escape behavior, and reparses text after a
  quoted value as another statement. Its EOF-only key bug is retained: a final
  bare `A` (also before a terminal lone CR) becomes empty key with value `A`.
  A normally LF-terminated bare declaration instead remains bare and does not
  consult a lookup. Dollar syntax, including `$$`, remains literal.
- Docker keeps values, hashes, quotes and internal CR bytes raw. Bare keys are
  retained but excluded from lookup; assigned duplicates stay ordered. Its
  scanner accepts at most 65,535 original bytes before LF, including CR and an
  initial BOM. Only ASCII space/tab invalidate an otherwise nonempty raw key;
  this follows the pinned implementation rather than broader documentation.

Presets select syntax, never expansion, environment access or loading. Compose
file roles remain distinct: the executed default/raw service loaders inherit
bare declarations from supplied project lookup and retain a preceding value
without lookup, while an explicit YAML `environment:` null is an unset override.
The reference's bare-unset wording differs from these pinned loader results;
[`conformance/manifest.json`](../../conformance/manifest.json) records this rather
than normalizing it away. Future resolution/loading APIs must supply explicit
role and lookup policy; parsing never infers either.

## Encoding per dialect (0.3)

`encode_value_with(&str, Dialect) -> Result<String, EncodeError>` writes an exact
literal token or returns `EncodeError::Unrepresentable`. Success is verified in
both canonical `A=<token>\n` and EOF `A=<token>` contexts. `A=` consumes two bytes
of the first physical line's configured limit. A token does not promise safety
with arbitrary headers, trailing syntax or interpolation-enabled consumers.

Portable, Python and Compose presets can represent arbitrary UTF-8 values.
Node's literal grammar cannot represent every value. A Node first-line cap that
cannot fit the opening quote may place it after a line break, which the grammar
treats as trivia between `=` and the value; the returned token then begins with
that line break. Native edits do not use that form: they operate on the existing
physical line and reject contexts whose budget cannot fit an exact token. Docker
raw excludes LF and enforces its scanner cap; a terminal CR needs a protecting
CR for LF/EOF.
That padding is not safe when blindly appending CRLF. `LiteralFallback` may use
a trailing inline-comment marker when that is the only exact spelling; the
marker is part of the returned token. Custom raw normalization can encode LF
with internal CR, but cannot retain a literal CR.

The codec uses bounded quote styles and a linear finite-state inverse for
scanner escapes, decoding and newline normalization. Selected single/double/
backtick policies, octal, unknown escapes, multiline controls and appended tails
are honored. Every candidate is parsed exactly, not accepted by appearance.

Native set/insert share the codec and validate actual source context, including
planned header changes, retained tails, generated separators and byte budgets.
Sets on one capped physical line resolve against other final set values and
retry until the group converges, so acceptance does not depend on request order;
capped next-statement insertion adds the physical separator it requires. Native
verification still checks every unselected region and the complete batch;
standalone representability does not waive those preservation constraints.
`encode_value(&str) -> Option<String>` keeps the original Portable helper's
control policy: controls other than CR, LF and tab return `None`.

## Portability check (0.3)

`Document::portability(&[dialects])` parses the retained text once for each
unique configuration and reports every physical line whose reading differs. The
document's own configuration is always the first reading; requested duplicates
are ignored. For each differing line, `PortabilityReading` exposes the ordered
parts under each dialect: declarations with key, decoded value, assignment
state, export prefix, lookup participation, effective lookup value and duplicate
winner, plus blank, comment, invalid, ignored and multiline-continuation parts.
Quote style alone is not a difference.

This is meant for files read by more than one tool, such as a `.env` used by
both a Node service and Docker Compose: a line reading `a` under `NodeDotenv`
and `a#b` under `DockerRaw` is reported with both readings. Inspection is
literal only: no expansion, lookup, inheritance or I/O. Statement grouping
changes appear as different ordered parts on the affected lines, and a
multiline item contributes a declaration on its first line and continuations on
its later lines.
