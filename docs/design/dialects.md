# Dialects

Status: planned for 0.3.

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

## How dialects work

The syntax layer always keeps raw tokens (see
[architecture.md](architecture.md)), so one parse serves any dialect. A
`Dialect` decides how tokens are recognized and what they mean. It is a
struct of independent switches:

| Switch | Choices |
|---|---|
| Inline comments | after whitespace only, any `#`, or none |
| Double-quote escapes | which escapes are decoded; what happens to unknown ones |
| Single-quote escapes | none, or `\'` |
| Backticks | a quote style, or literal text |
| `KEY: value` | accepted or not |
| Key characters | POSIX names, plus `.` and `-`, any Unicode letters, or anything but whitespace |
| Bare `KEY` | skipped, empty, unset, inherited, or an error |
| Raw values | quotes and whitespace processed, or kept exactly (as `docker run` does) |
| Line breaks | how a lone `\r` is treated |
| Byte-order mark | stripped, part of the key, or an error |
| Duplicates | last wins or first wins |
| Text after a closing quote | ignored, appended, or an error |
| Multi-line values | which quote styles may span lines |

Presets are named sets of switches. Callers can start from a preset and
change individual switches.

## Portable (the default)

Portable is strict where implementations disagree and literal everywhere:

- `export ` prefix accepted; spaces and tabs allowed around `=`.
- Keys: ASCII letters, digits, `_`, `.` and `-`; a key that is not a POSIX
  shell name gets a portability diagnostic.
- `#` starts a comment at the start of a line, or after whitespace. A `#`
  directly after value text is part of the value, with a portability
  diagnostic.
- Single quotes are fully literal and may span lines.
- Double quotes decode `\n`, `\r`, `\t`, `\\`, `\"` and `\$`, may span lines,
  and keep any other backslash pair as written with a diagnostic.
- Backticks are not quotes; a value starting with one gets a diagnostic.
- Text after a closing quote is an error diagnostic.
- A bare `KEY` is kept as its own state with a diagnostic.
- Duplicates: last wins, with a diagnostic.
- An unterminated quote is an error that ends at the end of its line.
- A leading BOM is stripped, with an informational diagnostic; lone carriage
  returns get a diagnostic.
- No expansion: `$` is literal unless the caller resolves references (see
  [expansion.md](expansion.md)).

## Presets in 0.3

| Preset | Reads like |
|---|---|
| `Portable` | the default above |
| `Compose` | Docker Compose `env_file` and `.env`, per the Compose specification |
| `NodeDotenv` | the `dotenv` package on npm |
| `DockerRaw` | `docker run --env-file` (values kept exactly as written) |
| `PythonDotenv` | the `python-dotenv` package |

Each preset is verified against the real implementation on the conformance
corpus (see [quality.md](quality.md)), with the implementation's version
recorded. A preset imitates documented behavior; where an implementation has
behavior that looks accidental, the preset follows it but the difference from
Portable is reported. More presets are added on request, each with inputs and
outputs from the implementation it imitates.

## Encoding per dialect

`encode_value(value, dialect)` writes a value so that the dialect reads it
back exactly, or returns an error when the dialect cannot represent it (for
example a newline under `DockerRaw`).

## Portability check

`portability(&doc, &[dialects])` parses under each dialect and reports every
line whose meaning differs, with the differing readings: "under `NodeDotenv`
this value is `a`; under `Compose` it is `a#b`". This is meant for files read
by more than one tool, such as a `.env` used by both a Node service and Docker
Compose.
