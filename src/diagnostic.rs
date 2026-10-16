use core::{error::Error, fmt, ops::Range};

/// The importance of a parsing diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Severity {
    /// The statement or policy is invalid under the selected dialect.
    Error,
    /// Valid syntax whose interpretation is non-portable or ambiguous.
    Warning,
    /// Source metadata that callers may want to display.
    Info,
}

/// Stable diagnostic identities; messages are descriptive, not API identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DiagnosticCode {
    /// A key is empty or contains a character outside the selected key alphabet.
    InvalidKey,
    /// A quoted value has no matching delimiter before EOF.
    UnterminatedQuote,
    /// Non-comment text follows a closing quote.
    TrailingText,
    /// An unknown double-quoted escape is retained literally.
    UnknownEscape,
    /// An earlier assignment has the same key.
    DuplicateKey,
    /// A declaration has no equals sign or value.
    BareKey,
    /// A key is not a POSIX shell variable name.
    NonPosixKey,
    /// A hash attached to an unquoted value is literal, not a comment.
    TightHash,
    /// Backticks are literal, not a Portable quoting style.
    Backtick,
    /// A UTF-8 byte-order mark precedes the source body.
    ByteOrderMark,
    /// A lone carriage return is retained as a source line ending.
    LoneCarriageReturn,
    /// The selected policy forbids key-only declarations.
    ForbiddenBareKey,
    /// The selected policy forbids a leading byte-order mark.
    ForbiddenByteOrderMark,
    /// The selected policy rejects an unrecognized escape.
    InvalidEscape,
    /// A statement is retained but ignored by the selected dialect.
    IgnoredStatement,
    /// A source line exceeds the selected scanner's byte limit.
    LineTooLong,
}

impl DiagnosticCode {
    /// Stable machine-readable identity, independent of enum order and wording.
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

    /// The default severity assigned to this diagnostic identity.
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

    /// A human-readable explanation; callers should match codes, not this text.
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
            Self::TightHash => "hash without preceding whitespace is literal",
            Self::Backtick => "backticks are literal in Portable syntax",
            Self::ByteOrderMark => "leading UTF-8 byte-order mark retained",
            Self::LoneCarriageReturn => "lone carriage return retained as a line ending",
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

/// A source-relative diagnostic. Its primary span always addresses original bytes.
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

    /// Stable identity of this diagnostic.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.code
    }
    /// The severity of this diagnostic.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.code.severity()
    }
    /// The primary byte range, including BOM offsets.
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }
    /// A related source location, such as the first occurrence of a duplicate.
    #[must_use]
    pub fn secondary_span(&self) -> Option<Range<usize>> {
        self.secondary.clone()
    }
    /// The explicitly one-based source line number of the primary span.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }
    /// Descriptive text; use [`DiagnosticCode::as_str`] for machine matching.
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
