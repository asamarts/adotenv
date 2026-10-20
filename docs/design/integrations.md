# Ecosystem integrations

Status: the `config` and `figment` companion adapters were published as 0.7.0 on
2026-10-19 and 0.7.1 on 2026-10-20. Trusted Publishing is configured for both
crates.

Both adapters consume effective literal values from either a `Document` or,
with their optional `load` feature, a `LoadedEnvironment`. They never expand
references, read files or mutate the process environment themselves.

## shared value rules

- Duplicate keys follow the document’s dialect policy.
- Nonparticipating and empty keys are omitted.
- Bare declarations contribute `""` only under `BareKeys::Empty`.
- Values are decoded literal strings.
- Loaded environments contribute their final effective values.

## `config`

`adotenv-config::AdotenvSource` implements `config::Source`:

```rust
use adotenv::parse_strict;
use adotenv_config::AdotenvSource;

let doc = parse_strict("host=localhost\nport=8080\n").unwrap();
let config = config::Config::builder()
    .add_source(AdotenvSource::from_document(&doc))
    .build()
    .unwrap();
assert_eq!(config.get_string("host").unwrap(), "localhost");
assert_eq!(config.get_int("port").unwrap(), 8080);
```

`from_environment` consumes a `LoadedEnvironment` when the `load` feature is
enabled; `from_values` accepts an explicit map.

`config` owns key normalization. It lowercases names and interprets `.` as a
nested path separator. Thus `.env` keys `A.B` and `a.b` collide, and `A.B`
becomes a nested `a.b` table rather than a literal flat key. The adapter does
not escape or rewrite keys; use `from_values` if you need to supply already
normalized native config keys. Values are strings and `config` handles its own
scalar coercion.

## `figment`

`adotenv-figment::AdotenvProvider` implements `figment::Provider` at
`Profile::Default`:

```rust
use adotenv::parse_strict;
use adotenv_figment::AdotenvProvider;
use figment::Figment;

let doc = parse_strict("host=localhost\nport=8080\n").unwrap();
let figment = Figment::new().merge(AdotenvProvider::from_document(&doc));
```

The provider preserves the original key spelling in its dictionary. Figment’s
path-oriented lookup APIs still treat dots as separators; extraction into a
literal map or a matching typed structure is the lossless route. Figment values
are strings. Deserialize string fields or convert them explicitly; the adapter
does not infer numeric or boolean types. `from_environment` is available with
the `load` feature, and `from_values` accepts an explicit map.

## Packaging

Both crates are published at version `0.7.1`. Their default feature set is
empty; `load` enables only the environment constructors and the core crate’s
file-loading feature. Docs.rs builds them with all features. `adotenv-config`
is built against `config` 0.14, the version compatible with the workspace MSRV.
See [RELEASING.md](../../RELEASING.md) for the release bootstrap and Trusted
Publishing setup.
