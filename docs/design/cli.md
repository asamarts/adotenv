# Command-line tool

`adotenv-cli` checks, formats, compares and loads `.env` files from a shell or
a CI job. It uses the library APIs, so it reads files exactly as the library
does.

## Install

```console
cargo install adotenv-cli --locked
```

This installs one binary, `adotenv-cli`. It needs Rust 1.81 or later to build.
`adotenv-cli --help` prints the usage and `adotenv-cli --version` prints the
version.

## Commands

```text
adotenv-cli [--dialect <portable|node|python|compose|docker>] <command> [options]

get <file> <key>
check [--format human|json|github] [--json] [--source] [--recursive] [--exclude <path>] <path>...
fmt [--write|--check] [--sort] [--recursive] [--exclude <path>] <path>...
diff [--format human|matrix|json] [--json] <baseline> <file>...
example <file>
load <directory> [mode]
set <file> <key> <value>
run <directory> [mode] -- <program> [args...]
```

| Command | Does |
|---|---|
| `get` | Prints the effective value of a key and a line break, without quoting. |
| `check` | Reports diagnostics for each file. |
| `fmt` | Prints formatted text, checks formatting with `--check` or rewrites files with `--write`. `--sort` also sorts each group. |
| `diff` | Compares each file with the baseline, the first path. |
| `example` | Prints the document with assigned values removed. |
| `load` | Prints the merged cascade of a directory as Portable lines. |
| `set` | Changes the value of a key in place. |
| `run` | Loads a cascade and runs a program with the loaded values. |

`set` changes an existing declaration. The key must be declared exactly once;
a missing or duplicated key is an error, and `set` never adds a key.

`--json` is short for `--format json`, `--format=<value>` is accepted, and `-r`
is short for `--recursive`. Two different formats in one command are a usage
error, as is `--source` with JSON or GitHub output, and `--write` with
`--check`. Arguments must be valid UTF-8.

### Dialects

`--dialect` selects the reading rules for every file in the command. It can
appear before or after the command name, but before `--`.

| Value | Library preset |
|---|---|
| `portable` (default) | `Dialect::PORTABLE` |
| `node` | `Dialect::NODE_DOTENV` |
| `python` | `Dialect::PYTHON_DOTENV` |
| `compose` | `Dialect::COMPOSE` |
| `docker` or `docker-raw` | `Dialect::DOCKER_RAW` |

See [Dialects](dialects.md) for what each preset accepts.

## Values in output

Values may be secrets. `check` prints source lines only with `--source`. These
commands print values or text that can contain them:

- `get` prints the effective value.
- `load` prints every loaded value.
- `diff` prints values in every format: human, matrix and JSON.
- `fmt` prints or writes the whole document.
- `example` removes values but keeps comments verbatim, including anything
  secret written in a comment.

## Diagnostics

`check` prints one line per diagnostic: path, one-based line and column,
severity, code and message. A summary line follows when any file has a
diagnostic or could not be read; a clean run prints nothing.

```text
$ adotenv-cli check bad.env
bad.env:1:1: error[dotenv.invalid-key]: key does not match the selected dialect
bad.env:3:1: warning[dotenv.duplicate-key]: duplicate key; lookup follows the selected dialect policy
1 error, 1 warning, 0 info in 1 file
```

`--source` adds the source line and a caret under the span. Diagnostics go to
standard output and read errors to standard error. Human output is not a
machine contract; use JSON or GitHub output in scripts. The codes are listed in
[Diagnostic codes](../reference/diagnostics.md).

### `check --format json`

The report is one line of JSON:

```json
{"version":1,"files":[{"path":"app.env","ok":false,"diagnostics":[{"line":1,"severity":"error","code":"dotenv.invalid-key","message":"key does not match the selected dialect","span":{"start":0,"end":7},"secondary_span":null}]}]}
```

Every field is listed below, in output order. The fields, codes, severities and
spans are stable.

| Object | Field | Type | Meaning |
|---|---|---|---|
| check report | `version` | number | Schema version, `1`. |
| check report | `files` | array of check file | One entry per file, in argument order. With `--recursive`, found files are sorted and duplicates removed. |
| check file | `path` | string | The path as given or found, or `-` or `--stdin` for standard input. Bytes that are not UTF-8 become U+FFFD. |
| check file | `ok` | boolean | `false` when the file has an error diagnostic or could not be read. |
| check file | `error` | string | Present only when the file could not be read or is not UTF-8. The message includes the path. |
| check file | `diagnostics` | array of diagnostic | The file's diagnostics. Empty when `error` is present. |
| diagnostic | `line` | number | One-based line of the primary span. |
| diagnostic | `severity` | string | `error`, `warning` or `info`. |
| diagnostic | `code` | string | Stable diagnostic code, such as `dotenv.invalid-key`. |
| diagnostic | `message` | string | Human-readable text. Match on `code` instead. |
| diagnostic | `span` | span | Primary source range. |
| diagnostic | `secondary_span` | span or null | A related range, such as the first declaration of a duplicate key, or `null`. |
| span | `start` | number | Byte offset into the file. A leading byte-order mark is counted. |
| span | `end` | number | Byte offset just past the range. |

A file with no diagnostics produces an empty `diagnostics` array.

### `check --format github`

One GitHub Actions workflow command per line:

```text
::<level> file=<path>,line=<line>,col=<column>,title=<code>::<message>
::error file=<path>,title=adotenv::<error>
```

The first form reports a diagnostic. `<level>` is `error`, `warning` or `notice`
for an `info` diagnostic. `<column>` is one-based and counts Unicode scalar
values. The second form reports a file that could not be read. In `<path>` and
`<code>`, `%`, CR, LF, `:` and `,` are percent-encoded. In `<message>` and
`<error>`, `%`, CR and LF are percent-encoded. This format is stable.

```yaml
- run: cargo install adotenv-cli --locked
- run: adotenv-cli check --format github --recursive
```

## Comparison

`diff` compares effective declarations. In human output, each candidate starts
with a `baseline -> file` header, followed by one line per difference: `-` for
a key that takes part in lookup only in the baseline, `+` for one that takes
part only in the candidate and `~` for a changed value, state or export prefix. A candidate with no differences prints
`(no differences)`.

```text
$ adotenv-cli diff base.env app.env
base.env -> app.env
~ B: two -> changed
~ C: 3 (export) -> 3
- BARE=<bare>
+ D=""
```

`--format matrix` prints one table with a row per key and a column per file.
A cell shows the value, `""` for an empty value, `<bare>` for a declaration
without `=`, `<unset>` for a missing key and an `export ` prefix for exported
declarations. Neither human output nor the matrix is a machine contract.

### `diff --format json`

The report is one line of JSON:

```json
{"version":1,"baseline":"base.env","files":[{"path":"app.env","ok":true,"differences":[{"key":"A","kind":"value","left":{"state":"value","value":"1","exported":false},"right":{"state":"value","value":"2","exported":false}}]}]}
```

Every field is listed below, in output order. The schema is stable.

| Object | Field | Type | Meaning |
|---|---|---|---|
| diff report | `version` | number | Schema version, `1`. |
| diff report | `baseline` | string | The baseline path as given. |
| diff report | `files` | array of diff file | One entry per candidate, in argument order. |
| diff file | `path` | string | The candidate path as given. |
| diff file | `ok` | boolean | Always `true`. A file that cannot be read or parsed stops `diff` with exit `2` and no JSON. |
| diff file | `differences` | array of difference | Baseline keys in the order of their first declaration that takes part in lookup, then the keys that take part in lookup only in the candidate, in the same order. Keys whose declarations agree are left out. |
| difference | `key` | string | The key. |
| difference | `kind` | string | The first difference found, checked in this order: `only-left` (takes part in lookup only in the baseline), `only-right` (only in the candidate), `state`, `value`, `export`. |
| difference | `left` | value or null | The baseline declaration, or `null` when the key is absent. |
| difference | `right` | value or null | The candidate declaration, or `null` when the key is absent. |
| value | `state` | string | `bare` for a key without `=`, `empty` for an empty value, otherwise `value`. |
| value | `value` | string or null | The decoded literal value: `""` for `empty`, `null` for `bare`. |
| value | `exported` | boolean | Whether the declaration has an `export` prefix. |

## Formatting

`fmt` applies `FormatOptions::normalize_groups()`:

- one space on each side of `=`, so `A=1` becomes `A = 1`, in dialects where
  that keeps the value;
- trailing spaces and tabs removed;
- each run of blank lines collapsed to one;
- quote styles kept;
- a final line break ensured.

`--sort` also sorts declarations within each blank-line group; see
[Formatting and sorting](formatting.md). Formatting never changes a value. A
change that would alter meaning is refused and exits `2`.

With several files, printed output starts each file with a `==> path <==`
header. `--check` prints `path: would reformat` for each file that would
change and exits `1` if any would. `--write` rewrites changed files in place.
Diagnostics are reported on standard error, and a file with error diagnostics
is still formatted.

## `load` output

`load` prints the merged cascade as a Portable document: one `KEY=value` line
per key, sorted by key, with LF line endings. Values are quoted and escaped by
the core Portable encoder, so the output parses back to the same keys and values
with the Portable dialect, whichever dialect read the files. No value spans
several lines.

When a key is not a Portable key, or a value contains a control character other
than tab, CR or LF, `load` prints nothing and exits `2`. The error names the key
but not the value.

A cascade file that does not exist is skipped, so a directory without any
cascade files, or one that does not exist, loads nothing: `load` prints nothing
and exits `0`.

## `run` environment

`run` starts the program with the parent environment plus the loaded values. A
loaded value replaces a parent variable of the same name. Other parent variables
are passed through unchanged. The parent environment is not changed. When no
cascade file exists, the program runs with the parent environment alone.

Loaded files are trusted like code. A loaded `PATH` changes which program runs,
and variables such as `LD_PRELOAD` change how it runs.

Values stay literal: `run` does not expand references or run command
substitutions. Keys that are empty or contain `=` or NUL, and values that
contain NUL, are rejected before the program starts.

## Discovery

With `--recursive`, `check` and `fmt` search each directory argument for
`.env` and `.env.*` files. With no path they search `.`. A file argument is
included whatever its name, and `-` reads standard input.

Without `--recursive`, every path argument must be a file. A directory is a
read failure, and `check` or `fmt` with no path is a usage error.

The search skips `.git`, `target`, `node_modules` and symbolic links. A
directory argument that is a symbolic link is refused. When the search finds
no files, `check` prints an empty report (`{"version":1,"files":[]}` for JSON),
`no .env files found` goes to standard error, and the exit code is `0`. `fmt`
behaves the same way.

`--exclude <path>` needs `--recursive` and can be repeated. Each exclude is
relative to each directory argument, or to the parent directory of a file
argument. It removes the path and everything below it, comparing whole path
components. A leading `./` is ignored and an empty exclude has no effect. An
absolute exclude is compared with paths as they are spelled, so it only matches
below an absolute directory argument.

## Input and output

File-reading commands accept `-` or `--stdin` for standard input, at most once
per command. `set` and `fmt --write` need a file and refuse standard input.

`fmt --write` and `set` replace a file through a temporary file in the same
directory:

- When the path is a symbolic link, the file it finally points to is replaced in
  that file's directory, and the link is kept.
- A path that is a directory or another non-regular file is refused.
- Hard links are not kept. The path gets a new file, and other links to the old
  file keep the old contents.
- Extended attributes and ACLs are not copied.

On Unix the temporary file is created with mode `0600` before anything is
written. It then receives the original owner, group and mode, is synced and is
renamed over the original. The directory is synced after the rename. The
original owner is kept when the running user may set it, for example as root;
otherwise the new file belongs to the running user. The group is kept when the
running user may set it. If it cannot be kept and the mode grants the group any
access, the write fails and the file is unchanged.

On Windows the command writes a temporary file, applies the original
permissions and attempts a direct replace. If that fails, it moves the old file
to a backup, installs the new file and restores the backup on error. The Windows
fallback is not crash-atomic.

## Exit codes

Every command except `run` exits with one of these codes:

- `0`: success.
- `1`: findings. `check` or `fmt` found an error diagnostic, `fmt --check` found
  a file that would change, `diff` found a difference, or `get` did not find the
  key.
- `2`: the command could not complete. This covers usage errors, files that
  cannot be read or are not UTF-8, failed writes, and input the command cannot
  process: an invalid document for `get`, `diff`, `set`, `load` or `run`, an
  edit that `set` cannot apply, `load` output that has no Portable spelling, or
  a `fmt` change that would alter meaning. Failed writes include writes to
  standard output and standard error. A closed output pipe also exits `2`,
  without a message.

`check` and `fmt` process every file and exit with the highest code, so a file
that fails takes precedence over findings in other files.

Warning and info diagnostics do not change the exit code. `example` reports
diagnostics on standard error and exits `0`.

`run` exits `2` for a usage error, when it cannot load the cascade, when a
loaded key or value cannot be passed to a child, or when the program cannot be
started. Otherwise it exits with the child's exit code. On Unix, a child ended
by signal `N` exits `128 + N`, as in POSIX shells. A child exit code outside
`0` to `255`, such as a Windows `NTSTATUS` crash code, exits `255`.

## Limits

The CLI never runs command substitutions from loaded files. To run them, use
the library: `Document::resolve_with_commands` with a callback, or
`exec::Shell` from the `exec` feature. To change the environment of the current
process, use [`adotenv-process`](../guide/process.md).
