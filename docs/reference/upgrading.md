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
- 1.1.2: `encode_value_with` represents a Node value whose only legal token
  needs a repeated backtick run to survive quote recovery, instead of returning
  `EncodeError::Unrepresentable`.
- 1.2.0: resolution scans at most 65,536 reference candidates per value by
  default and fails with `ResolveError::TooManyReferences` beyond that.
  `ExpandOptions::with_max_references` changes the limit. Delimiter matching
  is bounded by `max_depth`, so deeply nested forms also stop instead of
  retaining a record per level.
- 1.2.0: `parse` returns at most 10,000 ordinary diagnostics, plus the first
  error after the budget. A truncated list ends with
  `DiagnosticCode::TooManyDiagnostics`; the retained error can make the total
  10,002 entries. `parse_strict` and `parse_values` still return the
  first error directly.
- 1.2.0: value masking is opt-in with `Masked`/`mask` and
  `DeserializeError::masked`; normal `Debug` output is unchanged. `diff` gains
  `--mask`.
- 1.2.0: `Document::lookup_index` sizes its table from participating keys, so
  dialects that ignore many declarations use less memory.
- 1.2.0: `Document::example` keeps a leading BOM under every `BomPolicy`,
  including `Keep` and `Error`.
- 1.2.0: native encoding no longer strips a U+FEFF key at the start of an
  edit or format fragment, so `set` and quote formatting work for such keys.
- 1.2.0: cascade mode `local` removes the duplicate `.env.local` path, so
  report views no longer show the file overriding itself.
- 1.2.0: a custom `QuotedTail::Append` value normalizes CR line endings and
  reports Portable tight-hash/backtick warnings in the appended tail.
- 1.2.0: on Windows, `validate`/`apply` and `adotenv-cli run` reject keys that
  differ only by case and values over 32,767 UTF-16 code units. Recursive
  discovery also recognizes uppercase `.ENV` names, and cascade paths are
  canonicalized before mode deduplication.
- 1.2.0: CLI `--dialect=<value>` is accepted, repeated `--dialect` is a usage
  error, unknown `--` options are rejected, and temporary names are bounded.
- 1.2.0: `exec::Shell` caps standard output and standard error and stops
  commands after a timeout by default. The builders change the limits.
- 1.2.0: `fmt --write` and `set` resolve the write target once and check its
  device, inode, size and modification time before replacing it on Unix. A
  swapped symlink or in-place rewrite can no longer silently redirect the
  write; a target that changes after the read fails. This is best effort, not
  a lock against every concurrent writer.
- 1.2.0: `Document::set` with an empty value is idempotent when an inline
  comment is attached, and `Document::format` is idempotent for byte-capped
  dialects when spacing or trimming shortens the line.
- 1.2.0: temporary write and backup names use `.adotenv-*`, named input paths
  must be regular files, and `load`/`run` reject `-`/`--stdin` as a directory.
- 1.2.0: a dangling `.env.*` symlink is a load error rather than an absent
  optional file, and `resolve_decoded` keeps a leading U+FEFF key containing
  an apostrophe.
- 1.2.0: `Mask` also covers loaded entries, portability reports and their
  nested types.
- 1.2.0: `encode_value_with` tries a quoted body with an appended tail,
  including an empty or short quoted prefix, for custom `QuotedTail::Append`
  dialects, avoiding false `EncodeError::Unrepresentable` results at a line
  cap.
- 1.3.0: `adotenv-cli check` human and GitHub columns on line 1 no longer
  count a leading byte-order mark, and `--source` hides it. Use
  `LineIndex::without_bom` for the same columns in the library; the 1.x default
  still counts the mark.
- 1.3.0: `ParseOptions` and the `_with_options` parse functions add
  opt-in limits for input bytes, items and diagnostics. A limit failure is a
  `dotenv.limit-exceeded` diagnostic with `Diagnostic::limit()` data.
  `LoadOptions::with_limits` applies the same limits to loading and bounds the
  read itself; `LoadError::Limit` reports it. No limit is enabled by default.
- 1.4.0: `Document::rename_with_references` adds verified reference-aware key
  renaming (bracing unbraced references when needed).
- 1.5.0: `CompactValues`, `parse_values_compact*`, and
  `load_*_values_compact*` add single-buffer read-only storage for effective
  values with lower retained heap and allocation counts than `BTreeMap`.
- 1.5.4: `adotenv-cli diff --format json` publishes a versioned JSON Schema
  (`diff-report.v1.json`) alongside the check-report schema.
- 1.6.0: `ParseOptions::max_lines` / `LimitKind::Lines` bound physical lines,
  `BomPolicy::StripAll` strips repeated leading UTF-8 BOMs,
  `LookupIndex::unique` provides indexed uniqueness lookup, `EditOptions` /
  `Document::*_with_options` bound verified edit outputs, and `CompactEntries`
  preserves winning bare declarations in single-buffer storage.
- 1.6.1: `adotenv-cli` adds global `--max-input-bytes` / `--max-lines` and
  `set --max-output-bytes` / `--max-output-lines` flags.
- 1.7.0: `Document::rename_with_references_with_options` bounds reference-aware
  renames with `EditOptions`, `CompactValues` implements `PartialEq` and `Eq`,
  and `adotenv-process` adds `validate_compact` and `apply_compact`.
  Serde key-conversion errors now report the offending key name (`Some(key)`)
  instead of `Some("")`; delimiter-nesting depth overflows during resolution
  report `ResolveError::DepthExceeded` rather than
  `ResolveError::TooManyReferences`; `Document::rename_with_references` braces
  digit-leading keys, rejects escape/quote-breaking keys and shell-quote breaks
  inside `$(...)` commands, and verifies reference structure; Windows
  `exec::Shell` runs commands via `cmd /S /C` with raw argument quoting and
  rejects embedded line breaks; and `adotenv-cli` aborts `fmt`/`example` before
  output when `--max-input-bytes`/`--max-lines` limits trigger.
- 1.7.1: `Document::rename_with_references` verifies exact mapped spans across
  every reference field (`span`, `command`, `operator_span`, `argument` and
  `name`), rejects shell-quote changes on renamed references nested inside
  `$(...)` command substitutions, and fixes empty-key Compose renames around
  unclosed `${` and `QuotedTail::Append` boundaries.
- 1.7.2: `Document::rename_with_references` enforces a strict allowlist (ASCII
  alphanumeric, `_` and `.`) on renamed keys (`old_key` and `new_key`) nested
  inside `$(...)` command substitutions, rejecting shell metacharacters,
  parameter-expansion assignments, indirect expansions, array subscripts, globs
  and non-ASCII whitespace; `encode_value_with` also maps normalized LF to CR
  in `QuotedTail::Append` inverse tail search under `LineBreaks::Lf`.
- 1.7.3: `Document::rename_with_references` tightens the command-context
  allowlist to require an ASCII letter or `_` first byte with no trailing `.`
  or `..` (rejecting POSIX positional parameters like `0`/`1` and invalid
  `${1HOST}`/`..` substitutions) and enforces it for any matching reference
  inside or following a `$(` command substitution in the same entry;
  `encode_value_with` and `Document::set` also account for the mandatory
  trailing CR-padding byte under `LineBreaks::Lf` and `line_byte_limit` in
  `QuotedTail::Append` inverse search.
- 1.8.0: adds `CompactEntries` disk loaders (`load_cascade_entries_compact*`
  and `load_entries_compact*`), `adotenv-process::{validate_compact_entries,
  apply_compact_entries}`, bounded `Document::{comment,uncomment,set_export,
  format,sort}_with_options` with `EditOptions::max_references` /
  `LimitKind::References`, and `LineIndex::with_bom_policy`; `adotenv-cli` adds
  global `--max-items` / `--max-diagnostics` and bounded `fmt` flags
  (`--max-output-bytes`, `--max-output-lines`, `--max-references`), wires
  `get`/`load`/`run` to `CompactValues`, uses `LineIndex::with_bom_policy` in
  `check` (snapping `InsideCrLf` diagnostic offsets to line end), aligns
  `fmt --sort` on partially invalid files with `fmt` without `--sort`, and
  rejects duplicate `set --max-output-bytes` / `--max-output-lines` flags;
  `Document::deserialize` and `LoadedEnvironment::deserialize` parse non-string
  scalar map keys (such as `BTreeMap<u16, String>` and `BTreeMap<bool, String>`)
  with the same rules as values; `Document::example` emits bare text following a
  Compose closing quote on the same physical line (`A="x"tail\n`) as a `#`
  comment (`A=""\n#\n`) to prevent accidental secret tails from leaking while
  keeping explicit second assigned declarations (`A="x" B=2\n` -> `A=""\n B=\n`);
  and `Document::format` stays idempotent under `LineBreaks::Lf` (including
  custom `line_byte_limit`) when quote normalization combines with
  trailing-whitespace trimming or `FinalNewline::Remove`.

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
