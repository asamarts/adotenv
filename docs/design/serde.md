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

The document is read as a map from each key to its effective value, the value
that `Document::get` returns. Duplicate keys follow the dialect's rule, so the
last declaration of a repeated key wins in the Portable dialect. Declarations
that do not take part in lookup and empty keys are left out, and a bare key
appears, as an empty string, only under `BareKeys::Empty`. Values stay literal:
`${HOST}` is the text `${HOST}`. Keys the target type does not name are ignored
unless it uses `#[serde(deny_unknown_fields)]`.

## Loaded environments

With `load` and `serde` enabled, `LoadedEnvironment::deserialize` uses the
final merged values, the ones that `LoadedEnvironment::get` returns:

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
text. Each value converts like this:

| Target type | Accepted values |
|---|---|
| `String` | Any value |
| `bool` | Exactly `true` or `false` |
| Integers and floats | Rust's `FromStr` syntax for that type |
| `char` | Exactly one character |
| `Option<T>` | `None` when the key is absent, otherwise `Some` of the converted value |
| Unit and unit structs | An empty value |
| Newtype structs | The inner type's rule |
| Bytes, as requested by `serde_bytes::ByteBuf` | The value's UTF-8 bytes |

An empty value is still present: `PORT=` into `Option<u16>` is an error, not
`None`. Sequences, nested maps and structs, tuples and enums are rejected,
including enums with only unit variants and `Vec<u8>`, which Serde reads as a
sequence. Deserialize a `String` and convert it yourself when a field needs
another shape.

Map keys are passed as strings, so a map with non-string keys, such as
`BTreeMap<u16, String>`, fails even for a key like `8080`. Fields inside
`#[serde(flatten)]` also receive every value as a string, so only fields that
accept a string, such as `String`, work there.

## Errors

`DeserializeError` has `line()`, `key()` and `message()`:

- A value that does not convert reports the key and the one-based line of its
  declaration.
- In a loaded environment, the line comes from the retained origin, and the
  file is not reported; pass the key to `LoadedEnvironment::origin` to find
  it. With `LoadOptions::with_origins(false)` the line is `0`, but a value
  error still names the key.
- A key that does not convert, such as an unknown field under
  `#[serde(deny_unknown_fields)]` or any key of a map with non-string keys,
  reports its line and an empty key, `Some("")`.
- An error about the whole map, such as a missing field, has line `0` and no
  key.

The `Display` form is `line {line}: {message} (key "{key}")` when the line is
known, and the message alone when the line is `0`.

Error messages include the value that failed to convert, such as
`expected a u16, found "abc"`. Treat them as secret when values are secret.
