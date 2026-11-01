//! Literal, lossless `.env` parsing and verified editing.
//!
//! adotenv reads `.env` files the way tools need them. It keeps every byte of the source,
//! leaves values literal (`${HOME}` is text, not an expansion), gives every token a byte span
//! and checks every edit by parsing the result again before returning it. It is built for
//! linters, formatters, editors, scanners and other tools that maintain configuration files.
//!
//! The core never reads files or environment variables, never expands references and never
//! runs commands. Each of those happens only through an explicit API or an optional feature.
//!
//! The [user guide](https://asamarts.github.io/adotenv/) explains the behavior in more depth.
//! This reference documents each item.
//!
//! # Quick start
//!
//! ```
//! let source = "# app\nexport PORT=8080\nURL=\"${HOST}/api\" # base\n";
//! let doc = adotenv::parse_strict(source)?;
//!
//! // Values are literal: references are not expanded.
//! assert_eq!(doc.get("PORT"), Some("8080"));
//! assert_eq!(doc.get("URL"), Some("${HOST}/api"));
//!
//! // A document prints back the exact source.
//! assert_eq!(doc.to_string(), source);
//!
//! // An edit returns the new text, already parsed again and checked.
//! let edited = doc.set("PORT", "9090")?;
//! assert_eq!(edited.as_str(), "# app\nexport PORT=9090\nURL=\"${HOST}/api\" # base\n");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! [`parse_strict`] stops at the first error. [`parse`] never fails: it returns a document and
//! a list of [`Diagnostic`]s, so tools can inspect and report on a broken file without losing
//! any of it.
//!
//! ```
//! use adotenv::{DiagnosticCode, ItemKind};
//!
//! let (doc, diagnostics) = adotenv::parse("BAD KEY=value\nGOOD=kept\n");
//! assert_eq!(doc.get("GOOD"), Some("kept"));
//! assert_eq!(doc.items().next().unwrap().kind(), ItemKind::Invalid);
//! assert_eq!(diagnostics[0].code(), DiagnosticCode::InvalidKey);
//! assert_eq!(diagnostics[0].line(), 1);
//! ```
//!
//! # Where to start
//!
//! | Task | Start with |
//! |---|---|
//! | Parse text | [`parse`], [`parse_strict`], [`parse_owned`], [`parse_values`], and their `_with` variants that take a [`Dialect`] |
//! | Look up values | [`Document::get`], [`Document::get_all`], [`Document::effective_entries`], [`Document::lookup_index`] |
//! | Walk the source | [`Document::items`], [`Document::entries`], [`Item`], [`Entry`], [`EntryTokens`] |
//! | Report problems | [`Diagnostic`], [`DiagnosticCode`], [`Severity`] |
//! | Choose reading rules | [`Dialect`] and its presets |
//! | Edit | [`Document::set`], [`Document::remove`], [`Document::rename`], [`Document::insert`], or [`Document::edit`] with a batch of [`Edit`]s |
//! | Quote a value for a file | [`encode_value`], [`encode_value_with`] |
//! | Format and sort | [`Document::format`] with [`FormatOptions`], [`Document::sort`] |
//! | Compare two files | [`Document::compare`] |
//! | Write an example file | [`Document::example`] |
//! | Check portability across dialects | [`Document::portability`] |
//! | Groups and comments | [`Document::groups`], [`Document::header`], [`Entry::leading_comments`] |
//! | Lines and columns | [`Document::line_index`], [`LineIndex`], [`ColumnEncoding`] |
//! | Find `$VAR` references | [`Entry::references`], [`ReferenceRules`] |
//! | Resolve references | [`Document::resolve`] with [`ExpandOptions`] |
//! | Load files from disk | the [`load`] module |
//! | Deserialize into a struct | [`Document::deserialize`] |
//! | Run command substitutions | [`Document::resolve_with_commands`] and the [`exec`] module |
//!
//! # Features
//!
//! | Feature | Default | Adds |
//! |---|---|---|
//! | `std` | yes | Standard library support. Required by `load` and `exec`. |
//! | `load` | no | The [`load`] module, which reads `.env` cascades and file lists from disk. |
//! | `serde` | no | [`Document::deserialize`] and [`DeserializeError`]; with `load` also [`load::LoadedEnvironment::deserialize`]. |
//! | `exec` | no | The [`exec`] module, which runs command substitutions through the platform shell. |
//!
//! Without default features the crate is `no_std` and uses `alloc`:
//!
//! ```toml
//! adotenv = { version = "1", default-features = false }
//! ```
//!
//! The crate has no runtime dependencies and forbids unsafe code with every feature
//! combination. It supports Rust 1.81 and later.
//!
//! # Dialects
//!
//! Every parse uses a [`Dialect`]: the syntax and literal lookup rules of one `.env` reader.
//! The functions without a suffix use [`Dialect::PORTABLE`]. The other presets follow Node
//! dotenv, python-dotenv, Docker Compose and Docker `--env-file`, and a custom dialect starts
//! from a preset and changes single rules. A dialect never enables expansion, inheritance or
//! file loading.
//!
//! ```
//! use adotenv::{parse_with, Dialect};
//!
//! let (doc, _) = parse_with("A=a#b\n", Dialect::NODE_DOTENV);
//! assert_eq!(doc.get("A"), Some("a"));
//! assert_eq!(doc.reparse(Dialect::DOCKER_RAW).0.get("A"), Some("a#b"));
//! ```
//!
//! # Source, spans and positions
//!
//! A [`Document`] borrows its source, or owns it with [`parse_owned`]. Its items cover the
//! whole source without gaps, malformed statements included, and every range is a byte range
//! into the original text, BOM included. [`LineIndex`] converts byte offsets to zero-based
//! lines and columns in UTF-8 bytes, Unicode scalar values or UTF-16 code units, and
//! [`Position::one_based`] converts a position for display.
//!
//! # Verified edits
//!
//! Edits select declarations by key, or by item when keys repeat. Every edit and every batch
//! is applied to the whole source, parsed again with the same dialect and checked: the
//! requested change happened and nothing else changed meaning. [`VerifiedEdits`] then gives
//! the new text and the source-relative [`TextEdit`]s, for example to send to an editor.
//! [`Document::format`] and [`Document::sort`] are verified the same way.
//!
//! # References and resolution
//!
//! Values stay literal. [`Entry::references`] lists `$VAR`, `${VAR:-default}` and
//! `$(command)` forms with their spans without resolving anything. [`Document::resolve`]
//! substitutes references only on request, from a lookup closure you pass, with explicit
//! scope, precedence and size limits. Commands run only through a callback you pass to
//! [`Document::resolve_with_commands`].
//!
//! # Secrets
//!
//! Values may be secrets. `Debug` output of documents, entries, loaded environments and
//! resolved values includes them, and so do serde conversion errors. Do not log them where
//! values are secret. [`Document::example`] removes assigned values but keeps comments.
//!
//! # Related crates
//!
//! - [`adotenv-process`](https://docs.rs/adotenv-process) applies a loaded environment to the
//!   current process.
//! - [`adotenv-config`](https://docs.rs/adotenv-config) and
//!   [`adotenv-figment`](https://docs.rs/adotenv-figment) provide values to `config` and
//!   Figment.
//! - [`adotenv-cli`](https://crates.io/crates/adotenv-cli) checks, formats, compares and loads
//!   files from a shell.
//!
//! # Stability
//!
//! The public API follows semantic versioning. Enums and option structs are
//! `#[non_exhaustive]`, so a minor release can add variants, fields and builders.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
// Links to items behind optional features resolve only when those features are enabled.
// docs.rs and CI build the documentation with every feature.
#![cfg_attr(
    not(all(feature = "load", feature = "serde", feature = "exec")),
    allow(rustdoc::broken_intra_doc_links)
)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod compare;
mod diagnostic;
mod dialect;
mod edit;
mod encode;
mod example;
#[cfg(feature = "exec")]
pub mod exec;
mod format;
#[cfg(feature = "load")]
pub mod load;
mod model;
mod parser;
mod portability;
mod position;
mod references;
mod resolve;
#[cfg(feature = "serde")]
mod serde_support;
mod sort;
mod structure;

pub use compare::{ComparedValue, Comparison, ComparisonEntry, DifferenceKind};
pub use diagnostic::{Diagnostic, DiagnosticCode, Severity};
pub use dialect::{
    BareKeys, BomPolicy, ColonSeparator, Dialect, DoubleEscapes, Duplicates, InlineComments,
    KeySyntax, LineBreaks, MalformedLines, Multiline, QuotedTail, SingleEscapes, Syntax,
    UnknownEscapes, Whitespace,
};
pub use edit::{remove, set, Edit, EditError, InsertPosition, TextEdit, VerifiedEdits};
pub use encode::{encode_value, encode_value_with, EncodeError};
pub use format::{
    BlankLines, EqualsSpacing, FinalNewline, FormatError, FormatOptions, FormatResult, QuoteStyle,
};
pub use model::{
    AssignmentState, Document, EffectiveEntry, Entry, EntryTokens, Item, ItemKind, LineEnding,
    LookupIndex, Quote,
};
pub use parser::{
    parse, parse_owned, parse_owned_with, parse_strict, parse_strict_with, parse_values,
    parse_values_with, parse_with,
};
pub use portability::{
    PortabilityDeclaration, PortabilityDifference, PortabilityPart, PortabilityReading,
    PortabilityReport,
};
pub use position::{ColumnEncoding, LineIndex, Position, PositionError};
pub use references::{OperatorSet, Reference, ReferenceKind, ReferenceOperator, ReferenceRules};
pub use resolve::{
    CommandCall, CommandContext, CommandError, CommandOutcome, ExpandOptions, ExpandScope,
    LookupOrder, ResolveError, ResolveFailure, ResolvedDocument, ResolvedEntry, ResolvedSegment,
    ResolvedSegmentKind, Unresolved, ValueOrigin,
};
#[cfg(feature = "serde")]
pub use serde_support::DeserializeError;
pub use structure::{Group, Groups};

#[cfg(test)]
mod compare_tests;
#[cfg(test)]
mod diagnostic_tests;
#[cfg(test)]
mod dialect_tests;
#[cfg(test)]
mod edit_tests;
#[cfg(test)]
mod encode_tests;
#[cfg(test)]
mod example_tests;
#[cfg(test)]
mod format_tests;
#[cfg(all(test, feature = "load"))]
mod load_tests;
#[cfg(test)]
mod portability_tests;
#[cfg(test)]
mod position_tests;
#[cfg(test)]
mod reference_tests;
#[cfg(test)]
mod resolve_tests;
#[cfg(all(test, feature = "serde"))]
mod serde_tests;
#[cfg(test)]
mod sort_tests;
#[cfg(test)]
mod structure_tests;
#[cfg(test)]
mod tests;

/// Compile and execute the README examples with the crate's documentation.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;

/// Compile and execute the examples of the documentation site.
///
/// Every book page is listed, so a Rust example added to any page is tested.
/// Pages with examples that need optional features are tested with them.
#[cfg(doctest)]
pub mod book_doctests {
    #[doc = include_str!("../docs/index.md")]
    pub struct Introduction;
    #[cfg_attr(feature = "load", doc = include_str!("../docs/guide/getting-started.md"))]
    pub struct GettingStarted;
    #[doc = include_str!("../docs/guide/parsing.md")]
    pub struct Parsing;
    #[doc = include_str!("../docs/design/dialects.md")]
    pub struct Dialects;
    #[doc = include_str!("../docs/guide/editing.md")]
    pub struct Editing;
    #[doc = include_str!("../docs/design/formatting.md")]
    pub struct Formatting;
    #[doc = include_str!("../docs/design/comparison.md")]
    pub struct Comparison;
    #[cfg_attr(feature = "exec", doc = include_str!("../docs/design/expansion.md"))]
    pub struct References;
    #[cfg_attr(feature = "load", doc = include_str!("../docs/design/loading.md"))]
    pub struct Loading;
    #[cfg_attr(all(feature = "load", feature = "serde"), doc = include_str!("../docs/design/serde.md"))]
    pub struct Serde;
    #[cfg_attr(feature = "load", doc = include_str!("../docs/guide/process.md"))]
    pub struct Process;
    #[doc = include_str!("../docs/design/integrations.md")]
    pub struct Integrations;
    #[doc = include_str!("../docs/design/cli.md")]
    pub struct Cli;
    #[doc = include_str!("../docs/reference/diagnostics.md")]
    pub struct Diagnostics;
    #[doc = include_str!("../docs/reference/security.md")]
    pub struct Security;
    #[doc = include_str!("../docs/reference/upgrading.md")]
    pub struct Upgrading;
    #[doc = include_str!("../docs/design/architecture.md")]
    pub struct Architecture;
    #[doc = include_str!("../docs/design/feature-decisions.md")]
    pub struct FeatureDecisions;
    #[doc = include_str!("../docs/design/quality.md")]
    pub struct Quality;
    #[doc = include_str!("../docs/design/test-audit.md")]
    pub struct TestAudit;
    #[doc = include_str!("../docs/design/performance.md")]
    pub struct Performance;
    #[doc = include_str!("../docs/design/1.0-readiness.md")]
    pub struct Readiness;
    #[doc = include_str!("../docs/design/1.0-fix-round.md")]
    pub struct FixRound;
}
