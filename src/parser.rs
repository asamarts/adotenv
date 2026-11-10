use alloc::{
    borrow::{Cow, ToOwned},
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{cell::Cell, ops::Range};

use crate::dialect::{
    BareKeys, BomPolicy, ColonSeparator, Dialect, Duplicates, InlineComments, KeySyntax,
    LineBreaks, MalformedLines, QuotedTail, SingleEscapes, Syntax, Whitespace,
};
use crate::model::{EntryData, ItemData, NO_ENTRY};
use crate::{
    AssignmentState, Diagnostic, DiagnosticCode, Document, EntryTokens, ItemKind, LimitExceeded,
    LimitKind, LineEnding, Quote, Severity,
};

mod compose;
mod decode;
mod docker;
mod node;
mod python;
mod unicode;

/// The maximum number of ordinary diagnostics one `parse` call collects.
///
/// A pathological source can otherwise allocate one diagnostic per character.
/// A final [`DiagnosticCode::TooManyDiagnostics`] warning reports truncation.
pub(crate) const MAX_DIAGNOSTICS: usize = 10_000;

/// Optional bounds for parsing untrusted input.
///
/// The default has no byte or item limit and keeps the established 10,000
/// diagnostic budget. A limit is opt-in, so enabling one cannot change how
/// input that stays within it is parsed. Limits count the original source,
/// including a leading byte-order mark. A future major release may choose
/// default limits.
///
/// Use [`parse_with_options`], [`parse_strict_with_options`],
/// [`parse_values_with_options`] or [`parse_owned_with_options`] to apply
/// these options. A limit failure is a
/// [`DiagnosticCode::LimitExceeded`](crate::DiagnosticCode::LimitExceeded)
/// diagnostic; [`Diagnostic::limit`](crate::Diagnostic::limit) carries its
/// [`LimitExceeded`](crate::LimitExceeded) data.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict_with_options, Dialect, ParseOptions};
///
/// let options = ParseOptions::default().with_max_input_bytes(3);
/// let error = parse_strict_with_options("A=1\n", Dialect::PORTABLE, options).unwrap_err();
/// assert_eq!(error.limit().unwrap().kind(), adotenv::LimitKind::InputBytes);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ParseOptions {
    /// The maximum source length in bytes, or `None` for no limit.
    pub max_input_bytes: Option<usize>,
    /// The maximum number of parsed items, or `None` for no limit.
    ///
    /// The value parser counts statements rather than retaining item data.
    pub max_items: Option<usize>,
    /// The maximum number of collected diagnostics, or `None` for the
    /// established 10,000-entry budget.
    ///
    /// This applies to the recovering parser only. Strict and values-only
    /// parsing keep the first error and do not build a diagnostic list.
    pub max_diagnostics: Option<usize>,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl ParseOptions {
    /// Returns options with no byte, item or diagnostic limit beyond the
    /// established diagnostic budget.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_input_bytes: None,
            max_items: None,
            max_diagnostics: None,
        }
    }

    /// Sets the maximum source length in bytes.
    #[must_use]
    pub const fn with_max_input_bytes(mut self, limit: usize) -> Self {
        self.max_input_bytes = Some(limit);
        self
    }

    /// Sets the maximum number of parsed items.
    #[must_use]
    pub const fn with_max_items(mut self, limit: usize) -> Self {
        self.max_items = Some(limit);
        self
    }

    /// Sets the maximum number of collected diagnostics.
    ///
    /// `0` keeps no ordinary diagnostics and reports a limit failure if any
    /// exist.
    #[must_use]
    pub const fn with_max_diagnostics(mut self, limit: usize) -> Self {
        self.max_diagnostics = Some(limit);
        self
    }
}

/// Parses `.env` text with the Portable dialect, recovering from errors.
///
/// This never fails. A statement with an error is kept as an
/// [`ItemKind::Invalid`] item and reported in the returned diagnostics, and
/// parsing recovers at the smallest safe boundary, usually the end of the
/// line, so one broken line does not hide the rest of the file. Diagnostics of
/// every [`Severity`] are returned, ordered by their position in the source.
/// At most 10,000 ordinary diagnostics are kept; if more are found, later
/// warnings are omitted, the first omitted error is kept, and the last entry
/// is [`DiagnosticCode::TooManyDiagnostics`]. The document borrows `source`;
/// values are literal and nothing is expanded, read from files or taken from
/// the environment.
///
/// The other parse functions differ from this one as follows:
///
/// - [`parse_strict`] returns the document without diagnostics, or the first
///   error.
/// - [`parse_owned`] takes a `String` and returns a `Document<'static>` that
///   owns it, with the collected diagnostics and the same budget.
/// - [`parse_values`] returns only the effective values, or the first error.
///   It keeps no items or spans and uses much less memory.
///
/// All four use [`Dialect::PORTABLE`]. Their `_with` variants, such as
/// [`parse_with`], take the [`Dialect`] to use instead.
///
/// # Examples
///
/// ```
/// use adotenv::{DiagnosticCode, ItemKind};
///
/// let source = "BAD KEY=value\nGOOD=kept\n";
/// let (doc, diagnostics) = adotenv::parse(source);
///
/// assert_eq!(doc.get("GOOD"), Some("kept"));
/// assert_eq!(doc.items().next().unwrap().kind(), ItemKind::Invalid);
/// assert_eq!(diagnostics[0].code(), DiagnosticCode::InvalidKey);
/// assert_eq!(diagnostics[0].line(), 1);
/// assert_eq!(doc.source(), source);
/// ```
#[must_use]
pub fn parse(source: &str) -> (Document<'_>, Vec<Diagnostic>) {
    parse_with(source, Dialect::PORTABLE)
}

/// Parses `.env` text with the given dialect, recovering from errors.
///
/// This is [`parse`] with an explicit [`Dialect`]: a preset such as
/// [`Dialect::NODE_DOTENV`] or [`Dialect::COMPOSE`], or a preset changed with
/// its `with_` methods. The dialect sets both the syntax and the lookup rules,
/// and the document remembers it ([`Document::dialect`]). No dialect expands
/// values, reads files or uses the environment.
///
/// # Examples
///
/// ```
/// use adotenv::Dialect;
///
/// let source = "COLOR=blue#green\n";
///
/// // Portable reads a `#` without whitespace before it as part of the value.
/// let (portable, _) = adotenv::parse_with(source, Dialect::PORTABLE);
/// assert_eq!(portable.get("COLOR"), Some("blue#green"));
///
/// // Node dotenv starts a comment at any `#`.
/// let (node, _) = adotenv::parse_with(source, Dialect::NODE_DOTENV);
/// assert_eq!(node.get("COLOR"), Some("blue"));
/// ```
#[must_use]
pub fn parse_with(source: &str, dialect: Dialect) -> (Document<'_>, Vec<Diagnostic>) {
    parse_with_options(source, dialect, ParseOptions::default())
}

/// Parses `.env` text with the given dialect and opt-in limits, recovering
/// from errors.
///
/// This is [`parse_with`] with [`ParseOptions`]. Input that exceeds a limit
/// returns a document with an invalid item covering the remaining source and
/// a [`DiagnosticCode::LimitExceeded`] diagnostic. Byte offsets still refer to
/// the original source.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_with_options, Dialect, ParseOptions};
///
/// let options = ParseOptions::default().with_max_input_bytes(3);
/// let (document, diagnostics) =
///     parse_with_options("A=1\n", Dialect::PORTABLE, options);
/// assert_eq!(document.source(), "A=1\n");
/// assert_eq!(
///     diagnostics[0].limit().unwrap().kind(),
///     adotenv::LimitKind::InputBytes
/// );
/// ```
#[must_use]
pub fn parse_with_options(
    source: &str,
    dialect: Dialect,
    options: ParseOptions,
) -> (Document<'_>, Vec<Diagnostic>) {
    parse_impl::<true>(source, dialect, options)
}

fn parse_impl<const D: bool>(
    source: &str,
    dialect: Dialect,
    options: ParseOptions,
) -> (Document<'_>, Vec<Diagnostic>) {
    if let Some(limit) = options.max_input_bytes {
        if source.len() > limit {
            let diagnostic = limit_diagnostic(
                LimitKind::InputBytes,
                limit,
                source.len(),
                0..source.len(),
                1,
            );
            return (
                limited_document(source, dialect, diagnostic.clone()),
                vec![diagnostic],
            );
        }
    }
    let mut parser = Parser::<D>::new(source, dialect, None, options);
    parser.run();
    parser.document.has_duplicate_keys = if D {
        parser.has_duplicate_keys
    } else {
        has_duplicate_entries(&parser.document)
    };
    let parser_omitted_error = parser.first_omitted_error.take();
    let diagnostics_limit = options.max_diagnostics.unwrap_or(MAX_DIAGNOSTICS);
    let (mut diagnostics, omitted, merge_omitted_error) = if D
        && dialect.syntax == Syntax::Portable
        && dialect.line_breaks == LineBreaks::Universal
    {
        merge_lone_cr(
            source,
            parser.diagnostics,
            parser.omitted_diagnostics,
            diagnostics_limit,
        )
    } else {
        (parser.diagnostics, parser.omitted_diagnostics, None)
    };
    if omitted > 0 {
        if let Some(limit) = options.max_diagnostics {
            let observed = limit.saturating_add(omitted);
            diagnostics.push(limit_diagnostic(
                LimitKind::Diagnostics,
                limit,
                observed,
                source.len()..source.len(),
                line_breaks(source) + 1,
            ));
        } else {
            let first_omitted_error = match (parser_omitted_error, merge_omitted_error) {
                (Some(parser), Some(merged)) if merged.span.start < parser.span.start => {
                    Some(merged)
                }
                (Some(parser), _) => Some(parser),
                (None, merged) => merged,
            };
            if let Some(error) = first_omitted_error {
                diagnostics.push(error);
            }
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::TooManyDiagnostics,
                source.len()..source.len(),
                line_breaks(source) + 1,
            ));
        }
        diagnostics.sort_by_key(|diagnostic| diagnostic.span.start);
    }
    // Release geometric-growth slack before the tooling model is retained.
    // The 65,536-entry fixture otherwise keeps one full extra item block.
    parser.document.items.shrink_to_fit();
    parser.document.entries.shrink_to_fit();
    (parser.document, diagnostics)
}

fn limit_diagnostic(
    kind: LimitKind,
    limit: usize,
    observed: usize,
    span: Range<usize>,
    line: usize,
) -> Diagnostic {
    Diagnostic::new(DiagnosticCode::LimitExceeded, span, line)
        .with_limit(LimitExceeded::new(kind, limit, observed))
}

fn limited_document(source: &str, dialect: Dialect, diagnostic: Diagnostic) -> Document<'_> {
    let bom = source.starts_with('\u{feff}') && dialect.bom == BomPolicy::Strip;
    let start = if bom { 3 } else { 0 };
    let ending = if source.ends_with("\r\n") {
        LineEnding::CrLf
    } else if source.ends_with('\n') {
        LineEnding::Lf
    } else if source.ends_with('\r') {
        LineEnding::Cr
    } else {
        LineEnding::None
    };
    Document {
        source: Cow::Borrowed(source),
        items: vec![ItemData {
            kind: ItemKind::Invalid,
            span: start..source.len(),
            ending,
            entry: NO_ENTRY,
            comment: None,
            invalid: Some(Box::new(diagnostic)),
        }],
        entries: Vec::new(),
        bom,
        dialect,
        has_duplicate_keys: false,
    }
}

/// Parses `.env` text with the Portable dialect into a map of effective
/// values.
///
/// Use this when only the final values matter. The map holds the same keys
/// and values as [`Document::effective_entries`] of the document that
/// [`parse`] would build: the dialect's duplicate-key and bare-key rules
/// apply, and a bare key is left out unless the dialect reads it as empty.
/// An empty key, which Compose accepts, is also left out; use
/// [`Document::get`] when that declaration matters. No
/// items, spans or diagnostics are kept, so it uses much less memory than a
/// [`Document`]. Warnings are dropped. The map is sorted by key, not by source
/// order.
///
/// See [`parse_values_with`] to choose the dialect.
///
/// # Errors
///
/// Returns the first error-severity [`Diagnostic`] in source order, the same
/// one [`parse_strict`] returns, for example [`DiagnosticCode::InvalidKey`]
/// for `BAD KEY=1`.
///
/// # Examples
///
/// ```
/// use adotenv::DiagnosticCode;
///
/// let values = adotenv::parse_values("PORT=8080\nPORT=9090\nHOST=localhost\nDEBUG\n")?;
/// assert_eq!(values["PORT"], "9090");
/// assert_eq!(values.len(), 2);
///
/// let error = adotenv::parse_values("PORT=8080\nBAD KEY=1\n").unwrap_err();
/// assert_eq!(error.code(), DiagnosticCode::InvalidKey);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse_values(source: &str) -> Result<BTreeMap<String, String>, Diagnostic> {
    parse_values_with_options(source, Dialect::PORTABLE, ParseOptions::default())
}

/// Parses `.env` text with the given dialect into a map of effective values.
///
/// This is [`parse_values`] with an explicit [`Dialect`]. Its peak memory use
/// stays close to the size of the final map. Empty keys are left out, as in
/// [`parse_values`].
///
/// # Errors
///
/// Returns the first error-severity [`Diagnostic`] in source order, the same
/// one [`parse_strict_with`] returns. In dialects that ignore malformed lines
/// ([`MalformedLines::Ignore`], as in Node dotenv), only the policy errors
/// fail: [`DiagnosticCode::ForbiddenBareKey`],
/// [`DiagnosticCode::ForbiddenByteOrderMark`],
/// [`DiagnosticCode::InvalidEscape`] and [`DiagnosticCode::LineTooLong`].
///
/// # Panics
///
/// Panics only if an internal invariant is broken, which cannot happen
/// through this API.
///
/// # Examples
///
/// ```
/// use adotenv::{BareKeys, Dialect};
///
/// let dialect = Dialect::PORTABLE.with_bare_keys(BareKeys::Empty);
/// let values = adotenv::parse_values_with("DEBUG\nPORT=8080\n", dialect)?;
/// assert_eq!(values["DEBUG"], "");
///
/// // Node dotenv skips the malformed line instead of failing.
/// let values = adotenv::parse_values_with("BAD LINE\nPORT=8080\n", Dialect::NODE_DOTENV)?;
/// assert_eq!(values["PORT"], "8080");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse_values_with(
    source: &str,
    dialect: Dialect,
) -> Result<BTreeMap<String, String>, Diagnostic> {
    parse_values_with_options(source, dialect, ParseOptions::default())
}

/// Parses `.env` text into a map of effective values with opt-in limits.
///
/// This is [`parse_values_with`] with [`ParseOptions`]. A limit failure is
/// returned as one [`DiagnosticCode::LimitExceeded`] error.
///
/// # Errors
///
/// Returns the first error-severity diagnostic, including a limit failure.
///
/// # Panics
///
/// Panics only if an internal invariant is broken, which cannot happen
/// through this API.
pub fn parse_values_with_options(
    source: &str,
    dialect: Dialect,
    options: ParseOptions,
) -> Result<BTreeMap<String, String>, Diagnostic> {
    if let Some(limit) = options.max_input_bytes {
        if source.len() > limit {
            return Err(limit_diagnostic(
                LimitKind::InputBytes,
                limit,
                source.len(),
                0..source.len(),
                1,
            ));
        }
    }
    if source.is_empty() {
        return Ok(BTreeMap::new());
    }
    if dialect == Dialect::PORTABLE && options.max_items.is_none() {
        if let Some(values) = parse_simple_portable_values(source) {
            return Ok(values);
        }
    }
    let mut parser = Parser::<false>::new(source, dialect, Some(Projection::new(dialect)), options);
    parser.run();
    let projection = parser
        .projection
        .expect("projected parser has projection state");
    match projection.error {
        Some(error) => Err(error),
        None => Ok(projection.values),
    }
}

/// Fast path for the common Portable `KEY=value` line shape.
///
/// The regular scanner handles quoting, escapes, comments, whitespace,
/// non-POSIX keys, BOMs, CRLF and every diagnostic. This helper accepts only
/// lines that need none of that work, and returns `None` for the rest.
fn parse_simple_portable_values(source: &str) -> Option<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for line in source.split('\n') {
        if line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once('=')?;
        if key.is_empty() || !simple_portable_key(key) {
            return None;
        }
        if value.bytes().any(|byte| {
            matches!(
                byte,
                b' ' | b'\t' | b'\r' | b'\n' | b'\'' | b'"' | b'\\' | b'#' | b'\0'
            )
        }) {
            return None;
        }
        values.insert(key.to_owned(), value.to_owned());
    }
    Some(values)
}

fn simple_portable_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Parses an owned `String` with the Portable dialect, recovering from
/// errors.
///
/// This is [`parse`], except that the string moves into the document without
/// being copied, so the result is a `Document<'static>` that can outlive the
/// place where the text was read, for example a function that reads a file
/// and returns the document. [`Document::into_owned`] converts a borrowing
/// document instead.
///
/// # Examples
///
/// ```
/// fn load() -> adotenv::Document<'static> {
///     // For example, text from `std::fs::read_to_string`.
///     let text = String::from("PORT=8080\n");
///     let (doc, _) = adotenv::parse_owned(text);
///     doc
/// }
///
/// assert_eq!(load().get("PORT"), Some("8080"));
/// ```
#[must_use]
pub fn parse_owned(source: String) -> (Document<'static>, Vec<Diagnostic>) {
    parse_owned_with(source, Dialect::PORTABLE)
}

/// Parses an owned `String` with the given dialect, recovering from errors.
///
/// This is [`parse_owned`] with an explicit [`Dialect`], or [`parse_with`]
/// for an owned string.
///
/// # Examples
///
/// ```
/// use adotenv::Dialect;
///
/// let text = String::from("COLOR=blue#green\n");
/// let (doc, _) = adotenv::parse_owned_with(text, Dialect::NODE_DOTENV);
/// assert_eq!(doc.get("COLOR"), Some("blue"));
/// ```
#[must_use]
pub fn parse_owned_with(source: String, dialect: Dialect) -> (Document<'static>, Vec<Diagnostic>) {
    parse_owned_with_options(source, dialect, ParseOptions::default())
}

/// Parses an owned `String` with opt-in limits.
///
/// This is [`parse_owned_with`] with [`ParseOptions`].
#[must_use]
pub fn parse_owned_with_options(
    source: String,
    dialect: Dialect,
    options: ParseOptions,
) -> (Document<'static>, Vec<Diagnostic>) {
    let (items, entries, bom, has_duplicate_keys, diagnostics) = {
        let (document, diagnostics) = parse_with_options(&source, dialect, options);
        (
            document.items,
            document.entries,
            document.bom,
            document.has_duplicate_keys,
            diagnostics,
        )
    };
    (
        Document {
            source: Cow::Owned(source),
            items,
            entries,
            bom,
            dialect,
            has_duplicate_keys,
        },
        diagnostics,
    )
}

/// Parses `.env` text with the Portable dialect, failing on the first error.
///
/// On success the document is the same one [`parse`] builds. Only diagnostics
/// with [`Severity::Error`] fail; warnings and info diagnostics are not
/// returned, so use [`parse`] when you want to show them.
///
/// See [`parse_strict_with`] to choose the dialect.
///
/// # Errors
///
/// Returns the first error-severity [`Diagnostic`] in source order, for
/// example [`DiagnosticCode::InvalidKey`] for `BAD KEY=1` or
/// [`DiagnosticCode::UnterminatedQuote`] for a quote that is never closed.
///
/// # Examples
///
/// ```
/// use adotenv::DiagnosticCode;
///
/// let doc = adotenv::parse_strict("PORT=8080\nHOST=localhost\n")?;
/// assert_eq!(doc.get("PORT"), Some("8080"));
///
/// let error = adotenv::parse_strict("PORT=8080\nBAD KEY=1\n").unwrap_err();
/// assert_eq!(error.code(), DiagnosticCode::InvalidKey);
/// assert_eq!(error.line(), 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse_strict(source: &str) -> Result<Document<'_>, Diagnostic> {
    parse_strict_with(source, Dialect::PORTABLE)
}

/// Parses `.env` text with the given dialect, failing on the first error.
///
/// This is [`parse_strict`] with an explicit [`Dialect`]. On success the
/// document is the same one [`parse_with`] builds.
///
/// # Errors
///
/// Returns the first error-severity [`Diagnostic`] in source order: a
/// malformed statement or a broken dialect policy. In dialects that ignore
/// malformed lines ([`MalformedLines::Ignore`], as in Node dotenv), malformed
/// statements are skipped with a warning, so only the policy errors fail:
/// [`DiagnosticCode::ForbiddenBareKey`],
/// [`DiagnosticCode::ForbiddenByteOrderMark`],
/// [`DiagnosticCode::InvalidEscape`] and [`DiagnosticCode::LineTooLong`].
///
/// # Examples
///
/// ```
/// use adotenv::{BareKeys, DiagnosticCode, Dialect};
///
/// // Node dotenv skips the malformed line.
/// let doc = adotenv::parse_strict_with("BAD LINE\nPORT=8080\n", Dialect::NODE_DOTENV)?;
/// assert_eq!(doc.get("PORT"), Some("8080"));
///
/// // A broken policy fails in every dialect.
/// let dialect = Dialect::PORTABLE.with_bare_keys(BareKeys::Error);
/// let error = adotenv::parse_strict_with("DEBUG\n", dialect).unwrap_err();
/// assert_eq!(error.code(), DiagnosticCode::ForbiddenBareKey);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn parse_strict_with(source: &str, dialect: Dialect) -> Result<Document<'_>, Diagnostic> {
    parse_strict_with_options(source, dialect, ParseOptions::default())
}

/// Parses `.env` text with the given dialect and opt-in limits, failing on
/// the first error or limit.
///
/// This is [`parse_strict_with`] with [`ParseOptions`].
///
/// # Errors
///
/// Returns the first error-severity diagnostic. A limit failure is a
/// [`DiagnosticCode::LimitExceeded`] diagnostic with
/// [`Diagnostic::limit`](crate::Diagnostic::limit) data.
pub fn parse_strict_with_options(
    source: &str,
    dialect: Dialect,
    options: ParseOptions,
) -> Result<Document<'_>, Diagnostic> {
    let (document, diagnostics) = parse_impl::<false>(source, dialect, options);
    let error = diagnostics
        .into_iter()
        .find(|diagnostic| diagnostic.severity() == Severity::Error)
        .or_else(|| {
            document
                .items
                .iter()
                .filter_map(|item| item.invalid.as_ref())
                .find(|diagnostic| diagnostic.severity() == Severity::Error)
                .map(|diagnostic| diagnostic.as_ref().clone())
        });
    match error {
        Some(error) => Err(error),
        None => Ok(document),
    }
}

/// Recover without allocating discarded warnings or a duplicate-key trie.
pub(crate) fn parse_document(source: &str, dialect: Dialect) -> Document<'_> {
    parse_impl::<false>(source, dialect, ParseOptions::default()).0
}

fn posix_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    !bytes.is_empty()
        && (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

pub(crate) fn valid_key_in(key: &str, dialect: Dialect) -> bool {
    match dialect.keys {
        KeySyntax::Posix => posix_key(key),
        KeySyntax::Portable => {
            !key.is_empty()
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
        }
        KeySyntax::Unicode => {
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| matches!(c, '_' | '.' | '-') || unicode::letter_or_number(c))
        }
        KeySyntax::NonWhitespace => {
            !key.is_empty() && key.chars().all(|c| c != '=' && !c.is_whitespace())
        }
        KeySyntax::Python => {
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| !matches!(c, '=' | '#') && !is_space(c, Whitespace::Python))
        }
        KeySyntax::Docker => {
            !key.is_empty() && !key.bytes().any(|b| matches!(b, b' ' | b'\t' | b'='))
        }
        KeySyntax::Compose => key.chars().all(|c| {
            matches!(c, '_' | '.' | '-' | '[' | ']')
                || unicode::letter_or_number(c)
                || (c != ' ' && is_space(c, Whitespace::Compose))
        }),
    }
}

fn is_space(c: char, mode: Whitespace) -> bool {
    match mode {
        Whitespace::Ascii => matches!(c, ' ' | '\t'),
        Whitespace::Unicode => c.is_whitespace(),
        Whitespace::EcmaScript => (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}',
        Whitespace::Python => c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'),
        Whitespace::Compose => matches!(
            c,
            '\t' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{85}' | '\u{a0}'
        ),
    }
}

fn skip_whitespace(source: &str, start: usize, end: usize, mode: Whitespace) -> usize {
    source[start..end]
        .char_indices()
        .find(|(_, c)| !is_space(*c, mode))
        .map_or(end, |(i, _)| start + i)
}

fn trim_whitespace(source: &str, start: usize, end: usize, mode: Whitespace) -> usize {
    source[start..end]
        .char_indices()
        .rev()
        .find(|(_, c)| !is_space(*c, mode))
        .map_or(start, |(i, c)| start + i + c.len_utf8())
}

#[derive(Clone, Copy)]
struct PhysicalLine {
    content_end: usize,
    end: usize,
    ending: LineEnding,
}

fn physical_line(source: &str, start: usize, breaks: LineBreaks) -> PhysicalLine {
    let bytes = source.as_bytes();
    let mut at = start;
    while at < bytes.len() {
        if bytes[at] == b'\n' {
            let crlf = at > start && bytes[at - 1] == b'\r';
            return PhysicalLine {
                content_end: if crlf { at - 1 } else { at },
                end: at + 1,
                ending: if crlf {
                    LineEnding::CrLf
                } else {
                    LineEnding::Lf
                },
            };
        }
        if bytes[at] == b'\r' && breaks == LineBreaks::Universal {
            let crlf = bytes.get(at + 1) == Some(&b'\n');
            return PhysicalLine {
                content_end: at,
                end: at + if crlf { 2 } else { 1 },
                ending: if crlf {
                    LineEnding::CrLf
                } else {
                    LineEnding::Cr
                },
            };
        }
        at += 1;
    }
    let cr = breaks == LineBreaks::Lf && at > start && bytes[at - 1] == b'\r';
    PhysicalLine {
        content_end: if cr { at - 1 } else { at },
        end: at,
        ending: if cr { LineEnding::Cr } else { LineEnding::None },
    }
}

fn line_breaks(source: &str) -> usize {
    let bytes = source.as_bytes();
    bytes
        .iter()
        .enumerate()
        .filter(|&(i, b)| *b == b'\r' || (*b == b'\n' && (i == 0 || bytes[i - 1] != b'\r')))
        .count()
}

fn has_duplicate_entries(document: &Document<'_>) -> bool {
    let mut keys: Vec<&str> = document
        .entries()
        .filter(|entry| entry.participates_in_lookup() && !entry.key().is_empty())
        .map(crate::Entry::key)
        .collect();
    keys.sort_unstable();
    keys.windows(2).any(|pair| pair[0] == pair[1])
}

struct Parser<'src, const D: bool> {
    source: &'src str,
    document: Document<'src>,
    diagnostics: Vec<Diagnostic>,
    omitted_diagnostics: usize,
    first_omitted_error: Option<Diagnostic>,
    keys: KeyIndex,
    first_key: Option<usize>,
    has_duplicate_keys: bool,
    failed_quotes: [Option<usize>; 3],
    line: usize,
    options: ParseOptions,
    item_count: usize,
    projection: Option<Projection>,
    /// The physical line found by the latest scan, and where that scan began.
    /// It answers every lookup from that start up to the line's content end.
    line_cache: Cell<(usize, PhysicalLine)>,
    /// The latest LS/PS anchor scan: where it began and the anchor it found,
    /// or the physical line's content end. Node uses it for regex anchors.
    anchor_cache: Cell<(usize, usize)>,
    /// Bytes examined while locating line ends and LS/PS anchors.
    #[cfg(test)]
    scanned: Cell<usize>,
}

/// Count the bytes that line-end and anchor scans examine while parsing.
#[cfg(test)]
pub(crate) fn scanned_bytes(source: &str, dialect: Dialect) -> usize {
    let mut parser = Parser::<true>::new(source, dialect, None, ParseOptions::default());
    parser.run();
    parser.scanned.get()
}

struct Projection {
    values: BTreeMap<String, String>,
    seen: Option<BTreeSet<String>>,
    error: Option<Diagnostic>,
}

impl Projection {
    fn new(dialect: Dialect) -> Self {
        Self {
            values: BTreeMap::new(),
            seen: (dialect.duplicates == Duplicates::First).then(BTreeSet::new),
            error: None,
        }
    }

    fn record(&mut self, key: &str, value: Option<String>) {
        if let Some(seen) = &mut self.seen {
            if seen.insert(key.to_string()) {
                if let Some(value) = value {
                    self.values.insert(key.to_string(), value);
                }
            }
        } else if let Some(value) = value {
            self.values.insert(key.to_string(), value);
        } else {
            self.values.remove(key);
        }
    }
}

#[derive(Clone, Copy)]
struct ProjectedEntry<'a> {
    source: &'a str,
    dialect: Dialect,
    key: &'a str,
    participates: bool,
    invalid: Option<&'a Diagnostic>,
    tokens: &'a EntryTokens,
}

fn project_entry(
    projection: &mut Projection,
    input: ProjectedEntry<'_>,
    decoded: &mut Option<String>,
) {
    if let Some(error) = input.invalid {
        if error.severity() == Severity::Error && projection.error.is_none() {
            projection.error = Some(error.clone());
        }
    }
    if input.participates && !input.key.is_empty() {
        let value = match &input.tokens.body {
            Some(body) => Some(
                decoded
                    .take()
                    .unwrap_or_else(|| input.source[body.clone()].to_string()),
            ),
            None if input.dialect.bare_keys == BareKeys::Empty => Some(String::new()),
            None => None,
        };
        projection.record(input.key, value);
    }
}

impl<'src, const D: bool> Parser<'src, D> {
    fn new(
        source: &'src str,
        dialect: Dialect,
        projection: Option<Projection>,
        options: ParseOptions,
    ) -> Self {
        Self {
            source,
            document: Document {
                source: Cow::Borrowed(source),
                items: Vec::new(),
                entries: Vec::new(),
                bom: false,
                dialect,
                has_duplicate_keys: false,
            },
            diagnostics: Vec::new(),
            omitted_diagnostics: 0,
            first_omitted_error: None,
            keys: KeyIndex::default(),
            first_key: None,
            has_duplicate_keys: false,
            failed_quotes: [None; 3],
            line: 1,
            options,
            item_count: 0,
            projection,
            line_cache: Cell::new((
                usize::MAX,
                PhysicalLine {
                    content_end: 0,
                    end: 0,
                    ending: LineEnding::None,
                },
            )),
            anchor_cache: Cell::new((usize::MAX, 0)),
            #[cfg(test)]
            scanned: Cell::new(0),
        }
    }

    fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        let limit = self.options.max_diagnostics.unwrap_or(MAX_DIAGNOSTICS);
        if self.diagnostics.len() < limit {
            self.diagnostics.push(diagnostic);
        } else {
            self.omitted_diagnostics = self.omitted_diagnostics.saturating_add(1);
            if diagnostic.severity() == Severity::Error && self.first_omitted_error.is_none() {
                self.first_omitted_error = Some(diagnostic);
            }
        }
    }

    fn push_limit(
        &mut self,
        kind: LimitKind,
        limit: usize,
        observed: usize,
        span: Range<usize>,
        line: usize,
    ) {
        let diagnostic = limit_diagnostic(kind, limit, observed, span.clone(), line);
        if let Some(projection) = &mut self.projection {
            if projection.error.is_none() {
                projection.error = Some(diagnostic);
            }
            return;
        }
        self.diagnostics.push(diagnostic.clone());
        self.document.items.push(ItemData {
            kind: ItemKind::Invalid,
            span,
            ending: LineEnding::None,
            entry: NO_ENTRY,
            comment: None,
            invalid: Some(Box::new(diagnostic)),
        });
    }

    fn run(&mut self) {
        let source = self.source;
        let dialect = self.dialect();
        let initial_bom = source.starts_with('\u{feff}');
        // Scanner limits count original bytes, including a BOM that would otherwise
        // be stripped. Resource rejection precedes BOM interpretation on that line.
        let rejected_bom_line = if initial_bom {
            let first = physical_line(source, 0, dialect.line_breaks);
            let bytes = first.end
                - usize::from(source.as_bytes().get(first.end.wrapping_sub(1)) == Some(&b'\n'));
            dialect
                .line_byte_limit
                .filter(|limit| bytes > *limit)
                .map(|_| first)
        } else {
            None
        };
        let bom = initial_bom && dialect.bom == BomPolicy::Strip && rejected_bom_line.is_none();
        self.document.bom = bom;
        let mut at = if bom { 3 } else { 0 };
        if D && bom {
            self.diagnostics
                .push(Diagnostic::new(DiagnosticCode::ByteOrderMark, 0..3, 1));
        }
        if let Some(first) = rejected_bom_line {
            self.invalid(
                0,
                first,
                DiagnosticCode::LineTooLong,
                0..first.content_end,
                1,
            );
            self.line += line_breaks(&source[..first.end]);
            at = first.end;
        } else if initial_bom && dialect.bom == BomPolicy::Error {
            self.invalid(
                0,
                PhysicalLine {
                    content_end: 3,
                    end: 3,
                    ending: LineEnding::None,
                },
                DiagnosticCode::ForbiddenByteOrderMark,
                0..3,
                1,
            );
            at = 3;
        }
        while at < source.len() {
            let first = self.physical(at);
            let end = if self.over_limit(at, first) {
                self.invalid(
                    at,
                    first,
                    DiagnosticCode::LineTooLong,
                    at..first.content_end,
                    self.line,
                )
            } else {
                match dialect.syntax {
                    Syntax::Portable => self.statement(at, first),
                    Syntax::Node => node::statement(self, at, first),
                    Syntax::Python => python::statement(self, at, first),
                    Syntax::Compose => compose::statement(self, at, first),
                    Syntax::Docker => docker::statement(self, at, first),
                }
            };
            debug_assert!(end > at && end <= source.len());
            self.line += line_breaks(&source[at..end]);
            at = end;
            self.item_count = self.item_count.saturating_add(1);
            if let Some(limit) = self.options.max_items {
                let observed = if self.projection.is_some() {
                    self.item_count
                } else {
                    self.document.items.len()
                };
                if observed > limit {
                    self.push_limit(
                        LimitKind::Items,
                        limit,
                        observed,
                        at..source.len(),
                        self.line,
                    );
                    break;
                }
            }
        }
    }
}

impl<const D: bool> Parser<'_, D> {
    fn dialect(&self) -> Dialect {
        self.document.dialect
    }
    /// Whitespace that statement scanning skips before a statement and trims
    /// after an unquoted value. Compose trims more than its key scanner skips.
    fn statement_whitespace(&self) -> Whitespace {
        let dialect = self.dialect();
        if dialect.syntax == Syntax::Compose && dialect.whitespace == Whitespace::Compose {
            Whitespace::Unicode
        } else {
            dialect.whitespace
        }
    }
    /// The physical line containing `at`, from `at` to its end.
    ///
    /// Compose and Node can place many statements on one physical line. A
    /// scan from any position up to the line's content end finds the same
    /// line end, so reuse the latest result instead of scanning again. This
    /// keeps parsing linear in the input length.
    fn physical(&self, at: usize) -> PhysicalLine {
        let (from, line) = self.line_cache.get();
        if from <= at && at <= line.content_end {
            return line;
        }
        let line = physical_line(self.source, at, self.dialect().line_breaks);
        self.record_scan(line.end - at);
        self.line_cache.set((at, line));
        line
    }
    /// The first LS/PS anchor at or after `at` on its physical line, or the
    /// line's content end. Like [`Self::physical`], it reuses the latest scan.
    fn anchor(&self, at: usize) -> usize {
        let (from, anchor) = self.anchor_cache.get();
        if from <= at && at <= anchor {
            return anchor;
        }
        let physical = self.physical(at);
        let anchor = self.source[at..physical.content_end]
            .find(['\u{2028}', '\u{2029}'])
            .map_or(physical.content_end, |relative| at + relative);
        self.record_scan(anchor - at);
        self.anchor_cache.set((at, anchor));
        anchor
    }
    #[cfg(test)]
    fn record_scan(&self, bytes: usize) {
        self.scanned.set(self.scanned.get() + bytes);
    }
    #[cfg(not(test))]
    #[allow(clippy::unused_self)]
    fn record_scan(&self, _bytes: usize) {}
    fn over_limit(&self, start: usize, physical: PhysicalLine) -> bool {
        self.dialect().line_byte_limit.is_some_and(|limit| {
            let len = physical.end
                - start
                - usize::from(
                    self.source.as_bytes().get(physical.end.wrapping_sub(1)) == Some(&b'\n'),
                );
            len > limit
        })
    }
    fn limit_error(&self, start: usize, end: usize) -> Option<(Diagnostic, PhysicalLine)> {
        self.dialect().line_byte_limit?;
        let mut cursor = start;
        let mut line = self.line;
        while cursor < end {
            let physical = self.physical(cursor);
            if self.over_limit(cursor, physical) {
                return Some((
                    Diagnostic::new(
                        DiagnosticCode::LineTooLong,
                        cursor..physical.content_end,
                        line,
                    ),
                    physical,
                ));
            }
            if physical.end >= end {
                // The rest of a shared line may hold many more statements.
                // Its line breaks matter only for a later line of this item.
                break;
            }
            let scanned = &self.source[cursor..physical.end];
            self.record_scan(scanned.len());
            line += line_breaks(scanned);
            cursor = physical.end;
        }
        None
    }
    fn plain_item(
        &mut self,
        kind: ItemKind,
        span: Range<usize>,
        ending: LineEnding,
        comment: Option<Range<usize>>,
    ) -> usize {
        let end = span.end;
        if let Some((diagnostic, recovery)) = self.limit_error(span.start, end) {
            let last = if recovery.end >= end {
                recovery
            } else {
                PhysicalLine {
                    content_end: end - ending.as_str().len(),
                    end,
                    ending,
                }
            };
            return self.invalid(
                span.start,
                last,
                DiagnosticCode::LineTooLong,
                diagnostic.span,
                diagnostic.line,
            );
        }
        if self.projection.is_some() {
            return end;
        }
        self.document.items.push(ItemData {
            kind,
            span,
            ending,
            entry: NO_ENTRY,
            comment,
            invalid: None,
        });
        end
    }
    fn invalid(
        &mut self,
        start: usize,
        mut physical: PhysicalLine,
        code: DiagnosticCode,
        span: Range<usize>,
        line: usize,
    ) -> usize {
        let limit_error = (code != DiagnosticCode::LineTooLong)
            .then(|| self.limit_error(start, physical.end))
            .flatten();
        let (code, span, line) = if let Some((diagnostic, recovery)) = limit_error {
            if recovery.end > physical.end {
                physical = recovery;
            }
            (
                DiagnosticCode::LineTooLong,
                diagnostic.span,
                diagnostic.line,
            )
        } else {
            (code, span, line)
        };
        let explicit = matches!(
            code,
            DiagnosticCode::ForbiddenBareKey
                | DiagnosticCode::ForbiddenByteOrderMark
                | DiagnosticCode::InvalidEscape
                | DiagnosticCode::LineTooLong
        );
        let ignored = code == DiagnosticCode::IgnoredStatement
            || (!explicit && self.dialect().malformed_lines == MalformedLines::Ignore);
        let diagnostic = Diagnostic::new(
            if ignored {
                DiagnosticCode::IgnoredStatement
            } else {
                code
            },
            span,
            line,
        );
        if let Some(projection) = &mut self.projection {
            if diagnostic.severity() == Severity::Error && projection.error.is_none() {
                projection.error = Some(diagnostic);
            }
            return physical.end;
        }
        if D {
            self.push_diagnostic(diagnostic.clone());
        }
        self.document.items.push(ItemData {
            kind: if ignored {
                ItemKind::Ignored
            } else {
                ItemKind::Invalid
            },
            span: start..physical.end,
            ending: physical.ending,
            entry: NO_ENTRY,
            comment: None,
            invalid: Some(Box::new(diagnostic)),
        });
        physical.end
    }

    fn scan_key(
        &self,
        mut at: usize,
        first: PhysicalLine,
    ) -> Option<(Range<usize>, PhysicalLine, bool)> {
        let dialect = self.dialect();
        let start = at;
        let mut physical = first;
        let quoted = dialect.quoted_keys && self.source.as_bytes().get(at) == Some(&b'\'');
        if quoted {
            at += self.source[at + 1..].find('\'')? + 2;
            physical = self.physical(at);
        } else {
            while at < first.content_end {
                let c = self.source[at..].chars().next().expect("key character");
                let colon = c == ':'
                    && (dialect.colon == ColonSeparator::Any
                        || (dialect.colon == ColonSeparator::Whitespace
                            && self.source[at + 1..first.content_end]
                                .chars()
                                .next()
                                .is_some_and(|next| is_space(next, dialect.whitespace))));
                let mut buffer = [0; 4];
                let key_space = is_space(c, dialect.whitespace)
                    && valid_key_in(c.encode_utf8(&mut buffer), dialect);
                if c == '=' || colon || (is_space(c, dialect.whitespace) && !key_space) {
                    break;
                }
                at += c.len_utf8();
            }
        }
        Some((start..at, physical, quoted))
    }

    fn statement(&mut self, start: usize, first: PhysicalLine) -> usize {
        let dialect = self.dialect();
        let mut at = skip_whitespace(self.source, start, first.content_end, dialect.whitespace);
        let indent = start..at;
        if at == first.content_end {
            return self.plain_item(ItemKind::Blank, start..first.end, first.ending, None);
        }
        if self.source.as_bytes()[at] == b'#' {
            return self.plain_item(
                ItemKind::Comment,
                start..first.end,
                first.ending,
                Some(at + 1..first.content_end),
            );
        }
        let export = if dialect.export
            && self.source[at..first.content_end].starts_with("export")
            && self.source[at + 6..first.content_end]
                .chars()
                .next()
                .is_some_and(|c| is_space(c, dialect.whitespace))
        {
            let range = at..at + 6;
            at += 6;
            Some(range)
        } else {
            None
        };
        let after_export = at;
        if export.is_some() {
            at = skip_whitespace(self.source, at, first.content_end, dialect.whitespace);
        }
        let key_start = at;
        let Some((key, physical, quoted_key)) = self.scan_key(at, first) else {
            return self.invalid(
                start,
                first,
                DiagnosticCode::InvalidKey,
                at..first.content_end,
                self.line + line_breaks(&self.source[start..at]),
            );
        };
        at = key.end;
        let key_end = at;
        at = skip_whitespace(self.source, at, physical.content_end, dialect.whitespace);
        let key_ok = if quoted_key {
            key.len() > 2
        } else {
            valid_key_in(&self.source[key.clone()], dialect)
        };
        let separator = at < physical.content_end
            && (self.source.as_bytes()[at] == b'='
                || (self.source.as_bytes()[at] == b':'
                    && (dialect.colon == ColonSeparator::Any
                        || (dialect.colon == ColonSeparator::Whitespace
                            && self.source[at + 1..physical.content_end]
                                .chars()
                                .next()
                                .is_some_and(|c| is_space(c, dialect.whitespace))))));
        if !key_ok
            || (at < physical.content_end && !separator && self.source.as_bytes()[at] != b'#')
        {
            let end = self.source[key_start..physical.content_end]
                .find('=')
                .map_or(physical.content_end, |i| key_start + i);
            return self.invalid(
                start,
                physical,
                DiagnosticCode::InvalidKey,
                key_start..trim_whitespace(self.source, key_start, end, dialect.whitespace),
                self.line + line_breaks(&self.source[start..key_start]),
            );
        }
        let mut tokens = EntryTokens {
            indent,
            export,
            space_after_export: after_export..key_start,
            key,
            space_before_eq: key_end..at,
            eq: None,
            space_after_eq: at..at,
            value: None,
            body: None,
            quoted_tail: None,
            trailing_space: at..at,
            inline_comment: None,
        };
        if !separator {
            tokens.inline_comment = (at < physical.content_end).then_some(at..physical.content_end);
            return self.entry(start, physical, tokens, Quote::None);
        }
        tokens.eq = Some(at..at + 1);
        at += 1;
        tokens.space_after_eq =
            at..skip_whitespace(self.source, at, physical.content_end, dialect.whitespace);
        self.parse_value(start, physical, tokens)
    }

    fn parse_value(&mut self, start: usize, first: PhysicalLine, mut tokens: EntryTokens) -> usize {
        if self.dialect().raw_values {
            let at = tokens
                .eq
                .as_ref()
                .map_or(tokens.space_after_eq.start, |eq| eq.end);
            tokens.space_after_eq = at..at;
            tokens.value = Some(at..first.content_end);
            tokens.body = tokens.value.clone();
            tokens.trailing_space = first.content_end..first.content_end;
            return self.entry(start, first, tokens, Quote::None);
        }
        let at = tokens.space_after_eq.end;
        let quote = match self
            .source
            .as_bytes()
            .get(at)
            .filter(|_| at < first.content_end)
        {
            Some(b'\'') => Quote::Single,
            Some(b'"') => Quote::Double,
            Some(b'`') if self.dialect().backticks => Quote::Backtick,
            _ => Quote::None,
        };
        if quote == Quote::None {
            self.unquoted_statement(start, first, tokens)
        } else {
            self.quoted_statement(start, first, tokens, quote)
        }
    }

    fn comment_at(&self, at: usize, body_start: usize) -> bool {
        if self.source.as_bytes()[at] != b'#' {
            return false;
        }
        match self.dialect().inline_comments {
            InlineComments::None => false,
            InlineComments::AnyHash => true,
            InlineComments::Space => at > body_start && self.source.as_bytes()[at - 1] == b' ',
            InlineComments::Whitespace | InlineComments::Python => self.source[..at]
                .chars()
                .next_back()
                .is_some_and(|c| is_space(c, self.dialect().whitespace)),
        }
    }
    fn bare_comment_at(&self, at: usize, key_end: usize) -> bool {
        if self.dialect().inline_comments == InlineComments::Python {
            self.source.as_bytes()[at] == b'#'
        } else {
            self.comment_at(at, key_end)
        }
    }
    fn quoted_comment_at(&self, at: usize, close: usize) -> bool {
        if self.source.as_bytes()[at] != b'#'
            || self.dialect().inline_comments == InlineComments::None
        {
            return false;
        }
        if at == close + 1 {
            self.dialect().quoted_comments
        } else {
            self.comment_at(at, close + 1)
        }
    }
    fn unquoted_statement(
        &mut self,
        start: usize,
        first: PhysicalLine,
        mut tokens: EntryTokens,
    ) -> usize {
        let at = tokens.space_after_eq.end;
        let comment = self.source[at..first.content_end]
            .char_indices()
            .find(|(i, c)| *c == '#' && self.comment_at(at + i, at))
            .map_or(first.content_end, |(i, _)| at + i);
        let first = if self.dialect().syntax == Syntax::Node && comment < first.content_end {
            node::anchored_line(self, comment)
        } else {
            first
        };
        let end = trim_whitespace(self.source, at, comment, self.statement_whitespace());
        tokens.value = Some(at..end);
        tokens.body = Some(at..end);
        tokens.trailing_space = end..comment;
        tokens.inline_comment = (comment < first.content_end).then_some(comment..first.content_end);
        self.entry(start, first, tokens, Quote::None)
    }

    fn literal_fallback(
        &mut self,
        start: usize,
        first: PhysicalLine,
        mut tokens: EntryTokens,
    ) -> usize {
        let mut physical = first;
        if self.dialect().syntax == Syntax::Node {
            let mut at = tokens
                .eq
                .as_ref()
                .map_or(tokens.space_after_eq.start, |eq| eq.end);
            if tokens
                .eq
                .as_ref()
                .is_some_and(|eq| self.source.as_bytes()[eq.start] == b':')
                && self.dialect().colon == ColonSeparator::Whitespace
            {
                at += self.source[at..].chars().next().map_or(0, char::len_utf8);
                if self.source.as_bytes().get(at.wrapping_sub(1)) == Some(&b'\r')
                    && self.source.as_bytes().get(at) == Some(&b'\n')
                {
                    at += 1;
                }
            }
            physical = self.physical(at);
            tokens.space_after_eq.end = skip_whitespace(
                self.source,
                at,
                physical.content_end,
                self.dialect().whitespace,
            );
        }
        tokens.quoted_tail = None;
        self.unquoted_statement(start, physical, tokens)
    }

    fn quoted_statement(
        &mut self,
        start: usize,
        first: PhysicalLine,
        mut tokens: EntryTokens,
        quote: Quote,
    ) -> usize {
        let at = tokens.space_after_eq.end;
        let Some(close) = self.closing_quote(at, quote) else {
            if self.dialect().quoted_tail == QuotedTail::LiteralFallback {
                return self.literal_fallback(start, first, tokens);
            }
            return self.invalid(
                start,
                first,
                DiagnosticCode::UnterminatedQuote,
                at..at + 1,
                self.line + line_breaks(&self.source[start..at]),
            );
        };
        let last = if self.dialect().syntax == Syntax::Node {
            node::anchored_line(self, close + 1)
        } else {
            self.physical(close + 1)
        };
        tokens.value = Some(at..close + 1);
        tokens.body = Some(at + 1..close);
        if self.dialect().quoted_tail == QuotedTail::NextStatement {
            // Another statement may begin right after the closing quote. When
            // the rest of the line would only be a blank or comment item, it
            // stays with this entry, as trailing trivia does in other presets.
            let tail = skip_whitespace(
                self.source,
                close + 1,
                last.content_end,
                self.statement_whitespace(),
            );
            if tail == last.content_end || self.source.as_bytes()[tail] == b'#' {
                tokens.trailing_space = close + 1..tail;
                tokens.inline_comment = (tail < last.content_end).then_some(tail..last.content_end);
                return self.entry(start, last, tokens, quote);
            }
            tokens.trailing_space = close + 1..close + 1;
            let boundary = PhysicalLine {
                content_end: close + 1,
                end: close + 1,
                ending: LineEnding::None,
            };
            return self.entry(start, boundary, tokens, quote);
        }
        let tail = skip_whitespace(
            self.source,
            close + 1,
            last.content_end,
            self.dialect().whitespace,
        );
        let comment = tail < last.content_end && self.quoted_comment_at(tail, close);
        if tail < last.content_end && !comment {
            match self.dialect().quoted_tail {
                QuotedTail::LiteralFallback => return self.literal_fallback(start, first, tokens),
                QuotedTail::Error => {
                    return self.invalid(
                        start,
                        last,
                        DiagnosticCode::TrailingText,
                        tail..last.content_end,
                        self.line + line_breaks(&self.source[start..tail]),
                    )
                }
                QuotedTail::Skip => {
                    return self.invalid(
                        start,
                        last,
                        DiagnosticCode::IgnoredStatement,
                        tail..last.content_end,
                        self.line + line_breaks(&self.source[start..tail]),
                    );
                }
                QuotedTail::Ignore | QuotedTail::Append => {
                    let comment = self.source[tail..last.content_end]
                        .char_indices()
                        .find(|(index, ch)| *ch == '#' && self.comment_at(tail + index, close + 1))
                        .map_or(last.content_end, |(index, _)| tail + index);
                    let end =
                        trim_whitespace(self.source, close + 1, comment, self.dialect().whitespace);
                    tokens.quoted_tail = Some(close + 1..end);
                    tokens.trailing_space = end..comment;
                    tokens.inline_comment =
                        (comment < last.content_end).then_some(comment..last.content_end);
                }
                QuotedTail::NextStatement => unreachable!(),
            }
        } else {
            tokens.trailing_space = close + 1..tail;
            tokens.inline_comment = comment.then_some(tail..last.content_end);
        }
        self.entry(start, last, tokens, quote)
    }

    fn node_tail_valid(&self, close: usize) -> bool {
        let dialect = self.dialect();
        for (relative, c) in self.source[close + 1..].char_indices() {
            if matches!(c, '\n' | '\u{2028}' | '\u{2029}')
                || (c == '\r' && dialect.line_breaks == LineBreaks::Universal)
            {
                return true;
            }
            if !is_space(c, dialect.whitespace) {
                let at = close + 1 + relative;
                return self.quoted_comment_at(at, close);
            }
        }
        true
    }

    fn closing_quote(&mut self, open: usize, quote: Quote) -> Option<usize> {
        let dialect = self.dialect();
        let kind = match quote {
            Quote::Single => 0,
            Quote::Double => 1,
            _ => 2,
        };
        let multiline = match quote {
            Quote::Single => dialect.multiline.single,
            Quote::Double => dialect.multiline.double,
            _ => dialect.multiline.backtick,
        };
        if multiline && self.failed_quotes[kind].is_some_and(|failed| open >= failed) {
            return None;
        }
        let delimiter = match quote {
            Quote::Single => b'\'',
            Quote::Double => b'"',
            _ => b'`',
        };
        let bytes = self.source.as_bytes();
        let limit = if multiline {
            bytes.len()
        } else {
            self.physical(open).content_end
        };
        let mut at = open + 1;
        let mut node_fallback = None;
        while at < limit {
            if bytes[at] == delimiter {
                if dialect.syntax != Syntax::Node {
                    return Some(at);
                }
                let tail_valid = self.node_tail_valid(at);
                if bytes[at - 1] != b'\\' {
                    return if tail_valid || dialect.quoted_tail != QuotedTail::LiteralFallback {
                        Some(at)
                    } else {
                        node_fallback.or(Some(at))
                    };
                }
                if tail_valid {
                    node_fallback = Some(at);
                }
            }
            let single_escape = quote == Quote::Single
                && match dialect.single_escapes {
                    SingleEscapes::None => false,
                    SingleEscapes::Quote => {
                        bytes.get(at + 1) == Some(&b'\'')
                            || (dialect.syntax == Syntax::Compose
                                && bytes.get(at + 1) == Some(&b'\\'))
                    }
                    SingleEscapes::QuoteAndBackslash => true,
                };
            if dialect.syntax != Syntax::Node
                && bytes[at] == b'\\'
                && at + 1 < limit
                && (quote == Quote::Double
                    || quote == Quote::Backtick
                    || single_escape
                    || dialect.syntax == Syntax::Python)
            {
                at += 2;
            } else {
                at += 1;
            }
        }
        if node_fallback.is_some() {
            return node_fallback;
        }
        if multiline {
            self.failed_quotes[kind] = Some(open);
        }
        None
    }

    fn duplicate_key(&mut self, key: &str, span: Range<usize>, index: usize, line: usize) {
        if let Some(first) = self.first_key {
            if self.keys.nodes.is_empty() {
                let entry = &self.document.entries[first];
                let range = if entry.key_quoted {
                    entry.tokens.key.start + 1..entry.tokens.key.end - 1
                } else {
                    entry.tokens.key.clone()
                };
                self.keys.insert(&self.source[range], first);
            }
            if let Some(previous) = self.keys.insert(key, index) {
                if !key.is_empty() {
                    self.has_duplicate_keys = true;
                }
                if D {
                    let entry = &self.document.entries[previous];
                    let mut diagnostic = Diagnostic::new(DiagnosticCode::DuplicateKey, span, line);
                    diagnostic.secondary = Some(if entry.key_quoted {
                        entry.tokens.key.start + 1..entry.tokens.key.end - 1
                    } else {
                        entry.tokens.key.clone()
                    });
                    self.push_diagnostic(diagnostic);
                }
            }
        } else {
            self.first_key = Some(index);
        }
    }

    fn entry_limit(&mut self, start: usize, last: PhysicalLine) -> Option<usize> {
        let (diagnostic, recovery) = self.limit_error(start, last.end)?;
        let last = if recovery.end > last.end {
            recovery
        } else {
            last
        };
        Some(self.invalid(
            start,
            last,
            DiagnosticCode::LineTooLong,
            diagnostic.span,
            diagnostic.line,
        ))
    }

    // Return the actual consumed boundary, including a rejected original line.
    fn entry(
        &mut self,
        start: usize,
        last: PhysicalLine,
        tokens: EntryTokens,
        quote: Quote,
    ) -> usize {
        if let Some(end) = self.entry_limit(start, last) {
            return end;
        }
        let dialect = self.dialect();
        let index = self.document.entries.len();
        let key_quoted = dialect.quoted_keys
            && tokens.key.len() >= 2
            && self.source.as_bytes()[tokens.key.start] == b'\''
            && self.source.as_bytes()[tokens.key.end - 1] == b'\'';
        let key_span = if key_quoted {
            tokens.key.start + 1..tokens.key.end - 1
        } else {
            tokens.key.clone()
        };
        let source = self.source;
        let key = &source[key_span.clone()];
        let line = self.line + line_breaks(&source[start..tokens.key.start]);
        let participates = !(dialect.node_object_keys && key == "__proto__"
            || tokens.body.is_none() && dialect.bare_keys == BareKeys::Ignore);
        if D && dialect.syntax == Syntax::Portable && !posix_key(key) {
            self.push_diagnostic(Diagnostic::new(
                DiagnosticCode::NonPosixKey,
                key_span.clone(),
                line,
            ));
        }
        if D && participates {
            self.duplicate_key(key, key_span.clone(), index, line);
        }
        let mut invalid = None;
        let (state, mut decoded, map) = match &tokens.body {
            None => {
                if dialect.bare_keys == BareKeys::Error {
                    let diagnostic =
                        Diagnostic::new(DiagnosticCode::ForbiddenBareKey, key_span, line);
                    if D {
                        self.push_diagnostic(diagnostic.clone());
                    }
                    invalid = Some(diagnostic);
                } else if D && dialect.syntax == Syntax::Portable {
                    self.push_diagnostic(Diagnostic::new(DiagnosticCode::BareKey, key_span, line));
                }
                (AssignmentState::Bare, None, Vec::new())
            }
            Some(body) => {
                let (decoded, map, error) =
                    self.decode(body.clone(), quote, tokens.quoted_tail.as_ref(), start);
                invalid = error;
                let empty = decoded.as_ref().map_or(body.is_empty(), String::is_empty);
                (
                    if empty {
                        AssignmentState::Empty
                    } else {
                        AssignmentState::Value
                    },
                    decoded,
                    map,
                )
            }
        };
        if let Some(projection) = self.projection.as_mut() {
            project_entry(
                projection,
                ProjectedEntry {
                    source,
                    dialect,
                    key,
                    participates,
                    invalid: invalid.as_ref(),
                    tokens: &tokens,
                },
                &mut decoded,
            );
            return last.end;
        }
        let item = self.document.items.len();
        self.document.entries.push(EntryData {
            item,
            line,
            tokens,
            quote,
            state,
            key_quoted,
            participates,
            decoded: decoded.map(String::into_boxed_str),
            map: (!map.is_empty()).then(|| map.into_boxed_slice()),
        });
        self.document.items.push(ItemData {
            kind: ItemKind::Entry,
            span: start..last.end,
            ending: last.ending,
            entry: index,
            comment: None,
            invalid: invalid.map(Box::from),
        });
        last.end
    }
}

// A sparse byte trie bounds sibling searches by 256, supports arbitrary UTF-8,
// stores no key copies, and is completely absent from diagnostic-free parsing.
const ABSENT: usize = usize::MAX;
struct KeyNode {
    byte: u8,
    child: usize,
    sibling: usize,
    entry: usize,
}
#[derive(Default)]
struct KeyIndex {
    nodes: Vec<KeyNode>,
}
impl KeyIndex {
    fn insert(&mut self, key: &str, entry: usize) -> Option<usize> {
        if self.nodes.is_empty() {
            self.nodes.push(KeyNode {
                byte: 0,
                child: ABSENT,
                sibling: ABSENT,
                entry: ABSENT,
            });
        }
        let mut parent = 0;
        for byte in key.bytes() {
            let mut child = self.nodes[parent].child;
            while child != ABSENT && self.nodes[child].byte != byte {
                child = self.nodes[child].sibling;
            }
            if child == ABSENT {
                child = self.nodes.len();
                let sibling = self.nodes[parent].child;
                self.nodes.push(KeyNode {
                    byte,
                    child: ABSENT,
                    sibling,
                    entry: ABSENT,
                });
                self.nodes[parent].child = child;
            }
            parent = child;
        }
        let previous = self.nodes[parent].entry;
        if previous == ABSENT {
            self.nodes[parent].entry = entry;
            None
        } else {
            Some(previous)
        }
    }
}

// Both streams are already source-ordered; no sort or line index is needed.
fn merge_lone_cr(
    source: &str,
    diagnostics: Vec<Diagnostic>,
    mut omitted: usize,
    limit: usize,
) -> (Vec<Diagnostic>, usize, Option<Diagnostic>) {
    let bytes = source.as_bytes();
    let mut carriage_returns = Vec::new();
    let mut line = 1;
    for (at, byte) in bytes.iter().enumerate() {
        if *byte == b'\r' {
            if bytes.get(at + 1) != Some(&b'\n') {
                if carriage_returns.len() < limit {
                    carriage_returns.push(Diagnostic::new(
                        DiagnosticCode::LoneCarriageReturn,
                        at..at + 1,
                        line,
                    ));
                } else {
                    omitted = omitted.saturating_add(1);
                }
            }
            line += 1;
        } else if *byte == b'\n' && (at == 0 || bytes[at - 1] != b'\r') {
            line += 1;
        }
    }
    if carriage_returns.is_empty() {
        return (diagnostics, omitted, None);
    }
    let capacity = diagnostics
        .len()
        .saturating_add(carriage_returns.len())
        .min(limit);
    let mut output = Vec::with_capacity(capacity);
    let mut ordinary = diagnostics.into_iter().peekable();
    let mut carriage = carriage_returns.into_iter().peekable();
    while let (Some(a), Some(b)) = (ordinary.peek(), carriage.peek()) {
        if output.len() == limit {
            break;
        }
        if a.span.start <= b.span.start {
            output.push(ordinary.next().expect("peeked diagnostic"));
        } else {
            output.push(carriage.next().expect("peeked carriage return"));
        }
    }
    let mut omitted_error = None;
    if output.len() == limit {
        for diagnostic in ordinary.chain(carriage) {
            omitted = omitted.saturating_add(1);
            if diagnostic.severity() == Severity::Error && omitted_error.is_none() {
                omitted_error = Some(diagnostic);
            }
        }
    } else {
        for diagnostic in ordinary.chain(carriage) {
            if output.len() < limit {
                output.push(diagnostic);
            } else {
                omitted = omitted.saturating_add(1);
                if diagnostic.severity() == Severity::Error && omitted_error.is_none() {
                    omitted_error = Some(diagnostic);
                }
            }
        }
    }
    (output, omitted, omitted_error)
}
