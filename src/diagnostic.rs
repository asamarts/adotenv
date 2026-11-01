use core::{error::Error, fmt, ops::Range};

/// How serious a [`Diagnostic`] is.
///
/// Each [`DiagnosticCode`] has a fixed severity ([`DiagnosticCode::severity`]).
/// The enum is non-exhaustive, so a `match` needs a wildcard arm.
///
/// # Examples
///
/// ```
/// use adotenv::Severity;
///
/// let (_, diagnostics) = adotenv::parse("PORT=8080\nPORT=9090\nBAD KEY=1\n");
/// let count = |severity: Severity| {
///     diagnostics.iter().filter(|d| d.severity() == severity).count()
/// };
///
/// assert_eq!(count(Severity::Error), 1);
/// assert_eq!(count(Severity::Warning), 1);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Severity {
    /// The statement breaks the dialect's grammar or one of its policies.
    ///
    /// An error fails [`parse_strict`](crate::parse_strict),
    /// [`parse_values`](crate::parse_values) and their `_with` variants. A
    /// recovering [`parse`](crate::parse) keeps a malformed statement as an
    /// [`ItemKind::Invalid`](crate::ItemKind::Invalid) item; a declaration
    /// that breaks a policy, such as [`DiagnosticCode::ForbiddenBareKey`],
    /// stays an entry with the diagnostic attached
    /// ([`Item::diagnostic`](crate::Item::diagnostic)).
    Error,
    /// The text is accepted, but perhaps not as the author meant.
    ///
    /// Examples are a duplicate key, a `#` that is part of a value, and a
    /// statement the dialect skips. A warning never fails a parse.
    Warning,
    /// A neutral fact about the source, such as a byte-order mark.
    Info,
}

/// The kind of a [`Diagnostic`], as a stable identifier.
///
/// Match on codes, not on message text, which may change. Each code has a
/// fixed [`Severity`] and a stable text form from [`DiagnosticCode::as_str`],
/// such as `dotenv.duplicate-key`, which the CLI also prints.
///
/// In dialects that ignore malformed lines
/// ([`MalformedLines::Ignore`](crate::MalformedLines::Ignore), as in Node
/// dotenv), [`DiagnosticCode::IgnoredStatement`] replaces every error code
/// except the four policy codes: [`DiagnosticCode::ForbiddenBareKey`],
/// [`DiagnosticCode::ForbiddenByteOrderMark`],
/// [`DiagnosticCode::InvalidEscape`] and [`DiagnosticCode::LineTooLong`].
///
/// The enum is non-exhaustive: a minor release can add codes. Handle an
/// unknown code by its severity.
///
/// # Examples
///
/// ```
/// use adotenv::{DiagnosticCode, Severity};
///
/// let (_, diagnostics) = adotenv::parse("PORT=8080\nPORT=9090\n");
/// let code = diagnostics[0].code();
///
/// assert_eq!(code, DiagnosticCode::DuplicateKey);
/// assert_eq!(code.as_str(), "dotenv.duplicate-key");
/// assert_eq!(code.severity(), Severity::Warning);
///
/// let advice = match code {
///     DiagnosticCode::DuplicateKey => "remove one of the declarations",
///     DiagnosticCode::InvalidKey => "rename the key",
///     _ => code.message(),
/// };
/// assert_eq!(advice, "remove one of the declarations");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DiagnosticCode {
    /// A key is empty or contains a character the dialect does not allow.
    ///
    /// Error, `dotenv.invalid-key`. Portable keys use ASCII letters, digits,
    /// `_`, `.` and `-`; other dialects set their own rules
    /// ([`Dialect::keys`](crate::Dialect::keys)). A statement such as
    /// `BAD KEY=value`, whose key would contain a space, is reported with this
    /// code too. The statement becomes an
    /// [`ItemKind::Invalid`](crate::ItemKind::Invalid) item.
    InvalidKey,
    /// A quoted value has no closing quote.
    ///
    /// Error, `dotenv.unterminated-quote`. The quote is not closed before the
    /// end of the source, or before the end of the line when the dialect does
    /// not let that quote span lines
    /// ([`Dialect::multiline`](crate::Dialect::multiline)). Only the line with
    /// the opening quote becomes an
    /// [`ItemKind::Invalid`](crate::ItemKind::Invalid) item, and parsing
    /// continues on the next line.
    UnterminatedQuote,
    /// Text other than a comment follows a closing quote, as in `A="x"y`.
    ///
    /// Error, `dotenv.trailing-text`, in dialects that reject such text, such
    /// as Portable and Python dotenv
    /// ([`QuotedTail::Error`](crate::QuotedTail::Error)). The whole statement
    /// becomes an [`ItemKind::Invalid`](crate::ItemKind::Invalid) item.
    TrailingText,
    /// A double-quoted escape that the dialect does not define is kept as
    /// written.
    ///
    /// Warning, `dotenv.unknown-escape`, reported under
    /// [`UnknownEscapes::Warn`](crate::UnknownEscapes::Warn), as in Portable.
    /// For example `"\q"` keeps both the backslash and the `q`.
    UnknownEscape,
    /// A declaration that takes part in lookup repeats the key of an earlier
    /// one.
    ///
    /// Warning, `dotenv.duplicate-key`, reported on every later declaration
    /// of the key that takes part in lookup
    /// ([`Entry::participates_in_lookup`](crate::Entry::participates_in_lookup)).
    /// The primary span is the key of the later declaration and the secondary
    /// span is the key of the first declaration that takes part. Which
    /// declaration wins lookup depends on
    /// [`Dialect::duplicates`](crate::Dialect::duplicates).
    DuplicateKey,
    /// A declaration has no `=` and no value, such as `DEBUG`.
    ///
    /// Warning, `dotenv.bare-key`, Portable syntax only. When the dialect
    /// forbids bare keys, [`DiagnosticCode::ForbiddenBareKey`] is reported
    /// instead.
    BareKey,
    /// A key is accepted but is not a POSIX shell variable name, such as
    /// `app.port` or `1A`.
    ///
    /// Warning, `dotenv.non-posix-key`, Portable syntax only. A POSIX name
    /// uses ASCII letters, digits and `_` and does not start with a digit.
    NonPosixKey,
    /// A `#` in an unquoted value is literal text, not a comment, as in
    /// `A=a#b`.
    ///
    /// Warning, `dotenv.tight-hash`, Portable syntax only, reported for each
    /// such `#`. Portable starts a comment only at a `#` after whitespace, so
    /// `A=a #b` reads `a`. Some dialects, such as Node dotenv, start a comment
    /// at any `#`.
    TightHash,
    /// A backtick in an unquoted value is literal text, not a quote.
    ///
    /// Warning, `dotenv.backtick`, Portable syntax only, reported for each
    /// backtick. Node dotenv reads backtick-quoted values.
    Backtick,
    /// The source starts with a UTF-8 byte-order mark.
    ///
    /// Info, `dotenv.bom`, reported when the dialect strips the mark
    /// ([`BomPolicy::Strip`](crate::BomPolicy::Strip), used by every preset).
    /// See [`Document::has_bom`](crate::Document::has_bom).
    ByteOrderMark,
    /// A carriage return is not followed by a line feed.
    ///
    /// Warning, `dotenv.lone-cr`, Portable syntax with
    /// [`LineBreaks::Universal`](crate::LineBreaks::Universal) only. Portable
    /// ends a line at a lone CR. Every lone CR in the source is reported,
    /// including one inside a quoted value. Node dotenv and python-dotenv also
    /// end a line there, without this warning; Compose and Docker keep a lone
    /// CR as text inside the line.
    LoneCarriageReturn,
    /// A declaration has no `=` and the dialect forbids that.
    ///
    /// Error, `dotenv.forbidden-bare-key`, under
    /// [`BareKeys::Error`](crate::BareKeys::Error). The declaration stays an
    /// entry with this diagnostic attached
    /// ([`Item::diagnostic`](crate::Item::diagnostic)).
    ForbiddenBareKey,
    /// The source starts with a UTF-8 byte-order mark and the dialect forbids
    /// that.
    ///
    /// Error, `dotenv.forbidden-bom`, under
    /// [`BomPolicy::Error`](crate::BomPolicy::Error). The three bytes of the
    /// mark become an [`ItemKind::Invalid`](crate::ItemKind::Invalid) item.
    ForbiddenByteOrderMark,
    /// A double-quoted escape that the dialect does not define, in a dialect
    /// that rejects such escapes.
    ///
    /// Error, `dotenv.invalid-escape`, under
    /// [`UnknownEscapes::Error`](crate::UnknownEscapes::Error). The
    /// declaration stays an entry with this diagnostic attached
    /// ([`Item::diagnostic`](crate::Item::diagnostic)), and its value keeps
    /// the escape as written. When a value has several such escapes,
    /// [`parse`](crate::parse) reports each one and the entry carries the
    /// first.
    InvalidEscape,
    /// A statement is kept in the source, but the dialect ignores it.
    ///
    /// Warning, `dotenv.ignored-statement`. Dialects that ignore malformed
    /// lines ([`MalformedLines::Ignore`](crate::MalformedLines::Ignore), as in
    /// Node dotenv) report it in place of the error codes for malformed
    /// statements, and [`QuotedTail::Skip`](crate::QuotedTail::Skip) reports
    /// it for a statement with text after its closing quote. The statement
    /// becomes an [`ItemKind::Ignored`](crate::ItemKind::Ignored) item.
    IgnoredStatement,
    /// A line is longer than the dialect's byte limit.
    ///
    /// Error, `dotenv.line-too-long`, set with
    /// [`Dialect::with_line_byte_limit`](crate::Dialect::with_line_byte_limit).
    /// The count leaves out the line's final LF but includes a CR before it.
    /// The Docker preset allows 65,535 bytes; the other presets have no limit.
    /// The statement becomes an [`ItemKind::Invalid`](crate::ItemKind::Invalid)
    /// item.
    LineTooLong,
}

impl DiagnosticCode {
    /// Returns the stable text form of the code, such as
    /// `"dotenv.duplicate-key"`.
    ///
    /// Use it to identify a code in logs, configuration or machine-readable
    /// output. It does not change with the enum order or the message wording.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidKey => "dotenv.invalid-key",
            Self::UnterminatedQuote => "dotenv.unterminated-quote",
            Self::TrailingText => "dotenv.trailing-text",
            Self::UnknownEscape => "dotenv.unknown-escape",
            Self::DuplicateKey => "dotenv.duplicate-key",
            Self::BareKey => "dotenv.bare-key",
            Self::NonPosixKey => "dotenv.non-posix-key",
            Self::TightHash => "dotenv.tight-hash",
            Self::Backtick => "dotenv.backtick",
            Self::ByteOrderMark => "dotenv.bom",
            Self::LoneCarriageReturn => "dotenv.lone-cr",
            Self::ForbiddenBareKey => "dotenv.forbidden-bare-key",
            Self::ForbiddenByteOrderMark => "dotenv.forbidden-bom",
            Self::InvalidEscape => "dotenv.invalid-escape",
            Self::IgnoredStatement => "dotenv.ignored-statement",
            Self::LineTooLong => "dotenv.line-too-long",
        }
    }

    /// Returns the severity of every diagnostic with this code.
    #[must_use]
    pub const fn severity(self) -> Severity {
        match self {
            Self::InvalidKey
            | Self::UnterminatedQuote
            | Self::TrailingText
            | Self::ForbiddenBareKey
            | Self::ForbiddenByteOrderMark
            | Self::InvalidEscape
            | Self::LineTooLong => Severity::Error,
            Self::ByteOrderMark => Severity::Info,
            _ => Severity::Warning,
        }
    }

    /// Returns a short description of the code in English, for people to read.
    ///
    /// The text may change between releases; match on the code itself or on
    /// [`DiagnosticCode::as_str`] instead.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::InvalidKey => "key does not match the selected dialect",
            Self::UnterminatedQuote => "quoted value has no closing delimiter",
            Self::TrailingText => "unexpected text after the closing quote",
            Self::UnknownEscape => "unknown escape retained literally",
            Self::DuplicateKey => "duplicate key; lookup follows the selected dialect policy",
            Self::BareKey => "bare declaration retained without assigning a value",
            Self::NonPosixKey => "key is not a POSIX shell variable name",
            Self::TightHash => "hash in an unquoted value is literal text, not a comment",
            Self::Backtick => "backticks are literal in Portable syntax",
            Self::ByteOrderMark => "leading UTF-8 byte-order mark retained",
            Self::LoneCarriageReturn => "carriage return without a following line feed",
            Self::ForbiddenBareKey => "bare declaration forbidden by the selected policy",
            Self::ForbiddenByteOrderMark => {
                "leading byte-order mark forbidden by the selected policy"
            }
            Self::InvalidEscape => "escape rejected by the selected policy",
            Self::IgnoredStatement => "statement ignored by the selected dialect",
            Self::LineTooLong => "source line exceeds the selected scanner byte limit",
        }
    }
}

/// A problem or note that parsing reports about the source, with its location.
///
/// [`parse`](crate::parse) returns every diagnostic, ordered by position, and
/// [`parse_strict`](crate::parse_strict) and
/// [`parse_values`](crate::parse_values) return the first error as a
/// `Diagnostic`, which implements [`Error`].
///
/// A diagnostic has a stable [`DiagnosticCode`], a [`Severity`] set by that
/// code, a one-based line, a primary span and an optional secondary span.
/// Spans are byte ranges into the original source, byte-order mark included.
/// Match on [`Diagnostic::code`]; the message text may change. A diagnostic
/// displays as `line N: message (code)`.
///
/// # Examples
///
/// ```
/// use adotenv::{DiagnosticCode, Severity};
///
/// let (_, diagnostics) = adotenv::parse("A=1\nA=2\n");
/// let duplicate = &diagnostics[0];
///
/// assert_eq!(duplicate.code(), DiagnosticCode::DuplicateKey);
/// assert_eq!(duplicate.severity(), Severity::Warning);
/// assert_eq!(duplicate.line(), 2);
/// assert_eq!(duplicate.span(), 4..5);
/// assert_eq!(duplicate.secondary_span(), Some(0..1));
/// assert_eq!(
///     duplicate.to_string(),
///     "line 2: duplicate key; lookup follows the selected dialect policy (dotenv.duplicate-key)"
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub(crate) code: DiagnosticCode,
    pub(crate) span: Range<usize>,
    pub(crate) secondary: Option<Range<usize>>,
    pub(crate) line: usize,
}

impl Diagnostic {
    pub(crate) fn new(code: DiagnosticCode, span: Range<usize>, line: usize) -> Self {
        Self {
            code,
            span,
            secondary: None,
            line,
        }
    }

    /// Returns the diagnostic's code.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.code
    }
    /// Returns the diagnostic's severity, which its code determines.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.code.severity()
    }
    /// Returns the byte range in the source that the diagnostic is about.
    ///
    /// Offsets count from the start of the source, byte-order mark included.
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }
    /// Returns a related byte range in the source, if any.
    ///
    /// For [`DiagnosticCode::DuplicateKey`] it is the key of the first
    /// declaration; other codes have none.
    #[must_use]
    pub fn secondary_span(&self) -> Option<Range<usize>> {
        self.secondary.clone()
    }
    /// Returns the one-based line where the primary span starts.
    ///
    /// Lines are counted as [`LineIndex`](crate::LineIndex) and
    /// [`Entry::line`](crate::Entry::line) count them: LF, CRLF and a lone CR
    /// each end a line, in every dialect.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }
    /// Returns a short description in English, for people to read.
    ///
    /// This is the [`DiagnosticCode::message`] of the code. The text may
    /// change between releases; match on [`Diagnostic::code`] instead.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.code.message()
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}: {} ({})",
            self.line,
            self.message(),
            self.code.as_str()
        )
    }
}
impl Error for Diagnostic {}
