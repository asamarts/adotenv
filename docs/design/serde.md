# Serde

The `serde` feature adds typed deserialization for documents and loaded
environments.

```toml
adotenv = { version = "0.10.3", features = ["serde"] }
```

## Documents

```rust,ignore
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    host: String,
    port: u16,
}

let doc = adotenv::parse_strict("host=localhost\nport=8080\n").unwrap();
let config: Config = doc.deserialize()?;
```

Keys follow the document's duplicate policy and lookup participation. Bare
declarations contribute an empty string only under `BareKeys::Empty`.

## Loaded environments

With `load` and `serde` enabled, `LoadedEnvironment::deserialize` uses the final
merged values. Origin lines remain available for conversion errors.

```rust,ignore
let loaded = adotenv::load::load_cascade("/srv/my-app", "production")?;
let config: Config = loaded.deserialize()?;
```

## Conversion rules

Values are decoded literals before Serde sees them. The adapter converts scalars:

- strings;
- booleans;
- integers and floats;
- characters.

It does not invent nested maps, sequences or enum layouts. Deserialize a string
and convert explicitly when a field needs another shape.

`DeserializeError` reports the key and source line when an origin is available.
