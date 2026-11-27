//! Language Server Protocol support for `adotenv` documents.
//!
//! The library is transport-free: callers pass source text and an LSP
//! [`Uri`], and receive LSP types. The binary in this crate adapts the
//! functions to stdio using `lsp-server`.
//!
//! The operations are conservative. Definition follows keys and literal
//! `$VAR`/`${VAR}` references; rename uses
//! `Document::rename_with_references_with_options` so the declaration and its
//! literal references change together under the configured parse limits. Hover
//! values can be masked with [`LspOptions::with_mask_values`], the parser
//! dialect can be selected with [`LspOptions::with_dialect`], and linter
//! findings can be enabled with [`LspOptions::with_lint_options`].
//!
//! # Examples
//!
//! ```
//! use adotenv_lsp::{diagnostics, hover, rename, KeyCase, LintOptions, LspOptions};
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
//! assert!(diagnostics(source, &uri, &options).is_empty());
//! assert!(hover(source, Position::new(0, 0), &options).is_some());
//!
//! let edit = rename(source, &uri, Position::new(0, 0), "SERVER").unwrap();
//! assert_eq!(edit.changes.unwrap()[&uri].len(), 2);
//! ```

use std::{
    collections::{BTreeSet, HashMap},
    fmt,
};

use adotenv::{
    parse_with_options, ColonSeparator, ColumnEncoding, Diagnostic, DiagnosticCode, Document,
    DoubleEscapes, EditOptions, Entry, InlineComments, ItemKind, LineEnding, LineIndex,
    Position as ParserPosition, PositionError, Quote, Reference, SingleEscapes, Syntax,
};
pub use adotenv::{
    BomPolicy, Dialect, EditError, EqualsSpacing, LimitExceeded, LimitKind, ParseOptions,
    QuoteStyle, Severity,
};
use adotenv_lint::lint;
pub use adotenv_lint::{KeyCase, LintOptions, Rule};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, Diagnostic as LspDiagnostic,
    DiagnosticRelatedInformation, DiagnosticSeverity, DocumentSymbol, DocumentSymbolResponse,
    GotoDefinitionResponse, Hover, HoverContents, Location, MarkupContent, MarkupKind,
    NumberOrString, Position, Range, SymbolInformation, SymbolKind, TextEdit, Uri, WorkspaceEdit,
};

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
mod readme_doctests {}

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

    /// Disables linter findings, leaving only parser diagnostics enabled.
    #[must_use]
    pub const fn without_lint_options(mut self) -> Self {
        self.lint = None;
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
    let (document, _) = parse_with_options(text, options.dialect, options.parse_options);
    let Some((prefix, context)) =
        completion_context_at(&document, text, offset, mapped.bom_bytes, options.dialect)
    else {
        return CompletionResponse::Array(Vec::new());
    };
    let trimmed = prefix.trim_start();
    let key_prefix = if options.dialect.export {
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
            if is_escaped_dollar(value_prefix, dollar, context.value_quote, options.dialect) {
                return false;
            }
            let tail = &value_prefix[dollar + 1..];
            if let Some(braced) = tail.strip_prefix('{') {
                braced
                    .chars()
                    .all(|character| character.is_alphanumeric() || matches!(character, '_' | '.'))
            } else {
                is_unbraced_reference_prefix(tail)
            }
        });
    if !at_key_start && !after_reference {
        return CompletionResponse::Array(Vec::new());
    }

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
fn reference_nesting_at(before: &str) -> (bool, bool) {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Container {
        Braced,
        Command,
        Paren,
    }
    let bytes = before.as_bytes();
    let mut stack: Vec<Container> = Vec::new();
    let mut quote: Option<u8> = None;
    let mut index = 0;
    while index < bytes.len() {
        if stack.last() == Some(&Container::Braced) {
            match bytes[index] {
                b'$' if bytes.get(index + 1) == Some(&b'{') => {
                    stack.push(Container::Braced);
                    index += 2;
                }
                b'$' if bytes.get(index + 1) == Some(&b'(') => {
                    stack.push(Container::Command);
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
        if matches!(stack.last(), Some(Container::Command | Container::Paren)) {
            match quote {
                Some(b'\'') => {
                    if bytes[index] == b'\'' {
                        quote = None;
                    }
                    index += 1;
                    continue;
                }
                Some(_) => {
                    if bytes[index] == b'\\' {
                        index = index.saturating_add(2);
                    } else {
                        if bytes[index] == b'"' {
                            quote = None;
                        }
                        index += 1;
                    }
                    continue;
                }
                None => match bytes[index] {
                    b'\\' => {
                        index = index.saturating_add(2);
                        continue;
                    }
                    b'\'' => {
                        quote = Some(b'\'');
                        index += 1;
                        continue;
                    }
                    b'"' => {
                        quote = Some(b'"');
                        index += 1;
                        continue;
                    }
                    b'(' => {
                        stack.push(Container::Paren);
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
            }
        }
        match bytes[index] {
            b'$' if bytes.get(index + 1) == Some(&b'{') => {
                stack.push(Container::Braced);
                index += 2;
            }
            b'$' if bytes.get(index + 1) == Some(&b'(') => {
                stack.push(Container::Command);
                index += 2;
            }
            _ => index += 1,
        }
    }
    let in_command = stack
        .iter()
        .rev()
        .find(|container| **container != Container::Paren)
        == Some(&Container::Command);
    (stack.last() == Some(&Container::Braced), in_command)
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
                Phase::SingleValue => match dialect.single_escapes {
                    SingleEscapes::QuoteAndBackslash => {
                        if escaped {
                            escaped = false;
                        } else if character == '\\' {
                            escaped = true;
                        } else if character == '\'' {
                            close_index = index;
                            phase = Phase::AfterQuotedValue;
                        }
                    }
                    SingleEscapes::Quote => {
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
                    }
                    _ => {
                        if character == '\'' {
                            close_index = index;
                            phase = Phase::AfterQuotedValue;
                        }
                    }
                },
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
                value: format!(
                    "{}{target}",
                    format_markdown_inline_code(&format!("${{{name}}}"))
                ),
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
/// `Document::rename_with_references_with_options`.
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
/// This is [`rename`] with an explicit dialect and parse limits. Unrelated
/// invalid lines are preserved through recovering parsing and verified to
/// remain unchanged.
///
/// # Errors
///
/// Returns [`RenameError::NoSymbol`] when `position` is not on a key or
/// reference, and [`RenameError::Edit`] when a configured parse limit is
/// exceeded ([`EditError::Limit`]), the rename is invalid, matching references
/// exist and the new key cannot be written as a variable reference, or
/// verification fails (including `$(...)` command-context allowlist
/// rejections).
pub fn rename_with_options(
    text: &str,
    uri: &Uri,
    position: Position,
    new_name: &str,
    options: &LspOptions,
) -> Result<WorkspaceEdit, RenameError> {
    let (document, diagnostics) = parse_with_options(text, options.dialect, options.parse_options);
    if let Some(limit) = diagnostics.iter().find_map(Diagnostic::limit) {
        return Err(RenameError::Edit(EditError::Limit(limit)));
    }
    let mapped = Mapped::new_for_dialect(text, options.dialect);
    let offset = mapped.offset(position).ok_or(RenameError::NoSymbol)?;
    let old_name = name_at(&document, offset)
        .ok_or(RenameError::NoSymbol)?
        .to_owned();
    let edit_options = EditOptions::new().with_parse_limits(options.parse_options);
    let edits = document.rename_with_references_with_options(&old_name, new_name, edit_options)?;
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

struct Mapped<'a> {
    index: LineIndex<'a>,
    bom_chars: usize,
    bom_bytes: usize,
    first_visible_line0_utf16: u32,
}

impl<'a> Mapped<'a> {
    fn new_for_dialect(text: &'a str, dialect: Dialect) -> Self {
        let bom_chars = match dialect.bom {
            BomPolicy::StripAll => text.chars().take_while(|ch| *ch == '\u{feff}').count(),
            BomPolicy::Keep | BomPolicy::Error => 0,
            _ => usize::from(text.starts_with('\u{feff}')),
        };
        let bom_bytes = bom_chars * '\u{feff}'.len_utf8();
        let first_visible_line0_utf16 = text
            .get(bom_bytes..)
            .and_then(|rest| rest.chars().next())
            .filter(|ch| *ch != '\n' && *ch != '\r')
            .map_or(0, |ch| u32::try_from(ch.len_utf16()).unwrap_or(1));
        Self {
            index: LineIndex::new(text),
            bom_chars,
            bom_bytes,
            first_visible_line0_utf16,
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
            // to the full UTF-16 width of the first visible character on line 0
            // (so surrogate pairs are not split), or (0, 0)..(0, 0) when line 0
            // has no visible character.
            return Range::new(
                Position {
                    line: 0,
                    character: 0,
                },
                Position {
                    line: 0,
                    character: self.first_visible_line0_utf16,
                },
            );
        }
        Range::new(self.position(span.start), self.position(span.end))
    }

    fn position(&self, offset: usize) -> Position {
        let resolved = match self.index.to_position(offset, ColumnEncoding::Utf16) {
            Ok(position) => Ok(position),
            // A diagnostic span can land between `\r` and `\n` of a CRLF line
            // ending (for example an unknown escape `\\\r\n`); snap
            // monotonically to the end of that line instead of (0, 0).
            Err(PositionError::InsideCrLf) if offset > 0 => {
                self.index.to_position(offset - 1, ColumnEncoding::Utf16)
            }
            Err(error) => Err(error),
        };
        match resolved {
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
