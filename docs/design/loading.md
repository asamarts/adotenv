# Loading

The `load` feature reads `.env` files from disk. Parsing itself never reads
files, and loading never changes the process environment.

```toml
adotenv = { version = "1", features = ["load"] }
```

## Cascades

`load_cascade(directory, mode)` reads these files from `directory`, in order:

1. `.env`
2. `.env.local`
3. `.env.{mode}`
4. `.env.{mode}.local`

Every file is parsed with the same dialect. Within one file, the dialect's
duplicate-key and bare-key rules choose the effective value of each key, as
`Document::get` does. Across files, a later file's effective value replaces an
earlier one, key by key. When the declaration that wins in a file is a bare
key, the Portable dialect gives the key no value there, so the value from
earlier files stays: with `PORT=3000` in `.env`, and `PORT=8080` followed by a
bare `PORT` in `.env.local`, the result is `3000`.

A cascade file that does not exist is skipped, so a directory with none of
them, or a directory that does not exist, loads an empty environment. A file
that exists but cannot be read, is not valid UTF-8 or has an error-severity
diagnostic fails the whole load, and no values are returned. Warnings never
fail a load.

`load_cascade` reads with the Portable dialect. The other entry points choose
the dialect, the retained data or an explicit file list:

| Function | Reads | Returns |
|---|---|---|
| `load_cascade(directory, mode)` | Cascade, Portable | `LoadedEnvironment` |
| `load_cascade_with(directory, mode, dialect)` | Cascade | `LoadedEnvironment` |
| `load_cascade_with_options(directory, mode, options)` | Cascade | `LoadedEnvironment` |
| `load_files(paths, dialect)` | Ordered file list | `LoadedEnvironment` |
| `load_files_with_options(paths, options)` | Ordered file list | `LoadedEnvironment` |
| `load_cascade_values(directory, mode)` | Cascade, Portable | `LoadedValues` |
| `load_cascade_values_with(directory, mode, dialect)` | Cascade | `LoadedValues` |
| `load_cascade_values_with_options(directory, mode, options)` | Cascade | `LoadedValues` |
| `load_values(paths, dialect)` | Ordered file list | `LoadedValues` |
| `load_values_with_options(paths, options)` | Ordered file list | `LoadedValues` |
| `load_cascade_values_compact(directory, mode)` | Cascade, Portable | `CompactValues` |
| `load_cascade_values_compact_with(directory, mode, dialect)` | Cascade | `CompactValues` |
| `load_cascade_values_compact_with_options(directory, mode, options)` | Cascade | `CompactValues` |
| `load_values_compact(paths, dialect)` | Ordered file list | `CompactValues` |
| `load_values_compact_with_options(paths, options)` | Ordered file list | `CompactValues` |

The functions that take a file list read the files in the order given, and
later files override earlier ones as in a cascade. Every listed file must
exist: a missing one is `LoadError::Io`.

### Mode names

A mode may not contain `/`, `\` or NUL and may not be exactly `.` or `..`, so
it cannot name a file outside the directory. Such a mode returns
`LoadError::InvalidMode` before any file is read. Other dots are allowed, so mode
`prod.eu` reads `.env.prod.eu` and `.env.prod.eu.local`. An empty mode reads
only `.env` and `.env.local`. Duplicate cascade paths are removed in order:
mode `local` reads `.env.local` once even though it is both the base-local and
mode-local name.

## Loaded environments

```rust,no_run
use adotenv::load::load_cascade;

let loaded = load_cascade("/srv/my-app", "production")?;
assert_eq!(loaded.get("PORT"), Some("8080"));

let origin = loaded.origin("PORT").unwrap();
println!("{}:{}", origin.path().display(), origin.line());
# Ok::<(), adotenv::load::LoadError>(())
```

`LoadedEnvironment` stores the final values, sorted by key, and by default the
origin of each value and the raw source text of its winning declaration. The
views borrow the environment:

| Method | Returns |
|---|---|
| `get(key)` | `Option<&str>` |
| `values()` | `Iterator<Item = (&str, &str)>`, sorted by key |
| `len()`, `is_empty()` | Number of final values |
| `origin(key)` | `Option<LoadOrigin<'_>>` for the winning declaration |
| `entries()` | `Iterator<Item = LoadedEntry<'_>>` |
| `overrides()` | `Iterator<Item = LoadOverride<'_>>` |
| `repeated()` | `Iterator<Item = LoadRepeated<'_>>` |
| `skipped()` | `Iterator<Item = LoadSkipped<'_>>` |
| `retains_raw_tokens()` | Whether `resolve_contextual` is available |
| `into_values()` | `LoadedValues` |

`LoadOrigin` has `path()`, `span()` and `line()`. The path is the directory
joined with the file name for a cascade, or the path as given in a file list.
The span is a byte range that covers the value as written, quotes included; for
a bare key read as empty under `BareKeys::Empty`, it covers the key. The line
is the one-based line of the key.

### Reports

Reports are off by default. With reports enabled:

- `entries()` yields one `LoadedEntry` per key and file, with `key()`,
  `value()` and `origin()`: the effective declaration, including values that a
  later file replaced.
- `overrides()` yields a `LoadOverride` each time a later file replaces a key,
  with `key()`, `previous()` and `replacement()` origins.
- `repeated()` yields a `LoadRepeated` for a key declared more than once in one
  file, with `key()` and the `origins()` of the declarations that take part in
  lookup.
- `skipped()` yields a `LoadSkipped` for each key in a file that supplied no
  value, with `key()`, `origin()` and `reason()`. `SkipReason` is `Bare` when
  the winning declaration is a bare key, `NonParticipating` when the dialect
  excludes every declaration of the key from lookup, and `EmptyKey` for each
  declaration with an empty key.

Empty keys are not part of the final values; Compose declarations with an
empty key appear in `skipped()` with `SkipReason::EmptyKey`. Report records
share keys and values with the final values, so reports do not copy them.

```rust,no_run
use adotenv::load::{load_cascade_with_options, LoadOptions};
use adotenv::Dialect;

let loaded = load_cascade_with_options(
    "/srv/my-app",
    "production",
    LoadOptions::full(Dialect::PORTABLE),
)?;

for change in loaded.overrides() {
    println!(
        "{} from line {} replaced by line {}",
        change.key(),
        change.previous().line(),
        change.replacement().line(),
    );
}
# Ok::<(), adotenv::load::LoadError>(())
```

## Options

`LoadOptions::new(dialect)` keeps values, origins and the raw source text that
`resolve_contextual` needs. `LoadOptions::full(dialect)` also keeps every
report. `LoadOptions::default()` is `LoadOptions::new(Dialect::PORTABLE)`.
Builders change one group each:

| Builder | Effect |
|---|---|
| `with_dialect(dialect)` | Read with another dialect |
| `with_raw_tokens(false)` | Drop the raw source text; `resolve_contextual` then returns an error |
| `with_reports(true)` | Keep every report, as `full` does; without reports the report iterators yield nothing |
| `with_origins(false)` | Drop origins and the override report; resolution errors then have no origin and serde errors report line 0 |
| `with_limits(parse_options)` | Bound input bytes, items and diagnostics; input bytes also bounds the read |

## Resolution

Two methods resolve references in a loaded environment. Both return a
`ResolvedEnvironment` with `get(key)`, `values()`, `origin(key)`, `len()` and
`is_empty()`, or a `LoadedResolveError` with `key()`, `origin()` and
`message()`.

- `resolve_contextual(lookup, options)` parses the raw source text of each
  winning declaration again with the load dialect, so quotes and escapes keep
  their meaning: a single-quoted `'${HOST}'` stays literal, and an escaped
  `\$HOST` in double quotes stays `$HOST`. It needs the raw source text, which
  the default options keep. The rebuilt declarations are checked against the
  loaded keys and values; a key or value that cannot be reproduced exactly is
  an error naming the key.
- `resolve_decoded(lookup, options)` treats every final value as unquoted
  text, so a reference resolves wherever it appears, even where the file
  quoted it. Keys take part exactly as loaded, whatever dialect read them, so
  `${A B}` finds a Python key written `'A B'`. A key or value that cannot be
  written back exactly, such as a key with both an apostrophe and whitespace
  or a value with a control character other than tab, CR or LF, is an error
  naming that key. A key is never read as a different key.

In both methods a reference sees the final value of every loaded key,
whichever file set it. Names are looked up in those values and with the
`lookup` closure, in the `lookup_order` of the options; with the presets, the
loaded values win. Both methods need whole-file scope and return an error for
any other scope. Every `ExpandOptions` preset uses sequential scope, so select
whole-file scope explicitly:

```rust,no_run
use adotenv::load::load_cascade;
use adotenv::{ExpandOptions, ExpandScope};

let loaded = load_cascade("/srv/my-app", "production")?;
let options = ExpandOptions::portable().with_scope(ExpandScope::WholeFile);
let resolved = loaded.resolve_contextual(|key| std::env::var(key).ok(), &options)?;
println!("{:?}", resolved.get("URL"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

The winning declarations come from different files, so no single declaration
order gives sequential scope a meaning. With `B=1` and `A=${B}/x` in `.env` and
`B=2` in `.env.local`, a sequential reference from `A` would see no earlier
`B`. Whole-file scope resolves `A` to `2/x`.

Command substitutions stay literal in both methods. To run them, parse a file
yourself and call `Document::resolve_with_commands`, for example with
`exec::Shell` from the `exec` feature. See
[References and resolution](expansion.md#commands).

## Values only

`load_cascade_values`, `load_cascade_values_with` and `load_values` read the
same files with the same rules and errors, but keep only the final strings.
They parse each file like `parse_values_with`, without building a `Document`,
and keep no origins, raw source text or reports. `LoadedValues` has
`get(key)`, `values()` (a `BTreeMap`), `len()`, `is_empty()` and
`into_values()`. The `*_compact` variants return `CompactValues` instead,
using a single value buffer and one fixed-size entry per key.

```rust,no_run
use adotenv::load::load_cascade_values;

let values = load_cascade_values("/srv/my-app", "production")?;
assert_eq!(values.get("PORT"), Some("8080"));
# Ok::<(), adotenv::load::LoadError>(())
```

## Errors

`LoadError` has these variants:

| Variant | Meaning |
|---|---|
| `Io { path, source }` | A file exists but could not be read, or a listed file does not exist. `source()` returns the I/O error. |
| `InvalidUtf8 { path }` | A file is not valid UTF-8. |
| `Invalid { path, diagnostic }` | A file has an error-severity diagnostic; `diagnostic` is the first one in source order. |
| `Limit { path, limit }` | A configured read or parse limit was exceeded; `limit` reports its kind, maximum and observed value. |
| `InvalidMode` | The mode name is not allowed. |

The `Display` of a file error names the file, for example
`/srv/my-app/.env: line 2: key does not match the selected dialect (dotenv.invalid-key)`.
A `LoadedResolveError` displays as `{path}:{line}: {message}` when its origin
is known, and as the message alone otherwise.

`LoadOptions::with_limits` applies `ParseOptions` to every file.
`max_input_bytes` is checked against the file metadata and again while reading,
so a file that changes after the check cannot grow without bound. A limit
failure returns `LoadError::Limit`; no limit is enabled by default.

## Memory

For one 65,536-entry file, the default profile retains about 28 MB, the full
report profile about 38 MB and values-only loading about 10 MB. The default
loader keeps one record per key for its value, origin and raw source text. The
measurements and budgets are in [Performance](performance.md#memory-and-instructions).
