//! Language Server Protocol support for `adotenv` documents.
//!
//! The library is transport-free: callers pass source text and an LSP
//! [`Uri`], and receive LSP types. The binary in this crate adapts the
//! functions to stdio using `lsp-server`.
//!
//! The operations are conservative. Definition follows keys and literal
//! `$VAR`/`${VAR}` references; rename uses `Document::rename_with_references`
//! so the declaration and its literal references change together. Hover values
//! can be masked with [`LspOptions::with_mask_values`], the parser dialect can
//! be selected with [`LspOptions::with_dialect`], and linter findings can be
//! enabled with [`LspOptions::with_lint_options`].

use std::{
    collections::{BTreeSet, HashMap},
    fmt,
};

use adotenv::{
    parse_strict_with_options, parse_with_options, BomPolicy, ColumnEncoding, Document, EditError,
    Entry, InlineComments, LineIndex, Position as ParserPosition, Reference,
};
pub use adotenv::{
    Dialect, EqualsSpacing, LimitExceeded, LimitKind, ParseOptions, QuoteStyle, Severity,
};
use adotenv_lint::lint;
pub use adotenv_lint::{KeyCase, LintOptions, Rule};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, Diagnostic as LspDiagnostic,
    DiagnosticRelatedInformation, DiagnosticSeverity, DocumentSymbol, DocumentSymbolResponse,
    GotoDefinitionResponse, Hover, HoverContents, Location, MarkupContent, MarkupKind,
    NumberOrString, Position, Range, SymbolInformation, SymbolKind, TextEdit, Uri, WorkspaceEdit,
};

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
    /// Optional parser bounds applied to every document parse.
    pub parse_options: ParseOptions,
}

impl Default for LspOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl LspOptions {
    /// Returns Portable parsing, visible hover values, no linter and no parse
    /// limits.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            mask_values: false,
            dialect: Dialect::PORTABLE,
            lint: None,
            parse_options: ParseOptions::new(),
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

    /// Enables an explicit linter policy for diagnostics.
    #[must_use]
    pub const fn with_lint_options(mut self, lint: LintOptions) -> Self {
        self.lint = Some(lint);
        self
    }

    /// Sets opt-in [`ParseOptions`] bounds applied when parsing documents.
    #[must_use]
    pub const fn with_parse_options(mut self, parse_options: ParseOptions) -> Self {
        self.parse_options = parse_options;
        self
    }
}

/// A failed rename request.
#[derive(Debug)]
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

/// Converts parser diagnostics into LSP diagnostics.
///
/// Parser diagnostics are always included. Linter findings are added when
/// `options.lint` contains an explicit policy.
#[must_use]
pub fn diagnostics(text: &str, uri: &Uri, options: &LspOptions) -> Vec<LspDiagnostic> {
    let (document, diagnostics) = parse_with_options(text, options.dialect, options.parse_options);
    let mapped = Mapped::new_for_dialect(text, options.dialect);
    let mut output = diagnostics
        .into_iter()
        .map(|diagnostic| {
            let related_information = diagnostic.secondary_span().map(|span| {
                vec![DiagnosticRelatedInformation {
                    location: Location::new(uri.clone(), mapped.range(span)),
                    message: "related source location".to_owned(),
                }]
            });
            LspDiagnostic {
                range: mapped.range(diagnostic.span()),
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
    if let Some(lint_options) = options.lint {
        output.extend(
            lint(&document, lint_options)
                .into_iter()
                .map(|finding| LspDiagnostic {
                    range: mapped.range(finding.span()),
                    severity: Some(severity(finding.severity())),
                    code: Some(NumberOrString::String(format!(
                        "adotenv-lint.{}",
                        finding.rule().as_str()
                    ))),
                    source: Some("adotenv-lint".to_owned()),
                    message: finding.message().to_owned(),
                    ..LspDiagnostic::default()
                }),
        );
    }
    output
}

fn severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        _ => DiagnosticSeverity::INFORMATION,
    }
}

/// Returns completion items for declared keys.
///
/// The same list is returned for a key position and after `$`/`${`; clients
/// can filter it using their normal completion prefix.
#[must_use]
pub fn completion(text: &str, position: Position) -> CompletionResponse {
    completion_with_options(text, position, &LspOptions::new())
}

/// Returns completion items for declared keys using `options`.
///
/// This is [`completion`] with an explicit dialect and policy.
#[must_use]
pub fn completion_with_options(
    text: &str,
    position: Position,
    options: &LspOptions,
) -> CompletionResponse {
    let mapped = Mapped::new_for_dialect(text, options.dialect);
    let Some(offset) = mapped.offset(position) else {
        return CompletionResponse::Array(Vec::new());
    };
    let line_start = text[..offset]
        .rfind(['\n', '\r'])
        .map_or(0, |index| index + 1);
    let content_start = if line_start == 0 {
        mapped.bom_bytes.min(offset)
    } else {
        line_start
    };
    let prefix = &text[content_start..offset];
    let trimmed = prefix.trim_start();
    if trimmed.starts_with('#') {
        return CompletionResponse::Array(Vec::new());
    }
    let context = CompletionContext::of(prefix, options.dialect);
    if context.comment || context.in_single_quote {
        // A `#` outside quotes can start a comment depending on the dialect,
        // and a `$` in a single-quoted value is literal text, not a reference.
        return CompletionResponse::Array(Vec::new());
    }
    let key_prefix = trimmed
        .strip_prefix("export")
        .map_or(trimmed, |rest| rest.trim_start_matches([' ', '\t']));
    let at_key_start = !key_prefix.contains('=')
        && !key_prefix.contains('#')
        && !key_prefix.chars().any(char::is_whitespace);
    let after_reference = prefix.rfind('$').is_some_and(|dollar| {
        let tail = &prefix[dollar + 1..];
        let name = tail.strip_prefix('{').unwrap_or(tail);
        name.chars()
            .all(|character| character.is_alphanumeric() || character == '_')
    });
    if !at_key_start && !after_reference {
        return CompletionResponse::Array(Vec::new());
    }

    let (document, _) = parse_with_options(text, options.dialect, options.parse_options);
    let mut keys = BTreeSet::new();
    for entry in document.entries() {
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

#[derive(Debug, Default)]
struct CompletionContext {
    comment: bool,
    in_single_quote: bool,
}

impl CompletionContext {
    /// Scans the prefix of the current line without parsing the whole value.
    /// Double-quoted text can contain `#` and apostrophes literally, and the
    /// dialect decides whether a `#` starts an inline comment.
    fn of(prefix: &str, dialect: Dialect) -> Self {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Quote {
            None,
            Single,
            Double,
        }

        let mut quote = Quote::None;
        let mut escaped = false;
        let mut saw_equals = false;
        for (index, character) in prefix.char_indices() {
            match quote {
                Quote::Single => {
                    if character == '\'' {
                        quote = Quote::None;
                    }
                }
                Quote::Double => {
                    if escaped {
                        escaped = false;
                    } else if character == '\\' {
                        escaped = true;
                    } else if character == '"' {
                        quote = Quote::None;
                    }
                }
                Quote::None => match character {
                    '\'' => quote = Quote::Single,
                    '"' => quote = Quote::Double,
                    '=' => saw_equals = true,
                    '#' if inline_comment_starts(prefix, index, dialect) => {
                        return Self {
                            comment: true,
                            in_single_quote: false,
                        };
                    }
                    _ => {}
                },
            }
        }
        Self {
            comment: false,
            in_single_quote: saw_equals && quote == Quote::Single,
        }
    }
}

fn inline_comment_starts(prefix: &str, index: usize, dialect: Dialect) -> bool {
    match dialect.inline_comments {
        InlineComments::None => false,
        InlineComments::Space => index.checked_sub(1).is_some_and(|before| {
            prefix
                .as_bytes()
                .get(before)
                .is_some_and(|byte| *byte == b' ')
        }),
        InlineComments::Whitespace | InlineComments::Python => prefix[..index]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace),
        // `AnyHash` and future variants start a comment at any `#`.
        _ => true,
    }
}

/// Returns an outline of the document's declared keys.
///
/// Each entry becomes a [`SymbolKind::KEY`] symbol. The range covers the
/// declaration and the selection range covers its key. Values are never
/// included in the symbol data.
#[must_use]
#[allow(deprecated)]
pub fn document_symbols(text: &str) -> DocumentSymbolResponse {
    document_symbols_with_options(text, &LspOptions::new())
}

/// Returns an outline of the document's declared keys using `options`.
///
/// This is [`document_symbols`] with an explicit dialect.
#[must_use]
#[allow(deprecated)]
pub fn document_symbols_with_options(text: &str, options: &LspOptions) -> DocumentSymbolResponse {
    let (document, _) = parse_with_options(text, options.dialect, options.parse_options);
    let mapped = Mapped::new_for_dialect(text, options.dialect);
    DocumentSymbolResponse::Nested(
        document
            .entries()
            .map(|entry| DocumentSymbol {
                name: entry.key().to_owned(),
                detail: None,
                kind: SymbolKind::KEY,
                tags: None,
                deprecated: None,
                range: mapped.range(entry_content_span(&document, entry)),
                selection_range: mapped.range(entry.key_span()),
                children: None,
            })
            .collect(),
    )
}

/// Returns workspace symbols for declared keys in one document.
///
/// `query` is matched case-insensitively against the key. The result uses flat
/// [`SymbolInformation`] values so clients without hierarchical symbol support
/// can consume it. Values are never included.
#[must_use]
#[allow(deprecated)]
pub fn workspace_symbols(text: &str, uri: &Uri, query: &str) -> Vec<SymbolInformation> {
    workspace_symbols_with_options(text, uri, query, &LspOptions::new())
}

/// Returns workspace symbols for declared keys using `options`.
///
/// This is [`workspace_symbols`] with an explicit dialect.
#[must_use]
#[allow(deprecated)]
pub fn workspace_symbols_with_options(
    text: &str,
    uri: &Uri,
    query: &str,
    options: &LspOptions,
) -> Vec<SymbolInformation> {
    let (document, _) = parse_with_options(text, options.dialect, options.parse_options);
    let mapped = Mapped::new_for_dialect(text, options.dialect);
    let query = query.to_lowercase();
    document
        .entries()
        .filter(|entry| query.is_empty() || entry.key().to_lowercase().contains(&query))
        .map(|entry| SymbolInformation {
            name: entry.key().to_owned(),
            kind: SymbolKind::KEY,
            tags: None,
            deprecated: None,
            location: Location::new(
                uri.clone(),
                mapped.range(entry_content_span(&document, entry)),
            ),
            container_name: None,
        })
        .collect()
}

/// Applies one LSP content change to a document string.
///
/// A `None` range replaces the whole document. A ranged change uses UTF-16
/// line and column coordinates, including the same BOM-adjusted columns as the
/// rest of this crate. Returns `None` when the range is out of bounds.
#[must_use]
pub fn apply_change(text: &str, range: Option<Range>, replacement: &str) -> Option<String> {
    apply_change_with_options(text, range, replacement, &LspOptions::new())
}

/// Applies one LSP content change to a document string using `options`.
///
/// This is [`apply_change`] with an explicit dialect, matching the BOM policy
/// used for position mapping across the rest of the crate (including
/// [`BomPolicy::StripAll`] and [`BomPolicy::Keep`]).
#[must_use]
pub fn apply_change_with_options(
    text: &str,
    range: Option<Range>,
    replacement: &str,
    options: &LspOptions,
) -> Option<String> {
    let Some(range) = range else {
        return Some(replacement.to_owned());
    };
    let mapped = Mapped::new_for_dialect(text, options.dialect);
    let start = mapped.offset(range.start)?;
    let end = mapped.offset(range.end)?;
    if start > end || end > text.len() {
        return None;
    }
    let mut output = text.to_owned();
    output.replace_range(start..end, replacement);
    Some(output)
}

/// Finds the declaration of the key or reference at `position`.
#[must_use]
pub fn definition(text: &str, uri: &Uri, position: Position) -> Option<GotoDefinitionResponse> {
    definition_with_options(text, uri, position, &LspOptions::new())
}

/// Finds the declaration of the key or reference at `position` using `options`.
///
/// This is [`definition`] with an explicit dialect.
#[must_use]
pub fn definition_with_options(
    text: &str,
    uri: &Uri,
    position: Position,
    options: &LspOptions,
) -> Option<GotoDefinitionResponse> {
    let (document, _) = parse_with_options(text, options.dialect, options.parse_options);
    let mapped = Mapped::new_for_dialect(text, options.dialect);
    let offset = mapped.offset(position)?;
    let name = name_at(&document, offset)?;
    let entry = document.get_entry(name)?;
    Some(GotoDefinitionResponse::Scalar(Location::new(
        uri.clone(),
        mapped.range(entry.key_span()),
    )))
}

/// Returns hover information for a key or reference.
#[must_use]
pub fn hover(text: &str, position: Position, options: &LspOptions) -> Option<Hover> {
    let (document, _) = parse_with_options(text, options.dialect, options.parse_options);
    let mapped = Mapped::new_for_dialect(text, options.dialect);
    let offset = mapped.offset(position)?;
    if let Some(reference) = reference_at(&document, offset) {
        let name = reference_name(&document, &reference)?;
        let target = document.get_entry(name).map_or_else(
            || "; unresolved".to_owned(),
            |entry| format!("; declared on line {}", entry.line()),
        );
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("`${{{name}}}`{target}"),
            }),
            range: Some(mapped.range(reference.span.clone())),
        });
    }
    let entry = entry_at(&document, offset)?;
    if entry
        .tokens()
        .inline_comment
        .as_ref()
        .is_some_and(|comment| comment.contains(&offset))
    {
        return None;
    }
    let value = match entry.value() {
        Some(_) if options.mask_values => "`<redacted>`".to_owned(),
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
        range: Some(mapped.range(entry.key_span())),
    })
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

fn format_markdown_inline_code(text: &str) -> String {
    let fence_len = longest_backtick_run(text) + 1;
    let fence = "`".repeat(fence_len);
    let needs_pad = text.starts_with('`')
        || text.ends_with('`')
        || (text.starts_with(' ') && text.ends_with(' ') && text.bytes().any(|b| b != b' '));
    if needs_pad {
        format!("{fence} {text} {fence}")
    } else {
        format!("{fence}{text}{fence}")
    }
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

/// Finds declarations and value references for the symbol at `position`.
#[must_use]
pub fn references(
    text: &str,
    uri: &Uri,
    position: Position,
    include_declaration: bool,
) -> Vec<Location> {
    references_with_options(text, uri, position, include_declaration, &LspOptions::new())
}

/// Finds declarations and value references for the symbol at `position` using
/// `options`.
///
/// This is [`references`] with an explicit dialect.
#[must_use]
pub fn references_with_options(
    text: &str,
    uri: &Uri,
    position: Position,
    include_declaration: bool,
    options: &LspOptions,
) -> Vec<Location> {
    let (document, _) = parse_with_options(text, options.dialect, options.parse_options);
    let mapped = Mapped::new_for_dialect(text, options.dialect);
    let Some(offset) = mapped.offset(position) else {
        return Vec::new();
    };
    let Some(name) = name_at(&document, offset) else {
        return Vec::new();
    };
    let mut locations = Vec::new();
    for entry in document.entries() {
        if entry.key() == name && include_declaration {
            locations.push(Location::new(uri.clone(), mapped.range(entry.key_span())));
        }
        for reference in entry.references() {
            if reference_name(&document, &reference) == Some(name) {
                locations.push(Location::new(
                    uri.clone(),
                    mapped.range(reference.span.clone()),
                ));
            }
        }
    }
    locations
}

/// Renames a key declaration and its literal references.
///
/// The symbol may be selected from a declaration key or from a `$VAR`/`${VAR}`
/// reference. The result uses the verified text edits returned by
/// `Document::rename_with_references`.
///
/// # Errors
///
/// Returns [`RenameError::NoSymbol`] when `position` is not on a key or
/// reference, and [`RenameError::Edit`] when the rename is invalid or, with
/// matching references present, a new key cannot be written as a variable
/// reference.
pub fn rename(
    text: &str,
    uri: &Uri,
    position: Position,
    new_name: &str,
) -> Result<WorkspaceEdit, RenameError> {
    rename_with_options(text, uri, position, new_name, &LspOptions::new())
}

/// Renames a key declaration and its literal references using `options`.
///
/// This is [`rename`] with an explicit dialect.
///
/// # Errors
///
/// Returns [`RenameError::NoSymbol`] when the document cannot be parsed or
/// `position` is not on a key or reference, and [`RenameError::Edit`] when a
/// configured parse limit is exceeded ([`EditError::Limit`]), the rename is
/// invalid, or matching references exist and the new key cannot be written as a
/// variable reference.
pub fn rename_with_options(
    text: &str,
    uri: &Uri,
    position: Position,
    new_name: &str,
    options: &LspOptions,
) -> Result<WorkspaceEdit, RenameError> {
    let document = parse_strict_with_options(text, options.dialect, options.parse_options)
        .map_err(|diagnostic| {
            diagnostic.limit().map_or(RenameError::NoSymbol, |limit| {
                RenameError::Edit(EditError::Limit(limit))
            })
        })?;
    let mapped = Mapped::new_for_dialect(text, options.dialect);
    let offset = mapped.offset(position).ok_or(RenameError::NoSymbol)?;
    let old_name = name_at(&document, offset)
        .ok_or(RenameError::NoSymbol)?
        .to_owned();
    let edits = document.rename_with_references(&old_name, new_name)?;
    let changes = edits
        .edits()
        .iter()
        .map(|edit| TextEdit {
            range: mapped.range(edit.range.clone()),
            new_text: edit.replacement.clone(),
        })
        .collect::<Vec<_>>();
    Ok(WorkspaceEdit {
        changes: Some(HashMap::from([(uri.clone(), changes)])),
        ..WorkspaceEdit::default()
    })
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
    document
        .entries()
        .find(|entry| entry.line_span().contains(&offset))
}

fn reference_at(document: &Document<'_>, offset: usize) -> Option<Reference> {
    // An outer operator or command span contains every nested reference.
    // Select the deepest candidate so a position inside `${OUTER:-$INNER}`
    // resolves to `INNER`, not to the containing form.
    document
        .entries()
        .flat_map(Entry::references)
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

struct Mapped<'a> {
    index: LineIndex<'a>,
    bom_chars: usize,
    bom_bytes: usize,
}

impl<'a> Mapped<'a> {
    fn new_for_dialect(text: &'a str, dialect: Dialect) -> Self {
        let bom_chars = match dialect.bom {
            BomPolicy::StripAll => text.chars().take_while(|ch| *ch == '\u{feff}').count(),
            BomPolicy::Keep => 0,
            _ => usize::from(text.starts_with('\u{feff}')),
        };
        Self {
            index: LineIndex::new(text),
            bom_chars,
            bom_bytes: bom_chars * '\u{feff}'.len_utf8(),
        }
    }

    fn offset(&self, position: Position) -> Option<usize> {
        let line = usize::try_from(position.line).ok()?;
        let mut column = usize::try_from(position.character).ok()?;
        if line == 0 && self.bom_chars > 0 {
            column = column.checked_add(self.bom_chars)?;
        }
        self.index
            .to_offset(ParserPosition { line, column }, ColumnEncoding::Utf16)
            .ok()
    }

    fn range(&self, span: std::ops::Range<usize>) -> Range {
        if self.bom_bytes > 0 && span.start == 0 && span.end == self.bom_bytes {
            // The byte-order mark has no visible width. Anchor its diagnostic
            // to the first visible character instead of a zero-width range.
            return Range::new(
                Position {
                    line: 0,
                    character: 0,
                },
                Position {
                    line: 0,
                    character: 1,
                },
            );
        }
        Range::new(self.position(span.start), self.position(span.end))
    }

    fn position(&self, offset: usize) -> Position {
        match self.index.to_position(offset, ColumnEncoding::Utf16) {
            Ok(position) => {
                let column = if position.line == 0 && self.bom_chars > 0 {
                    position.column.saturating_sub(self.bom_chars)
                } else {
                    position.column
                };
                Position {
                    line: u32::try_from(position.line).unwrap_or(u32::MAX),
                    character: u32::try_from(column).unwrap_or(u32::MAX),
                }
            }
            Err(_) => Position {
                line: 0,
                character: 0,
            },
        }
    }
}
