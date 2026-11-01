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

Patch releases fix bugs without changing the API. One fix changes results:

- 1.0.1: with a custom dialect that uses `Duplicates::First`,
  `Document::resolve` and `Document::resolve_with_commands` bind a repeated key
  to its first visible declaration, the one `Document::get` returns. 1.0.0
  bound it to the last one. The presets use `Duplicates::Last` and are
  unaffected.

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
