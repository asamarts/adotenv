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

Later files override earlier files by effective key. A cascade file that does
not exist is skipped, so a directory with none of them, or a directory that
does not exist, loads an empty environment. A file that exists but cannot be
read, is not UTF-8 or has a syntax error fails the whole load.

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
| `load_values(paths, dialect)` | Ordered file list | `LoadedValues` |

Every file in an explicit list must exist. Later files override earlier ones.

### Mode names

A mode may not contain `/`, `\` or NUL and may not be exactly `.` or `..`. Such
a mode returns `LoadError::InvalidMode`. Other dots are allowed, so mode
`prod.eu` reads `.env.prod.eu` and `.env.prod.eu.local`. An empty mode reads
only `.env` and `.env.local`.

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
origin and raw winning token of each value. The views borrow the environment:

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

`LoadOrigin` has `path()`, `span()` and `line()`. The span covers the value in
the file; for a bare key read as empty under `BareKeys::Empty`, it covers the
key.

### Reports

Reports are off by default. With reports enabled:

- `entries()` yields one `LoadedEntry` per key and file, with `key()`,
  `value()` and `origin()`.
- `overrides()` yields a `LoadOverride` each time a later file replaces a key,
  with `key()`, `previous()` and `replacement()` origins.
- `repeated()` yields a `LoadRepeated` for a key declared more than once in one
  file, with `key()` and the `origins()` of the declarations that take part in
  lookup.
- `skipped()` yields a `LoadSkipped` for each declaration that has no value,
  with `key()`, `origin()` and `reason()`. `SkipReason` is `Bare` for a
  key-only declaration, `NonParticipating` for a declaration the dialect
  excludes from lookup and `EmptyKey` for an empty key.

Entry records share keys and values with the final table, so reports do not
copy final values.

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

`LoadOptions::new(dialect)` keeps values, origins and raw tokens.
`LoadOptions::full(dialect)` also keeps every report. `LoadOptions::default()`
is `LoadOptions::new(Dialect::PORTABLE)`. Builders change one group each:

| Builder | Effect |
|---|---|
| `with_dialect(dialect)` | Read with another dialect |
| `with_raw_tokens(false)` | Drop raw tokens; `resolve_contextual` then returns an error |
| `with_reports(false)` | Leave the report iterators empty |
| `with_origins(false)` | Drop origins and override reports; serde errors then report line 0 |

## Resolution

Two methods resolve references in a loaded environment. Both return a
`ResolvedEnvironment` with `get(key)`, `values()`, `origin(key)`, `len()` and
`is_empty()`, or a `LoadedResolveError` with `key()`, `origin()` and
`message()`.

- `resolve_contextual(lookup, options)` rebuilds each winning raw token,
  reparses it with the load dialect and resolves its references, so quote,
  escape and dialect context survive the merge. It needs raw tokens, which the
  default options keep.
- `resolve_decoded(lookup, options)` treats every final value as unquoted
  decoded text. Keys take part exactly as loaded, whatever dialect read them,
  so `${A B}` finds a Python key written `'A B'`. A key that the synthetic
  source cannot spell, such as one that contains both an apostrophe and
  whitespace, is an error naming that key. It is never read as a different
  key.

Both methods need whole-file scope. Every `ExpandOptions` preset uses
sequential scope, so select whole-file scope explicitly:

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

`load_cascade_values`, `load_cascade_values_with` and `load_values` keep only
the final strings. They use the same projection path as core `parse_values`
and keep no origins, raw tokens, reports or documents. `LoadedValues` has
`get(key)`, `values()` (a `BTreeMap`), `len()`, `is_empty()` and
`into_values()`.

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
| `Io { path, source }` | A file exists but could not be read. `source()` returns the I/O error. |
| `InvalidUtf8 { path }` | A file is not valid UTF-8. |
| `Invalid { path, diagnostic }` | The first syntax or policy error in a file. |
| `InvalidMode` | The mode name is not allowed. |

## Memory

For one 65,536-entry file, the default profile retains about 28 MB, the full
report profile about 38 MB and values-only loading about 10 MB. The default
loader keeps one record per key for its value, origin and raw token. The
measurements and budgets are in [Performance](performance.md#memory-and-instructions).
