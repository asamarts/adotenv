# Command-line interface

Status: the zero-dependency `adotenv-cli` companion was published as 0.7.0 on
2026-10-19, 0.7.1 on 2026-10-20, 0.8.0 and 0.9.0 on 2026-10-21, 0.9.1 through
0.9.4 on 2026-10-22, and 0.9.5 through 0.9.13 on 2026-10-23/24. `run` starts a child process with loaded values but does not mutate
the parent environment.

## Commands

```text
adotenv-cli [--dialect <portable|node|python|compose|docker>] <command> ...

adotenv-cli get <file> <key>
adotenv-cli check [--format human|json|github] [--json] [--source] [--recursive] [--exclude <path>] <file-or-dir>...
adotenv-cli fmt [--write|--check] [--sort] [--recursive] [--exclude <path>] <file-or-dir>...
adotenv-cli diff [--format human|matrix|json] [--json] <baseline> <file>...
adotenv-cli example <file>
adotenv-cli load <directory> [mode]
adotenv-cli set <file> <key> <value>
adotenv-cli run <directory> [mode] -- <program> [args...]
```

Commands that read source accept `-` or `--stdin` in place of a file.
Standard input may be used at most once per invocation; `fmt --write` and `set`
reject it because they require a writable file.

### get

Reads one file with strict parsing under the selected dialect and prints the
effective decoded value. Missing keys exit 1. Empty values print an empty line. Parse failures
exit 2.

### check

Parses every file recoveringly. Human output, the default, prints one line per
diagnostic with the path, one-based line and character column, severity, stable
code and message, followed by a summary of errors, warnings, info diagnostics and
affected files. Any error-severity diagnostic makes the command exit 1.

`--format json` (also available as the compatibility alias `--json`) emits one
versioned JSON object:

```json
{"version":1,"files":[{"path":".env","ok":false,"diagnostics":[{"line":1,"severity":"error","code":"dotenv.invalid-key","message":"...","span":{"start":0,"end":3},"secondary_span":null}]}]}
```

Read failures produce `{"path":"...","ok":false,"error":"...","diagnostics":[]}`.
The schema version and stable diagnostic codes are the machine contract; message
wording remains descriptive.

`--format github` emits GitHub Actions workflow commands instead, using
`::error`, `::warning` and `::notice` severities with `file`, `line`, `col` and
the stable diagnostic code as the title. File paths and messages are escaped
according to the workflow-command rules. Read failures become `::error`
annotations without a line. The output goes to standard output and the exit
status is unchanged.

Human output does not echo source lines by default because `.env` files commonly
contain secrets. `--source` opts in to a source line and caret display; it may
reveal values and is rejected with JSON or GitHub output.

### fmt

Reads, repairs invalid syntax as the parser permits, formats with
`FormatOptions::normalize_groups`, and optionally sorts each blank-line group
with `Document::sort`. `--check` reports files that would change and exits 1
without writing. `--write` replaces files in place using a same-directory
temporary file. Unix commits with an atomic rename. On Windows it first attempts
a direct replace; if that fails, it moves the existing target to a backup, moves
the temporary file into place and removes the backup, restoring the backup on
failure. This fallback is not crash-atomic and may leave a backup after abrupt
termination.
Otherwise output is printed. Multiple files are labeled. Invalid syntax is
reported and the command exits 1, though the formatter still preserves invalid
statements.

### Recursive discovery

`check --recursive` and `fmt --recursive` walk supplied directories, or `.` if
none are supplied, and select files named `.env` or `.env.*`. Repeatable
`--exclude <path>` values are interpreted relative to each discovery root, with
a leading `./` normalized; absolute excludes match when the discovery roots are
absolute. The defaults skip `.git`, `target` and `node_modules` while walking;
an explicitly supplied root is honored even if it has one of those names.
Symbolic links are never followed. Explicit files are included regardless of
name. Multiple roots are sorted and deduplicated. Paths that are not valid
UTF-8 are read correctly, but text and JSON rendering displays them lossily.
If a recursive search finds nothing, the command reports that on stderr and
exits successfully.

### diff

The first file is the baseline; every following file is compared with it using
`Document::compare`. With exactly two files, the existing output is unchanged:
`-` for baseline-only, `+` for candidate-only and `~` for state/value/export
changes. Exported values are annotated with `(export)` in human output. With
three or more files, each candidate gets a `baseline -> candidate` heading, and
identical candidates print `(no differences)`. The command exits 1 if any
candidate differs from the baseline.

`--format matrix` prints a table with keys as rows and the baseline plus every
candidate as columns. Missing keys show `<unset>`, bare declarations show
`<bare>`, empty values show `""`, and exported declarations are prefixed with
`export`. Values are escaped for tabs, carriage returns and line feeds. Matrix
mode extends a compact row table one candidate at a time rather than retaining
every document; output is still buffered until all candidates validate.

`--format json` (also available as `--json`) emits one versioned object:

```json
{"version":1,"baseline":"base.env","files":[{"path":"one.env","ok":true,"differences":[{"key":"A","kind":"value","left":{"state":"value","value":"1","exported":false},"right":{"state":"value","value":"2","exported":false}}]}]}
```

Difference kinds are `only-left`, `only-right`, `state`, `value` and `export`.
Values are decoded literals; absent sides are `null`. Non-matrix human/JSON
modes parse the baseline once and candidates incrementally, buffering output
until every candidate validates. Comparison uses the explicit lookup index, with
empty-key fallback, rather than scanning a document per key. Read and strict
parse failures therefore exit 2 without partial output. Matrix mode retains
every document to build its columns. Standard input may be used at most once.

### example

Prints `Document::example` output, preserving comments and blanking assigned
values while redacting malformed/ignored values.

### load

Uses `load_cascade` with an explicit directory and optional mode, printing final
`KEY=value` pairs. It never applies values to the process environment.

### set

Requires a unique key, uses the verified native edit API and writes the file.
Values are passed exactly as one argument; shell quoting is the caller’s
responsibility.

### run

Loads the standard cascade, starts the requested program with `Command`, and
adds loaded values to the child environment. The child inherits the parent
environment, with loaded values taking precedence for their keys. The parent
process environment is not changed. The child exit code is returned; spawn
errors return 2. Keys and values are checked for empty/NUL/`=` conditions before
spawning. Command substitution from `.env` files is not enabled.

## Machine-output compatibility

The JSON formats are the stable machine contracts. Both currently use
`"version":1`:

- `check --json` (and `check --format json`) emits a top-level `version` and
  `files` array. File entries contain `path`, `ok`, and either `diagnostics` or
  `error`. Diagnostics contain `line`, `severity`, `code`, `message`, `span`
  and `secondary_span`.
- `diff --format json` (and `diff --json`) emits `version`, `baseline` and a
  `files` array. Candidate entries contain `path`, `ok` and `differences`;
  differences contain `key`, `kind`, `left` and `right`; values contain
  `state`, `value` and `exported`.
- Diagnostic `code` strings, difference `kind` strings, assignment `state`
  strings, severity strings and exit-code meanings are part of the contract.
  Spans are zero-based byte offsets into the original UTF-8 source; `line` is
  one-based. Paths are rendered lossily when they are not valid UTF-8.
- Within a version, fields may be added, but existing fields are not renamed,
  removed, retyped or given different semantics. Consumers must ignore unknown
  JSON fields. Breaking changes require a new schema version and a changelog
  entry.
- Check file order is deterministic (discovered paths are sorted); diff
  candidates follow argument order; diagnostics remain source-ordered.
  Consumers should use field names rather than rely on JSON object key order.
- GitHub Actions output is a second machine contract: `::error`, `::warning`
  or `::notice`, with `file`, `line`, `col` and `title` properties and the
  escaped message. Human-readable output and the matrix table are diagnostic
  conveniences, not stable machine formats.
- Exit codes: `0` success/no differences; `1` check error diagnostics or check
  read failures, or diff candidate differences; `2` usage, configuration or
  discovery failure, or diff read/strict-parse failure. A recursive search that
  finds no files reports that on stderr and exits `0`.
- `--json` remains an alias for `--format json`; `--format` values are
  `human`, `json` and `github` for `check`, and `human`, `matrix` and `json` for
  `diff`.

## Dialects

The global `--dialect` option accepts `portable` (default), `node`, `python`,
`compose`, `docker` or `docker-raw`. It selects the exact library parser,
lookup policy and native edit verification for every command, including
`load`/`run` cascades.

## Deliberate limits

- Command substitution from loaded files is not enabled.
- The CLI is published as `adotenv-cli` 0.9.13; see
  [RELEASING.md](../../RELEASING.md).
- The companion `config` and `figment` adapters live in separate workspace crates.
- Unix writes use atomic rename. Windows attempts a direct replace and falls
  back to a best-effort backup swap without unsafe platform APIs; the fallback
  is not crash-atomic.
