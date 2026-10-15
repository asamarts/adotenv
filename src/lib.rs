//! Lossless, literal Portable `.env` parsing and verified native text edits.
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
//! interpretation. External edit appliers must check document versions.
//!
//! # Platform
//!
//! Zero runtime dependencies. Disable default features for `no_std` with `alloc`.
//! Unsafe code is forbidden regardless of features. Dialect presets, reference
//! resolution, loading and process-environment mutation are not part of 0.2.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod diagnostic;
mod edit;
mod model;
mod parser;
mod position;

pub use diagnostic::{Diagnostic, DiagnosticCode, Severity};
pub use edit::{
    encode_value, remove, set, Edit, EditError, InsertPosition, TextEdit, VerifiedEdits,
};
pub use model::{AssignmentState, Document, Entry, EntryTokens, Item, ItemKind, LineEnding, Quote};
pub use parser::{parse, parse_owned, parse_strict};
pub use position::{ColumnEncoding, LineIndex, Position, PositionError};

#[cfg(test)]
mod edit_tests;
#[cfg(test)]
mod position_tests;
#[cfg(test)]
mod tests;

/// Compile and execute the README examples with the crate's documentation.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
