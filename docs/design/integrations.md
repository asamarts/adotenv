# config and Figment

Two adapter crates expose literal values to configuration libraries. Both keep
their own 0.x version line, separate from the core.

## config

```toml
[dependencies]
adotenv = "1"
adotenv-config = "0.12"
config = "0.15"
```

`adotenv-config` provides `AdotenvSource`, a `config` 0.15 `Source`. It needs
Rust 1.88, because `config` 0.15 does.

```rust,ignore
use adotenv_config::AdotenvSource;

let doc = adotenv::parse_strict("host=localhost\nport=8080\n")?;
let config = config::Config::builder()
    .add_source(AdotenvSource::from_document(&doc))
    .build()?;

assert_eq!(config.get_string("host")?, "localhost");
assert_eq!(config.get_int("port")?, 8080);
```

`config` converts the string values to typed fields with its own scalar
coercion, so `config.try_deserialize::<App>()` can fill a `u16` field from
`8080`.

Keys are passed to `config` unchanged. `config` 0.15 keeps key case and treats
`.` as a nested path separator: `A.B` is key `B` inside table `A`, and `A.B`
and `a.b` are different keys. Typed extraction matches field names exactly, so
`HOST=localhost` does not fill a `host` field. Use `from_values` to supply keys
that are already normalized for `config`.

## Figment

```toml
[dependencies]
adotenv = "1"
adotenv-figment = "0.12"
figment = "0.10"
```

`adotenv-figment` provides `AdotenvProvider`, a Figment 0.10 `Provider` that
exposes a flat `Profile::Default` dictionary with the original key spelling.
It needs Rust 1.81.

```rust,ignore
use adotenv_figment::AdotenvProvider;
use figment::Figment;

let doc = adotenv::parse_strict("host=localhost\nport=8080\n")?;
let figment = Figment::new().merge(AdotenvProvider::from_document(&doc));
```

Figment values remain strings: Figment does not coerce them. Deserialize
string fields and convert them yourself, or deserialize with the core `serde`
feature instead. Figment's path lookups treat dots as separators, so extract
into a map or a matching structure when dots are part of a key.

## Constructors

Both types have the same constructors:

| Constructor | Source |
|---|---|
| `from_document(&Document)` | The effective values of a parsed document |
| `from_environment(&LoadedEnvironment)` | The final values of a loaded cascade; needs the adapter's `load` feature |
| `from_values(BTreeMap<String, String>)` | An explicit map |

`values()` returns the map the adapter holds.

```toml
adotenv-config = { version = "0.12", features = ["load"] }
```

## Shared rules

Both adapters follow the document's or environment's duplicate policy, lookup
participation and bare-key behavior, so bare keys appear as empty strings only
under `BareKeys::Empty`. They do not expand references, read files or change
the process environment. Their `Debug` output includes values.
