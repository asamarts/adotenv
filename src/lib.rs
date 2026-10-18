//! Lossless, literal `.env` dialect parsing and verified native text edits.
//!
//! Parsing borrows UTF-8 source and retains every byte, including malformed
//! statements. It never reads files, consults the environment, or expands values.
//!
//! ```
//! use adotenv::{parse, parse_owned, parse_strict, Severity};
//!
//! let text = "# app\nexport PORT=8080\nURL=\"${HOST}/api\" # base\n";
//! let (doc, diagnostics) = parse(text);
//! assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
//! assert_eq!(doc.get("PORT"), Some("8080"));
//! assert_eq!(doc.get("URL"), Some("${HOST}/api"));
//! let edits = doc.set("PORT", "9090")?;
//! assert_eq!(edits.edits()[0].replacement, "9090");
//! assert_eq!(parse_strict(edits.as_str())?.get("PORT"), Some("9090"));
//! let (owned, _) = parse_owned(String::from("KEY='value'\n"));
//! assert_eq!(owned.get("KEY"), Some("value"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Explicit dialects
//!
//! [`parse`], [`parse_owned`] and [`parse_strict`] retain Portable defaults.
//! Their `_with` variants accept a [`Dialect`]; [`Document::reparse`] borrows
//! retained source and rebuilds its interpretation. Presets never enable
//! interpolation, inheritance, file loading or ambient environment access.
//!
//! ```
//! use adotenv::{parse_with, Dialect};
//! let (doc, _) = parse_with("A=a#b\n", Dialect::NODE_DOTENV);
//! assert_eq!(doc.get("A"), Some("a"));
//! assert_eq!(doc.dialect(), Dialect::NODE_DOTENV);
//! assert_eq!(doc.reparse(Dialect::DOCKER_RAW).0.get("A"), Some("a#b"));
//! ```
//!
//! [`Document::portability`] reparses the retained source once per unique
//! configuration, includes the document's own reading first, and reports every
//! physical line whose declarations, values, states, export prefixes, lookup
//! participation or item grouping differ.
//!
//! [`Entry::references`] reports raw `$VAR`, `${VAR:-default}` and `$(command)`
//! candidates with original-source spans and quote/escape context. It never
//! resolves a lookup or executes a command; [`Reference::is_eligible`] applies
//! explicit caller-selected rules.
//!
//! [`Document::resolve`] performs opt-in variable substitution with a caller
//! lookup closure. Sequential or whole-file scope, precedence, unresolved
//! policy, operators, cycles and resource limits are explicit. Command forms
//! stay literal unless [`Document::resolve_with_commands`] receives an explicit
//! callback; callback calls are provenance-recorded and never run by default.
//!
//! # Source and positions
//!
//! Ranges always address the original source, BOM included. Item ranges plus
//! the optional BOM partition it without gaps. A [`LineIndex`] offers checked
//! zero-based byte, scalar and UTF-16 coordinates. One-based display conversion
//! is explicit; scalar columns are not terminal-cell widths.
//!
//! # Editing and recovery
//!
//! Key-only helpers require unique declarations. [`Edit`] selects explicit
//! occurrences and comments from one source snapshot. Every complete batch is
//! reparsed and checked before [`VerifiedEdits`] exposes its output or native
//! edits. Unrelated malformed statements may remain, but must not change their
//! interpretation. Verification uses the exact selected dialect; values or
//! contexts its grammar cannot represent are rejected. External appliers check
//! versions.
//!
//! # Platform
//!
//! Zero runtime dependencies. Disable default features for `no_std` with `alloc`.
//! Unsafe code is forbidden regardless of features. Reference resolution,
//! loading and process-environment mutation are not part of literal parsing.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod diagnostic;
mod dialect;
mod edit;
mod encode;
mod model;
mod parser;
mod portability;
mod position;
mod references;
mod resolve;

pub use diagnostic::{Diagnostic, DiagnosticCode, Severity};
pub use dialect::{
    BareKeys, BomPolicy, ColonSeparator, Dialect, DoubleEscapes, Duplicates, InlineComments,
    KeySyntax, LineBreaks, MalformedLines, Multiline, QuotedTail, SingleEscapes, Syntax,
    UnknownEscapes, Whitespace,
};
pub use edit::{remove, set, Edit, EditError, InsertPosition, TextEdit, VerifiedEdits};
pub use encode::{encode_value, encode_value_with, EncodeError};
pub use model::{AssignmentState, Document, Entry, EntryTokens, Item, ItemKind, LineEnding, Quote};
pub use parser::{
    parse, parse_owned, parse_owned_with, parse_strict, parse_strict_with, parse_with,
};
pub use portability::{
    PortabilityDeclaration, PortabilityDifference, PortabilityPart, PortabilityReading,
    PortabilityReport,
};
pub use position::{ColumnEncoding, LineIndex, Position, PositionError};
pub use references::{
    OperatorSet, Reference, ReferenceContainer, ReferenceKind, ReferenceOperator, ReferenceRules,
};
pub use resolve::{
    CommandCall, CommandContext, CommandError, CommandOutcome, ExpandOptions, ExpandScope,
    LookupOrder, ResolveError, ResolveFailure, ResolvedDocument, ResolvedEntry, ResolvedSegment,
    ResolvedSegmentKind, Unresolved, ValueOrigin,
};

#[cfg(test)]
mod dialect_tests;
#[cfg(test)]
mod edit_tests;
#[cfg(test)]
mod encode_tests;
#[cfg(test)]
mod portability_tests;
#[cfg(test)]
mod position_tests;
#[cfg(test)]
mod reference_tests;
#[cfg(test)]
mod resolve_tests;
#[cfg(test)]
mod tests;

/// Compile and execute the README examples with the crate's documentation.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
