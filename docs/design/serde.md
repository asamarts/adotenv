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
text. Each key and value converts like this:

| Target type | Accepted text |
|---|---|
| `String` | Any text |
| `bool` | Exactly `true` or `false` |
| Integers and floats | Rust's `FromStr` syntax for that type |
| `char` | Exactly one character |
| `Option<T>` | For struct fields, `None` when the key is absent, otherwise `Some` of the converted value; for map keys, `Some` of the converted key |
| Unit and unit structs | Empty text |
| Newtype structs | The inner type's rule |
| Bytes, as requested by `serde_bytes::ByteBuf` | The UTF-8 bytes |

An empty value is still present: `PORT=` into `Option<u16>` is an error, not
`None`. Sequences, nested maps and structs, tuples and enums are rejected,
including enums with only unit variants and `Vec<u8>`, which Serde reads as a
sequence. Untagged enums are the exception: Serde buffers the string and can
choose a string-compatible variant, so `value=hello` may deserialize as
`Text("hello")`. Deserialize a `String` and convert it yourself when a field
needs another shape.

Map keys use the same scalar conversion rules as values, so maps with
non-string scalar keys parse directly:

```rust
use std::collections::BTreeMap;

let doc = adotenv::parse_strict("8080=http\n443=https\n").unwrap();
let ports: BTreeMap<u16, String> = doc.deserialize().unwrap();
assert_eq!(ports[&8080], "http");
assert_eq!(ports[&443], "https");
```

Fields inside `#[serde(flatten)]` are buffered by Serde through
`deserialize_any` before the flattened target sees them. Because `.env` files
do not distinguish strings from numbers syntactically, buffered keys and values
stay strings so flattened `String` fields and `BTreeMap<String, String>` maps
keep values like `8080` or `true` intact. Types that Serde can read from a
buffered string (`String`, `char`, `serde_bytes::ByteBuf`) work directly inside
`#[serde(flatten)]`. For numeric or boolean fields inside `#[serde(flatten)]`,
parse from a string with `#[serde(deserialize_with = ...)]` (or a
`#[serde(try_from = "String")]` wrapper):

```rust
use std::str::FromStr;
use serde::Deserialize;

fn from_str<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: FromStr,
    T::Err: std::fmt::Display,
{
    String::deserialize(deserializer)?
        .parse()
        .map_err(serde::de::Error::custom)
}

#[derive(Deserialize)]
struct Server {
    #[serde(deserialize_with = "from_str")]
    port: u16,
}

#[derive(Deserialize)]
struct Config {
    host: String,
    #[serde(flatten)]
    server: Server,
}

let doc = adotenv::parse_strict("host=localhost\nport=8080\n").unwrap();
let config: Config = doc.deserialize().unwrap();
assert_eq!(config.host, "localhost");
assert_eq!(config.server.port, 8080);
```

## Errors

`DeserializeError` has `line()`, `key()` and `message()`:

- A value that does not convert reports the key and the one-based line of its
  declaration.
- In a loaded environment, the line comes from the retained origin, and the
  file is not reported; pass the key to `LoadedEnvironment::origin` to find
  it. With `LoadOptions::with_origins(false)` the line is `0`, but a key or
  value error still names the key.
- A key that does not convert, such as an unknown field under
  `#[serde(deny_unknown_fields)]` or a map key that fails to parse into its
  target scalar type, reports its line and key.
- An error about the whole map, such as a missing field, has line `0` and no
  key.
- A value consumed through `#[serde(flatten)]` is decoded after map access;
  serde reports line `0` and no key for that value. The message still describes
  the conversion failure.

The `Display` form is `line {line}: {message} (key "{key}")`, without the
`line` part when the line is `0` and without the `key` part when there is no
key.

Error messages include the value that failed to convert, such as
`expected a u16, found "abc"`. Treat them as secret when values are secret.
Use `DeserializeError::masked()` for a `Debug` and `Display` form that keeps
the line and key but replaces the message with `<redacted>`.
