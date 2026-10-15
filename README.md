# adotenv

A tooling-grade `.env` parser for Rust: literal values, exact byte spans, and
edits that leave the rest of the file untouched.

[![crates.io](https://img.shields.io/crates/v/adotenv.svg)](https://crates.io/crates/adotenv)
[![docs.rs](https://img.shields.io/docsrs/adotenv)](https://docs.rs/adotenv)
[![CI](https://github.com/asamarts/adotenv/actions/workflows/ci.yml/badge.svg)](https://github.com/asamarts/adotenv/actions/workflows/ci.yml)

Most `.env` crates exist to *load* a `.env` file into a process. `adotenv`
is for programs that *read or rewrite* one: linters, formatters, editors and
language servers, secret scanners, migration tools. That needs different
guarantees:

- **Literal values.** `${HOME}` and `$USER` are returned exactly as written,
  never expanded, so results don't depend on the machine running the tool.
- **Exact spans.** Every entry carries byte ranges for its line, key and value,
  ready for diagnostics or editor highlights.
- **Lossless edits.** `set` and `remove` change one value or one line and keep
  every other byte: comments, blank lines, quoting style, CRLF, a BOM. `set`
  verifies value readback; `remove` verifies the remaining assignments keep
  their meaning, refusing a deletion that would reinterpret them.
- **No side effects.** No file I/O, no process-environment changes, no
  dependencies, no `unsafe`.

```rust
let text = "# app\nexport PORT=8080\nURL=\"${HOST}/api\" # base\n";
let doc = adotenv::parse(text).unwrap();

assert_eq!(doc.get("PORT"), Some("8080"));
assert_eq!(doc.get("URL"), Some("${HOST}/api")); // literal

let url = &doc.entries()[1];
assert_eq!(url.line(), 3);
assert_eq!(&text[url.value_span()], "\"${HOST}/api\"");

let edited = adotenv::set(text, "URL", "https://example.com").unwrap();
assert_eq!(edited, "# app\nexport PORT=8080\nURL=https://example.com # base\n");
```

## Syntax

- `KEY=VALUE`, one per line; lines end at LF or CRLF.
- Blank lines and `#` comment lines are ignored; an optional `export ` prefix
  is accepted.
- Unquoted values are trimmed; an inline comment starts at ` #`, while a `#`
  with no space before it is part of the value.
- Single-quoted values are fully literal. Double-quoted values honor `\n`,
  `\r`, `\t`, `\\` and `\"`, and keep any other backslash pair as written.
- In 0.1, text after a closing quote is ignored, even when it is not a comment;
  parsing a quoted value is not strict validation of its trailing text.
- Duplicate keys are all kept as entries; lookups use the last one, and edits
  refuse an ambiguous key instead of guessing.
- A leading UTF-8 byte-order mark is skipped.

A line with no `=`, an empty key, a key containing whitespace, or an
unterminated quote is a parse error with a line number. Multi-line values are
not supported yet.

## Roadmap

0.1 is the first step. The plan to 1.0 (a lossless document model with error
recovery, dialects, opt-in expansion, formatting, loading and integrations) is
in [ROADMAP.md](https://github.com/asamarts/adotenv/blob/main/ROADMAP.md), with
design notes in [docs/design/](https://github.com/asamarts/adotenv/tree/main/docs/design).

## Minimum supported Rust version

1.74. Raising it is a minor-version change.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
