# Serde deserialization

Status: opt-in `serde` deserialization is implemented in 0.6. The unreleased
post-0.7.1 slice adds `LoadedEnvironment::deserialize` when `load` and `serde`
are both enabled. It is a literal mapping into Serde; it never expands
references, reads files or inspects the process environment by itself.

## Feature and API

Enable the `serde` feature:

```toml
adotenv = { version = "0.7", features = ["serde"] }
```

```rust
use adotenv::parse_strict;

#[derive(serde::Deserialize)]
struct Config {
    host: String,
    port: u16,
}

let doc = parse_strict("host=localhost\nport=8080\n").unwrap();
let config: Config = doc.deserialize().unwrap();
```

`Document::deserialize<T: DeserializeOwned>` deserializes the document as a
map. It uses the document’s effective declarations: duplicates follow the
selected dialect’s policy, nonparticipating declarations and empty keys are
ignored, and bare declarations are exposed only when `BareKeys::Empty` makes
them effective empty strings.

## Loaded environments

With `load` and `serde` enabled, the same literal mapping is available from the
final merged cascade:

```rust,ignore
use adotenv::load::load_cascade;

let loaded = load_cascade("/srv/my-app", "production")?;
let config: Config = loaded.deserialize()?;
```

`LoadedEnvironment::deserialize` uses the final effective values after cascade
precedence and dialect lookup policy. Value-conversion errors report the
one-based line of the winning declaration when an origin is available. Loading
itself remains explicit and all-or-nothing.

## Value mapping

The map values are the decoded literal strings. The value deserializer parses
them on demand:

- strings and string-like visitors receive the literal text;
- booleans accept `true` and `false`;
- integers and floats use `str::parse`;
- characters require exactly one character;
- bytes receive the UTF-8 bytes of the literal;
- `Option<T>` treats every present declaration as `Some`;
- unit values require an empty literal.

Sequences, maps, tuples, enums and nested structs are not synthesized from a
single scalar string. A type requiring one receives an `expected ...` value
error instead.

Unknown map keys are left to Serde’s normal behavior (struct derives ignore
them by default). Missing struct fields follow Serde’s normal missing-field
behavior.

## Errors

`DeserializeError` implements `serde::de::Error` and reports:

- `line()`: one-based source line for value-conversion errors, or zero for
  structural Serde errors without a value location;
- `key()`: the declaration key when available;
- `message()`: the conversion message.

No process-global mutation is performed. File loading and process application
remain separate opt-in surfaces.
