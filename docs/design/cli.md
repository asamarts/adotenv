# CLI

`adotenv-cli` provides command-line tools built on the library APIs.

## Commands

```text
adotenv-cli [--dialect portable|node|python|compose|docker] <command> ...

get <file> <key>
check [--format human|json|github] [--source] [--recursive] [--exclude <path>] <path>...
fmt [--write|--check] [--sort] [--recursive] [--exclude <path>] <path>...
diff [--format human|matrix|json] [--json] <baseline> <file>...
example <file>
load <directory> [mode]
set <file> <key> <value>
run <directory> [mode] -- <program> [args...]
```

`check` reads one or more files or directories. `fmt` formats, checks or writes
in place. `diff` treats the first file as the baseline. `run` loads a cascade and
starts a child process with those values. The parent environment is not changed.

## Diagnostics

Human output contains path, one-based line and column, severity, code, message
and a summary. Values are not printed by default because they may be secrets.
`--source` adds source lines and carets for human output only.

`check --format json` uses this machine schema:

```json
{"version":1,"files":[{"path":"...","diagnostics":[]}]}
```

The exact fields, diagnostic codes, severities and spans are stable contracts.

`check --format github` emits GitHub Actions workflow annotations. The field
names and escaping are stable.

`diff --format json` emits one result per candidate file. The versioned schema is
stable. Human tables are not a machine contract.

## Discovery

`check --recursive` and `fmt --recursive` find `.env` and `.env.*` below each
root. With no path they use `.`.

`--exclude` is repeatable and relative to each root. Discovery skips `.git`,
`target`, `node_modules` and symbolic links.

## Input and output

File-reading commands accept `-` or `--stdin` for standard input. `fmt` prints
formatted text unless `--write` or `--check` is selected.

On Unix, `--write` uses a same-directory atomic rename. On Windows, it writes a
temporary file and attempts a direct replace. If that fails, it moves the old
file to a backup, installs the new file and restores the backup on error. The
Windows fallback is not crash-atomic.

## Exit codes

- `0`: success or no differences.
- `1`: differences, diagnostics or a required formatting change.
- `2`: usage, read or write failure.

`diff` exits `1` when any candidate differs. `fmt --check` exits `1` when a file
would change. `run` returns the child exit code.

## Limits

Command substitution from loaded files is disabled. Use the library or the
`adotenv-process` crate for other workflows.
