# Dialects

Status: Portable syntax is implemented in 0.2. Named presets,
the `Dialect` configuration API, per-dialect encoding and portability
comparison below are future 0.3 designs, not shipped interfaces.

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

## How dialects will work (future 0.3)

Each parse retains the original source and raw tokens (see
[architecture.md](architecture.md)), but its entries and diagnostics belong
to the selected dialect. Changing dialect reparses the retained text:
multiline, raw-value and line-break rules can change statement boundaries.
Borrowed interpretations share the source without copying it.

A future `Dialect` will decide how tokens are recognized and what they mean,
using independent switches. The proposed design is:

| Switch | Choices |
|---|---|
| Inline comments | after whitespace only, any `#`, or none |
| Double-quote escapes | which escapes are decoded; what happens to unknown ones |
| Single-quote escapes | none, or `\'` |
| Backticks | a quote style, or literal text |
| `KEY: value` | accepted or not |
| Key characters | POSIX names, plus `.` and `-`, any Unicode letters, or anything but whitespace |
| Bare `KEY` policy | skipped, empty, unset, inherited through a caller lookup, or an error; syntax state is always retained |
| Raw values | quotes and whitespace processed, or kept exactly (as `docker run` does) |
| Line breaks | how a lone `\r` is treated |
| Byte-order mark | stripped, part of the key, or an error |
| Duplicates | last wins or first wins |
| Text after a closing quote | ignored, appended, literal fallback, skipped with a diagnostic, or an error |
| Multi-line values | which quote styles may span lines |

Presets are named sets of switches. Callers can start from a preset and
change individual switches.

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
  Reference inspection and resolution remain future 0.4 interfaces (see
  [expansion.md](expansion.md)).

## Presets (future 0.3)

| Preset | Syntax reads like |
|---|---|
| `Portable` | the default above |
| `Compose` | Docker Compose syntax, with an explicit project `.env` or service `env_file` role policy |
| `NodeDotenv` | the `dotenv` package on npm |
| `DockerRaw` | `docker run --env-file` (values kept exactly as written) |
| `PythonDotenv` | the `python-dotenv` package |

Each preset is verified against the real implementation on the conformance
corpus (see [quality.md](quality.md)), with the implementation's version
recorded. A preset imitates documented behavior; where an implementation has
behavior that looks accidental, the preset follows it but the difference from
Portable is reported. More presets are added on request, each with inputs and
outputs from the implementation it imitates.

Presets select syntax and literal-value rules, not implicit expansion or
environment access. Resolution remains opt-in in 0.4; loading remains 0.6.

Compose has one syntax preset but distinct file-role policies. A project
`.env` bare declaration may inherit through an explicitly supplied lookup;
a service `env_file` bare declaration means unset. The document retains the
bare state in both cases. Role-dependent interpretation is selected
explicitly during resolution/loading, never guessed by parsing.

Conformance expectations identify the API, options and file role. A generic
compose-go lookup helper is not an oracle for service `env_file` behavior.

## Encoding per dialect (future 0.3)

0.2 exposes `encode_value(&str) -> Option<String>` for a standalone Portable
token, with contextual safety provided by verified native edits. A future
per-dialect encoder will write exact values or fail when the selected dialect
cannot represent them (for example a newline under `DockerRaw`). Its proposed
API is `encode_value(value, dialect)`; it is not the current signature.

## Portability check (future 0.3)

`portability(&doc, &[dialects])` parses under each dialect and reports every
line whose meaning differs, with the differing readings: "under `NodeDotenv`
this value is `a`; under `Compose` it is `a#b`". This is meant for files read
by more than one tool, such as a `.env` used by both a Node service and Docker
Compose.
