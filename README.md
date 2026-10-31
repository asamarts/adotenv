# adotenv

A literal, lossless `.env` parser and editing library for Rust.

[![crates.io](https://img.shields.io/crates/v/adotenv.svg)](https://crates.io/crates/adotenv)
[![docs.rs](https://img.shields.io/docsrs/adotenv)](https://docs.rs/adotenv)
[![CI](https://github.com/asamarts/adotenv/actions/workflows/ci.yml/badge.svg)](https://github.com/asamarts/adotenv/actions/workflows/ci.yml)

Documentation: <https://asamarts.github.io/adotenv/>

adotenv is built for linters, formatters, editors, scanners and codebase maintenance
tools. It keeps source text exactly as written, reports precise locations and
verifies every edit before returning it.

## What it does

- Values stay literal. `${HOME}`, `$USER` and command-like text are never
  expanded or executed by the parser.
- Any UTF-8 source round-trips byte for byte, including invalid statements,
  comments, spacing, a BOM and mixed line endings.
- Every token has a source span. Decoded values map back to the original bytes.
- Edits return source-relative text edits and a re-parsed, verified result.
- The core has no runtime dependencies, no file I/O, no environment access and
  no process spawning. `#![forbid(unsafe_code)]` is unconditional.
- Filesystem loading, process mutation and command execution are separate
  opt-in features or crates.

## Install

```toml
[dependencies]
adotenv = "0.10.4"
```

| Feature | Purpose |
|---|---|
| default (`std`) | Error traits and standard desktop use |
| `load` | Read `.env` cascades from files |
| `serde` | Deserialize documents and loaded environments |
| `exec` | Run command substitutions with an explicit directory |

Disable default features for a `no_std` core that uses `alloc`:

```toml
adotenv = { version = "0.10.4", default-features = false }
```

## Parse

```rust
let text = "# app\nexport PORT=8080\nURL=\"${HOST}/api\" # base\n";
let doc = adotenv::parse_strict(text).unwrap();

assert_eq!(doc.get("PORT"), Some("8080"));
assert_eq!(doc.get("URL"), Some("${HOST}/api"));
assert_eq!(doc.to_string(), text);

let url = doc.get_entry("URL").unwrap();
assert_eq!(url.line(), 3);
assert_eq!(&text[url.value_span().unwrap()], "\"${HOST}/api\"");

let edited = doc.set("URL", "https://example.invalid").unwrap();
assert_eq!(
    adotenv::parse_strict(edited.as_str()).unwrap().get("URL"),
    Some("https://example.invalid")
);
```

`parse_strict` returns the first error. `parse` always returns a document and a
list of diagnostics, so callers can inspect broken files without losing source:

```rust
let (doc, diagnostics) = adotenv::parse("BAD KEY=value\nGOOD=retained\n");
assert_eq!(doc.get("GOOD"), Some("retained"));
assert_eq!(doc.items().next().unwrap().kind(), adotenv::ItemKind::Invalid);
assert_eq!(diagnostics[0].code(), adotenv::DiagnosticCode::InvalidKey);
assert_eq!(doc.source(), "BAD KEY=value\nGOOD=retained\n");
```

Diagnostics have stable codes, severities, one-based lines and primary or
secondary spans. Match codes and spans, not message text.

`parse_owned(String)` moves the source into the document. `Document::into_owned`
copies only when the document still borrows its input.

When only final values are needed, `parse_values` uses the projection parser and
avoids retaining items and spans. It returns the first error and discards
warnings.

```rust
let values = adotenv::parse_values("A=1\nA=2\nBARE\nC=3\n").unwrap();
assert_eq!(values.get("A").map(String::as_str), Some("2"));
assert_eq!(values.get("C").map(String::as_str), Some("3"));
assert_eq!(values.len(), 2);
```

## Dialects

The default is `Dialect::PORTABLE`. Other presets are `NODE_DOTENV`,
`PYTHON_DOTENV`, `COMPOSE` and `DOCKER_RAW`. Presets choose parsing rules only.
They do not enable interpolation, inheritance or loading.

```rust
use adotenv::{parse_with, Dialect, InlineComments};

let source = "A=a#b\nB=`two words`\n";
let (node, _) = parse_with(source, Dialect::NODE_DOTENV);
assert_eq!(node.get("A"), Some("a"));
assert_eq!(node.get("B"), Some("two words"));

let (raw, _) = node.reparse(Dialect::DOCKER_RAW);
assert_eq!(raw.get("A"), Some("a#b"));
assert_eq!(raw.get("B"), Some("`two words`"));

let custom = Dialect::NODE_DOTENV.with_inline_comments(InlineComments::None);
assert_eq!(parse_with(source, custom).0.get("A"), Some("a#b"));
```

Each document retains its exact configuration. `reparse` borrows the same source
and rebuilds items, entries and source maps for another dialect. Snapshot IDs do
not carry across interpretations.

## Lookups and raw tokens

Duplicate declarations are retained. `get_entry` selects the effective
declaration, `get_all` returns every occurrence and `unique` rejects ambiguity.

```rust
let doc = adotenv::parse_strict("A=1\nA=2\nB=3\n").unwrap();
assert_eq!(doc.get("A"), Some("2"));
assert_eq!(doc.get_all("A").count(), 2);
assert!(doc.unique("A").is_err());

let index = doc.lookup_index();
assert_eq!(index.get("A"), Some("2"));
assert_eq!(index.len(), 2);

let winners: Vec<_> = doc
    .effective_entries()
    .map(|entry| (entry.key(), entry.value()))
    .collect();
assert_eq!(winners, [("A", "2"), ("B", "3")]);
```

Raw tokens remain available:

```rust
let source = "export A=\"two words\" # note\n";
let doc = adotenv::parse_strict(source).unwrap();
let entry = doc.unique("A").unwrap();

assert!(entry.exported());
assert_eq!(&source[entry.key_span()], "A");
assert_eq!(entry.raw_value(), Some("\"two words\""));
assert_eq!(entry.value(), Some("two words"));
assert_eq!(entry.inline_comment(), Some("# note"));
assert_eq!(entry.decoded_span_to_raw(0..3).unwrap(), 10..13);
```

## References and resolution

`Entry::references` reports `$VAR`, `${VAR:-default}` and `$(command)` candidates
without expanding them. Each reference records spans, quote and escape context,
operators and nested forms.

```rust
use adotenv::{parse_strict, ReferenceOperator, ReferenceRules};

let source = "A=${HOST:-localhost}\n";
let doc = parse_strict(source).unwrap();
let reference = &doc.unique("A").unwrap().references()[0];

assert_eq!(&source[reference.span.clone()], "${HOST:-localhost}");
assert_eq!(
    reference.operator,
    Some(ReferenceOperator::DefaultIfUnsetOrEmpty)
);
assert!(reference.is_eligible(ReferenceRules::COMPOSE));
assert!(!reference.is_eligible(ReferenceRules::PORTABLE));
```

Resolution uses document bindings plus a caller lookup closure. It never reads
the environment, files or commands on its own.

```rust
use adotenv::{parse_strict, ExpandOptions, LookupOrder};

let doc = parse_strict("BASE=base\nURL=${BASE}/api\n").unwrap();
let resolved = doc.resolve(|_| None, &ExpandOptions::portable()).unwrap();
assert_eq!(resolved.entries()[1].value.as_deref(), Some("base/api"));

let caller = doc
    .resolve(
        |key| (key == "BASE").then(|| "external".to_string()),
        &ExpandOptions::portable().with_lookup_order(LookupOrder::CallerFirst),
    )
    .unwrap();
assert_eq!(caller.entries()[1].value.as_deref(), Some("external/api"));
```

Commands run only through an explicit callback or the opt-in `exec` feature:

```rust
use adotenv::{parse_strict, ExpandOptions};

let doc = parse_strict("A=$(echo hi)\n").unwrap();
let resolved = doc
    .resolve_with_commands(
        |_| None,
        |context| Ok(format!("ran:{}", context.command)),
        Some("synthetic.env"),
        &ExpandOptions::dotenv_expand(),
    )
    .unwrap();
assert_eq!(resolved.entries()[0].value.as_deref(), Some("ran:echo hi"));
assert_eq!(resolved.commands().len(), 1);
```

## Format and sort

`Document::format` changes only requested trivia and verifies the complete result
with the document dialect.

```rust
use adotenv::{parse_strict, FormatOptions};

let source = "# app   \n\nA=1   \nB =  2 # note   \n";
let doc = parse_strict(source).unwrap();
let formatted = doc.format(&FormatOptions::normalize()).unwrap();
assert_eq!(formatted.as_str(), "# app\n\nA = 1\nB = 2 # note\n");
assert!(parse_strict(formatted.as_str()).is_ok());
```

`Document::sort` orders declarations inside blank-line-separated groups and
keeps comments and multiline entries intact. When one key references another in
the same group, the two keep their current relative order, so resolved values
do not change. Groups with duplicates, empty keys, malformed items or reference
cycles are left unchanged.

```rust
let doc = adotenv::parse_strict("# header\nC=3\nB=${A}\n# A note\nA=1\n# bottom\n").unwrap();
assert_eq!(
    doc.sort().unwrap().as_str(),
    "# header\nB=${A}\n# A note\nA=1\nC=3\n# bottom\n"
);
```

## Compare and generate examples

```rust
use adotenv::{parse_strict, DifferenceKind};

let left = parse_strict("A=1\nB=\nC=3\n").unwrap();
let right = parse_strict("A=2\nB\nD=4\n").unwrap();
let comparison = left.compare(&right);
let changes: Vec<_> = comparison
    .differences()
    .iter()
    .map(|entry| (entry.key.as_str(), entry.kind))
    .collect();

assert_eq!(
    changes,
    [
        ("A", DifferenceKind::Value),
        ("B", DifferenceKind::State),
        ("C", DifferenceKind::OnlyLeft),
        ("D", DifferenceKind::OnlyRight),
    ]
);
```

`Document::example` removes assigned values while preserving layout. Malformed
statements and empty-key entries become comments so they cannot leak values.

```rust
let source = "# app\nA=1\nB=\"two\" # note\nBARE\nBAD KEY=secret\n";
assert_eq!(
    adotenv::parse(source).0.example(),
    "# app\nA=\nB= # note\nBARE\n#\n"
);
```

## Edit a snapshot

Every edit method returns `VerifiedEdits`. Ranges refer to the original source.
The edits are sorted by start offset and do not overlap. Apply them in reverse
order of the returned slice, from the last edit to the first. Two edits can
start at the same offset, so do not re-sort them by offset.

```rust
use adotenv::{parse_strict, Edit, InsertPosition};

let doc = parse_strict("A=one\nA=two\n").unwrap();
let second = doc.get_all("A").nth(1).unwrap();
let verified = doc
    .edit(&[
        Edit::Set { entry: second.id(), value: "changed" },
        Edit::Insert { key: "B", value: None, position: InsertPosition::End },
    ])
    .unwrap();

let (after, _) = adotenv::parse(verified.as_str());
assert_eq!(after.get("A"), Some("changed"));
assert_eq!(after.get_entry("B").unwrap().value(), None);

let mut text = doc.source().to_string();
for edit in verified.edits().iter().rev() {
    text.replace_range(edit.range.clone(), &edit.replacement);
}
assert_eq!(text, verified.as_str());
```

`set`, `remove`, `rename`, `insert`, `comment`, `uncomment`, `set_export` and
`edit` verify the whole batch and reject changes that alter unrelated syntax.

Use `encode_value_with(value, dialect)` when you need a standalone literal token:

```rust
use adotenv::{encode_value_with, parse_strict_with, Dialect};

let value = "literal #\0\n";
let token = encode_value_with(value, Dialect::PYTHON_DOTENV).unwrap();
let source = format!("A={token}\n");
assert_eq!(
    parse_strict_with(&source, Dialect::PYTHON_DOTENV).unwrap().get("A"),
    Some(value)
);
assert!(encode_value_with("two\nlines", Dialect::DOCKER_RAW).is_err());
```

## Positions

Line and column conversions are explicit and checked.

```rust
use adotenv::{parse_strict, ColumnEncoding, Position};

let doc = parse_strict("A=é\n").unwrap();
let index = doc.line_index();
let position = index.to_position(2, ColumnEncoding::Scalar).unwrap();

assert_eq!(position, Position { line: 0, column: 2 });
assert_eq!(
    index.to_offset(position, ColumnEncoding::Scalar).unwrap(),
    2
);
```

`ColumnEncoding` supports UTF-8 bytes, Unicode scalars and UTF-16 code units.
Out-of-range positions and split surrogates return an error instead of being
clamped. Entry and diagnostic lines are one-based; `Position` is zero-based.

## Load files

Enable the `load` feature to read a cascade from an explicit directory. The order
is `.env`, `.env.local`, `.env.{mode}` and `.env.{mode}.local`. Later files win
by effective key. Missing cascade files are skipped. Read, UTF-8 and syntax
errors invalidate the whole load.

```rust,ignore
use adotenv::load::load_cascade;

let loaded = load_cascade("/srv/my-app", "production")?;
assert_eq!(loaded.get("PORT"), Some("8080"));

let origin = loaded.origin("PORT").unwrap();
println!("{}:{}", origin.path().display(), origin.line());
```

The default loader keeps values, shared-file origins and raw winning tokens.
Reports are opt-in:

```rust,ignore
use adotenv::load::{load_cascade_with_options, LoadOptions};

let loaded = load_cascade_with_options(
    "/srv/my-app",
    "production",
    LoadOptions::full(adotenv::Dialect::PORTABLE),
)?;

for entry in loaded.entries() {
    println!(
        "{}={} at {}:{}",
        entry.key(),
        entry.value(),
        entry.origin().path().display(),
        entry.origin().line()
    );
}
```

`LoadOptions::new(dialect)` is the default profile. Builders can disable raw
tokens, origins or reports. `load_values` and `load_cascade_values` retain only
final strings.

`LoadedEnvironment::resolve_contextual` resolves merged values with their original
quote and escape context. `resolve_decoded` treats final values as unquoted text.

## Process environment

`adotenv-process` applies a loaded environment to the current process. The API
is unsafe because the process environment is global and not thread-safe on all
platforms. Validation happens before the first mutation.

```rust,ignore
use adotenv::load::load_cascade;
use adotenv_process::apply;

let loaded = load_cascade("/srv/my-app", "production")?;
// The caller must guarantee no concurrent environment access.
unsafe { apply(&loaded)? };
```

Prefer iterating `LoadedEnvironment::values()` when mutation is not required.

## Serde

With the `serde` feature, documents and loaded environments deserialize into
typed values:

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

Only strings, booleans, numbers and characters are converted from literal
values. Nested structures and enum layouts are not invented. Errors carry the
source line and key.

## CLI

The workspace includes `adotenv-cli`:

```text
get <file> <key>
check [--format human|json|github] [--source] [--recursive] <path>...
fmt [--write|--check] [--sort] [--recursive] <path>...
diff [--format human|matrix|json] <baseline> <file>...
example <file>
load <directory> [mode]
set <file> <key> <value>
run <directory> [mode] -- <program> [args...]
```

`check --format json` and `diff --format json` use versioned machine schemas.
`check --format github` emits workflow annotations. Human output omits source
lines unless `--source` is passed.

`run` starts a child with the parent environment plus the loaded values, which
replace parent variables of the same name. Loaded files are trusted like code: a
loaded `PATH` changes which program runs. The parent process is not changed.

`get`, `load` and `diff` print values, and `example` keeps comments verbatim.
`load` prints Portable lines that parse back to the same values.

See [CLI details](docs/design/cli.md).

## Adapters

`adotenv-config` exposes literal values as a `config` source. `adotenv-figment`
exposes a Figment dictionary. Both support loaded environments through their
`load` feature.

```rust,ignore
use adotenv::parse_strict;
use adotenv_config::AdotenvSource;

let doc = parse_strict("host=localhost\nport=8080\n").unwrap();
let config = config::Config::builder()
    .add_source(AdotenvSource::from_document(&doc))
    .build()?;
```

## Design limits

- Input is UTF-8. Decode bytes before parsing; invalid UTF-8 is rejected rather
  than replaced.
- Parsing does not read files, environment variables or commands.
- Retained documents keep token and item metadata. Apply byte and line budgets
  for untrusted input.
- Windows writes use a direct replace when possible and a backup swap otherwise.
  The fallback is not crash-atomic.
- The `config` integration uses `config` 0.15 and requires Rust 1.88.

## Rust version

Rust 1.81, edition 2021. `adotenv-config` requires Rust 1.88 because `config`
0.15 does. Development and fuzz dependencies may require a newer toolchain.

## License

Licensed under either [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.
