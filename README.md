# adotenv

A tooling-grade `.env` parser for Rust: literal values, exact byte spans, and
verified edits that leave unrelated source untouched.

[![crates.io](https://img.shields.io/crates/v/adotenv.svg)](https://crates.io/crates/adotenv)
[![docs.rs](https://img.shields.io/docsrs/adotenv)](https://docs.rs/adotenv)
[![CI](https://github.com/asamarts/adotenv/actions/workflows/ci.yml/badge.svg)](https://github.com/asamarts/adotenv/actions/workflows/ci.yml)

Most `.env` crates load a file into a process. `adotenv` is for linters,
formatters, editors, language servers, scanners and migration tools:

- **Literal values.** `${HOME}`, `$USER` and command-looking text are never
  expanded or executed. Results do not depend on the host environment.
- **Lossless recovery.** Any UTF-8 source is retained byte for byte, including
  invalid statements, comments, spacing, a BOM and mixed line endings.
- **Precise source locations.** Raw token spans, decoded-to-raw value mapping,
  and checked UTF-8, Unicode-scalar and UTF-16 positions address the original
  source, not a normalized copy.
- **Verified native edits.** Set, remove, rename, insert, comment, uncomment
  and export changes return source-relative text edits and an already-verified
  output. Unrelated entries and invalid syntax must keep their meaning.
- **Small, side-effect-free core.** Zero runtime dependencies, no file I/O,
  environment access or process spawning, and unconditional `forbid(unsafe_code)`.
  Disable the default `std` feature for `no_std` with `alloc`.

## Read a document

```rust
let text = "# app\nexport PORT=8080\nURL=\"${HOST}/api\" # base\n";
let doc = adotenv::parse_strict(text).unwrap();

assert_eq!(doc.get("PORT"), Some("8080"));
assert_eq!(doc.get("URL"), Some("${HOST}/api")); // literal, never expanded
let url = doc.get_entry("URL").unwrap();
assert_eq!(url.line(), 3); // explicitly one-based
assert_eq!(&text[url.value_span().unwrap()], "\"${HOST}/api\"");
assert_eq!(doc.to_string(), text);

let edited = doc.set("URL", "https://example.invalid").unwrap();
assert_eq!(adotenv::parse_strict(edited.as_str()).unwrap().get("URL"),
           Some("https://example.invalid"));
```

`parse` recovers instead of failing. Diagnostics have stable `DiagnosticCode`
values and `code().as_str()` identifiers, severity, primary and optional
secondary spans, and one-based line numbers. Match codes, not message wording.
`parse_strict` rejects the first **error**, not warnings or informational notes.

```rust
let (doc, diagnostics) = adotenv::parse("BAD KEY=value\nGOOD=retained\n");
assert_eq!(doc.get("GOOD"), Some("retained"));
assert_eq!(doc.items().next().unwrap().kind(), adotenv::ItemKind::Invalid);
assert_eq!(diagnostics[0].code(), adotenv::DiagnosticCode::InvalidKey);
assert_eq!(doc.source(), "BAD KEY=value\nGOOD=retained\n");

let (owned, _) = adotenv::parse_owned(String::from("A=value\n"));
assert_eq!(owned.get("A"), Some("value")); // owns its source, no self-references
```

`entries()` and `items()` are exact-size, double-ended **view iterators**.
Use `entry(id)` and `item(id)` for indexed access; IDs are zero-based and local
only to that source snapshot. Borrowed parsing does not copy the source;
`parse_owned(String)` moves it, and `Document::into_owned()` copies only when
necessary. Decoded values allocate only when actual unescaping is required.

## Portable syntax in 0.2

- Keys are nonempty ASCII letters, digits, `_`, `.` and `-`. Non-POSIX names
  such as `app.key` or `1KEY` are accepted with a portability warning.
  Other characters, including non-ASCII letters, are invalid keys.
- An optional `export` prefix and spaces/tabs around `=` are accepted.
- Outside quotes, surrounding spaces/tabs are trivia. `A=a # comment` has
  value `a`; `A=a#b` has literal value `a#b` and a portability warning.
  `A= # comment` is an assigned empty value.
- Single quotes are fully literal. Double quotes decode `\n`, `\r`, `\t`,
  `\\`, `\"` and `\$`; unknown backslash pairs remain literal with a warning.
  Both quote styles can span physical lines, retaining original line endings.
- Non-comment text after a closing quote is an error, not silently discarded.
  An unmatched quote makes its opening physical line invalid; recovery resumes
  at the next line. A matching later quote follows the grammar, even across
  assignment-looking lines.
- `KEY` is a **bare declaration**: `value()` is `None`, unlike `KEY=` and
  `KEY=""`, whose values are `Some("")`. `AssignmentState` exposes the state.
  Bare declarations never consult the process environment.
- Duplicates remain ordered and generate warnings. `get_entry()` selects the
  last declaration; `get()` returns its value, including `None` if it is bare.
  Use `get_all()` to inspect every occurrence and `unique()` to reject ambiguity.
- LF, CRLF and lone CR line endings, a leading UTF-8 BOM and final-newline
  state are retained. BOM presence is informational; lone CR generates a warning.
- Backticks are literal, not quotes; unquoted backticks generate warnings.
  No form of expansion or command execution is part of parsing.

The input domain is UTF-8 (`&str` or `String`). Decoding bytes is the caller's
responsibility; silently replacing invalid UTF-8 would invalidate byte spans.

## Edit a snapshot

Key-only convenience methods require a unique declaration. Explicit native
selectors can target individual duplicate occurrences and remove comments or
invalid items without guessing their ownership:

```rust
use adotenv::{Edit, InsertPosition};
let doc = adotenv::parse_strict("A=one\nA=two\n").unwrap();
let second = doc.get_all("A").nth(1).unwrap();
let verified = doc.edit(&[
    Edit::Set { entry: second.id(), value: "changed" },
    Edit::Insert { key: "B", value: None, position: InsertPosition::End },
]).unwrap();
let (after, _) = adotenv::parse(verified.as_str());
assert_eq!(after.get("A"), Some("changed"));
assert_eq!(after.get_entry("B").unwrap().value(), None);
assert!(!verified.edits().is_empty());
```

`Document::set`, `remove`, `rename`, `insert`, `comment`, `uncomment` and
`set_export` produce `VerifiedEdits`, as does `Document::edit(&[Edit])`.
Insertion supports end, after a unique key, after an explicit entry, and sorted
position (before the first original entry whose key is greater). All anchors
refer to the original snapshot. Comments are separate items: removal never
attaches them implicitly. Commenting a multiline item prefixes every physical
line; restoration can require uncommenting several comment items in one batch,
including across explicitly removed intervening items. Empty selected EOF
comments can disappear. A selected uncomment may expose its own leading BOM;
exposing a BOM from unrelated source is refused.

`VerifiedEdits::edits()` borrows the native `TextEdit { range, replacement }`
list; `into_edits()` consumes it. `as_str()` borrows the verified output and
`into_string()` consumes it. Ranges refer to the **original** snapshot: an
editor must check the document version and apply non-overlapping ranges from
highest offset to lowest, not reinterpret them as sequential offsets.

Verification reparses the whole batch and rejects conflicting operations,
unrepresentable values, or changed interpretation of unrelated syntax,
including changed multiline quote grouping. Unrelated invalid statements do
not prevent editing a valid target. Existing quotes are retained when exact;
otherwise an exact safe style is selected. `encode_value(&str) -> Option<String>`
encodes a standalone Portable token, not a context-safe edit by itself; control
characters other than newline, carriage return and tab are refused.

The free `adotenv::set(text, key, value)` and `remove(text, key)` helpers still
return `Result<String, EditError>`, but now use the recovering parser and native
verified producer. They are not a 0.1 grammar compatibility layer.

## Editor positions

`doc.line_index()` constructs an explicit index. `to_position(offset, encoding)`
and `to_offset(Position { line, column }, encoding)` are checked conversions;
`ColumnEncoding` is `Utf8` (bytes), `Scalar` or `Utf16` (code units). Positions
are zero-based; `Position::one_based()` explicitly performs checked display
conversion. `Entry::line()` and `Diagnostic::line()` remain one-based.

EOF is valid, and a terminal line ending creates a final empty line. Line-ending
starts map to the prior content end; the interior of CRLF is not representable.
Columns beyond line content and split UTF-16 surrogate pairs are rejected,
never rounded or clamped. Scalar/UTF-16 conversions reject interior UTF-8
bytes; UTF-8 columns preserve exact byte offsets, including those inside a
character. Text-edit boundaries must still be UTF-8 aligned. The BOM counts
in original-source coordinates. `Entry::decoded_offset_to_raw` and
`decoded_span_to_raw` map decoded value byte boundaries back to raw source
spans, including escapes.

## Migrating from 0.1

0.2 is a breaking release, not an alternate legacy parser:

1. Replace `parse(text)?` / `parse(text).unwrap()` with `parse_strict(text)` for
   first-error handling, or destructure `let (doc, diagnostics) = parse(text)`.
2. Replace `doc.entries()[i]` with `doc.entry(i)` or iterator methods. Views are
   borrowed from the document; owned source uses `parse_owned` / `into_owned`.
3. Handle `Option` from `Entry::value()`, `raw_value()` and `value_span()` because
   bare declarations are retained, not rejected. A last bare duplicate does
   not fall back to an earlier assigned value.
4. Review ASCII key restrictions, whitespace-sensitive hash comments (including
   `A= # comment`), `\$` decoding and unknown-escape diagnostics. Rejecting
   quoted tails and accepting multiline quotes can change statement grouping.
5. Match `DiagnosticCode`, severity and spans instead of the old parse error
   type or message wording. Strict parsing still allows portability warnings.
6. Prefer document-native edits for selected occurrences and verified batches;
   their result is `VerifiedEdits`, not a string. Free `set` / `remove` retain
   string results but use the new Portable grammar and verification rules.
7. Update the toolchain to Rust **1.81**. The default `std` feature is optional:
   `adotenv = { version = "0.2", default-features = false }` uses `alloc` only.

## Quality and future work

Criterion workloads cover small, typical and multi-megabyte parsing and native
edits. Public synthetic seeds feed parser and edit fuzz targets; scheduled CI
runs bounded campaigns. Reproduction commands and evidence policy are in
[quality.md](docs/design/quality.md). Harness availability is not a claim that
any particular benchmark or fuzz duration has passed.

The public [upstream conformance corpus](conformance/manifest.json) records
version-pinned real Node, Python, Compose and Docker env-file observations for
future dialect work. Run `python3 conformance/run.py --setup` on Linux;
[profile details and validation evidence](docs/design/quality.md#public-upstream-conformance-corpus-03-preparation)
separate parsing, resolution and loading. It does not claim that Portable
already implements those dialects.

The runner binds declared versions to locked artifacts and executed package
identities, validates each profile's observation protocol, and hashes exact input
snapshots before setup/execution. Changes to cases, profiles, manifest or locks
during a run abort comparison/recording rather than blessing mixed inputs.
Reviewed source commits remain distinct from distribution/module artifact pins.

Recovering `parse` / `parse_owned` retain items and all diagnostics. Strict
parsing and native edit verification omit discarded diagnostic lists and
duplicate-warning key indexes; errors remain attached to malformed items.
Callers handling untrusted input must still apply their own byte and
line/declaration budgets before parsing: retained documents, decoded escape
maps and requested diagnostics consume memory. The core does not impose
application-specific resource limits.

0.2 is the lossless-core release. Dialect presets, reference analysis/expansion,
formatting, loading and integration crates remain **future** milestones; none
are available in this release. See
[ROADMAP.md](ROADMAP.md) and [design notes](docs/design/).

## Minimum supported Rust version

Rust 1.81 (edition 2021). Raising the MSRV is a minor-version change;
development and fuzz dependencies may require a newer toolchain.

## License

Licensed under either [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.
