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
  Trivia-only formatting returns the same kind of verified edits.
- **Comparison and examples.** Effective-key differences distinguish missing,
  bare, empty and assigned values. Example generation removes values while
  keeping comments and never copies malformed value text.
- **Small, side-effect-free core.** Zero runtime dependencies, no file I/O,
  environment access or process spawning by default, and unconditional
  `forbid(unsafe_code)`. The opt-in `exec` feature is the only
  process-spawning path. Disable the default `std` feature for `no_std` with
  `alloc`.

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

## Select a dialect (0.3)

`parse`, `parse_owned` and `parse_strict` still default to Portable.
`parse_with`, `parse_owned_with` and `parse_strict_with` take an explicit
`Dialect`: `PORTABLE`, `NODE_DOTENV`, `PYTHON_DOTENV`, `COMPOSE` or `DOCKER_RAW`.
Presets select literal syntax, never interpolation, inheritance or loading.

```rust
use adotenv::{parse_with, Dialect, InlineComments};

let source = "A=a#b\nB=`two words`\n";
let (node, _) = parse_with(source, Dialect::NODE_DOTENV);
assert_eq!(node.get("A"), Some("a"));
assert_eq!(node.get("B"), Some("two words"));
let (raw, _) = node.reparse(Dialect::DOCKER_RAW);
assert_eq!(raw.get("A"), Some("a#b"));
assert_eq!(raw.get("B"), Some("`two words`"));
assert_eq!(raw.source(), source);

let custom = Dialect {
    inline_comments: InlineComments::None,
    ..Dialect::NODE_DOTENV
};
assert_eq!(parse_with(source, custom).0.get("A"), Some("a#b"));
```

Each document retains its exact configuration. `reparse` borrows the original
source but rebuilds dialect-specific items, entries and source maps; snapshot
IDs must not be reused across interpretations. Typed fields control comments,
escapes, key syntax, bare declarations, quotes, raw values, duplicate lookup,
BOMs, line breaks and other grammar policies. See [dialect contracts and
upstream limitations](docs/design/dialects.md).

`entries()` and `get_all()` retain declarations excluded from effective lookup.
`participates_in_lookup()` distinguishes ignored bare declarations and Node's
special `__proto__` property. `BareKeys::Empty` changes effective `get()` to an
empty string without changing a bare entry's `value() == None` or syntax state.
Malformed syntax ignored by a preset remains an `ItemKind::Ignored` item with
an `IgnoredStatement` warning; explicit forbidden-policy violations stay errors.

For 0.2 consumers, Portable parsing and the existing entry/edit APIs keep their
defaults. Code constructing `EntryTokens` must supply the new `quoted_tail`
field (`None` when absent). Under quoted-key presets, `EntryTokens::key` covers
the raw quote wrapper; `Entry::key_span()` covers only the semantic key.

## Check portability (0.3)

`Document::portability` reparses the retained source once for each unique
configuration and reports every physical line whose reading differs. The
document's own configuration is always included first. Declarations, decoded
values, assignment states, export prefixes, lookup participation, effective
lookup values, duplicate lookup winners and statement grouping are compared;
quote style alone is not a difference. No expansion, lookup or I/O occurs.

```rust
use adotenv::{parse_with, Dialect, PortabilityPart};

let (doc, _) = parse_with("A=a#b\n", Dialect::PORTABLE);
let report = doc.portability(&[Dialect::NODE_DOTENV]);
assert_eq!(report.len(), 1);
let difference = &report.differences()[0];
assert_eq!(difference.first_line, 1);
assert!(difference.readings[0].parts.iter().any(|part| {
    if let PortabilityPart::Declaration(declaration) = part {
        declaration.value.as_deref() == Some("a#b")
    } else {
        false
    }
}));
assert!(difference.readings[1].parts.iter().any(|part| {
    if let PortabilityPart::Declaration(declaration) = part {
        declaration.value.as_deref() == Some("a")
    } else {
        false
    }
}));
```

## Inspect references (0.4)

`Entry::references` reports raw `$VAR`, `${VAR:-default}` and `$(command)`
candidates in source order. Each candidate carries source spans, quote/escape
context, variable operators, nested containers and malformed-form metadata.
Escaped dollars, single-quoted text and unterminated forms remain observable;
they are not automatically eligible. `ReferenceRules` and `OperatorSet` make
eligibility explicit, with no lookup, expansion, I/O or command execution.

```rust
use adotenv::{parse_strict, ReferenceOperator, ReferenceRules};

let source = "A=${HOST:-localhost}\n";
let doc = parse_strict(source).unwrap();
let references = doc.unique("A").unwrap().references();
assert_eq!(references.len(), 1);
let reference = &references[0];
assert_eq!(&source[reference.span.clone()], "${HOST:-localhost}");
assert_eq!(
    reference.operator,
    Some(ReferenceOperator::DefaultIfUnsetOrEmpty)
);
assert_eq!(&source[reference.argument.clone().unwrap()], "localhost");
assert!(reference.is_eligible(ReferenceRules::COMPOSE));
assert!(!reference.is_eligible(ReferenceRules::PORTABLE));
```

## Resolve values (0.4)

`Document::resolve` substitutes eligible variable references using the
document's own bindings and an explicit caller lookup closure. It never reads
the process environment, files or commands. Caller values are literal and are
not re-expanded. `ExpandOptions` selects sequential or whole-file scope,
caller-first or file-first precedence, unresolved-variable policy, quote/escape
rules and expansion limits. Required `:?` variables, reachable cycles, depth and
size limits return structured `ResolveError` values; successful substitutions
record file or caller origins and nested file provenance.

```rust
use adotenv::{parse_strict, ExpandOptions};

let doc = parse_strict("BASE=base\nURL=${BASE}/api\n").unwrap();
let resolved = doc.resolve(|_| None, &ExpandOptions::portable()).unwrap();
assert_eq!(resolved.entries()[1].value.as_deref(), Some("base/api"));

let caller = doc
    .resolve(
        |key| (key == "BASE").then(|| "external".to_string()),
        &ExpandOptions {
            lookup_order: adotenv::LookupOrder::CallerFirst,
            ..ExpandOptions::portable()
        },
    )
    .unwrap();
assert_eq!(caller.entries()[1].value.as_deref(), Some("external/api"));
```

Command substitutions and ineligible references remain literal in this slice.
No preset enables ambient environment lookup, loading or command execution.

### Command substitution by explicit callback

Commands never run by default. `resolve_with_commands` accepts a caller-supplied
callback only when the selected rules enable command substitutions. The callback
receives the raw command text, the whole reference and payload spans, and an
optional caller-supplied source identity. Outputs are literal; every call is
recorded, and a failure retains calls already made.

For trusted files, the optional `exec` feature provides `exec::Shell` with an
explicit working directory and optional source-identity-to-directory map. It
runs commands through the platform shell, trims one trailing newline and never
falls back to the host working directory. It is never enabled by default.

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

## Inspect structure (0.5)

`Document::groups` partitions nonblank items on blank lines. `Group::entries`,
`Group::comments` and `Group::span` inspect one run without copying source.
`Document::header` is the structural prefix before the first declaration.
`Entry::leading_comments` returns the contiguous whole-line comments directly
above an entry; a blank line breaks attachment. Inline comments remain available
through `Entry::inline_comment`.

```rust
use adotenv::parse_strict;

let source = "# header\nA=1\n# note for B\nB=2\n\nC=3\n";
let doc = parse_strict(source).unwrap();

let header = doc.header().unwrap();
assert_eq!(header.entries().count(), 0);
assert_eq!(&source[header.span()], "# header\n");

let groups: Vec<_> = doc.groups().collect();
assert_eq!(groups.len(), 2);
assert_eq!(groups[0].entries().count(), 2);
assert_eq!(groups[0].leading_comments().count(), 1);

let notes: Vec<_> = doc.unique("B").unwrap().leading_comments().collect();
assert_eq!(notes[0].raw(), "# note for B\n");
```

Groups and headers are views over the selected dialect's items. They do not
guess comment ownership, move comments, or perform expansion.

## Format whitespace (0.5)

`Document::format` changes only selected syntax trivia: spacing around
assignment separators, trailing spaces/tabs and the final newline. It reparses the complete result with
the document's exact dialect and verifies that every declaration, comment
boundary, item kind and malformed statement survives. The returned edits refer
to the original snapshot and the output is already verified.

```rust
use adotenv::{parse_strict, FormatOptions};

let source = "# app   \n\nA=1   \nB =  2 # note   \n";
let doc = parse_strict(source).unwrap();
let formatted = doc.format(&FormatOptions::normalize()).unwrap();
assert_eq!(
    formatted.as_str(),
    "# app\n\nA = 1\nB = 2 # note\n"
);
assert!(parse_strict(formatted.as_str()).is_ok());
```

`EqualsSpacing::{Preserve, Spaces, None}`,
`BlankLines::{Preserve, Collapse}`, `QuoteStyle::{Preserve, Double, Single,
Unquoted}` and `FinalNewline::{Preserve, Ensure, Remove}` are explicit.
`FormatOptions::normalize_groups` also collapses runs of consecutive blank
lines to one group separator. Quote changes are accepted only when the exact
decoded value remains representable; empty assigned values keep their existing
quote style in this slice. Raw-value dialects, multiline value bodies,
invalid/ignored statements, empty assignments whose spacing could become
indentation, and mixed lone-CR boundaries are preserved. `FinalNewline::Remove`
removes one terminator from a nonblank final item; a trailing blank line is
structure and is kept. Group sorting and line-ending conversion are not part of
this slice.

## Compare files by key (0.5)

`Document::compare` reports effective keys that differ between two documents.
Missing keys, bare versus empty versus assigned values, decoded value changes and
export differences are distinct. Duplicate-key resolution and lookup
participation follow each document's own dialect.

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

Comparison is literal and side-effect-free. It does not expand values, read the
environment, execute commands or inspect files.

## Generate an example (0.5)

`Document::example` removes assigned values while retaining comments, blank
lines, keys, export prefixes, formatting, line endings and the BOM. Bare
declarations stay bare; an empty quoted placeholder keeps an inline comment
attached where the grammar requires a value body. Malformed/ignored statements
and entries with an empty key become indentation-preserving comments so they
cannot leak a value.

```rust
use adotenv::parse;

let source = "# app\nA=1\nB=\"two\" # note\nBARE\nBAD KEY=secret\n";
let (doc, _) = parse(source);
assert_eq!(doc.example(), "# app\nA=\nB= # note\nBARE\n#\n");
```

## Portable syntax (unchanged default)

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
attaches them implicitly. Commenting prefixes each line under the selected
scanner's line-break policy; restoration can require several comment items,
including across explicitly removed intervening items. Empty selected EOF
comments can disappear. A selected uncomment may expose its own leading BOM;
exposing a BOM from unrelated source is refused.

`VerifiedEdits::edits()` borrows the native `TextEdit { range, replacement }`
list; `into_edits()` consumes it. `as_str()` borrows the verified output and
`into_string()` consumes it. Ranges refer to the **original** snapshot: an
editor must check the document version and apply non-overlapping ranges from
highest offset to lowest, not reinterpret them as sequential offsets.

Verification reparses the whole batch with the document's exact `Dialect` and
rejects conflicting operations, unrepresentable values, or changed interpretation
of unrelated syntax, including changed multiline quote grouping. Unrelated
invalid statements do not prevent editing a valid target. Existing quotes are
retained when exact; otherwise a safe candidate is verified. Python key quotes,
Node backticks, appended tails and raw Docker values retain their selected
grammar. Native set/insert use the same exact codec as standalone encoding,
with the actual header, suffix and line budget; they accept literal controls
when the selected grammar can represent them. Whole-batch preservation remains
mandatory even when a standalone value is encodable.

`encode_value_with(&str, Dialect) -> Result<String, EncodeError>` returns a
standalone literal token or `EncodeError::Unrepresentable`. Its contract is
`A=<token>\n` **and** `A=<token>` at EOF. The canonical `A=` header consumes two
bytes of a configured first-line budget. A Node token may begin with a line
break when a first-line cap cannot fit the opening quote; the grammar treats
that break as trivia between `=` and the value. This is not a context-safe
edit or an interpolation guarantee. Syntax padding is part of the returned
token: LF-only raw values ending in CR use an extra CR, while `LiteralFallback`
can require a trailing inline-comment marker. Do not append CRLF to raw padding;
use a native edit for an existing document.

```rust
use adotenv::{encode_value_with, parse_strict_with, Dialect};

let value = "literal #\0\n";
let token = encode_value_with(value, Dialect::PYTHON_DOTENV).unwrap();
let source = format!("A={token}\n");
assert_eq!(parse_strict_with(&source, Dialect::PYTHON_DOTENV).unwrap().get("A"),
           Some(value));
assert!(encode_value_with("two\nlines", Dialect::DOCKER_RAW).is_err());
```

`encode_value(&str) -> Option<String>` keeps its Portable default and its
published refusal of controls other than newline, carriage return and tab.

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

## Migrating from 0.3

0.4 is additive. Parsing, editing and 0.3 dialect APIs keep their behavior.
`Entry::references`, `Document::resolve`, command callbacks and the `exec`
feature are opt-in; commands remain literal unless a callback is supplied and
the selected rules enable them. The `exec` feature is disabled by default and
is the only process-spawning path.

## Migrating from 0.2

0.3 is additive for parsing and editing. Portable defaults are unchanged.
Code that constructs `EntryTokens` must supply the new `quoted_tail` field
(`None` when absent). `Dialect`, `parse_with`, `encode_value_with` and
`Document::portability` are new APIs; no preset enables expansion, inheritance
or loading. See the dialect section for the exact token span rules.

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

Criterion workloads cover small, typical and multi-megabyte parsing, exact
per-dialect value encoding, portability reports, and native edits including
large set/set-rename batches. Public synthetic seeds feed parser, edit,
encoding and resolution fuzz targets; scheduled CI runs bounded campaigns. Reproduction commands and evidence policy
are in [quality.md](docs/design/quality.md). Harness availability is not a claim
that any particular benchmark or fuzz duration has passed.

The public [upstream conformance corpus](conformance/manifest.json) records
version-pinned real Node, Python, Compose and Docker env-file observations.
Run `python3 conformance/check_adotenv.py --setup` on Linux to replay the real
APIs and compare applicable literal results with the Rust presets.
[Profile details and validation evidence](docs/design/quality.md#public-upstream-conformance-corpus-03-preparation)
separate parsing, resolution and loading. Compose comparisons exclude
dollar-containing inputs and supplied lookup environments; its real default API
does not provide an interpolation-off oracle. Portable is not asserted equivalent
to an upstream dialect.

The runner binds declared versions to locked artifacts and executed package
identities, validates each profile's observation protocol, and hashes exact input
snapshots before setup/execution. Changes to cases, profiles, manifest or locks
during a run abort comparison/recording rather than blessing mixed inputs.
Reviewed source commits remain distinct from distribution/module artifact pins.

Recovering `parse` / `parse_owned` retain items and all diagnostics. Strict
parsing and native edit verification omit discarded diagnostic lists and
duplicate-warning key indexes; primary item diagnostics remain available.
Callers handling untrusted input must still apply their own byte and
line/declaration budgets before parsing: retained documents, decoded escape
maps and requested diagnostics consume memory. Docker's preset includes its
upstream scanner line-byte limit; this is not an application memory ceiling.

0.3 includes explicit dialect configuration, five presets, per-dialect
encoding, order-independent same-line native set batches under byte caps and
cross-dialect portability reporting. 0.4 adds literal reference inspection,
opt-in variable resolution, explicit command callbacks and an opt-in `exec`
helper; formatting, loading and integration crates remain future milestones.
See [ROADMAP.md](ROADMAP.md) and [design notes](docs/design/).

## Minimum supported Rust version

Rust 1.81 (edition 2021). Raising the MSRV is a minor-version change;
development and fuzz dependencies may require a newer toolchain.

## License

Licensed under either [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.
