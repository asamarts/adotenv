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

The adapter implements the `config` 0.15 `Source` trait and requires Rust 1.88.

`config` keeps key case and treats `.` as a nested path separator, so `A.B` is
key `B` inside table `A`, and `A.B` and `a.b` are different keys. Typed
extraction matches field names exactly: `HOST=localhost` does not fill a `host`
field. The adapter does not rewrite keys. Use `from_values` to supply keys that
are already normalized for `config`.

Values are decoded literals. Duplicate policy, lookup participation and
`BareKeys::Empty` follow the source document or loaded environment. The adapter
performs no expansion, command execution or file access.
