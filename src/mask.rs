//! Opt-in value masking for `Debug` output.
//!
//! A plain `Debug` print includes source text and values. Wrap a value in
//! [`Masked`] to keep the useful structure while replacing every value with
//! `<redacted>`:
//!
//! ```
//! let doc = adotenv::parse_strict("TOKEN=secret\nHOST=localhost\n")?;
//! let debug = format!("{:?}", adotenv::mask(&doc));
//! assert!(debug.contains("TOKEN"));
//! assert!(!debug.contains("secret"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Masking is explicit. It does not change the [`Document`] or any other value,
//! and a normal `Debug` print still shows the values. The CLI has a separate
//! `diff --mask` option for its output.

use alloc::vec::Vec;
use core::fmt;

#[cfg(feature = "load")]
use crate::load::{
    LoadedEntry, LoadedEnvironment, LoadedResolveError, LoadedValues, ResolvedEnvironment,
};
use crate::{
    CommandCall, CommandError, CommandOutcome, CompactValues, ComparedValue, Comparison,
    ComparisonEntry, Document, EffectiveEntry, Entry, FormatResult, Group, Groups, Item, LineIndex,
    LookupIndex, PortabilityDeclaration, PortabilityDifference, PortabilityPart,
    PortabilityReading, PortabilityReport, ResolveError, ResolveFailure, ResolvedDocument,
    ResolvedEntry, ResolvedSegment, TextEdit, VerifiedEdits,
};

/// The replacement text used for a masked value.
pub const REDACTED: &str = "<redacted>";

/// A wrapper whose [`Debug`] output masks values.
///
/// Create one with [`Masked::new`] or [`mask`]. The inner value is returned by
/// [`into_inner`](Self::into_inner).
///
/// ```
/// let doc = adotenv::parse_strict("TOKEN=secret\n")?;
/// let masked = adotenv::Masked::new(&doc);
/// assert!(!format!("{masked:?}").contains("secret"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy)]
pub struct Masked<T>(T);

impl<T> Masked<T> {
    /// Wraps `value`.
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// Returns the wrapped value.
    #[must_use]
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// A value with a structure-preserving, value-masking [`Debug`] form.
///
/// [`Masked`] calls [`Mask::fmt_masked`]. Implement this trait for a type that
/// should have a masked form; use [`mask`] to wrap it in [`Masked`].
pub trait Mask {
    /// Formats `self` without revealing value contents.
    ///
    /// # Errors
    ///
    /// Returns the formatter's error.
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl<T: Mask + ?Sized> Mask for &T {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt_masked(formatter)
    }
}

impl<T: Mask> fmt::Debug for Masked<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt_masked(formatter)
    }
}

/// Wraps `value` so its [`Debug`] output masks values.
#[must_use]
pub const fn mask<T>(value: T) -> Masked<T> {
    Masked::new(value)
}

impl Mask for Document<'_> {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<&str> = self.entries().map(Entry::key).collect();
        formatter
            .debug_struct("Document")
            .field("source", &REDACTED)
            .field("items", &self.items.len())
            .field("entries", &self.entries.len())
            .field("keys", &keys)
            .field("bom", &self.has_bom())
            .field("dialect", &self.dialect())
            .field("has_duplicate_keys", &self.has_duplicate_keys)
            .finish()
    }
}

impl Mask for Entry<'_> {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Entry")
            .field("key", &self.key())
            .field("line", &self.line())
            .field("state", &self.state())
            .field("exported", &self.exported())
            .field("value", &REDACTED)
            .field("raw_value", &REDACTED)
            .finish()
    }
}

impl Mask for Item<'_> {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Item")
            .field("id", &self.id())
            .field("kind", &self.kind())
            .field("span", &self.span())
            .field("line_ending", &self.line_ending())
            .field("entry", &self.entry().map(Entry::id))
            .field("diagnostic", &self.diagnostic())
            .finish()
    }
}

impl Mask for LineIndex<'_> {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LineIndex")
            .field("source", &REDACTED)
            .field("lines", &self.line_count())
            .finish()
    }
}

impl Mask for Group<'_> {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Group")
            .field("span", &self.span())
            .field("items", &self.items().len())
            .finish()
    }
}

impl Mask for Groups<'_> {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Groups")
            .field("source", &REDACTED)
            .finish()
    }
}

impl Mask for LookupIndex<'_> {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LookupIndex")
            .field("len", &self.len())
            .field("document", &REDACTED)
            .finish()
    }
}

impl Mask for EffectiveEntry<'_> {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EffectiveEntry")
            .field("entry", &Masked::new(self.entry()))
            .field("key", &self.key())
            .field("value", &REDACTED)
            .field("line", &self.line())
            .finish()
    }
}

impl Mask for CompactValues {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompactValues")
            .field("len", &self.len())
            .field("values", &REDACTED)
            .finish()
    }
}

impl Mask for TextEdit {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TextEdit")
            .field("range", &self.range)
            .field("replacement", &REDACTED)
            .finish()
    }
}

impl Mask for VerifiedEdits {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifiedEdits")
            .field("edits", &self.edits().len())
            .field("output", &REDACTED)
            .finish()
    }
}

impl Mask for FormatResult {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FormatResult")
            .field("edits", &self.edits().len())
            .field("output", &REDACTED)
            .finish()
    }
}

impl Mask for ComparedValue {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComparedValue")
            .field("state", &self.state)
            .field("value", &REDACTED)
            .field("exported", &self.exported)
            .finish()
    }
}

impl Mask for ComparisonEntry {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let left = self.left.as_ref().map(Masked::new);
        let right = self.right.as_ref().map(Masked::new);
        formatter
            .debug_struct("ComparisonEntry")
            .field("key", &self.key)
            .field("left", &left)
            .field("right", &right)
            .field("kind", &self.kind)
            .finish()
    }
}

impl Mask for Comparison {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Comparison")
            .field("differences", &masked_list(self.differences()))
            .finish()
    }
}

#[cfg(feature = "load")]
impl Mask for LoadedValues {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedValues")
            .field("len", &self.len())
            .field("values", &REDACTED)
            .finish()
    }
}

#[cfg(feature = "load")]
impl Mask for LoadedEnvironment {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedEnvironment")
            .field("len", &self.len())
            .field("values", &REDACTED)
            .field("retains_raw_tokens", &self.retains_raw_tokens())
            .finish()
    }
}

#[cfg(feature = "load")]
impl Mask for LoadedResolveError {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedResolveError")
            .field("key", &self.key())
            .field("origin", &self.origin())
            .field("message", &REDACTED)
            .finish()
    }
}

#[cfg(feature = "load")]
impl Mask for ResolvedEnvironment {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedEnvironment")
            .field("len", &self.len())
            .field("values", &REDACTED)
            .finish()
    }
}

impl Mask for ResolvedEntry {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedEntry")
            .field("entry", &self.entry)
            .field("key", &self.key)
            .field("state", &self.state)
            .field("value", &REDACTED)
            .field("effective_value", &REDACTED)
            .field("segments", &masked_list(&self.segments))
            .finish()
    }
}

impl Mask for ResolvedSegment {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedSegment")
            .field("value", &REDACTED)
            .field("kind", &REDACTED)
            .finish()
    }
}

impl Mask for ResolvedDocument {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedDocument")
            .field("entries", &masked_list(self.entries()))
            .field("commands", &masked_list(self.commands()))
            .finish()
    }
}

impl Mask for CommandError {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandError")
            .field("message", &REDACTED)
            .finish()
    }
}

impl Mask for CommandOutcome {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Output(_) => formatter.debug_tuple("Output").field(&REDACTED).finish(),
            Self::Error(_) => formatter.debug_tuple("Error").field(&REDACTED).finish(),
        }
    }
}

impl Mask for CommandCall {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandCall")
            .field("reference", &self.reference)
            .field("payload", &self.payload)
            .field("source", &self.source)
            .field("outcome", &Masked::new(&self.outcome))
            .finish()
    }
}

impl Mask for ResolveError {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut entry = formatter.debug_struct("ResolveError");
        match self {
            Self::Unresolved { key, name, span } => {
                entry
                    .field("kind", &"Unresolved")
                    .field("key", key)
                    .field("name", name)
                    .field("span", span);
            }
            Self::Required {
                key, name, span, ..
            } => {
                entry
                    .field("kind", &"Required")
                    .field("key", key)
                    .field("name", name)
                    .field("span", span)
                    .field("message", &REDACTED);
            }
            Self::Cycle { key, chain } => {
                entry
                    .field("kind", &"Cycle")
                    .field("key", key)
                    .field("chain", chain);
            }
            Self::DepthExceeded { key, limit, span } => {
                entry
                    .field("kind", &"DepthExceeded")
                    .field("key", key)
                    .field("limit", limit)
                    .field("span", span);
            }
            Self::TooManyReferences { key, limit, span } => {
                entry
                    .field("kind", &"TooManyReferences")
                    .field("key", key)
                    .field("limit", limit)
                    .field("span", span);
            }
            Self::ValueTooLarge { key, limit, span } => {
                entry
                    .field("kind", &"ValueTooLarge")
                    .field("key", key)
                    .field("limit", limit)
                    .field("span", span);
            }
            Self::OutputTooLarge { key, limit, span } => {
                entry
                    .field("kind", &"OutputTooLarge")
                    .field("key", key)
                    .field("limit", limit)
                    .field("span", span);
            }
            Self::Command { key, reference, .. } => {
                entry
                    .field("kind", &"Command")
                    .field("key", key)
                    .field("reference", reference)
                    .field("message", &REDACTED);
            }
        }
        entry.finish()
    }
}

impl Mask for ResolveFailure {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolveFailure")
            .field("error", &Masked::new(&self.error))
            .field("commands", &masked_list(&self.commands))
            .finish()
    }
}

#[cfg(feature = "load")]
impl Mask for LoadedEntry<'_> {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedEntry")
            .field("key", &self.key())
            .field("value", &REDACTED)
            .field("origin", &self.origin())
            .finish()
    }
}

impl Mask for PortabilityDeclaration {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PortabilityDeclaration")
            .field("key", &self.key)
            .field("value", &REDACTED)
            .field("state", &self.state)
            .field("exported", &self.exported)
            .field("participates", &self.participates)
            .field("effective_value", &REDACTED)
            .field("wins_lookup", &self.wins_lookup)
            .finish()
    }
}

impl Mask for PortabilityPart {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Declaration(declaration) => formatter
                .debug_tuple("Declaration")
                .field(&Masked::new(declaration))
                .finish(),
            Self::Blank => formatter.write_str("Blank"),
            Self::Comment => formatter.write_str("Comment"),
            Self::Invalid => formatter.write_str("Invalid"),
            Self::Ignored => formatter.write_str("Ignored"),
            Self::Continuation => formatter.write_str("Continuation"),
        }
    }
}

impl Mask for PortabilityReading {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PortabilityReading")
            .field("dialect", &self.dialect)
            .field("parts", &masked_list(&self.parts))
            .finish()
    }
}

impl Mask for PortabilityDifference {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PortabilityDifference")
            .field("span", &self.span)
            .field("first_line", &self.first_line)
            .field("last_line", &self.last_line)
            .field("readings", &masked_list(&self.readings))
            .finish()
    }
}

impl Mask for PortabilityReport {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PortabilityReport")
            .field("differences", &masked_list(self.differences()))
            .finish()
    }
}

fn masked_list<T: Mask>(items: &[T]) -> impl fmt::Debug + '_ {
    struct List<'a, T: Mask>(&'a [T]);
    impl<T: Mask> fmt::Debug for List<'_, T> {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_list()
                .entries(self.0.iter().map(Masked::new))
                .finish()
        }
    }
    List(items)
}
