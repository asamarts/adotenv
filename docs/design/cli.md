# Command-line interface

Status: the zero-dependency `adotenv-cli` companion was published as 0.7.0 on
2026-10-19, 0.7.1 on 2026-10-20 and 0.8.0 on 2026-10-21. Recursive discovery
and diagnostic renderers are in 0.9 development. `run` starts a child process
with loaded values but does not mutate the parent environment.

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
temporary file and rename on Unix; other platforms fall back to direct writes.
Otherwise output is printed. Multiple files are labeled. Invalid syntax is
reported and the command exits 1, though the formatter still preserves invalid
statements.

### Recursive discovery

`check --recursive` and `fmt --recursive` walk supplied directories, or `.` if
none are supplied, and select files named `.env` or `.env.*`. Repeatable
`--exclude <path>` values are interpreted relative to each discovery root;
absolute excludes match when the discovery roots are absolute. The defaults
skip `.git`, `target` and `node_modules`, and symbolic links are never
followed. Explicit files are included regardless of name. Multiple roots are
sorted and deduplicated.
If a recursive search finds nothing, the command reports that on stderr and
exits successfully.

### diff

The first file is the baseline; every following file is compared with it using
`Document::compare`. With exactly two files, the existing output is unchanged:
`-` for baseline-only, `+` for candidate-only and `~` for state/value/export
changes. With three or more files, each candidate gets a `baseline -> candidate`
heading, and identical candidates print `(no differences)`. The command exits 1
if any candidate differs from the baseline.

`--format matrix` prints a table with keys as rows and the baseline plus every
candidate as columns. Missing keys show `<unset>`, bare declarations show
`<bare>`, empty values show `""`, and exported declarations are prefixed with
`export`. Values are escaped for tabs, carriage returns and line feeds.

`--format json` (also available as `--json`) emits one versioned object:

```json
{"version":1,"baseline":"base.env","files":[{"path":"one.env","ok":true,"differences":[{"key":"A","kind":"value","left":{"state":"value","value":"1","exported":false},"right":{"state":"value","value":"2","exported":false}}]}]}
```

Difference kinds are `only-left`, `only-right`, `state`, `value` and `export`.
Values are decoded literals; absent sides are `null`. Read and strict parse
failures exit 2 without partial output. Standard input may be used at most once.

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

## Dialects

The global `--dialect` option accepts `portable` (default), `node`, `python`,
`compose`, `docker` or `docker-raw`. It selects the exact library parser,
lookup policy and native edit verification for every command, including
`load`/`run` cascades.

## Deliberate limits

- Command substitution from loaded files is not enabled.
- The CLI is published as `adotenv-cli` 0.7.1; see
  [RELEASING.md](../../RELEASING.md).
- The companion `config` and `figment` adapters live in separate workspace crates.
- Windows writes fall back to direct replacement; Unix uses atomic rename.
