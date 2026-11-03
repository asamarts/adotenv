# Security and limits

`.env` files often hold secrets, and tools that read them often read files
they did not write. This page collects what adotenv does and does not do on
both fronts.

## What the core never does

Parsing, lookup, edits, formatting, sorting, comparison and example generation:

- never read files, environment variables or the working directory;
- never expand `$VAR` references or run `$(command)` substitutions;
- never write to the process environment;
- never use unsafe code. The core is `#![forbid(unsafe_code)]` with every
  feature combination.

Each of these happens only through an explicit API: the `load` feature reads
files, `Document::resolve` substitutes references from a lookup you pass,
`Document::resolve_with_commands` and the `exec` feature run commands, and
`adotenv-process` writes the process environment.

## Untrusted input

- Input must be UTF-8. Decode bytes yourself; the loader rejects invalid UTF-8
  instead of replacing it.
- A retained `Document` keeps item and token metadata for the whole file, so
  memory grows with the input. Check the byte length, line count or number of
  declarations before parsing a file from an untrusted source.
  `parse_values` keeps only final values and uses much less memory.
- `parse` collects at most 10,000 ordinary diagnostics; a final
  `dotenv.too-many-diagnostics` warning reports truncation. The first error
  found after the budget is retained. `parse_strict` and `parse_values` stop
  at the first error and do not collect the list.
- Parsing and reference scanning run in linear time, including inputs with
  many statements on one line, long `$` runs and deeply nested forms.
  Resolution time is linear in the number of references in a value, and edit
  batches that set and insert entries stay linear.
- Public `Entry::references` builds one candidate record per possible
  reference, so its memory grows much faster than the value. Bound or reject
  untrusted values before inspecting them.
- Resolution enforces limits on nesting depth (64 in every preset), reference
  candidates scanned per value (65,536), the expanded size of each value
  (1 MiB) and the total output size (64 MiB). Exceeding them fails with
  `ResolveError::DepthExceeded`, `TooManyReferences`, `ValueTooLarge` or
  `OutputTooLarge`. See
  [References and resolution](../design/expansion.md#resolution).
- Fuzz targets cover parsing, edits, encoding, resolution and formatting. A
  bounded fuzz run is evidence, not a proof; see [Quality](../design/quality.md).

## Secret values

Values may be secrets. These outputs include them:

- `Debug` output of documents, items, entries, edit and format results,
  comparisons, loaded environments, resolved values and the adapter types. Do not log it when values are secret.
- Serde conversion errors name the key and line, and their messages include
  the value. Figment and `config` errors may also include the value they failed
  to convert.
- Command call records (`CommandCall`) keep each command's output, and
  `exec::Shell` error messages include the command's standard error.
- `adotenv-cli get`, `load` and `diff` print values in every output format.
  `fmt` prints or writes the whole file. `check` prints source lines only with
  `--source`.
- `Document::example` and `adotenv-cli example` remove assigned values but keep
  comments verbatim, including any secret written in a comment.

## Commands and child processes

- Command substitution runs only when you pass a callback to
  `Document::resolve_with_commands` and the reference rules allow commands.
  `exec::Shell` runs commands through the platform shell in a directory you
  name, caps standard output at 1 MiB, caps standard error at 64 KiB and stops
  a command after 300 seconds by default. The builders change or disable those
  limits. Use it only with trusted files.
- `adotenv-cli run` starts a program with the parent environment plus the
  loaded values. Loaded files are trusted like code: a loaded `PATH` changes
  which program runs, and variables such as `LD_PRELOAD` change how it runs.
  `run` never expands references or runs command substitutions.
- `adotenv_process::apply` changes the environment of the current process. It
  is unsafe because the environment is shared by every thread. `validate` runs
  the same checks without changing anything.

## File writes

`adotenv-cli fmt --write` and `set` resolve the write target once before
reading, so a symbolic link swapped after that point cannot redirect the write.
They do not resolve the target again while replacing it. On Unix they also
check the target's device and inode before the temporary file and immediately
before the replace, and fail if it changed after the read. They leave an unchanged
file untouched, so its inode, hard links, modification time, extended
attributes and ACLs stay. They replace a changed file through a temporary file
in the same directory. On Unix the temporary file is created with mode `0600`
before anything is written, then
receives the original mode, owner and group where the user may set them. Hard
links, extended attributes and ACLs are not kept across a replace. On Windows
the fallback replace is not crash-atomic. See
[input and output](../design/cli.md#input-and-output) for the details.

## Reporting a vulnerability

Report security problems privately through
[GitHub security advisories](https://github.com/asamarts/adotenv/security/advisories/new)
rather than a public issue.
