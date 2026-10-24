# Ecosystem integrations

Status: the `config` and `figment` companion adapters were published as 0.7.0 on
2026-10-19, 0.7.1 on 2026-10-20, 0.8.0 and 0.9.0 on 2026-10-21, 0.9.1 through
0.9.4 on 2026-10-22, and 0.9.5 through 0.9.16 on 2026-10-23/24. Trusted Publishing is configured for both crates.

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

Typed extraction uses `config`'s own scalar coercion; the adapter does not add a
second conversion layer:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct App {
    host: String,
    port: u16,
}

let app: App = config.try_deserialize()?;
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
are strings and Figment does not coerce them to numbers or booleans. Deserialize
string fields and convert explicitly:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct RawApp {
    host: String,
    port: String,
}

let raw: RawApp = Figment::new()
    .merge(AdotenvProvider::from_document(&doc))
    .extract()?;
let port: u16 = raw.port.parse()?;
```

`from_environment` is available with the `load` feature, and `from_values`
accepts an explicit map.

## Packaging

Both crates are published at version `0.9.16`. Their default feature set is
empty; `load` enables only the environment constructors and the core crate’s
file-loading feature. Docs.rs builds them with all features. `adotenv-config`
is built against `config` 0.14, the version compatible with the workspace MSRV.
See [RELEASING.md](../../RELEASING.md) for the release bootstrap and Trusted
Publishing setup.

### `config` 0.15 evaluation

`config` remains at 0.14. Version 0.15.0 declares Rust 1.75 support, but its
key-normalization behavior differs from 0.14: the adapter’s `A.B` source no
longer resolves through the lowercase `a.b` lookup in the compatibility test.
The current 0.15 release line also requires Rust 1.88 and edition 2024. Upgrading
would therefore require pinning an old 0.15 patch while accepting or documenting
a key-semantics change, so the workspace keeps 0.14 and its native semantics.
