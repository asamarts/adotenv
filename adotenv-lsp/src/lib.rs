//! Language Server Protocol support for `adotenv` documents.
//!
//! The library is transport-free: embedders can either call the stateless
//! operations over source text and an [`LspOptions`] policy, or retain a
//! [`ParsedDocument`] / [`Session`] so documents are parsed and line-indexed
//! once per open or edit instead of on every request. The default-on `stdio`
//! feature builds the `adotenv-lsp` binary with a bounded JSON-RPC 2.0
//! `Content-Length` stdio transport; disable default features to depend only on
//! the transport-free library.
//!
//! All operations are conservative and never read the process environment or
//! execute commands. Definition, references, `prepareRename` and rename follow
//! keys and literal `$VAR`/`${VAR}` references; rename uses
//! `Document::rename_with_references_with_options`, formatting uses
//! `Document::format`, and code actions apply verified fixes from
//! `adotenv-lint`. Hover values can be masked with
//! [`LspOptions::with_mask_values`], the parser dialect can be selected with
//! [`LspOptions::with_dialect`], position coordinates support UTF-16 (the LSP
//! 3.17 default), UTF-8 and UTF-32 via [`LspOptions::with_position_encoding`],
//! and linter findings and quick fixes can be enabled with
//! [`LspOptions::with_lint_options`].
//!
//! # Examples
//!
//! ```
//! use adotenv_lsp::{
//!     diagnostics, hover, rename, KeyCase, LintOptions, LspOptions, ParsedDocument,
//! };
//! use lsp_types::{Position, Uri};
//!
//! let uri: Uri = "file:///tmp/.env".parse().unwrap();
//! let source = "HOST=localhost\nURL=${HOST}/api\n";
//! let options = LspOptions::new()
//!     .with_mask_values(true)
//!     .with_lint_options(
//!         LintOptions::new()
//!             .with_duplicate_keys(false)
//!             .with_key_case(KeyCase::Upper),
//!     );
//!
//! // Stateless helpers take `&LspOptions` consistently:
//! assert!(diagnostics(source, &uri, &options).is_empty());
//! assert!(hover(source, Position::new(0, 0), &options).is_some());
//!
//! // Stateful `ParsedDocument` parses and builds its line map once:
//! let doc = ParsedDocument::parse(source, options);
//! assert!(doc.prepare_rename(Position::new(1, 7)).is_some());
//! let edit = doc.rename(&uri, Position::new(0, 0), "SERVER").unwrap();
//! assert_eq!(edit.changes.unwrap()[&uri].len(), 2);
//!
//! let stat = rename(source, &uri, Position::new(0, 0), "SERVER", &options).unwrap();
//! assert_eq!(stat.changes.unwrap()[&uri].len(), 2);
//! ```

use std::{
    collections::{BTreeSet, HashMap},
    fmt,
};

use adotenv::{
    parse_with_options, ColonSeparator, Diagnostic, DiagnosticCode, DoubleEscapes, EditOptions,
    Entry, InlineComments, ItemKind, LineEnding, Quote, Reference, SingleEscapes, Syntax,
};
pub use adotenv::{
    BlankLines, BomPolicy, Dialect, Document, EditError, EqualsSpacing, FinalNewline, FormatError,
    FormatOptions, LimitExceeded, LimitKind, ParseOptions, QuoteStyle, Severity,
};
use adotenv_lint::{apply_batch, apply_fix, apply_fixes, lint, Finding, Fix, LintError};
pub use adotenv_lint::{KeyCase, LintOptions, Rule};
use lsp_types::{
    CodeAction, CodeActionContext, CodeActionKind, CodeActionOrCommand, CodeActionResponse,
    CompletionItem, CompletionItemKind, CompletionResponse, Diagnostic as LspDiagnostic,
    DiagnosticRelatedInformation, DiagnosticSeverity, DocumentSymbol, DocumentSymbolResponse,
    FormattingOptions, GotoDefinitionResponse, Hover, HoverContents, Location, MarkupContent,
    MarkupKind, NumberOrString, Position, PositionEncodingKind, PrepareRenameResponse, Range,
    SymbolInformation, SymbolKind, TextDocumentContentChangeEvent, TextEdit, Uri, WorkspaceEdit,
};

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
mod readme_doctests {}

/// Maximum number of distinct `quickfix` actions computed per `codeAction`
/// request to avoid quadratic verification cost on very large selections.
/// `source.fixAll` still applies all auto-fixable findings in one batch.
const MAX_QUICKFIX_ACTIONS: usize = 64;

/// Column coordinate encoding used for LSP positions and ranges.
///
/// Matches LSP 3.17 [`PositionEncodingKind`]: UTF-16 is the mandatory default,
/// while UTF-8 and UTF-32 (Unicode scalar values) can be negotiated with
/// clients advertising `general.positionEncodings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum PositionEncoding {
    /// UTF-8 code units (bytes).
    Utf8,
    /// UTF-16 code units (the LSP 3.17 default).
    #[default]
    Utf16,
    /// UTF-32 code units (Unicode scalar values).
    Utf32,
}

impl PositionEncoding {
    /// Returns the LSP 3.17 encoding tag (`"utf-8"`, `"utf-16"`, or `"utf-32"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf16 => "utf-16",
            Self::Utf32 => "utf-32",
        }
    }

    /// Converts this encoding to its [`PositionEncodingKind`].
    #[must_use]
    pub const fn to_lsp_kind(self) -> PositionEncodingKind {
        match self {
            Self::Utf8 => PositionEncodingKind::UTF8,
            Self::Utf16 => PositionEncodingKind::UTF16,
            Self::Utf32 => PositionEncodingKind::UTF32,
        }
    }

    /// Parses a [`PositionEncodingKind`] advertised by an LSP client.
    #[must_use]
    pub fn from_lsp_kind(kind: &PositionEncodingKind) -> Option<Self> {
        match kind.as_str() {
            "utf-8" => Some(Self::Utf8),
            "utf-16" => Some(Self::Utf16),
            "utf-32" => Some(Self::Utf32),
            _ => None,
        }
    }

    /// Picks the highest-priority supported encoding from a client's
    /// `general.positionEncodings` preference list, defaulting to
    /// [`PositionEncoding::Utf16`] per LSP 3.17 when omitted or unrecognized.
    #[must_use]
    pub fn negotiate(client_encodings: Option<&[PositionEncodingKind]>) -> Self {
        let Some(encodings) = client_encodings else {
            return Self::Utf16;
        };
        encodings
            .iter()
            .find_map(Self::from_lsp_kind)
            .unwrap_or(Self::Utf16)
    }
}

/// Options for LSP operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct LspOptions {
    /// Replace hover values with `<redacted>`.
    pub mask_values: bool,
    /// The dialect used to parse every document. Defaults to
    /// [`Dialect::PORTABLE`].
    pub dialect: Dialect,
    /// Optional linter policy. Parser diagnostics remain enabled.
    pub lint: Option<LintOptions>,
    /// Optional base formatting policy used by [`formatting`].
    ///
    /// When `None`, formatting derives its base policy from [`Self::lint`] if
    /// present, or otherwise trims trailing whitespace and ensures a final
    /// newline while preserving assignment spacing, quotes and blank lines.
    pub format: Option<FormatOptions>,
    /// Optional parser bounds applied to every document parse and edit output.
    pub parse_options: ParseOptions,
    /// Coordinate encoding used for LSP positions and ranges. Defaults to
    /// [`PositionEncoding::Utf16`].
    pub position_encoding: PositionEncoding,
}

impl Default for LspOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl LspOptions {
    /// Returns Portable parsing, visible hover values, UTF-16 coordinates, no
    /// linter, no custom format policy and no parse limits.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            mask_values: false,
            dialect: Dialect::PORTABLE,
            lint: None,
            format: None,
            parse_options: ParseOptions::new(),
            position_encoding: PositionEncoding::Utf16,
        }
    }

    /// Sets whether hover values are masked.
    #[must_use]
    pub const fn with_mask_values(mut self, mask: bool) -> Self {
        self.mask_values = mask;
        self
    }

    /// Sets the dialect used to parse documents for every operation.
    #[must_use]
    pub const fn with_dialect(mut self, dialect: Dialect) -> Self {
        self.dialect = dialect;
        self
    }

    /// Enables an explicit linter policy for diagnostics and code actions.
    #[must_use]
    pub const fn with_lint_options(mut self, lint: LintOptions) -> Self {
        self.lint = Some(lint);
        self
    }

    /// Disables linter findings and quick fixes, leaving parser diagnostics enabled.
    #[must_use]
    pub const fn without_lint_options(mut self) -> Self {
        self.lint = None;
        self
    }

    /// Sets an explicit base [`FormatOptions`] policy for [`formatting`].
    #[must_use]
    pub const fn with_format_options(mut self, format: FormatOptions) -> Self {
        self.format = Some(format);
        self
    }

    /// Clears any explicit formatting policy, falling back to [`Self::lint`]
    /// or default whitespace/final-newline formatting.
    #[must_use]
    pub const fn without_format_options(mut self) -> Self {
        self.format = None;
        self
    }

    /// Sets opt-in [`ParseOptions`] bounds applied when parsing documents and
    /// verifying edits.
    #[must_use]
    pub const fn with_parse_options(mut self, parse_options: ParseOptions) -> Self {
        self.parse_options = parse_options;
        self
    }

    /// Sets the LSP position encoding used for line/character coordinates.
    #[must_use]
    pub const fn with_position_encoding(mut self, encoding: PositionEncoding) -> Self {
        self.position_encoding = encoding;
        self
    }
}

/// A failed rename request.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenameError {
    /// No declaration or reference exists at the requested position.
    NoSymbol,
    /// The rename was refused by `adotenv`.
    Edit(EditError),
}

impl fmt::Display for RenameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSymbol => formatter.write_str("no .env key at the requested position"),
            Self::Edit(error) => write!(formatter, "rename refused: {error}"),
        }
    }
}

impl std::error::Error for RenameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Edit(error) => Some(error),
            _ => None,
        }
    }
}

impl From<EditError> for RenameError {
    fn from(error: EditError) -> Self {
        Self::Edit(error)
    }
}

/// A failed document-formatting request.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormattingError {
    /// Formatting was refused by `adotenv`.
    Format(FormatError),
    /// A configured parse or output limit was exceeded.
    Limit(LimitExceeded),
}

impl fmt::Display for FormattingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(error) => write!(formatter, "formatting refused: {error}"),
            Self::Limit(limit) => write!(formatter, "formatting refused: {limit}"),
        }
    }
}

impl std::error::Error for FormattingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Format(error) => Some(error),
            Self::Limit(_) => None,
        }
    }
}

impl From<FormatError> for FormattingError {
    fn from(error: FormatError) -> Self {
        Self::Format(error)
    }
}

impl From<LimitExceeded> for FormattingError {
    fn from(limit: LimitExceeded) -> Self {
        Self::Limit(limit)
    }
}

/// A failed document-synchronization operation on a [`Session`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SessionError {
    /// The target URI is not currently open in the session.
    NotOpen,
    /// An incremental content change had an invalid line, character boundary,
    /// split UTF-16 surrogate or inverted range.
    InvalidChange,
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotOpen => formatter.write_str("document is not open in session"),
            Self::InvalidChange => formatter.write_str("invalid incremental document change"),
        }
    }
}

impl std::error::Error for SessionError {}

/// A parsed, line-indexed document handle for repeated LSP queries.
///
/// Parsing and line-map construction run once in [`ParsedDocument::parse`] and
/// again only when edits are applied through [`ParsedDocument::apply_change`]
/// or [`ParsedDocument::apply_changes`].
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    document: Document<'static>,
    parse_diagnostics: Vec<Diagnostic>,
    lines: LineMap,
    options: LspOptions,
}

impl ParsedDocument {
    /// Parses `source` once under `options` and builds its coordinate map.
    #[must_use]
    pub fn parse(source: impl Into<String>, options: LspOptions) -> Self {
        let source = source.into();
        let (document, parse_diagnostics) =
            parse_with_options(&source, options.dialect, options.parse_options);
        let document = document.into_owned();
        let lines = LineMap::new(
            document.source(),
            options.dialect,
            options.position_encoding,
        );
        Self {
            document,
            parse_diagnostics,
            lines,
            options,
        }
    }

    /// Returns the raw UTF-8 source text owned by this document.
    #[must_use]
    pub fn source(&self) -> &str {
        self.document.source()
    }

    /// Returns the [`LspOptions`] active for this document.
    #[must_use]
    pub const fn options(&self) -> LspOptions {
        self.options
    }

    /// Returns the underlying parsed [`Document`].
    #[must_use]
    pub const fn document(&self) -> &Document<'static> {
        &self.document
    }

    /// Applies one LSP content change and returns a newly parsed document.
    ///
    /// A `None` range replaces the whole document. A ranged change uses the
    /// configured [`PositionEncoding`] with LSP 3.17 §3.1.1 column clamping
    /// (`character` past end-of-line clamps to end-of-line). Returns `None`
    /// when a line is out of bounds, a range is inverted, a UTF-16 column
    /// splits a surrogate pair, or a UTF-8 column lands inside a multi-byte
    /// character.
    #[must_use]
    pub fn apply_change(&self, range: Option<Range>, replacement: &str) -> Option<Self> {
        let updated = self.lines.apply_change(self.source(), range, replacement)?;
        Some(Self::parse(updated, self.options))
    }

    /// Applies a batch of LSP content changes atomically.
    ///
    /// If every change in `changes` is valid, `self` is replaced with the
    /// re-parsed result and `true` is returned. If any change is invalid,
    /// `self` is left unchanged and `false` is returned.
    pub fn apply_changes(&mut self, changes: &[TextDocumentContentChangeEvent]) -> bool {
        if changes.is_empty() {
            return true;
        }
        let mut text = self.source().to_owned();
        for (index, change) in changes.iter().enumerate() {
            let applied = if index == 0 {
                self.lines
                    .apply_change_in_place(&mut text, change.range, &change.text)
            } else if change.range.is_none() {
                text.clear();
                text.push_str(&change.text);
                true
            } else {
                let map = LineMap::new(&text, self.options.dialect, self.options.position_encoding);
                map.apply_change_in_place(&mut text, change.range, &change.text)
            };
            if !applied {
                return false;
            }
        }
        *self = Self::parse(text, self.options);
        true
    }

    fn limit_error(&self) -> Option<LimitExceeded> {
        self.parse_diagnostics.iter().find_map(Diagnostic::limit)
    }

    /// Converts parser diagnostics (and optional linter findings) into LSP
    /// diagnostics.
    #[must_use]
    pub fn diagnostics(&self, uri: &Uri) -> Vec<LspDiagnostic> {
        let text = self.source();
        let mut output = self
            .parse_diagnostics
            .iter()
            .map(|diagnostic| {
                let related_information = diagnostic.secondary_span().map(|span| {
                    vec![DiagnosticRelatedInformation {
                        location: Location::new(uri.clone(), self.lines.range(text, span)),
                        message: "related source location".to_owned(),
                    }]
                });
                LspDiagnostic {
                    range: self.lines.range(text, diagnostic.span()),
                    severity: Some(severity(diagnostic.severity())),
                    code: Some(NumberOrString::String(
                        diagnostic.code().as_str().to_owned(),
                    )),
                    source: Some("adotenv".to_owned()),
                    message: diagnostic.message().to_owned(),
                    related_information,
                    ..LspDiagnostic::default()
                }
            })
            .collect::<Vec<_>>();
        if self.limit_error().is_none() {
            if let Some(lint_options) = self.options.lint {
                output.extend(
                    lint(&self.document, lint_options)
                        .into_iter()
                        .map(|finding| finding_to_lsp_diagnostic(text, &self.lines, &finding)),
                );
            }
        }
        output
    }

    /// Returns completion items for declared keys at a key position or after
    /// `$`/`${`.
    #[must_use]
    pub fn completion(&self, position: Position) -> CompletionResponse {
        let text = self.source();
        let Some(offset) = self.lines.offset(text, position) else {
            return CompletionResponse::Array(Vec::new());
        };
        let Some((prefix, context)) = completion_context_at(
            &self.document,
            text,
            offset,
            self.lines.bom_bytes,
            self.options.dialect,
        ) else {
            return CompletionResponse::Array(Vec::new());
        };
        let trimmed = prefix.trim_start();
        let key_prefix = if self.options.dialect.export {
            strip_export_prefix(trimmed)
        } else {
            trimmed
        };
        let at_key_start = !context.saw_separator
            && !key_prefix.contains('#')
            && !key_prefix.chars().any(char::is_whitespace);
        let value_prefix = &prefix[context.value_start.min(prefix.len())..];
        let after_reference = context.saw_separator
            && value_prefix.rfind('$').is_some_and(|dollar| {
                if is_escaped_dollar(
                    value_prefix,
                    dollar,
                    context.value_quote,
                    self.options.dialect,
                ) {
                    return false;
                }
                let tail = &value_prefix[dollar + 1..];
                if let Some(braced) = tail.strip_prefix('{') {
                    braced.chars().all(|character| {
                        character.is_alphanumeric() || matches!(character, '_' | '.')
                    })
                } else {
                    is_unbraced_reference_prefix(tail)
                }
            });
        if !at_key_start && !after_reference {
            return CompletionResponse::Array(Vec::new());
        }

        let mut keys = BTreeSet::new();
        for entry in self.document.entries() {
            keys.insert(entry.key().to_owned());
        }
        CompletionResponse::Array(
            keys.into_iter()
                .map(|key| CompletionItem {
                    label: key.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some("declared .env key".to_owned()),
                    insert_text: Some(key),
                    ..CompletionItem::default()
                })
                .collect(),
        )
    }

    /// Returns an outline of the document's declared keys. Values are never
    /// included.
    #[must_use]
    #[allow(deprecated)]
    pub fn document_symbols(&self) -> DocumentSymbolResponse {
        let text = self.source();
        DocumentSymbolResponse::Nested(
            self.document
                .entries()
                .map(|entry| DocumentSymbol {
                    name: entry.key().to_owned(),
                    detail: None,
                    kind: SymbolKind::KEY,
                    tags: None,
                    deprecated: None,
                    range: self
                        .lines
                        .range(text, entry_content_span(&self.document, entry)),
                    selection_range: self.lines.range(text, entry.key_span()),
                    children: None,
                })
                .collect(),
        )
    }

    /// Returns workspace symbols for declared keys in this document matching
    /// `query` case-insensitively. Values are never included.
    #[must_use]
    #[allow(deprecated)]
    pub fn workspace_symbols(&self, uri: &Uri, query: &str) -> Vec<SymbolInformation> {
        let text = self.source();
        let query = query.to_lowercase();
        self.document
            .entries()
            .filter(|entry| query.is_empty() || entry.key().to_lowercase().contains(&query))
            .map(|entry| SymbolInformation {
                name: entry.key().to_owned(),
                kind: SymbolKind::KEY,
                tags: None,
                deprecated: None,
                location: Location::new(
                    uri.clone(),
                    self.lines
                        .range(text, entry_content_span(&self.document, entry)),
                ),
                container_name: None,
            })
            .collect()
    }

    /// Finds the declaration of the key or reference at `position`.
    #[must_use]
    pub fn definition(&self, uri: &Uri, position: Position) -> Option<GotoDefinitionResponse> {
        let text = self.source();
        let offset = self.lines.offset(text, position)?;
        let name = name_at(&self.document, offset)?;
        let entry = self.document.get_entry(name)?;
        Some(GotoDefinitionResponse::Scalar(Location::new(
            uri.clone(),
            self.lines.range(text, entry.key_span()),
        )))
    }

    /// Returns hover information for a key or reference.
    #[must_use]
    pub fn hover(&self, position: Position) -> Option<Hover> {
        let text = self.source();
        let offset = self.lines.offset(text, position)?;
        if let Some(reference) = reference_at(&self.document, offset) {
            let name = reference_name(&self.document, &reference)?;
            let target = self.document.get_entry(name).map_or_else(
                || "; unresolved".to_owned(),
                |entry| format!("; declared on line {}", entry.line()),
            );
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "{}{target}",
                        format_markdown_inline_code(&format!("${{{name}}}"))
                    ),
                }),
                range: Some(self.lines.range(text, reference.span)),
            });
        }
        let entry = entry_at(&self.document, offset)?;
        if entry
            .tokens()
            .inline_comment
            .as_ref()
            .is_some_and(|comment| comment.start <= offset && offset <= comment.end)
        {
            return None;
        }
        let value = match entry.value() {
            Some(_) if self.options.mask_values => "`<redacted>`".to_owned(),
            Some(value) => format_markdown_code(value),
            None => "`<bare>`".to_owned(),
        };
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "{} on line {} ({:?})\n\nValue: {value}",
                    format_markdown_inline_code(entry.key()),
                    entry.line(),
                    entry.state()
                ),
            }),
            range: Some(self.lines.range(text, entry.key_span())),
        })
    }

    /// Finds declarations and value references for the symbol at `position`.
    ///
    /// Reference locations cover each reference's identifier (`reference.name`)
    /// span rather than an enclosing `${VAR:-default}` operator expression,
    /// matching [`Self::definition`] and [`Self::prepare_rename`].
    #[must_use]
    pub fn references(
        &self,
        uri: &Uri,
        position: Position,
        include_declaration: bool,
    ) -> Vec<Location> {
        let text = self.source();
        let Some(offset) = self.lines.offset(text, position) else {
            return Vec::new();
        };
        let Some(name) = name_at(&self.document, offset) else {
            return Vec::new();
        };
        let mut locations = Vec::new();
        for entry in self.document.entries() {
            if entry.key() == name && include_declaration {
                locations.push(Location::new(
                    uri.clone(),
                    self.lines.range(text, entry.key_span()),
                ));
            }
            for reference in entry.references() {
                if reference_name(&self.document, &reference) == Some(name) {
                    let span = reference.name.unwrap_or(reference.span);
                    locations.push(Location::new(uri.clone(), self.lines.range(text, span)));
                }
            }
        }
        locations
    }

    /// Validates whether `position` names a uniquely declared renameable key
    /// or a reference to one, returning its exact symbol range and placeholder
    /// text.
    #[must_use]
    pub fn prepare_rename(&self, position: Position) -> Option<PrepareRenameResponse> {
        if self.limit_error().is_some() {
            return None;
        }
        let text = self.source();
        let offset = self.lines.offset(text, position)?;
        if let Some(reference) = reference_at(&self.document, offset) {
            let name_span = reference.name.filter(|span| !span.is_empty())?;
            if !(name_span.start..=name_span.end).contains(&offset) {
                return None;
            }
            let name = text.get(name_span.clone())?;
            self.document.unique(name).ok()?;
            return Some(PrepareRenameResponse::RangeWithPlaceholder {
                range: self.lines.range(text, name_span),
                placeholder: name.to_owned(),
            });
        }
        let entry = entry_at(&self.document, offset)?;
        if entry.key().is_empty() {
            return None;
        }
        let key_span = entry.key_span();
        if !(key_span.start..=key_span.end).contains(&offset) {
            return None;
        }
        self.document.unique(entry.key()).ok()?;
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: self.lines.range(text, key_span),
            placeholder: entry.key().to_owned(),
        })
    }

    /// Renames a key declaration and its literal references using
    /// `Document::rename_with_references_with_options`.
    ///
    /// # Errors
    ///
    /// Returns [`RenameError::NoSymbol`] when `position` is not on a key or
    /// reference, and [`RenameError::Edit`] when a configured parse limit is
    /// exceeded ([`EditError::Limit`]), the rename is invalid, matching
    /// references exist and the new key cannot be written as a variable
    /// reference, or verification fails (including `$(...)` command-context
    /// allowlist rejections).
    pub fn rename(
        &self,
        uri: &Uri,
        position: Position,
        new_name: &str,
    ) -> Result<WorkspaceEdit, RenameError> {
        if let Some(limit) = self.limit_error() {
            return Err(RenameError::Edit(EditError::Limit(limit)));
        }
        let text = self.source();
        let offset = self
            .lines
            .offset(text, position)
            .ok_or(RenameError::NoSymbol)?;
        let old_name = name_at(&self.document, offset)
            .ok_or(RenameError::NoSymbol)?
            .to_owned();
        let edit_options = EditOptions::new().with_parse_limits(self.options.parse_options);
        let edits =
            self.document
                .rename_with_references_with_options(&old_name, new_name, edit_options)?;
        let changes = edits
            .edits()
            .iter()
            .map(|edit| TextEdit {
                range: self.lines.range(text, edit.range.clone()),
                new_text: edit.replacement.clone(),
            })
            .collect::<Vec<_>>();
        Ok(WorkspaceEdit {
            changes: Some(HashMap::from([(uri.clone(), changes)])),
            ..WorkspaceEdit::default()
        })
    }

    /// Formats the document using `Document::format`, honoring [`LspOptions`]
    /// and LSP [`FormattingOptions`].
    ///
    /// `FormattingOptions::trim_trailing_whitespace` and
    /// `FormattingOptions::insert_final_newline` override the base policy
    /// (preserving an explicit [`FinalNewline::Remove`] when
    /// `insert_final_newline` is `Some(false)`). `FormattingOptions::trim_final_newlines`
    /// is not mapped because `adotenv::FormatOptions` models blank-line
    /// collapsing ([`BlankLines::Collapse`]) rather than trailing-only trimming.
    ///
    /// # Errors
    ///
    /// Returns [`FormattingError::Limit`] when the document or formatted output
    /// exceeds configured [`ParseOptions`] limits, and
    /// [`FormattingError::Format`] when `Document::format` refuses the edit.
    pub fn formatting(
        &self,
        formatting_options: &FormattingOptions,
    ) -> Result<Vec<TextEdit>, FormattingError> {
        if let Some(limit) = self.limit_error() {
            return Err(FormattingError::Limit(limit));
        }
        let mut policy = if let Some(explicit) = self.options.format {
            explicit
        } else if let Some(lint_options) = self.options.lint {
            FormatOptions::default()
                .with_equals_spacing(lint_options.equals_spacing)
                .with_quote_style(lint_options.quote_style)
                .with_trim_trailing_whitespace(lint_options.trim_trailing_whitespace)
        } else {
            FormatOptions::default()
                .with_trim_trailing_whitespace(true)
                .with_final_newline(FinalNewline::Ensure)
        };
        if let Some(trim) = formatting_options.trim_trailing_whitespace {
            policy = policy.with_trim_trailing_whitespace(trim);
        }
        if let Some(insert_final) = formatting_options.insert_final_newline {
            if insert_final {
                policy = policy.with_final_newline(FinalNewline::Ensure);
            } else if policy.final_newline == FinalNewline::Ensure {
                policy = policy.with_final_newline(FinalNewline::Preserve);
            }
        }
        // `adotenv::Document::format` intentionally keeps `QuoteStyle::Unquoted`
        // separate from other trivia rewrites; when combined (for example via
        // editor `insertFinalNewline` or `trimTrailingWhitespace`), run
        // unquoting first and apply the remaining options in a second pass.
        let needs_two_pass = policy.quote_style == QuoteStyle::Unquoted
            && (policy.equals_spacing != EqualsSpacing::Preserve
                || policy.trim_trailing_whitespace
                || policy.blank_lines != BlankLines::Preserve
                || policy.final_newline != FinalNewline::Preserve);
        if needs_two_pass {
            let unquote_only = FormatOptions::default().with_quote_style(QuoteStyle::Unquoted);
            let pass1 = self.document.format(&unquote_only)?;
            let rest_policy = policy.with_quote_style(QuoteStyle::Preserve);
            if pass1.is_empty() {
                let pass2 = self.document.format(&rest_policy)?;
                check_output_parse_limits(
                    pass2.as_str(),
                    self.options.dialect,
                    self.options.parse_options,
                )?;
                return Ok(self.text_edits_from(pass2.edits()));
            }
            check_output_parse_limits(
                pass1.as_str(),
                self.options.dialect,
                self.options.parse_options,
            )?;
            let (mid_doc, _) = parse_with_options(
                pass1.as_str(),
                self.options.dialect,
                self.options.parse_options,
            );
            let pass2 = mid_doc.format(&rest_policy)?;
            if pass2.is_empty() {
                return Ok(self.text_edits_from(pass1.edits()));
            }
            check_output_parse_limits(
                pass2.as_str(),
                self.options.dialect,
                self.options.parse_options,
            )?;
            if pass2.as_str() == self.source() {
                return Ok(Vec::new());
            }
            return Ok(vec![self.bom_preserving_full_edit(pass2.as_str())]);
        }
        let formatted = self.document.format(&policy)?;
        check_output_parse_limits(
            formatted.as_str(),
            self.options.dialect,
            self.options.parse_options,
        )?;
        Ok(self.text_edits_from(formatted.edits()))
    }

    fn text_edits_from(&self, edits: &[adotenv::TextEdit]) -> Vec<TextEdit> {
        let text = self.source();
        edits
            .iter()
            .map(|edit| TextEdit {
                range: self.lines.range(text, edit.range.clone()),
                new_text: edit.replacement.clone(),
            })
            .collect()
    }

    fn bom_preserving_full_edit(&self, output: &str) -> TextEdit {
        let text = self.source();
        let visible_start = self.lines.bom_bytes.min(text.len());
        let replacement = output
            .strip_prefix(&text[..visible_start])
            .unwrap_or(output)
            .to_owned();
        TextEdit {
            range: self.lines.range(text, visible_start..text.len()),
            new_text: replacement,
        }
    }

    /// Returns verified quick-fix and `source.fixAll` code actions derived from
    /// configured [`LintOptions`].
    ///
    /// At most 64 distinct `quickfix` actions are materialized per request to
    /// keep large selections bounded; `source.fixAll` still applies all
    /// auto-fixable findings in one batch.
    #[must_use]
    pub fn code_actions(
        &self,
        uri: &Uri,
        range: Range,
        context: &CodeActionContext,
    ) -> CodeActionResponse {
        let Some(lint_options) = self.options.lint else {
            return Vec::new();
        };
        if self.limit_error().is_some() || range.start > range.end {
            return Vec::new();
        }
        let only = context.only.as_deref();
        let want_quickfix = code_action_kind_requested(only, &CodeActionKind::QUICKFIX);
        let want_fix_all = code_action_kind_requested(only, &CodeActionKind::SOURCE_FIX_ALL);
        if !want_quickfix && !want_fix_all {
            return Vec::new();
        }
        let findings = lint(&self.document, lint_options);
        if findings.is_empty() {
            return Vec::new();
        }
        let text = self.source();
        let mut actions = Vec::new();

        if want_quickfix {
            let mut grouped: Vec<(Fix, Vec<LspDiagnostic>)> = Vec::new();
            for finding in &findings {
                let Some(fix) = finding.fix() else {
                    continue;
                };
                let diag = finding_to_lsp_diagnostic(text, &self.lines, finding);
                if !ranges_overlap(diag.range, range) {
                    continue;
                }
                if let Some((_, existing_diags)) = grouped.iter_mut().find(|(f, _)| f == fix) {
                    existing_diags.push(diag);
                } else if grouped.len() < MAX_QUICKFIX_ACTIONS {
                    grouped.push((fix.clone(), vec![diag]));
                }
            }
            for (fix, matching_diags) in grouped {
                let Ok(result) = apply_fix(&self.document, &fix) else {
                    continue;
                };
                if result.edits().is_empty() {
                    continue;
                }
                if check_output_parse_limits(
                    result.as_str(),
                    self.options.dialect,
                    self.options.parse_options,
                )
                .is_err()
                {
                    continue;
                }
                let edits = self.text_edits_from(result.edits());
                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: fix_title(&fix),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(matching_diags),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(uri.clone(), edits)])),
                        ..WorkspaceEdit::default()
                    }),
                    is_preferred: Some(true),
                    ..CodeAction::default()
                }));
            }
        }

        if want_fix_all {
            let all_fixes: Vec<Fix> = findings
                .iter()
                .filter_map(|finding| finding.fix().cloned())
                .collect();
            if !all_fixes.is_empty() {
                if let Some(edits) = self.build_fix_all_edits(&all_fixes) {
                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: "Fix all auto-fixable lint issues".to_owned(),
                        kind: Some(CodeActionKind::SOURCE_FIX_ALL),
                        edit: Some(WorkspaceEdit {
                            changes: Some(HashMap::from([(uri.clone(), edits)])),
                            ..WorkspaceEdit::default()
                        }),
                        ..CodeAction::default()
                    }));
                }
            }
        }

        actions
    }

    fn build_fix_all_edits(&self, fixes: &[Fix]) -> Option<Vec<TextEdit>> {
        match apply_batch(&self.document, fixes) {
            Ok(batch) => {
                if batch.is_empty() {
                    return None;
                }
                check_output_parse_limits(
                    batch.output(),
                    self.options.dialect,
                    self.options.parse_options,
                )
                .ok()?;
                if batch.stages().len() == 1 {
                    let stage_edits = batch.stages()[0].edits();
                    if !stage_edits.is_empty() {
                        return Some(self.text_edits_from(stage_edits));
                    }
                }
                // Multi-stage edits are relative to intermediate outputs; emit
                // one BOM-preserving replacement covering the visible document.
                Some(vec![self.bom_preserving_full_edit(batch.output())])
            }
            Err(LintError::Parse) => {
                // `apply_batch` re-parses strictly between stages; when
                // unrelated invalid lines exist, fall back to a single
                // verified batch via `apply_fixes` if the fixes are compatible.
                let result = apply_fixes(&self.document, fixes).ok()?;
                if result.edits().is_empty() {
                    return None;
                }
                check_output_parse_limits(
                    result.as_str(),
                    self.options.dialect,
                    self.options.parse_options,
                )
                .ok()?;
                Some(self.text_edits_from(result.edits()))
            }
            Err(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
struct OpenDocument {
    uri: Uri,
    parsed: ParsedDocument,
    version: Option<i32>,
}

/// Stateful multi-document LSP workspace session.
///
/// Stores open [`ParsedDocument`] buffers keyed by URI string together with
/// their latest document versions so repeated queries avoid re-parsing and
/// `workspace/symbol` aggregates across every open document deterministically.
#[derive(Debug, Clone)]
pub struct Session {
    options: LspOptions,
    documents: HashMap<String, OpenDocument>,
}

impl Session {
    /// Creates an empty session using `options`.
    #[must_use]
    pub fn new(options: LspOptions) -> Self {
        Self {
            options,
            documents: HashMap::new(),
        }
    }

    /// Returns the [`LspOptions`] active for this session.
    #[must_use]
    pub const fn options(&self) -> LspOptions {
        self.options
    }

    /// Returns the number of currently open documents.
    #[must_use]
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Returns `true` when no documents are open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Opens or replaces a document in the session, parsing and indexing it
    /// once.
    pub fn open(&mut self, uri: Uri, version: Option<i32>, text: impl Into<String>) {
        let key = uri.as_str().to_owned();
        let parsed = ParsedDocument::parse(text, self.options);
        self.documents.insert(
            key,
            OpenDocument {
                uri,
                parsed,
                version,
            },
        );
    }

    /// Applies a batch of LSP content changes atomically to an open document
    /// and updates its version.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::NotOpen`] if `uri` is not open, or
    /// [`SessionError::InvalidChange`] if any change fails (leaving the stored
    /// document and version untouched).
    pub fn change(
        &mut self,
        uri: &Uri,
        version: Option<i32>,
        changes: &[TextDocumentContentChangeEvent],
    ) -> Result<(), SessionError> {
        let doc = self
            .documents
            .get_mut(uri.as_str())
            .ok_or(SessionError::NotOpen)?;
        if !doc.parsed.apply_changes(changes) {
            return Err(SessionError::InvalidChange);
        }
        doc.version = version;
        Ok(())
    }

    /// Closes a document and returns whether it was open.
    pub fn close(&mut self, uri: &Uri) -> bool {
        self.documents.remove(uri.as_str()).is_some()
    }

    /// Returns the cached [`ParsedDocument`] for `uri`, when open.
    #[must_use]
    pub fn document(&self, uri: &Uri) -> Option<&ParsedDocument> {
        self.documents.get(uri.as_str()).map(|doc| &doc.parsed)
    }

    /// Returns the latest LSP document version for `uri`, when open and
    /// versioned.
    #[must_use]
    pub fn version(&self, uri: &Uri) -> Option<i32> {
        self.documents.get(uri.as_str()).and_then(|doc| doc.version)
    }

    /// Aggregates workspace symbols matching `query` across all open
    /// documents, sorted deterministically by name, URI and start position.
    #[must_use]
    pub fn workspace_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        let mut symbols = Vec::new();
        for doc in self.documents.values() {
            symbols.extend(doc.parsed.workspace_symbols(&doc.uri, query));
        }
        symbols.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.location.uri.as_str().cmp(right.location.uri.as_str()))
                .then_with(|| left.location.range.start.cmp(&right.location.range.start))
        });
        symbols
    }
}

/// Converts parser diagnostics into LSP diagnostics.
///
/// Parser diagnostics are always included. Linter findings are added when
/// `options.lint` contains an explicit policy and no parse limit was exceeded.
#[must_use]
pub fn diagnostics(text: &str, uri: &Uri, options: &LspOptions) -> Vec<LspDiagnostic> {
    ParsedDocument::parse(text, *options).diagnostics(uri)
}

/// Returns completion items for declared keys using `options`.
///
/// The same list is returned for a key position and after `$`/`${`; clients
/// can filter it using their normal completion prefix.
#[must_use]
pub fn completion(text: &str, position: Position, options: &LspOptions) -> CompletionResponse {
    ParsedDocument::parse(text, *options).completion(position)
}

/// Returns an outline of the document's declared keys using `options`.
///
/// Each entry becomes a [`SymbolKind::KEY`] symbol. The range covers the
/// declaration and the selection range covers its key. Values are never
/// included in the symbol data.
#[must_use]
pub fn document_symbols(text: &str, options: &LspOptions) -> DocumentSymbolResponse {
    ParsedDocument::parse(text, *options).document_symbols()
}

/// Returns workspace symbols for declared keys in one document using `options`.
///
/// `query` is matched case-insensitively against the key. The result uses flat
/// [`SymbolInformation`] values so clients without hierarchical symbol support
/// can consume it. Values are never included.
#[must_use]
pub fn workspace_symbols(
    text: &str,
    uri: &Uri,
    query: &str,
    options: &LspOptions,
) -> Vec<SymbolInformation> {
    ParsedDocument::parse(text, *options).workspace_symbols(uri, query)
}

/// Applies one LSP content change to a document string using `options`.
///
/// A `None` range replaces the whole document. A ranged change uses the
/// configured [`PositionEncoding`] (UTF-16 by default) with BOM-adjusted line 0
/// columns and LSP 3.17 §3.1.1 end-of-line clamping (`character > line_len`
/// clamps to the end of that line). Returns `None` when a line is out of
/// bounds, the range is inverted, a UTF-16 column splits a surrogate pair, or a
/// UTF-8 column lands inside a multi-byte character.
#[must_use]
pub fn apply_change(
    text: &str,
    range: Option<Range>,
    replacement: &str,
    options: &LspOptions,
) -> Option<String> {
    let lines = LineMap::new(text, options.dialect, options.position_encoding);
    lines.apply_change(text, range, replacement)
}

/// Finds the declaration of the key or reference at `position` using `options`.
#[must_use]
pub fn definition(
    text: &str,
    uri: &Uri,
    position: Position,
    options: &LspOptions,
) -> Option<GotoDefinitionResponse> {
    ParsedDocument::parse(text, *options).definition(uri, position)
}

/// Returns hover information for a key or reference using `options`.
#[must_use]
pub fn hover(text: &str, position: Position, options: &LspOptions) -> Option<Hover> {
    ParsedDocument::parse(text, *options).hover(position)
}

/// Finds declarations and value references for the symbol at `position` using
/// `options`.
///
/// Reference locations span each reference's identifier (`reference.name`)
/// rather than an enclosing `${VAR:-default}` operator expression.
#[must_use]
pub fn references(
    text: &str,
    uri: &Uri,
    position: Position,
    include_declaration: bool,
    options: &LspOptions,
) -> Vec<Location> {
    ParsedDocument::parse(text, *options).references(uri, position, include_declaration)
}

/// Validates whether `position` names a uniquely declared renameable key or
/// reference using `options`.
#[must_use]
pub fn prepare_rename(
    text: &str,
    position: Position,
    options: &LspOptions,
) -> Option<PrepareRenameResponse> {
    ParsedDocument::parse(text, *options).prepare_rename(position)
}

/// Renames a key declaration and its literal references using `options`.
///
/// The symbol may be selected from a declaration key or from a `$VAR`/`${VAR}`
/// reference. Unrelated invalid lines are preserved through recovering parsing
/// and verified to remain unchanged.
///
/// # Errors
///
/// Returns [`RenameError::NoSymbol`] when `position` is not on a key or
/// reference, and [`RenameError::Edit`] when a configured parse limit is
/// exceeded ([`EditError::Limit`]), the rename is invalid, matching references
/// exist and the new key cannot be written as a variable reference, or
/// verification fails (including `$(...)` command-context allowlist
/// rejections).
pub fn rename(
    text: &str,
    uri: &Uri,
    position: Position,
    new_name: &str,
    options: &LspOptions,
) -> Result<WorkspaceEdit, RenameError> {
    ParsedDocument::parse(text, *options).rename(uri, position, new_name)
}

/// Formats a document using `Document::format`, honoring `options` and LSP
/// `formatting_options`.
///
/// `FormattingOptions::trim_final_newlines` is not mapped; use
/// [`FormatOptions::with_blank_lines`] with [`BlankLines::Collapse`] instead.
///
/// # Errors
///
/// Returns [`FormattingError::Limit`] when the document or formatted output
/// exceeds configured [`ParseOptions`] limits, and
/// [`FormattingError::Format`] when `Document::format` refuses the edit.
pub fn formatting(
    text: &str,
    formatting_options: &FormattingOptions,
    options: &LspOptions,
) -> Result<Vec<TextEdit>, FormattingError> {
    ParsedDocument::parse(text, *options).formatting(formatting_options)
}

/// Returns verified quick-fix and `source.fixAll` code actions derived from
/// configured [`LintOptions`].
///
/// At most 64 distinct `quickfix` actions are returned per request;
/// `source.fixAll` applies all auto-fixable findings in one batch.
#[must_use]
pub fn code_actions(
    text: &str,
    uri: &Uri,
    range: Range,
    context: &CodeActionContext,
    options: &LspOptions,
) -> CodeActionResponse {
    ParsedDocument::parse(text, *options).code_actions(uri, range, context)
}

fn severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        _ => DiagnosticSeverity::INFORMATION,
    }
}

fn finding_to_lsp_diagnostic(text: &str, lines: &LineMap, finding: &Finding) -> LspDiagnostic {
    LspDiagnostic {
        range: lines.range(text, finding.span()),
        severity: Some(severity(finding.severity())),
        code: Some(NumberOrString::String(format!(
            "adotenv-lint.{}",
            finding.rule().as_str()
        ))),
        source: Some("adotenv-lint".to_owned()),
        message: finding.message().to_owned(),
        ..LspDiagnostic::default()
    }
}

fn fix_title(fix: &Fix) -> String {
    match fix {
        Fix::Rename { key, .. } => format!("Rename key to `{key}`"),
        Fix::Sort => "Sort keys in each group".to_owned(),
        Fix::Format(_) => "Format document according to lint policy".to_owned(),
        _ => "Apply lint fix".to_owned(),
    }
}

fn code_action_kind_requested(only: Option<&[CodeActionKind]>, candidate: &CodeActionKind) -> bool {
    let Some(only) = only else {
        return true;
    };
    let candidate = candidate.as_str();
    only.iter().any(|requested| {
        let prefix = requested.as_str();
        prefix.is_empty()
            || candidate == prefix
            || (candidate.starts_with(prefix)
                && candidate.as_bytes().get(prefix.len()) == Some(&b'.'))
    })
}

fn ranges_overlap(left: Range, right: Range) -> bool {
    left.start <= right.end && right.start <= left.end
}

fn check_output_parse_limits(
    output: &str,
    dialect: Dialect,
    parse_options: ParseOptions,
) -> Result<(), LimitExceeded> {
    if parse_options.max_input_bytes.is_none()
        && parse_options.max_lines.is_none()
        && parse_options.max_items.is_none()
        && parse_options.max_diagnostics.is_none()
    {
        return Ok(());
    }
    let (_, diagnostics) = parse_with_options(output, dialect, parse_options);
    if let Some(limit) = diagnostics.iter().find_map(Diagnostic::limit) {
        return Err(limit);
    }
    Ok(())
}

fn completion_context_at<'a>(
    document: &Document<'_>,
    text: &'a str,
    offset: usize,
    bom_bytes: usize,
    dialect: Dialect,
) -> Option<(&'a str, CompletionContext)> {
    let enclosing_item = document.items().find(|item| {
        let span = item.span();
        span.contains(&offset)
            || (offset == text.len()
                && span.end == offset
                && item.line_ending() == LineEnding::None)
    });
    if let Some(entry) = enclosing_item
        .and_then(adotenv::Item::entry)
        .filter(|entry| entry.tokens().eq.is_some())
    {
        return completion_for_entry(text, entry, offset, bom_bytes);
    }
    let unterminated_start = unterminated_multiline_start(
        document,
        text,
        enclosing_item.map(adotenv::Item::id),
        offset,
        dialect,
    );
    if unterminated_start.is_none()
        && enclosing_item.is_some_and(|item| item.kind() == ItemKind::Comment)
    {
        return None;
    }
    let scan_start = unterminated_start.unwrap_or_else(|| {
        enclosing_item.map_or_else(
            || {
                text[..offset]
                    .rfind(['\n', '\r'])
                    .map_or(0, |index| index + 1)
            },
            |item| item.span().start,
        )
    });
    let content_start = if scan_start == 0 {
        bom_bytes.min(offset)
    } else {
        scan_start
    };
    let prefix = &text[content_start..offset];
    if prefix.trim_start().starts_with('#') {
        return None;
    }
    let context = CompletionContext::of(prefix, dialect);
    if context.comment || context.in_literal_quote {
        return None;
    }
    Some((prefix, context))
}

fn completion_for_entry<'a>(
    text: &'a str,
    entry: Entry<'_>,
    offset: usize,
    bom_bytes: usize,
) -> Option<(&'a str, CompletionContext)> {
    let tokens = entry.tokens();
    if tokens
        .inline_comment
        .as_ref()
        .is_some_and(|comment| comment.start <= offset && offset <= comment.end)
    {
        return None;
    }
    let start = entry.line_span().start;
    let content_start = if start == 0 {
        bom_bytes.min(offset)
    } else {
        start
    };
    let prefix = &text[content_start..offset];
    let Some(eq) = tokens.eq.as_ref() else {
        return Some((prefix, CompletionContext::default()));
    };
    if offset < eq.end {
        return Some((prefix, CompletionContext::default()));
    }
    let in_body = tokens
        .body
        .as_ref()
        .is_some_and(|body| body.start <= offset && offset <= body.end);
    if in_body && matches!(entry.quote(), Quote::Single | Quote::Backtick) {
        return None;
    }
    let value_quote = if in_body { entry.quote() } else { Quote::None };
    let value_start = eq.end.saturating_sub(content_start);
    Some((
        prefix,
        CompletionContext {
            comment: false,
            in_literal_quote: false,
            saw_separator: true,
            value_quote,
            value_start,
        },
    ))
}

fn strip_export_prefix(trimmed: &str) -> &str {
    let Some(rest) = trimmed.strip_prefix("export") else {
        return trimmed;
    };
    if rest.starts_with([' ', '\t']) {
        rest.trim_start_matches([' ', '\t'])
    } else {
        trimmed
    }
}

fn is_unbraced_reference_prefix(tail: &str) -> bool {
    let bytes = tail.as_bytes();
    let Some(&first) = bytes.first() else {
        return true;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

fn unterminated_multiline_start(
    document: &Document<'_>,
    text: &str,
    enclosing_id: Option<usize>,
    offset: usize,
    dialect: Dialect,
) -> Option<usize> {
    let multiline = dialect.multiline;
    if !(multiline.single || multiline.double || (dialect.backticks && multiline.backtick)) {
        return None;
    }
    let limit_id = enclosing_id.unwrap_or_else(|| document.items().len());
    for id in (0..limit_id).rev() {
        let item = document.item(id)?;
        if item.span().start >= offset {
            continue;
        }
        match item.kind() {
            ItemKind::Entry
                if item
                    .entry()
                    .is_some_and(|entry| entry.tokens().eq.is_some()) =>
            {
                break;
            }
            ItemKind::Invalid if item.invalid_code() == Some(DiagnosticCode::UnterminatedQuote) => {
                let quote_byte = item
                    .diagnostic()
                    .and_then(|diag| text.as_bytes().get(diag.span().start).copied())?;
                let spans_lines = match quote_byte {
                    b'\'' => multiline.single,
                    b'"' => multiline.double,
                    b'`' => dialect.backticks && multiline.backtick,
                    _ => false,
                };
                if spans_lines {
                    return Some(item.span().start);
                }
            }
            ItemKind::Ignored => {
                let ctx = CompletionContext::of(item.raw(), dialect);
                let spans_lines = match ctx.open_quote_style() {
                    Some(Quote::Single) => multiline.single,
                    Some(Quote::Double) => multiline.double,
                    Some(Quote::Backtick) => dialect.backticks && multiline.backtick,
                    _ => false,
                };
                if spans_lines {
                    return Some(item.span().start);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_escaped_dollar(value_prefix: &str, dollar: usize, quote: Quote, dialect: Dialect) -> bool {
    if dialect.raw_values {
        return false;
    }
    let before = &value_prefix[..dollar];
    let (in_braced, in_command) = reference_nesting_at(before);
    let backslashes = before
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'\\')
        .count();
    let backslash = !in_braced
        && backslashes % 2 == 1
        && ((quote == Quote::Double && dialect.double_escapes.contains(DoubleEscapes::DOLLAR))
            || in_command);
    let compose_dollar = dialect.syntax == Syntax::Compose
        && before
            .bytes()
            .rev()
            .take_while(|byte| *byte == b'$')
            .count()
            % 2
            == 1;
    backslash || compose_dollar
}

/// Tracks whether the end of `before` sits inside an unclosed `${...` or
/// `$(...` container so backslash escape rules match `adotenv::references`.
#[allow(clippy::too_many_lines)]
fn reference_nesting_at(before: &str) -> (bool, bool) {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Container {
        Braced,
        Command(Option<u8>),
        Paren(Option<u8>),
    }
    let bytes = before.as_bytes();
    let mut stack: Vec<Container> = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match stack.last_mut() {
            Some(Container::Braced) => {
                match bytes[index] {
                    b'$' if bytes.get(index + 1) == Some(&b'{') => {
                        stack.push(Container::Braced);
                        index += 2;
                    }
                    b'$' if bytes.get(index + 1) == Some(&b'(') => {
                        stack.push(Container::Command(None));
                        index += 2;
                    }
                    b'}' => {
                        stack.pop();
                        index += 1;
                    }
                    _ => index += 1,
                }
                continue;
            }
            Some(Container::Command(quote) | Container::Paren(quote)) => match *quote {
                Some(b'\'') => {
                    if bytes[index] == b'\'' {
                        *quote = None;
                        index += 1;
                    } else if bytes[index] == b'$' && bytes.get(index + 1) == Some(&b'{') {
                        stack.push(Container::Braced);
                        index += 2;
                    } else if bytes[index] == b'$' && bytes.get(index + 1) == Some(&b'(') {
                        stack.push(Container::Command(None));
                        index += 2;
                    } else {
                        index += 1;
                    }
                    continue;
                }
                Some(_) => {
                    if bytes[index] == b'\\' && matches!(bytes.get(index + 1), Some(b'\\' | b'"')) {
                        index = index.saturating_add(2);
                    } else if bytes[index] == b'"' {
                        *quote = None;
                        index += 1;
                    } else if bytes[index] == b'$' && bytes.get(index + 1) == Some(&b'{') {
                        stack.push(Container::Braced);
                        index += 2;
                    } else if bytes[index] == b'$' && bytes.get(index + 1) == Some(&b'(') {
                        stack.push(Container::Command(None));
                        index += 2;
                    } else {
                        index += 1;
                    }
                    continue;
                }
                None => match bytes[index] {
                    b'\\' if bytes.get(index + 1) == Some(&b'$') => {
                        if let Some((open, close, container)) = match bytes.get(index + 2) {
                            Some(b'{') => Some((b'{', b'}', Container::Braced)),
                            Some(b'(') => Some((b'(', b')', Container::Command(None))),
                            _ => None,
                        } {
                            if let Some(closed) = find_literal_close(bytes, index + 3, open, close)
                            {
                                index = closed + 1;
                            } else {
                                stack.push(container);
                                index += 3;
                            }
                        } else {
                            index = index.saturating_add(2);
                        }
                        continue;
                    }
                    b'\\' => {
                        index = index.saturating_add(2);
                        continue;
                    }
                    b'\'' => {
                        *quote = Some(b'\'');
                        index += 1;
                        continue;
                    }
                    b'"' => {
                        *quote = Some(b'"');
                        index += 1;
                        continue;
                    }
                    b'(' => {
                        stack.push(Container::Paren(None));
                        index += 1;
                        continue;
                    }
                    b')' => {
                        stack.pop();
                        index += 1;
                        continue;
                    }
                    _ => {}
                },
            },
            None => {}
        }
        match bytes[index] {
            b'$' if bytes.get(index + 1) == Some(&b'{') => {
                stack.push(Container::Braced);
                index += 2;
            }
            b'$' if bytes.get(index + 1) == Some(&b'(') => {
                stack.push(Container::Command(None));
                index += 2;
            }
            _ => index += 1,
        }
    }
    let in_command = matches!(
        stack
            .iter()
            .rev()
            .find(|container| !matches!(container, Container::Paren(_))),
        Some(Container::Command(_))
    );
    (stack.last() == Some(&Container::Braced), in_command)
}

fn find_literal_close(bytes: &[u8], start: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 1usize;
    let mut quote = None;
    let mut at = start;
    while at < bytes.len() {
        match quote {
            Some(b'\'') => {
                if bytes[at] == b'\'' {
                    quote = None;
                }
                at += 1;
            }
            Some(_) => {
                if bytes[at] == b'\\' {
                    at = at.saturating_add(2);
                } else {
                    if bytes[at] == b'"' {
                        quote = None;
                    }
                    at += 1;
                }
            }
            None => match bytes[at] {
                b'\\' => at = at.saturating_add(2),
                b'\'' => {
                    quote = Some(b'\'');
                    at += 1;
                }
                b'"' => {
                    quote = Some(b'"');
                    at += 1;
                }
                byte if byte == open => {
                    depth += 1;
                    at += 1;
                }
                byte if byte == close => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(at);
                    }
                    at += 1;
                }
                _ => at += 1,
            },
        }
    }
    None
}

#[derive(Debug)]
struct CompletionContext {
    comment: bool,
    in_literal_quote: bool,
    saw_separator: bool,
    value_quote: Quote,
    value_start: usize,
}

impl Default for CompletionContext {
    fn default() -> Self {
        Self {
            comment: false,
            in_literal_quote: false,
            saw_separator: false,
            value_quote: Quote::None,
            value_start: 0,
        }
    }
}

impl CompletionContext {
    fn open_quote_style(&self) -> Option<Quote> {
        if !self.saw_separator || self.comment {
            return None;
        }
        match self.value_quote {
            Quote::Single | Quote::Double | Quote::Backtick => Some(self.value_quote),
            _ => None,
        }
    }

    /// Scans the prefix of an incomplete or invalid item from its start to the
    /// cursor, following `adotenv`'s quote-open, escape and comment rules.
    #[allow(clippy::too_many_lines)]
    fn of(prefix: &str, dialect: Dialect) -> Self {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Phase {
            KeyStart,
            QuotedKey,
            UnquotedKey,
            ValueStart,
            UnquotedValue,
            SingleValue,
            DoubleValue,
            BacktickValue,
            AfterQuotedValue,
        }

        let mut phase = Phase::KeyStart;
        let mut escaped = false;
        let mut value_start = 0usize;
        let mut value_body_start = 0usize;
        let mut close_index = 0usize;
        let mut chars = prefix.char_indices().peekable();
        while let Some((index, character)) = chars.next() {
            match phase {
                Phase::KeyStart => {
                    if character == ' ' || character == '\t' {
                        continue;
                    }
                    if character == '#' {
                        return Self {
                            comment: true,
                            ..Self::default()
                        };
                    }
                    if character == '\'' && dialect.quoted_keys {
                        phase = Phase::QuotedKey;
                    } else if is_assignment_separator(character, chars.peek(), dialect) {
                        value_start = index + character.len_utf8();
                        value_body_start = value_start;
                        phase = Phase::ValueStart;
                    } else {
                        phase = Phase::UnquotedKey;
                    }
                }
                Phase::QuotedKey => {
                    if character == '\'' {
                        phase = Phase::UnquotedKey;
                    }
                }
                Phase::UnquotedKey => {
                    if is_assignment_separator(character, chars.peek(), dialect) {
                        value_start = index + character.len_utf8();
                        value_body_start = value_start;
                        phase = Phase::ValueStart;
                    } else if character == '#' && inline_comment_starts(prefix, index, 0, dialect) {
                        return Self {
                            comment: true,
                            ..Self::default()
                        };
                    }
                }
                Phase::ValueStart => {
                    if dialect.raw_values {
                        value_body_start = value_start;
                        phase = Phase::UnquotedValue;
                        continue;
                    }
                    if character == ' ' || character == '\t' {
                        value_body_start = index + character.len_utf8();
                        continue;
                    }
                    value_body_start = index;
                    match character {
                        '\'' => phase = Phase::SingleValue,
                        '"' => phase = Phase::DoubleValue,
                        '`' if dialect.backticks => phase = Phase::BacktickValue,
                        '#' if inline_comment_starts(prefix, index, value_body_start, dialect) => {
                            return Self {
                                comment: true,
                                saw_separator: true,
                                value_start,
                                ..Self::default()
                            };
                        }
                        _ => phase = Phase::UnquotedValue,
                    }
                }
                Phase::UnquotedValue => {
                    if !dialect.raw_values
                        && character == '#'
                        && inline_comment_starts(prefix, index, value_body_start, dialect)
                    {
                        return Self {
                            comment: true,
                            saw_separator: true,
                            value_start,
                            ..Self::default()
                        };
                    }
                }
                Phase::SingleValue => {
                    if dialect.syntax == Syntax::Node {
                        if character == '\'' {
                            close_index = index;
                            phase = Phase::AfterQuotedValue;
                        }
                    } else if dialect.syntax == Syntax::Python
                        || dialect.single_escapes == SingleEscapes::QuoteAndBackslash
                    {
                        if escaped {
                            escaped = false;
                        } else if character == '\\' {
                            escaped = true;
                        } else if character == '\'' {
                            close_index = index;
                            phase = Phase::AfterQuotedValue;
                        }
                    } else if dialect.single_escapes == SingleEscapes::Quote {
                        if character == '\\'
                            && chars.peek().is_some_and(|(_, next)| {
                                *next == '\''
                                    || (dialect.syntax == Syntax::Compose && *next == '\\')
                            })
                        {
                            let _ = chars.next();
                        } else if character == '\'' {
                            close_index = index;
                            phase = Phase::AfterQuotedValue;
                        }
                    } else if character == '\'' {
                        close_index = index;
                        phase = Phase::AfterQuotedValue;
                    }
                }
                Phase::DoubleValue => {
                    if escaped {
                        escaped = false;
                    } else if character == '\\' && dialect.syntax != Syntax::Node {
                        escaped = true;
                    } else if character == '"' {
                        close_index = index;
                        phase = Phase::AfterQuotedValue;
                    }
                }
                Phase::BacktickValue => {
                    if escaped {
                        escaped = false;
                    } else if character == '\\' && dialect.syntax != Syntax::Node {
                        escaped = true;
                    } else if character == '`' {
                        close_index = index;
                        phase = Phase::AfterQuotedValue;
                    }
                }
                Phase::AfterQuotedValue => {
                    if character == '#'
                        && dialect.inline_comments != InlineComments::None
                        && ((index == close_index + 1 && dialect.quoted_comments)
                            || inline_comment_starts(prefix, index, close_index + 1, dialect))
                    {
                        return Self {
                            comment: true,
                            saw_separator: true,
                            value_start,
                            ..Self::default()
                        };
                    }
                }
            }
        }
        let saw_separator = !matches!(
            phase,
            Phase::KeyStart | Phase::QuotedKey | Phase::UnquotedKey
        );
        let value_quote = match phase {
            Phase::SingleValue => Quote::Single,
            Phase::DoubleValue => Quote::Double,
            Phase::BacktickValue => Quote::Backtick,
            _ => Quote::None,
        };
        Self {
            comment: false,
            in_literal_quote: matches!(phase, Phase::SingleValue | Phase::BacktickValue),
            saw_separator,
            value_quote,
            value_start,
        }
    }
}

fn is_assignment_separator(
    character: char,
    next: Option<&(usize, char)>,
    dialect: Dialect,
) -> bool {
    if character == '=' {
        return true;
    }
    if character != ':' {
        return false;
    }
    match dialect.colon {
        ColonSeparator::Any => true,
        ColonSeparator::Whitespace => next.is_some_and(|(_, ch)| ch.is_whitespace()),
        _ => false,
    }
}

fn inline_comment_starts(prefix: &str, index: usize, body_start: usize, dialect: Dialect) -> bool {
    match dialect.inline_comments {
        InlineComments::None => false,
        InlineComments::Space => {
            index > body_start
                && index.checked_sub(1).is_some_and(|before| {
                    prefix
                        .as_bytes()
                        .get(before)
                        .is_some_and(|byte| *byte == b' ')
                })
        }
        InlineComments::Whitespace | InlineComments::Python => prefix[..index]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace),
        // `AnyHash` and future variants start a comment at any `#`.
        _ => true,
    }
}

fn format_markdown_inline_code(text: &str) -> String {
    let escaped;
    let content = if text.contains(['\n', '\r']) {
        escaped = text
            .replace("\r\n", "\\n")
            .replace('\n', "\\n")
            .replace('\r', "\\r");
        escaped.as_str()
    } else {
        text
    };
    let fence_len = longest_backtick_run(content) + 1;
    let fence = "`".repeat(fence_len);
    let needs_pad = content.starts_with('`')
        || content.ends_with('`')
        || (content.starts_with(' ')
            && content.ends_with(' ')
            && content.bytes().any(|b| b != b' '));
    if needs_pad {
        format!("{fence} {content} {fence}")
    } else {
        format!("{fence}{content}{fence}")
    }
}

fn longest_backtick_run(text: &str) -> usize {
    let mut longest = 0usize;
    let mut current = 0usize;
    for byte in text.bytes() {
        if byte == b'`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn format_markdown_code(text: &str) -> String {
    if text.contains(['\n', '\r']) {
        let fence_len = longest_backtick_run(text).max(2) + 1;
        let fence = "`".repeat(fence_len);
        format!("\n{fence}text\n{text}\n{fence}")
    } else {
        format_markdown_inline_code(text)
    }
}

fn name_at<'a>(document: &'a Document<'_>, offset: usize) -> Option<&'a str> {
    if let Some(reference) = reference_at(document, offset) {
        return reference_name(document, &reference);
    }
    let entry = entry_at(document, offset)?;
    let key = entry.key_span();
    (key.start..=key.end)
        .contains(&offset)
        .then_some(entry.key())
}

fn entry_at<'a>(document: &'a Document<'_>, offset: usize) -> Option<Entry<'a>> {
    document.entries().find(|entry| {
        entry.line_span().contains(&offset)
            || (offset == document.source().len() && entry.key_span().end == offset)
    })
}

fn reference_at(document: &Document<'_>, offset: usize) -> Option<Reference> {
    // An outer operator or command span contains every nested reference.
    // Select the deepest candidate in the containing entry so a position
    // inside `${OUTER:-$INNER}` resolves to `INNER`, not the containing form.
    let entry = entry_at(document, offset)?;
    entry
        .references()
        .into_iter()
        .filter(|reference| reference.span.contains(&offset))
        .max_by_key(|reference| reference.depth)
}

fn reference_name<'a>(document: &'a Document<'_>, reference: &Reference) -> Option<&'a str> {
    let name = reference.name.clone()?;
    document.source().get(name)
}

fn entry_content_span(document: &Document<'_>, entry: Entry<'_>) -> std::ops::Range<usize> {
    document
        .item(entry.item_id())
        .map_or_else(|| entry.line_span(), adotenv::Item::content_span)
}

/// Owned line-offset map that converts between UTF-8 byte offsets and LSP
/// line/character positions under UTF-8, UTF-16 or UTF-32 without borrowing
/// the source text.
#[derive(Debug, Clone)]
struct LineMap {
    lines: Vec<(usize, usize)>,
    bom_bytes: usize,
    first_visible_line0_width: u32,
    encoding: PositionEncoding,
}

impl LineMap {
    fn new(text: &str, dialect: Dialect, encoding: PositionEncoding) -> Self {
        let bytes = text.as_bytes();
        let mut lines = Vec::new();
        let mut start = 0usize;
        let mut offset = 0usize;
        while offset < bytes.len() {
            match bytes[offset] {
                b'\r' | b'\n' => {
                    lines.push((start, offset));
                    if bytes[offset] == b'\r' && bytes.get(offset + 1) == Some(&b'\n') {
                        offset += 2;
                    } else {
                        offset += 1;
                    }
                    start = offset;
                }
                _ => offset += 1,
            }
        }
        lines.push((start, text.len()));

        let bom_chars = match dialect.bom {
            BomPolicy::StripAll => text.chars().take_while(|ch| *ch == '\u{feff}').count(),
            BomPolicy::Keep | BomPolicy::Error => 0,
            _ => usize::from(text.starts_with('\u{feff}')),
        };
        let bom_bytes = bom_chars * '\u{feff}'.len_utf8();
        let first_visible_line0_width = text
            .get(bom_bytes..)
            .and_then(|rest| rest.chars().next())
            .filter(|ch| *ch != '\n' && *ch != '\r')
            .map_or(0, |ch| match encoding {
                PositionEncoding::Utf8 => u32::try_from(ch.len_utf8()).unwrap_or(1),
                PositionEncoding::Utf16 => u32::try_from(ch.len_utf16()).unwrap_or(1),
                PositionEncoding::Utf32 => 1,
            });
        Self {
            lines,
            bom_bytes,
            first_visible_line0_width,
            encoding,
        }
    }

    fn content_start(&self, line: usize) -> usize {
        if line == 0 {
            self.bom_bytes
        } else {
            self.lines[line].0
        }
    }

    /// Converts an LSP `Position` to a valid UTF-8 byte offset in `text`.
    ///
    /// Adopts LSP 3.17 §3.1.1 clamping: when `position.line` is valid and
    /// `position.character` exceeds the line's visible length, clamps to the
    /// end of that line. Out-of-bounds lines, split UTF-16 surrogates, and
    /// mid-character UTF-8 byte columns return `None`.
    fn offset(&self, text: &str, position: Position) -> Option<usize> {
        let line = usize::try_from(position.line).ok()?;
        let &(_, line_end) = self.lines.get(line)?;
        let content_start = self.content_start(line);
        let content = &text[content_start..line_end];
        let target_col = usize::try_from(position.character).ok()?;
        match self.encoding {
            PositionEncoding::Utf8 => {
                if target_col >= content.len() {
                    Some(line_end)
                } else if content.is_char_boundary(target_col) {
                    Some(content_start + target_col)
                } else {
                    None
                }
            }
            PositionEncoding::Utf16 => {
                let mut col = 0usize;
                for (byte_offset, ch) in content.char_indices() {
                    if col == target_col {
                        return Some(content_start + byte_offset);
                    }
                    col += ch.len_utf16();
                    if col > target_col {
                        return None;
                    }
                }
                Some(line_end)
            }
            PositionEncoding::Utf32 => {
                for (col, (byte_offset, _)) in content.char_indices().enumerate() {
                    if col == target_col {
                        return Some(content_start + byte_offset);
                    }
                }
                Some(line_end)
            }
        }
    }

    fn apply_change_in_place(
        &self,
        text: &mut String,
        range: Option<Range>,
        replacement: &str,
    ) -> bool {
        let Some(range) = range else {
            text.clear();
            text.push_str(replacement);
            return true;
        };
        if range.start > range.end {
            return false;
        }
        let Some(start) = self.offset(text, range.start) else {
            return false;
        };
        let Some(end) = self.offset(text, range.end) else {
            return false;
        };
        if start > end || end > text.len() {
            return false;
        }
        text.replace_range(start..end, replacement);
        true
    }

    fn apply_change(&self, text: &str, range: Option<Range>, replacement: &str) -> Option<String> {
        if range.is_none() {
            return Some(replacement.to_owned());
        }
        let mut output = text.to_owned();
        self.apply_change_in_place(&mut output, range, replacement)
            .then_some(output)
    }

    fn range(&self, text: &str, span: std::ops::Range<usize>) -> Range {
        if self.bom_bytes > 0 && span.start == 0 && span.end == self.bom_bytes {
            // The byte-order mark has no visible width. Anchor its diagnostic
            // to the full encoded width of the first visible character on line
            // 0 (so surrogate pairs or multi-byte chars are not split), or
            // (0, 0)..(0, 0) when line 0 has no visible character.
            return Range::new(
                Position {
                    line: 0,
                    character: 0,
                },
                Position {
                    line: 0,
                    character: self.first_visible_line0_width,
                },
            );
        }
        Range::new(
            self.position(text, span.start),
            self.position(text, span.end),
        )
    }

    fn position(&self, text: &str, offset: usize) -> Position {
        if offset > text.len() {
            return Position::new(0, 0);
        }
        let line = self
            .lines
            .partition_point(|&(start, _)| start <= offset)
            .saturating_sub(1);
        let (_, line_end) = self.lines[line];
        // A diagnostic span can land between `\r` and `\n` of a CRLF line
        // ending (for example an unknown escape `\\\r\n`); snap monotonically
        // to the end of that line instead of falling back to (0, 0).
        let clamped = offset.min(line_end);
        let content_start = self.content_start(line);
        let column = if clamped <= content_start {
            0
        } else {
            match self.encoding {
                PositionEncoding::Utf8 => clamped - content_start,
                PositionEncoding::Utf16 | PositionEncoding::Utf32 => {
                    if !text.is_char_boundary(clamped) {
                        return Position::new(0, 0);
                    }
                    let prefix = &text[content_start..clamped];
                    match self.encoding {
                        PositionEncoding::Utf16 => prefix.chars().map(char::len_utf16).sum(),
                        PositionEncoding::Utf32 => prefix.chars().count(),
                        PositionEncoding::Utf8 => unreachable!(),
                    }
                }
            }
        };
        Position {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            character: u32::try_from(column).unwrap_or(u32::MAX),
        }
    }
}
