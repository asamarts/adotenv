//! Language Server Protocol support for `adotenv` documents.
//!
//! The library is transport-free: callers pass source text and an LSP
//! [`Uri`], and receive LSP types. The binary in this crate adapts the
//! functions to stdio using `lsp-server`.
//!
//! The operations are conservative. Definition follows keys and literal
//! `$VAR`/`${VAR}` references; rename uses `Document::rename_with_references`
//! so the declaration and its literal references change together. Hover values
//! can be masked with [`LspOptions::with_mask_values`] and linter findings can
//! be enabled with [`LspOptions::with_lint_options`].

use std::{
    collections::{BTreeSet, HashMap},
    fmt,
};

use adotenv::{
    parse, parse_strict, ColumnEncoding, Document, EditError, Entry, LineIndex,
    Position as ParserPosition, Reference,
};
pub use adotenv::{EqualsSpacing, QuoteStyle, Severity};
use adotenv_lint::lint;
pub use adotenv_lint::{KeyCase, LintOptions, Rule};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, Diagnostic as LspDiagnostic,
    DiagnosticRelatedInformation, DiagnosticSeverity, GotoDefinitionResponse, Hover, HoverContents,
    Location, MarkupContent, MarkupKind, NumberOrString, Position, Range, TextEdit, Uri,
    WorkspaceEdit,
};

/// Options for LSP operations.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct LspOptions {
    /// Replace hover values with `<redacted>`.
    pub mask_values: bool,
    /// Optional linter policy. Parser diagnostics remain enabled.
    pub lint: Option<LintOptions>,
}

impl LspOptions {
    /// Returns options that leave hover values visible.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            mask_values: false,
            lint: None,
        }
    }

    /// Sets whether hover values are masked.
    #[must_use]
    pub const fn with_mask_values(mut self, mask: bool) -> Self {
        self.mask_values = mask;
        self
    }

    /// Enables an explicit linter policy for diagnostics.
    #[must_use]
    pub const fn with_lint_options(mut self, lint: LintOptions) -> Self {
        self.lint = Some(lint);
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
    let (document, diagnostics) = parse(text);
    let mapped = Mapped::new(text);
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
    let Some(offset) = Mapped::new(text).offset(position) else {
        return CompletionResponse::Array(Vec::new());
    };
    let line_start = text[..offset]
        .rfind(['\n', '\r'])
        .map_or(0, |index| index + 1);
    let prefix = &text[line_start..offset];
    let trimmed = prefix.trim_start();
    if trimmed.starts_with('#') {
        return CompletionResponse::Array(Vec::new());
    }
    let key_prefix = trimmed.strip_prefix("export ").unwrap_or(trimmed);
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

    let (document, _) = parse(text);
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

/// Applies one LSP content change to a document string.
///
/// A `None` range replaces the whole document. A ranged change uses UTF-16
/// line and column coordinates, including the same BOM-adjusted columns as the
/// rest of this crate. Returns `None` when the range is out of bounds.
#[must_use]
pub fn apply_change(text: &str, range: Option<Range>, replacement: &str) -> Option<String> {
    let Some(range) = range else {
        return Some(replacement.to_owned());
    };
    let mapped = Mapped::new(text);
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
    let (document, _) = parse(text);
    let offset = Mapped::new(text).offset(position)?;
    let name = name_at(&document, offset)?;
    let entry = document.get_entry(name)?;
    Some(GotoDefinitionResponse::Scalar(Location::new(
        uri.clone(),
        Mapped::new(text).range(entry.key_span()),
    )))
}

/// Returns hover information for a key or reference.
#[must_use]
pub fn hover(text: &str, position: Position, options: &LspOptions) -> Option<Hover> {
    let (document, _) = parse(text);
    let mapped = Mapped::new(text);
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
    let value = match entry.value() {
        Some(_) if options.mask_values => "`<redacted>`".to_owned(),
        Some(value) => format!("`{value}`"),
        None => "`<bare>`".to_owned(),
    };
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!(
                "`{}` on line {} ({:?})\n\nValue: {value}",
                entry.key(),
                entry.line(),
                entry.state()
            ),
        }),
        range: Some(mapped.range(entry.key_span())),
    })
}

/// Finds declarations and value references for the symbol at `position`.
#[must_use]
pub fn references(
    text: &str,
    uri: &Uri,
    position: Position,
    include_declaration: bool,
) -> Vec<Location> {
    let (document, _) = parse(text);
    let Some(offset) = Mapped::new(text).offset(position) else {
        return Vec::new();
    };
    let Some(name) = name_at(&document, offset) else {
        return Vec::new();
    };
    let mapped = Mapped::new(text);
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
    let document = parse_strict(text).map_err(|_| RenameError::NoSymbol)?;
    let mapped = Mapped::new(text);
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
    entry.key_span().contains(&offset).then_some(entry.key())
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

struct Mapped<'a> {
    index: LineIndex<'a>,
    bom: bool,
}

impl<'a> Mapped<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            index: LineIndex::new(text),
            bom: text.starts_with('\u{feff}'),
        }
    }

    fn offset(&self, position: Position) -> Option<usize> {
        let line = usize::try_from(position.line).ok()?;
        let mut column = usize::try_from(position.character).ok()?;
        if line == 0 && self.bom {
            column = column.checked_add(1)?;
        }
        self.index
            .to_offset(ParserPosition { line, column }, ColumnEncoding::Utf16)
            .ok()
    }

    fn range(&self, span: std::ops::Range<usize>) -> Range {
        Range::new(self.position(span.start), self.position(span.end))
    }

    fn position(&self, offset: usize) -> Position {
        match self.index.to_position(offset, ColumnEncoding::Utf16) {
            Ok(position) => {
                let column = if position.line == 0 && self.bom {
                    position.column.saturating_sub(1)
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
