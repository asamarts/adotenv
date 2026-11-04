# Getting started

adotenv reads `.env` files as source code: it keeps every byte, reports where
each token is and changes text only through verified edits. This page installs
the crates and walks through the main tasks.

## Install the library

```toml
[dependencies]
adotenv = "1"
```

The library needs Rust 1.81 or later and has no required dependencies.
Optional features add the parts that touch the outside world:

| Feature | Adds | Needs |
|---|---|---|
| `std` (default) | Standard library support. Error types implement `std::error::Error` with or without it | |
| `load` | Reading `.env` cascades and file lists from disk | `std` |
| `serde` | Deserializing documents, and with `load` loaded environments, into typed structs | |
| `exec` | `exec::Shell`, a command runner for command substitution | `std` |

```toml
adotenv = { version = "1", features = ["load", "serde"] }
```

Disable default features for a `no_std` build that uses `alloc`. Parsing,
lookup, edits, formatting, sorting, comparison and resolution stay available.

```toml
adotenv = { version = "1", default-features = false }
```

## Install the command-line tool

```console
cargo install adotenv-cli --locked
```

This installs the `adotenv-cli` binary:

```console
adotenv-cli check .env
adotenv-cli fmt --check .env
adotenv-cli diff .env.example .env
```

See [Command-line tool](../design/cli.md) for every command, the JSON output
and the exit codes.

## Companion crates

| Crate | Version | Purpose |
|---|---|---|
| `adotenv-process` | `1` | Apply a loaded environment to the current process. See [Process environment](process.md). |
| `adotenv-config` | `0.12` | A `config` 0.15 source. Needs Rust 1.88. See [config and Figment](../design/integrations.md). |
| `adotenv-figment` | `0.12` | A Figment 0.10 provider. See [config and Figment](../design/integrations.md). |

The adapters keep their own 0.x versions, so a new major version of `config`
or Figment is a minor release of the adapter.

## Parse and inspect

`parse_strict` returns a `Document` or the first error. Values are literal:
nothing is expanded.

```rust
let source = "# app\nexport PORT=8080\nURL=\"${HOST}/api\" # base\n";
let doc = adotenv::parse_strict(source).unwrap();

assert_eq!(doc.get("PORT"), Some("8080"));
assert_eq!(doc.get("URL"), Some("${HOST}/api"));

let url = doc.get_entry("URL").unwrap();
assert_eq!(url.line(), 3);
assert_eq!(url.raw_value(), Some("\"${HOST}/api\""));
assert_eq!(url.inline_comment(), Some("# base"));
```

`parse` never fails. It returns the document and at most 10,000 ordinary
diagnostics, plus the first error after that budget, so a tool can report
problems without losing the rest of the file. If more problems exist, the last
diagnostic is `dotenv.too-many-diagnostics`:

```rust
let (doc, diagnostics) = adotenv::parse("BAD KEY=value\nGOOD=kept\n");
assert_eq!(doc.get("GOOD"), Some("kept"));
assert_eq!(diagnostics[0].code().as_str(), "dotenv.invalid-key");
assert_eq!(diagnostics[0].line(), 1);
```

[Parsing and lookup](parsing.md) covers duplicates, raw tokens, spans and
positions. [Dialects](../design/dialects.md) covers files written for Node,
Python, Docker Compose or Docker.

## Edit

Edits never change the document. They return `VerifiedEdits`: the new text
plus the text edits that produce it. The output is parsed again and checked
before it is returned.

```rust
let doc = adotenv::parse_strict("A=1\nB=2\n").unwrap();

let edited = doc.set("B", "two words").unwrap();
assert_eq!(edited.as_str(), "A=1\nB=two words\n");

// The value is quoted only when it needs to be.
let quoted = doc.set("B", "a # b").unwrap();
assert_eq!(quoted.as_str(), "A=1\nB=\"a # b\"\n");

let renamed = doc.rename("A", "ALPHA").unwrap();
assert_eq!(renamed.as_str(), "ALPHA=1\nB=2\n");
```

[Editing](editing.md) covers batches, insert positions and how to apply the
text edits yourself. [Formatting and sorting](../design/formatting.md) covers
whole-file changes.

## Load a cascade

With the `load` feature, `load_cascade` reads `.env`, `.env.local`,
`.env.{mode}` and `.env.{mode}.local` from one directory. Later files win.

```rust,no_run
use adotenv::load::load_cascade;

let loaded = load_cascade("/srv/my-app", "production")?;
if let Some(port) = loaded.get("PORT") {
    let origin = loaded.origin("PORT").unwrap();
    println!("PORT={port} from {}:{}", origin.path().display(), origin.line());
}
# Ok::<(), adotenv::load::LoadError>(())
```

Loading never changes the process environment. Pass values to a child process
yourself, use `adotenv-cli run`, or use [`adotenv-process`](process.md).
[Loading files](../design/loading.md) covers origins, reports and values-only
loading.

## Next steps

- [Security and limits](../reference/security.md) before handling untrusted
  files or secret values.
- [Diagnostic codes](../reference/diagnostics.md) for the stable codes that
  tools match on.
- The [API reference](https://docs.rs/adotenv) on docs.rs.
