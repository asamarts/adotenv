# Command-line interface

Status: the zero-dependency `adotenv-cli` workspace companion is implemented in
the unreleased 0.7 tree. It is not yet published to crates.io. `run` starts a
child process with loaded values but does not mutate the parent environment.

## Commands

```text
adotenv-cli get <file> <key>
adotenv-cli check <file>...
adotenv-cli fmt [--write] [--sort] <file>...
adotenv-cli diff <left> <right>
adotenv-cli example <file>
adotenv-cli load <directory> [mode]
adotenv-cli set <file> <key> <value>
adotenv-cli run <directory> [mode] -- <program> [args...]
```

### get

Reads one file with strict Portable parsing and prints the effective decoded
value. Missing keys exit 1. Empty values print an empty line. Parse failures
exit 2.

### check

Parses every file recoveringly and prints one line per diagnostic with the path,
one-based line, severity, stable code and message. Any error-severity
diagnostic makes the command exit 1.

### fmt

Reads, repairs invalid syntax as the parser permits, formats with
`FormatOptions::normalize_groups`, and optionally sorts each blank-line group
with `Document::sort`. `--write` replaces files in place; otherwise output is
printed. Multiple files are labeled. Invalid syntax is reported and the command
exits 1, though the formatter still preserves invalid statements.

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
errors return 2. Command substitution from `.env` files is not enabled.

## Deliberate limits

- Command substitution from loaded files is not enabled.
- The CLI is not published yet.
- File writes are direct rather than atomic; callers should use version-control
  workflows or external atomic-write tooling where required.
- The CLI exposes only Portable syntax; a dialect flag is future work.
