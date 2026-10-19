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

`from_values` accepts an explicit map. Enable the `load` feature to use
`from_environment` with a `LoadedEnvironment`.

## Key semantics

`config` owns key normalization. It lowercases names and interprets `.` as a
nested path separator. Thus `.env` keys `A.B` and `a.b` collide, and `A.B`
becomes a nested `a.b` table rather than a literal flat key. The adapter does
not escape or rewrite keys; use `from_values` for already normalized config
keys.

Values are decoded literal strings. Duplicate policy, nonparticipating keys and
`BareKeys::Empty` follow the wrapped document or loaded environment. No
expansion, command execution or implicit file access occurs.

This crate is built against `config` 0.14, which supports the workspace MSRV of
Rust 1.81. It is part of the
[`adotenv` workspace](https://github.com/asamarts/adotenv).
