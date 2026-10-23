# Loading

Status: standard file-cascade loading and the separate process adapter are
implemented in 0.6. `serde` integration is documented separately.

The core parser still performs no I/O. Loading is a std-only adapter around the
lossless document model.

## Feature and API

```toml
adotenv = { version = "0.9.6", features = ["load"] }
```

- `load_cascade(directory, mode)` uses Portable syntax.
- `load_cascade_with(directory, mode, dialect)` selects a dialect.
- `load_files(paths, dialect)` loads an explicit ordered list; every listed file
  must exist.
- `load_cascade_values` / `load_cascade_values_with` and `load_values` are the
  compact application path: they retain only final key/value strings, not
  origins, raw tokens or reports.
- `LoadedEnvironment::into_values` drops provenance and reports from an already
  loaded environment.

The standard cascade order is:

1. `.env`
2. `.env.local`
3. `.env.{mode}`
4. `.env.{mode}.local`

Later files override earlier files by effective key. An empty mode loads only
the base and local files. Mode names containing path separators, NUL, `.` or
`..` are rejected.

## Validation and atomicity

Every participating file is read as bytes and decoded as strict UTF-8 before
parsing. Invalid UTF-8 is an error, never replacement decoding. Each document is
parsed with the selected dialect; the first error-severity diagnostic rejects
the whole load. Warnings and ignored statements remain subject to the selected
dialect’s normal policy.

The result is built only after all files have been read and validated. A missing
cascade file is skipped, but an existing file that fails to read, decode or parse
returns `LoadError` and no partial environment. No process-environment variables
are changed by this module.

## Merging and origins

Each document contributes its effective declarations according to the dialect’s
duplicate and lookup-participation policy. Values remain literal; no expansion,
environment lookup or command execution occurs.

`LoadedEnvironment` contains:

```text
values        final BTreeMap<String, String>
origin(key)   final path, value span and one-based line
entries()     every loaded effective declaration, including overridden ones
overrides()   previous -> replacement origins for cross-file wins
repeated()    duplicate keys within a single file
skipped()     bare, nonparticipating or empty-key declarations
```

`LoadOrigin.span` addresses the original file bytes. `entries()` preserves file
load order and source order; `values` is sorted by key. Repeated declarations
are still reported even when the duplicate policy selects one winner.

Bare declarations are skipped rather than guessed as empty, except when the selected dialect’s `BareKeys::Empty` policy makes them effective empty values. Nonparticipating
declarations (for example Node’s `__proto__` object property or ignored bare
keys) and empty keys are reported with a `SkipReason`.

## Values-only loading

Application configuration often needs only the final effective strings. The
values-only path uses the same file reads, strict UTF-8 decoding, parser,
dialect, duplicate policy and later-file override rules as the full loader, but
retains only `LoadedValues` (a sorted key/value map). It does not expose origins,
raw winning tokens, overrides, repeats or skipped declarations.

```rust
use adotenv::{load::load_cascade_values, Dialect};

let values = load_cascade_values("/srv/my-app", "production")?;
assert_eq!(values.get("PORT"), Some("8080"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Use `LoadedEnvironment` when provenance or `resolve_contextual` is required.
For a 65,536-entry file, the local memory profile recorded in
[performance.md](performance.md) is about 38.8 MB peak / 13.5 MB retained for
values-only loading versus 81.3 MB / 60.5 MB for the full loader.

## Errors

`LoadError` distinguishes:

- I/O failure with path and OS error;
- invalid UTF-8;
- the first syntax/policy error with path and diagnostic;
- invalid mode names.

Missing explicit files passed to `load_files` are I/O errors; only the standard
cascade skips missing names.

## Resolution

`LoadedEnvironment::resolve_contextual` reconstructs the winning raw key/value
tokens in cascade order, reparses them with the load dialect and resolves the
result. Quote, escape and dialect context therefore survive the merge. Errors
retain the winning declaration's `LoadOrigin`, key and message. Both whole-file
and sequential scope are supported; the reconstructed source order is cascade
order.

`LoadedEnvironment::resolve_decoded` is the explicit decoded-value policy: every
final value is treated as unquoted decoded text. It requires whole-file scope.
Command substitutions remain literal in both APIs. The returned
`ResolvedEnvironment` retains the winning `LoadOrigin` for each key.

Use `Document::resolve` for context-aware resolution over a single retained
source document. See [expansion.md](expansion.md).

## Process application

The separate `adotenv-process` workspace crate depends on `adotenv` with `load`
enabled and exposes:

```text
validate(&LoadedEnvironment) -> Result<(), ApplyError>
unsafe apply(&LoadedEnvironment) -> Result<(), ApplyError>
```

`validate` checks every key for emptiness/NUL/`=` and every value for NUL before
mutation. `apply` performs the same validation, then calls the process
environment API. It sets only loaded keys and never clears unrelated variables.
The operation is explicitly unsafe because process environments are global and
not thread-safe on all platforms; changes cannot be rolled back. Returning the
`LoadedEnvironment` map remains the default.

## Future work

- Optional command/expansion callbacks during loading, never enabled implicitly.
