# Loading

The `load` feature adds strict filesystem loading. Parsing itself never reads
files.

## Cascades

`load_cascade(directory, mode)` reads, in order:

1. `.env`
2. `.env.local`
3. `.env.{mode}`
4. `.env.{mode}.local`

A missing cascade file is skipped. A file that exists but cannot be read,
decoded as UTF-8 or parsed invalidates the whole load. Later files override
earlier files by effective key.

`load_files(paths, dialect)` reads an explicit ordered list. Every listed file
must exist.

## Environment data

`LoadedEnvironment` stores:

- final values, sorted by key;
- shared-file origins with path, value span and line;
- raw winning tokens for contextual resolution;
- optional reports for entries, overrides, repeats and skipped declarations.

The default profile keeps values, origins and raw tokens. Reports are off.

```rust,ignore
use adotenv::load::load_cascade;

let loaded = load_cascade("/srv/my-app", "production")?;
assert_eq!(loaded.get("PORT"), Some("8080"));

let origin = loaded.origin("PORT").unwrap();
println!("{}:{}", origin.path().display(), origin.line());
```

Values and report views borrow the environment:

```text
get(key)                 Option<&str>
values()                 Iterator<Item = (&str, &str)>
origin(key)              Option<LoadOrigin<'_>>
entries()                Iterator<Item = LoadedEntry<'_>>
overrides()              Iterator<Item = LoadedOverride<'_>>
repeated()               Iterator<Item = LoadRepeated<'_>>
skipped()                Iterator<Item = LoadSkipped<'_>>
into_values()            LoadedValues
```

`LoadOrigin<'_>` exposes `path()`, `span()` and `line()`. Entry, override,
repeat and skipped views expose accessors rather than public fields.

## Options

`LoadOptions::new(dialect)` is the default. `LoadOptions::full(dialect)` enables
all reports. Builders control each group:

| Builder | Effect |
|---|---|
| `with_raw_tokens(false)` | Drop raw tokens; `resolve_contextual` returns an error |
| `with_reports(false)` | Leave report iterators empty |
| `with_origins(false)` | Drop final origins and override reports |

With reports enabled, entry records share keys and values with the final value
table. Reports do not duplicate final values.

## Resolution

`resolve_contextual` reconstructs each winning raw token, reparses it with the
load dialect and resolves references. Quote, escape and dialect context survive
the merge. It requires raw-token retention.

`resolve_decoded` treats every final value as unquoted decoded text. It requires
whole-file resolution scope.

Both methods return `ResolvedEnvironment`. Origins are available through
`origin(key)`. Command substitution remains literal unless a caller callback is
provided through the document resolver.

## Values only

`load_cascade_values` and `load_values` retain only final strings. They use the
parser projection path and do not keep origins, raw tokens, reports or a
lossless document.

```rust,ignore
use adotenv::{load::load_cascade_values, Dialect};

let values = load_cascade_values("/srv/my-app", "production")?;
assert_eq!(values.get("PORT"), Some("8080"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Errors

`LoadError` distinguishes:

- I/O failure with path and OS error;
- invalid UTF-8;
- the first syntax or policy error with path and diagnostic;
- an invalid mode name.

Mode names may not contain path separators, `.` or `..`.

## Memory

Measured on a release build with one 65,536-entry file:

| Profile | Peak | Retained |
|---|---:|---:|
| Default | 47.3 MB | 27.8 MB |
| `LoadOptions::full` | 57.1 MB | 37.6 MB |
| Values only | 11.2 MB | 9.9 MB |

The default loader keeps one record per key for its value, origin and raw token.
Budgets and the full benchmark method are in [performance.md](performance.md).
