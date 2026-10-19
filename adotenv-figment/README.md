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

`from_values` accepts an explicit map. Enable the `load` feature to use
`from_environment` with a `LoadedEnvironment`.

## Value and key behavior

The provider exposes a `Profile::Default` dictionary with the original key
spelling. Figment's path-oriented lookup APIs still treat dots as separators;
extraction into a literal map or a matching typed structure is the lossless
route. Values are strings: deserialize string fields or convert explicitly.
The adapter does not infer numeric or boolean types.

Duplicate policy, nonparticipating keys and `BareKeys::Empty` follow the wrapped
document or loaded environment. No expansion, command execution or implicit
file access occurs.

This crate is part of the [`adotenv` workspace](https://github.com/asamarts/adotenv)
and supports Rust 1.81.
