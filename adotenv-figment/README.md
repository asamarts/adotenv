# adotenv-figment

A [`figment`](https://crates.io/crates/figment) provider for literal
[`adotenv`](https://crates.io/crates/adotenv) documents.

```rust,ignore
use adotenv::parse_strict;
use adotenv_figment::AdotenvProvider;
use figment::Figment;

let doc = parse_strict("host=localhost\nport=8080\n")?;
let figment = Figment::new().merge(AdotenvProvider::from_document(&doc));
```

`from_values` accepts an explicit map. The `load` feature adds `from_environment`
for `LoadedEnvironment`.

## Values and keys

The provider uses `Profile::Default` and keeps original key spelling. Figment's
path lookups still treat dots as separators. Extract into a literal map or a
matching typed structure when dots are part of a key.

Values are strings. Deserialize string fields and convert numbers or booleans
explicitly.

```rust,ignore
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

Duplicate policy, lookup participation and `BareKeys::Empty` follow the source
document or loaded environment. The adapter performs no expansion, command
execution or file access.

The crate supports Rust 1.81.
