# adotenv

adotenv is a literal, lossless `.env` parser and editing library for Rust.

It is built for linters, formatters, editors, scanners and codebase maintenance
tools that need to inspect source exactly as written.

## What it provides

- Literal values with no implicit expansion or command execution.
- Byte-for-byte source retention, including comments, invalid statements, BOM
  and mixed line endings.
- Precise source spans and checked UTF-8, scalar and UTF-16 positions.
- Verified edits, formatting, sorting, comparison and example generation.
- Five syntax presets and explicit reference resolution.
- Optional file loading with values, origins and reports.
- No required runtime dependencies and a `no_std` build with `alloc`.

## Install

```toml
[dependencies]
adotenv = "0.10.2"
```

Enable optional features as needed:

```toml
adotenv = { version = "0.10.2", features = ["load", "serde"] }
```

## Quick start

```rust
let text = "# app\nexport PORT=8080\nURL=\"${HOST}/api\"\n";
let doc = adotenv::parse_strict(text).unwrap();

assert_eq!(doc.get("PORT"), Some("8080"));
assert_eq!(doc.get("URL"), Some("${HOST}/api"));
assert_eq!(doc.to_string(), text);

let edited = doc.set("PORT", "9090").unwrap();
assert_eq!(
    adotenv::parse_strict(edited.as_str()).unwrap().get("PORT"),
    Some("9090")
);
```

When only final values are needed, use the values-only parser:

```rust
let values = adotenv::parse_values("A=1\nA=2\nB=3\n").unwrap();
assert_eq!(values.get("A").map(String::as_str), Some("2"));
```

## Documentation

- [Architecture](design/architecture.md)
- [Dialects](design/dialects.md)
- [References and resolution](design/expansion.md)
- [Formatting and sorting](design/formatting.md)
- [Comparison and examples](design/comparison.md)
- [Loading](design/loading.md)
- [Serde](design/serde.md)
- [CLI](design/cli.md)
- [Integrations](design/integrations.md)
- [Quality](design/quality.md)
- [Performance](design/performance.md)
- [Competitive analysis](design/competitive-analysis.md)
- [1.0 readiness](design/1.0-readiness.md)

Source code and issues:
[github.com/asamarts/adotenv](https://github.com/asamarts/adotenv).
