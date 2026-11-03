# Upgrading to 1.0

`adotenv`, `adotenv-process` and `adotenv-cli` 1.0.0 are the first stable
releases. From 1.0, the library API, the CLI commands, exit codes and JSON
output follow semantic versioning. `adotenv-config` and `adotenv-figment` keep
their own 0.x versions; 0.12 requires `adotenv` 1.

Update the dependency:

```toml
[dependencies]
adotenv = "1"
```

Parsed values and Portable item boundaries are the same as in 0.10. The full
list of changes is in the
[changelog](https://github.com/asamarts/adotenv/blob/main/CHANGELOG.md).

## Within 1.x

Releases within 1.x keep the API. These fixes change results:

- 1.0.1: with a custom dialect that uses `Duplicates::First`,
  `Document::resolve` and `Document::resolve_with_commands` bind a repeated key
  to its first visible declaration, the one `Document::get` returns. 1.0.0
  bound it to the last one. The presets use `Duplicates::Last` and are
  unaffected.
- 1.1.0: `Document::uncomment` and `Edit::Uncomment` fail with
  `EditError::Duplicate` when the restored key is still declared, unless the
  same batch removes, comments out or renames away the other declaration.
- 1.1.0: `Document::format` and `Document::sort` return the verified
  output where they used to fail, leaving out only the changes that would read
  differently. A quote style no longer changes values that contain `$`.
- 1.1.0: the messages of `ResolveError::Required`, `dotenv.tight-hash`,
  `dotenv.lone-cr`, `EditError::InvalidTarget` and
  `EditError::WouldChangeMeaning` changed. Match on the error or diagnostic
  code, not on its text.
- 1.1.1: `LoadedEnvironment::resolve_contextual` keeps a value that ends with a
  carriage return in an LF-only dialect and a key that begins with a
  byte-order mark. It now returns a `LoadedResolveError` naming the key when a
  loaded key or value cannot be reproduced exactly, instead of resolving a
  changed one.
- 1.1.1: reference scanning ends a `${...}` operand at the first `}` outside
  a nested form, as Compose and dotenv-expand do. Quotes and backslashes inside
  a default, message or alternative are ordinary text.
  `${VAR:?can't continue}` now raises `ResolveError::Required` instead of
  staying literal, and a backslash before a nested `$` in an operand no longer
  marks it as escaped.
- 1.1.1: `fmt --write` and `set` leave a file untouched when the result is
  unchanged. The file keeps its inode, hard links, modification time,
  permissions and extended attributes.
- 1.1.1: `diff` human output starts each candidate with a
  `baseline -> file` header and prints `(no differences)` when it matches,
  including when there is exactly one candidate. Before, one candidate printed
  neither.
- 1.1.1: recursive `check` and `fmt` continue after a missing, unreadable or
  refused path. `check --format json` reports it as a `files` entry with
  `ok:false`, GitHub output adds an annotation, and the command exits `2`.
  Differing spellings that resolve to the same file are reported once.
- 1.1.1: `Document::format` with `FinalNewline::Remove` and trailing-whitespace
  trimming removes a final lone CR and the trailing whitespace in one pass.
  When several trailing CRs would leave another line ending behind, the file
  is left unchanged.
- 1.1.1: `encode_value_with` represents a Node literal-fallback value that
  needs an escaped terminal control character at the line cap, instead of
  returning `EncodeError::Unrepresentable`.
- 1.2.0: resolution scans at most 65,536 reference candidates per value by
  default and fails with `ResolveError::TooManyReferences` beyond that.
  `ExpandOptions::with_max_references` changes the limit.
- 1.2.0: `parse` returns at most 10,000 ordinary diagnostics. A truncated
  list ends with `DiagnosticCode::TooManyDiagnostics`; the first error after
  the budget is retained. `parse_strict` and `parse_values` still return the
  first error directly.
- 1.2.0: value masking is opt-in with `Masked`/`mask` and
  `DeserializeError::masked`; normal `Debug` output is unchanged. `diff` gains
  `--mask`.
- 1.2.0: `exec::Shell` caps standard output and standard error and stops
  commands after a timeout by default. The builders change the limits.
- 1.2.0: `fmt --write` and `set` resolve the write target once and check its
  device and inode before replacing it on Unix. A swapped symlink can no
  longer redirect the write; a target replaced after the read fails.
- 1.1.2: `encode_value_with` represents a Node value whose only legal token
  needs a repeated backtick run to survive quote recovery, instead of returning
  `EncodeError::Unrepresentable`.

## From 0.11

One change needs attention:

- `Edit::Rename` and `Document::rename` follow the collision rule of
  `Edit::Insert`. The new key must name exactly one declaration of the output.
  Removing, commenting out or renaming away its existing declarations in the
  same batch frees it, so a batch can swap two keys. Two renames to the same
  key fail with `EditError::Duplicate` instead of
  `EditError::WouldChangeMeaning`.

## From 0.10

Check these points as well as the rename change above.

### Library

- `insert` of a key that the document already declares fails with
  `EditError::Duplicate`. Use `Edit::Set` to change the existing declaration,
  or remove or rename it in the same batch.
- `sort` keeps two keys connected by a one-way reference in their source
  order, so its output can differ where one key references another in the same
  group.
  Re-run formatting checks that compare against stored output.
- Code that walks `Document::items` sees each blank line of a Node or Python
  file as a separate `Blank` item. In Compose files, a quoted entry's item now
  includes the rest of its line, so its trailing comment is its inline
  comment.
- `LoadedEnvironment::resolve_contextual` requires `ExpandScope::WholeFile`,
  like `resolve_decoded`.
- `Reference::containers` and `ReferenceContainer` are gone. Walk up
  `Reference::parent` through the list that `Entry::references` returned.
- A resolution whose output exceeds 64 MiB fails with
  `ResolveError::OutputTooLarge`. Raise the limit with
  `ExpandOptions::with_max_output_bytes` where large outputs are expected.

### Command-line tool

- A read or write failure exits with `2` in every command, including `check`
  and `fmt`. Multi-file `check` and `fmt` exit with the highest code across
  files.
- `run` reports a child killed by signal `N` as `128 + N`, and a status outside
  `0` to `255` as `255`.
- `load` prints Portable assignments, quoted where needed, instead of raw
  `KEY=value` lines. It exits with `2` before printing anything when a key or
  value has no Portable spelling.

See [exit codes](../design/cli.md#exit-codes) for the full contract.

### Adapters

- Depend on `adotenv-config` and `adotenv-figment` by their own versions. They
  no longer follow the core version.
- `adotenv-config` uses `config` 0.15 and requires Rust 1.88. `config` 0.15
  keys are case-sensitive: read `A.B` with the case it has in the file.
