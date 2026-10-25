# Integrations

## config

`adotenv-config` exposes document values as a `config` source.

```rust,ignore
use adotenv_config::AdotenvSource;

let doc = adotenv::parse_strict("host=localhost\nport=8080\n").unwrap();
let config = config::Config::builder()
    .add_source(AdotenvSource::from_document(&doc))
    .build()?;
```

The adapter follows `config` key rules: names are case-insensitive and dots
create nested paths. Its `load` feature adds `from_environment` for
`LoadedEnvironment`.

`config` can deserialize typed structures with its own scalar coercion.

## Figment

`adotenv-figment` exposes a `Profile::Default` dictionary with original key
spelling.

```rust,ignore
use adotenv_figment::AdotenvProvider;

let doc = adotenv::parse_strict("host=localhost\nport=8080\n").unwrap();
let figment = figment::Figment::new().merge(AdotenvProvider::from_document(&doc));
```

Figment values remain strings. Deserialize string fields and convert explicitly.

The `load` feature adds `from_environment` for loaded environments.

## Shared rules

Both adapters use the document or environment's duplicate policy, lookup
participation and bare-key behavior. They do not expand references, read files
or mutate the process environment.
