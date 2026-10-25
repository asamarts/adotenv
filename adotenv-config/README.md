# adotenv-config

A [`config`](https://crates.io/crates/config) source adapter for literal
[`adotenv`](https://crates.io/crates/adotenv) documents.

```rust,ignore
use adotenv::parse_strict;
use adotenv_config::AdotenvSource;

let doc = parse_strict("host=localhost\nport=8080\n")?;
let config = config::Config::builder()
    .add_source(AdotenvSource::from_document(&doc))
    .build()?;

assert_eq!(config.get_string("host")?, "localhost");
assert_eq!(config.get_int("port")?, 8080);
```

Typed extraction uses `config`'s scalar coercion:

```rust,ignore
use serde::Deserialize;

#[derive(Deserialize)]
struct App {
    host: String,
    port: u16,
}

let app: App = config.try_deserialize()?;
```

`from_values` accepts an explicit map. The `load` feature adds `from_environment`
for `LoadedEnvironment`.

## Keys

`config` lowercases names and treats `.` as a nested path separator. Keys `A.B`
and `a.b` therefore collide. The adapter does not rewrite keys. Use `from_values`
when keys are already normalized for `config`.

Values are decoded literals. Duplicate policy, lookup participation and
`BareKeys::Empty` follow the source document or loaded environment. The adapter
performs no expansion, command execution or file access.

The crate is built against `config` 0.14 and supports Rust 1.81.
