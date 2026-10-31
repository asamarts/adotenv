# CLI

`adotenv-cli` provides command-line tools built on the library APIs.

## Commands

```text
adotenv-cli [--dialect portable|node|python|compose|docker] <command> ...

get <file> <key>
check [--format human|json|github] [--json] [--source] [--recursive] [--exclude <path>] <path>...
fmt [--write|--check] [--sort] [--recursive] [--exclude <path>] <path>...
diff [--format human|matrix|json] [--json] <baseline> <file>...
example <file>
load <directory> [mode]
set <file> <key> <value>
run <directory> [mode] -- <program> [args...]
```

`check` reads one or more files or directories. `fmt` formats, checks or writes
in place. `diff` treats the first file as the baseline. `example` prints the
document without values. `load` prints the merged cascade. `set` changes one
declaration in place. `run` loads a cascade and starts a child process.

Arguments must be valid UTF-8.

## Values in output

Values may be secrets. `check` prints them only with `--source`. These commands
print values or text that can contain them:

- `get` prints the effective value and a line break, without quoting.
- `load` prints every loaded value.
- `diff` prints values in every format: human, matrix and JSON.
- `fmt` prints or writes the whole document.
- `example` removes values but keeps comments verbatim, including anything
  secret written in a comment.

## Diagnostics

Human output contains path, one-based line and column, severity, code, message
and a summary. `--source` adds source lines and carets for human output only.
Human output is not a machine contract.

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

A file with no diagnostics produces an empty `diagnostics` array. A recursive
search that finds no files prints `{"version":1,"files":[]}`.

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

## Comparison

`diff` compares effective declarations. Human output and the matrix table are
not machine contracts.

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
| diff file | `differences` | array of difference | Baseline keys in baseline order, then keys found only in the candidate, in candidate order. Keys whose declarations agree are left out. |
| difference | `key` | string | The key. |
| difference | `kind` | string | The first difference found, checked in this order: `only-left` (only in the baseline), `only-right` (only in the candidate), `state`, `value`, `export`. |
| difference | `left` | value or null | The baseline declaration, or `null` when the key is absent. |
| difference | `right` | value or null | The candidate declaration, or `null` when the key is absent. |
| value | `state` | string | `bare` for a key without `=`, `empty` for an empty value, otherwise `value`. |
| value | `value` | string or null | The decoded literal value: `""` for `empty`, `null` for `bare`. |
| value | `exported` | boolean | Whether the declaration has an `export` prefix. |

## `load` output

`load` prints the merged cascade as a Portable document: one `KEY=value` line
per key, sorted by key, with LF line endings. Values are quoted and escaped by
the core Portable encoder, so the output parses back to the same keys and values
with the Portable dialect, whichever dialect read the files. No value spans
several lines.

When a key is not a Portable key, or a value contains a control character other
than tab, CR or LF, `load` prints nothing and exits `2`. The error names the key
but not the value.

## `run` environment

`run` starts the program with the parent environment plus the loaded values. A
loaded value replaces a parent variable of the same name. Other parent variables
are passed through unchanged. The parent environment is not changed.

Loaded files are trusted like code. A loaded `PATH` changes which program runs,
and variables such as `LD_PRELOAD` change how it runs.

Values stay literal: `run` does not expand references or run command
substitutions. Keys that are empty or contain `=` or NUL, and values that
contain NUL, are rejected before the program starts.

## Discovery

`check --recursive` and `fmt --recursive` find `.env` and `.env.*` below each
root. With no path they use `.`.

`--exclude` is repeatable and relative to each root. Discovery skips `.git`,
`target`, `node_modules` and symbolic links. A recursive root that is a symbolic
link is refused.

## Input and output

File-reading commands accept `-` or `--stdin` for standard input. `fmt` prints
formatted text unless `--write` or `--check` is selected.

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
that fails takes precedence over findings in other files. `fmt` still prints or
writes a file that has error diagnostics.

Warning and info diagnostics do not change the exit code. `example` reports
diagnostics on standard error and exits `0`.

`run` exits `2` when it cannot load the cascade or start the program. Otherwise
it exits with the child's exit code. On Unix, a child ended by signal `N` exits
`128 + N`, as in POSIX shells. A child exit code outside `0` to `255`, such as a
Windows `NTSTATUS` crash code, exits `255`.

## Limits

Command substitution from loaded files is disabled. Use the library or the
`adotenv-process` crate for other workflows.
