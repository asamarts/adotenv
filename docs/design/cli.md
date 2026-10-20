# Command-line interface

Status: the zero-dependency `adotenv-cli` companion was published as 0.7.0 on
2026-10-19 and 0.7.1 on 2026-10-20. `run` starts a child process with loaded
values but does not mutate the parent environment.

## Commands

```text
adotenv-cli [--dialect <portable|node|python|compose|docker>] <command> ...

adotenv-cli get <file> <key>
adotenv-cli check <file>...
adotenv-cli fmt [--write|--check] [--sort] <file>...
adotenv-cli diff <left> <right>
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

Parses every file recoveringly and prints one line per diagnostic with the path,
one-based line, severity, stable code and message. Any error-severity
diagnostic makes the command exit 1.

### fmt

Reads, repairs invalid syntax as the parser permits, formats with
`FormatOptions::normalize_groups`, and optionally sorts each blank-line group
with `Document::sort`. `--check` reports files that would change and exits 1
without writing. `--write` replaces files in place using a same-directory
temporary file and rename on Unix; other platforms fall back to direct writes.
Otherwise output is printed. Multiple files are labeled. Invalid syntax is
reported and the command exits 1, though the formatter still preserves invalid
statements.

### diff

Compares effective keys with `Document::compare`. It prints `-` for left-only,
`+` for right-only and `~` for state/value/export changes, and exits 1 when
there are differences.

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
