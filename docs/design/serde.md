# Serde

The `serde` feature deserializes documents and loaded environments into typed
values. It does not need `std`, so it also works in a `no_std` build with
`alloc`.

```toml
adotenv = { version = "1", features = ["serde"] }
```

## Documents

`Document::deserialize` maps the effective declarations into any type that
implements `DeserializeOwned`:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    host: String,
    port: u16,
    debug: Option<bool>,
}

let doc = adotenv::parse_strict("host=localhost\nport=8080\n").unwrap();
let config: Config = doc.deserialize().unwrap();
assert_eq!(config.host, "localhost");
assert_eq!(config.port, 8080);
assert_eq!(config.debug, None);
```

Keys follow the document's duplicate policy and lookup participation, so the
last declaration of a duplicated key wins in the Portable dialect. Bare
declarations contribute an empty string only under `BareKeys::Empty`;
otherwise they are absent. Values stay literal: `${HOST}` is the text
`${HOST}`.

## Loaded environments

With `load` and `serde` enabled, `LoadedEnvironment::deserialize` uses the
final merged values of the cascade:

```rust,no_run
# use serde::Deserialize;
# #[derive(Deserialize)]
# struct Config { host: String, port: u16 }
let loaded = adotenv::load::load_cascade("/srv/my-app", "production")?;
let config: Config = loaded.deserialize()?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Conversion rules

The top level is a struct or a map. Keys and values are the decoded literal
text, converted like this:

| Target type | Accepted values |
|---|---|
| `String` | Any value |
| `bool` | Exactly `true` or `false` |
| Integers and floats | Rust's `FromStr` syntax for that type |
| `char` | Exactly one character |
| `Option<T>` | `None` when the key is absent, otherwise `Some` of the converted value |
| Unit and unit structs | An empty value |
| Newtype structs | The inner type's rule |
| Bytes | The value's UTF-8 bytes |

An empty value is still present: `PORT=` into `Option<u16>` is an error, not
`None`. Sequences, nested maps, tuples and enums are rejected, including enums
with only unit variants. Deserialize a `String` and convert it yourself when a
field needs another shape.

## Errors

`DeserializeError` has `line()`, `key()` and `message()`:

- A value that does not convert reports the key and the one-based line of its
  declaration.
- In a loaded environment, the line comes from the retained origin. With
  `LoadOptions::with_origins(false)` it is `0`.
- A map key that does not convert, such as `abc` into a `u16` key, reports its
  line and an empty key.
- A structural error, such as a missing field, has line `0` and no key.

Error messages include the value that failed to convert, such as
`expected a u16, found "abc"`. Treat them as secret when values are secret.
