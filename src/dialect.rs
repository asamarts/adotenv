//! Explicit literal syntax configuration. Presets perform no expansion or I/O.

/// The statement grammar used to recognize bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Syntax {
    /// The crate's conservative, lossless default grammar.
    Portable,
    /// Node dotenv's default regular-expression grammar.
    Node,
    /// Python dotenv's stream grammar.
    Python,
    /// Compose's dotenv grammar, without interpolation.
    Compose,
    /// Docker's raw key/value-file grammar.
    Docker,
}

/// Recognition of inline comments (whole-statement comments are always enabled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InlineComments {
    /// A hash preceded by configured whitespace.
    Whitespace,
    /// A hash preceded by a literal ASCII space within the value/header tail.
    Space,
    /// Any hash starts a comment.
    AnyHash,
    /// Whitespace-delimited values, but any hash after a bare key (Python).
    Python,
    /// Hashes in values are literal.
    None,
}

/// Independently enabled double-quoted escape sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DoubleEscapes(u16);
impl DoubleEscapes {
    /// No recognized escapes.
    pub const NONE: Self = Self(0);
    /// `\\n`.
    pub const LF: Self = Self(1);
    /// `\\r`.
    pub const CR: Self = Self(1 << 1);
    /// `\\t`.
    pub const TAB: Self = Self(1 << 2);
    /// `\\\\`.
    pub const BACKSLASH: Self = Self(1 << 3);
    /// `\\"`.
    pub const DOUBLE_QUOTE: Self = Self(1 << 4);
    /// `\\$` (literal dollar, not interpolation).
    pub const DOLLAR: Self = Self(1 << 5);
    /// `\\'`.
    pub const SINGLE_QUOTE: Self = Self(1 << 6);
    /// `\\a`.
    pub const BELL: Self = Self(1 << 7);
    /// `\\b`.
    pub const BACKSPACE: Self = Self(1 << 8);
    /// `\\f`.
    pub const FORM_FEED: Self = Self(1 << 9);
    /// `\\v`.
    pub const VERTICAL_TAB: Self = Self(1 << 10);
    /// Compose's `\\0` followed by up to three octal digits.
    pub const OCTAL: Self = Self(1 << 11);
    /// Portable's newline, carriage return, tab, slash, quote and dollar escapes.
    pub const PORTABLE: Self = Self(0x3f);
    /// Node's newline and carriage return substitutions.
    pub const NODE: Self = Self(3);
    /// Python dotenv's recognized double-quoted escapes.
    pub const PYTHON: Self = Self(0x7df);
    /// Compose's recognized escapes, including its octal-prefix behavior.
    pub const COMPOSE: Self = Self(0xfbf);
    /// Enable every sequence enabled by either set.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    /// Whether every sequence in `other` is enabled.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Handling of unrecognized double-quoted escapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnknownEscapes {
    /// Keep both characters literally.
    Preserve,
    /// Keep both characters and issue a warning.
    Warn,
    /// Remove the backslash only.
    DropBackslash,
    /// Retain the binding but report an error.
    Error,
}
/// Escapes recognized inside single quotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SingleEscapes {
    /// No single-quoted escapes.
    None,
    /// Decode escaped apostrophes.
    Quote,
    /// Decode escaped apostrophes and backslashes.
    QuoteAndBackslash,
}
/// Colon assignment separators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColonSeparator {
    /// Only equals signs separate keys and values.
    None,
    /// A colon separates only when followed by configured whitespace.
    Whitespace,
    /// Any colon separates.
    Any,
}
/// Validation of unquoted key bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeySyntax {
    /// ASCII letters/underscore followed by letters, digits or underscore.
    Posix,
    /// Nonempty ASCII letters, digits, underscore, dot or hyphen.
    Portable,
    /// Unicode 17 letters/numbers, underscore, dot or hyphen.
    Unicode,
    /// Nonempty, excluding Unicode whitespace and equals signs.
    NonWhitespace,
    /// Nonempty, excluding Python whitespace, equals signs and hashes.
    Python,
    /// Compose's Unicode letters/numbers, punctuation and horizontal whitespace.
    Compose,
    /// Nonempty, excluding literal ASCII space, tab and equals signs.
    Docker,
}
/// Effective treatment of declarations with no assignment separator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BareKeys {
    /// Retain a participating declaration whose value is absent.
    Preserve,
    /// Retain syntax but exclude it from lookup.
    Ignore,
    /// Look up the declaration as an empty string without changing its syntax.
    Empty,
    /// Retain syntax and report an error.
    Error,
}
/// Physical line recognition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineBreaks {
    /// LF, CRLF and lone CR terminate lines.
    Universal,
    /// LF terminates lines; one final CR belongs to the line ending.
    Lf,
}
/// Treatment of an initial UTF-8 byte-order mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BomPolicy {
    /// Retain it as a separate prelude and diagnose it informationally.
    Strip,
    /// Treat it as ordinary statement text.
    Keep,
    /// Retain it as an invalid item and report an error.
    Error,
}
/// Which participating duplicate declaration wins lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Duplicates {
    /// The first declaration wins.
    First,
    /// The last declaration wins.
    Last,
}
/// Handling of noncomment text following a closing quote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuotedTail {
    /// Reject the statement.
    Error,
    /// Keep the tail token but omit it from the decoded value.
    Ignore,
    /// Append the literal tail to the decoded quoted body.
    Append,
    /// Reinterpret the opening physical line as an unquoted value.
    LiteralFallback,
    /// Ignore the complete statement with a warning.
    Skip,
    /// End the item at the quote and parse the remainder as another statement.
    NextStatement,
}
/// Independently enabled multiline quote forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Multiline {
    /// Permit line endings inside single quotes.
    pub single: bool,
    /// Permit line endings inside double quotes.
    pub double: bool,
    /// Permit line endings inside backticks.
    pub backtick: bool,
}
impl Multiline {
    /// Permit multiline content in every quote form.
    pub const ALL: Self = Self {
        single: true,
        double: true,
        backtick: true,
    };
    /// Forbid multiline content in every quote form.
    pub const NONE: Self = Self {
        single: false,
        double: false,
        backtick: false,
    };
}
/// Whitespace classification used by syntax policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Whitespace {
    /// ASCII space and tab.
    Ascii,
    /// Rust's Unicode `White_Space` classification.
    Unicode,
    /// ECMAScript whitespace, including BOM but excluding NEL.
    EcmaScript,
    /// Unicode `White_Space` plus U+001C through U+001F.
    Python,
    /// Compose's horizontal whitespace classification.
    Compose,
}
/// Handling of malformed statements, excluding explicit policy violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MalformedLines {
    /// Retain invalid syntax and report an error.
    Error,
    /// Retain ignored syntax and report a warning.
    Ignore,
}

/// Complete, explicit literal parsing and lookup configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
// These orthogonal syntax switches are independently overrideable, not states.
#[allow(clippy::struct_excessive_bools)]
#[non_exhaustive]
pub struct Dialect {
    /// Statement grammar.
    pub syntax: Syntax,
    /// Unquoted inline comment recognition.
    pub inline_comments: InlineComments,
    /// Recognized double-quoted escapes.
    pub double_escapes: DoubleEscapes,
    /// Unrecognized double-quoted escape policy.
    pub unknown_escapes: UnknownEscapes,
    /// Single-quoted escape policy.
    pub single_escapes: SingleEscapes,
    /// Recognize backtick-quoted values.
    pub backticks: bool,
    /// Colon assignment separator policy.
    pub colon: ColonSeparator,
    /// Unquoted key validation.
    pub keys: KeySyntax,
    /// Bare declaration lookup/error policy.
    pub bare_keys: BareKeys,
    /// Keep all bytes after the separator as a literal value.
    pub raw_values: bool,
    /// Physical line recognition.
    pub line_breaks: LineBreaks,
    /// Leading byte-order mark policy.
    pub bom: BomPolicy,
    /// Winning duplicate declaration.
    pub duplicates: Duplicates,
    /// Noncomment text following a quote.
    pub quoted_tail: QuotedTail,
    /// Multiline quote permissions.
    pub multiline: Multiline,
    /// Whitespace classification.
    pub whitespace: Whitespace,
    /// Recognize the export prefix.
    pub export: bool,
    /// Permit single-quoted key tokens.
    pub quoted_keys: bool,
    /// Permit comments immediately after closing quotes.
    pub quoted_comments: bool,
    /// Normalize decoded CRLF and CR to LF, preserving original source maps.
    pub normalize_newlines: bool,
    /// Malformed statement policy.
    pub malformed_lines: MalformedLines,
    /// Exclude Node's special `__proto__` property from lookup.
    pub node_object_keys: bool,
    /// Maximum bytes per physical line before its final LF, including CR.
    pub line_byte_limit: Option<usize>,
}
impl Dialect {
    /// Conservative Portable syntax; the default configuration.
    pub const PORTABLE: Self = Self {
        syntax: Syntax::Portable,
        inline_comments: InlineComments::Whitespace,
        double_escapes: DoubleEscapes::PORTABLE,
        unknown_escapes: UnknownEscapes::Warn,
        single_escapes: SingleEscapes::None,
        backticks: false,
        colon: ColonSeparator::None,
        keys: KeySyntax::Portable,
        bare_keys: BareKeys::Preserve,
        raw_values: false,
        line_breaks: LineBreaks::Universal,
        bom: BomPolicy::Strip,
        duplicates: Duplicates::Last,
        quoted_tail: QuotedTail::Error,
        multiline: Multiline::ALL,
        whitespace: Whitespace::Ascii,
        export: true,
        quoted_keys: false,
        quoted_comments: false,
        normalize_newlines: false,
        malformed_lines: MalformedLines::Error,
        node_object_keys: false,
        line_byte_limit: None,
    };
    /// Node dotenv 18's default grammar and literal substitutions.
    pub const NODE_DOTENV: Self = Self {
        syntax: Syntax::Node,
        whitespace: Whitespace::EcmaScript,
        inline_comments: InlineComments::AnyHash,
        double_escapes: DoubleEscapes::NODE,
        unknown_escapes: UnknownEscapes::Preserve,
        backticks: true,
        colon: ColonSeparator::Whitespace,
        bare_keys: BareKeys::Ignore,
        quoted_tail: QuotedTail::LiteralFallback,
        quoted_comments: true,
        normalize_newlines: true,
        malformed_lines: MalformedLines::Ignore,
        node_object_keys: true,
        ..Self::PORTABLE
    };
    /// Python dotenv 1.2's stream grammar, without interpolation.
    pub const PYTHON_DOTENV: Self = Self {
        syntax: Syntax::Python,
        whitespace: Whitespace::Python,
        inline_comments: InlineComments::Python,
        double_escapes: DoubleEscapes::PYTHON,
        unknown_escapes: UnknownEscapes::Preserve,
        single_escapes: SingleEscapes::QuoteAndBackslash,
        keys: KeySyntax::Python,
        quoted_keys: true,
        quoted_comments: true,
        ..Self::PORTABLE
    };
    /// Compose dotenv's grammar and escapes, without interpolation or inheritance.
    pub const COMPOSE: Self = Self {
        syntax: Syntax::Compose,
        whitespace: Whitespace::Compose,
        inline_comments: InlineComments::Space,
        double_escapes: DoubleEscapes::COMPOSE,
        unknown_escapes: UnknownEscapes::Preserve,
        single_escapes: SingleEscapes::Quote,
        colon: ColonSeparator::Any,
        keys: KeySyntax::Compose,
        bare_keys: BareKeys::Ignore,
        line_breaks: LineBreaks::Lf,
        quoted_tail: QuotedTail::NextStatement,
        quoted_comments: true,
        ..Self::PORTABLE
    };
    /// Docker CLI's raw kvfile grammar, with no environment inheritance.
    pub const DOCKER_RAW: Self = Self {
        syntax: Syntax::Docker,
        whitespace: Whitespace::Unicode,
        inline_comments: InlineComments::None,
        double_escapes: DoubleEscapes::NONE,
        unknown_escapes: UnknownEscapes::Preserve,
        keys: KeySyntax::Docker,
        bare_keys: BareKeys::Ignore,
        raw_values: true,
        line_breaks: LineBreaks::Lf,
        multiline: Multiline::NONE,
        export: false,
        line_byte_limit: Some(65_535),
        ..Self::PORTABLE
    };
}
impl Dialect {
    /// Use the given statement grammar.
    #[must_use]
    pub const fn with_syntax(mut self, syntax: Syntax) -> Self {
        self.syntax = syntax;
        self
    }
    /// Use the given unquoted inline-comment policy.
    #[must_use]
    pub const fn with_inline_comments(mut self, value: InlineComments) -> Self {
        self.inline_comments = value;
        self
    }
    /// Use the given double-quoted escape set.
    #[must_use]
    pub const fn with_double_escapes(mut self, value: DoubleEscapes) -> Self {
        self.double_escapes = value;
        self
    }
    /// Use the given unknown-escape policy.
    #[must_use]
    pub const fn with_unknown_escapes(mut self, value: UnknownEscapes) -> Self {
        self.unknown_escapes = value;
        self
    }
    /// Use the given single-quoted escape policy.
    #[must_use]
    pub const fn with_single_escapes(mut self, value: SingleEscapes) -> Self {
        self.single_escapes = value;
        self
    }
    /// Enable or disable backtick-quoted values.
    #[must_use]
    pub const fn with_backticks(mut self, value: bool) -> Self {
        self.backticks = value;
        self
    }
    /// Use the given colon-separator policy.
    #[must_use]
    pub const fn with_colon(mut self, value: ColonSeparator) -> Self {
        self.colon = value;
        self
    }
    /// Use the given unquoted key syntax.
    #[must_use]
    pub const fn with_keys(mut self, value: KeySyntax) -> Self {
        self.keys = value;
        self
    }
    /// Use the given bare-key policy.
    #[must_use]
    pub const fn with_bare_keys(mut self, value: BareKeys) -> Self {
        self.bare_keys = value;
        self
    }
    /// Enable or disable raw value retention.
    #[must_use]
    pub const fn with_raw_values(mut self, value: bool) -> Self {
        self.raw_values = value;
        self
    }
    /// Use the given physical line-break policy.
    #[must_use]
    pub const fn with_line_breaks(mut self, value: LineBreaks) -> Self {
        self.line_breaks = value;
        self
    }
    /// Use the given byte-order-mark policy.
    #[must_use]
    pub const fn with_bom(mut self, value: BomPolicy) -> Self {
        self.bom = value;
        self
    }
    /// Use the given duplicate lookup policy.
    #[must_use]
    pub const fn with_duplicates(mut self, value: Duplicates) -> Self {
        self.duplicates = value;
        self
    }
    /// Use the given quoted-tail policy.
    #[must_use]
    pub const fn with_quoted_tail(mut self, value: QuotedTail) -> Self {
        self.quoted_tail = value;
        self
    }
    /// Use the given multiline quote permissions.
    #[must_use]
    pub const fn with_multiline(mut self, value: Multiline) -> Self {
        self.multiline = value;
        self
    }
    /// Use the given whitespace classification.
    #[must_use]
    pub const fn with_whitespace(mut self, value: Whitespace) -> Self {
        self.whitespace = value;
        self
    }
    /// Enable or disable the export prefix.
    #[must_use]
    pub const fn with_export(mut self, value: bool) -> Self {
        self.export = value;
        self
    }
    /// Enable or disable quoted key tokens.
    #[must_use]
    pub const fn with_quoted_keys(mut self, value: bool) -> Self {
        self.quoted_keys = value;
        self
    }
    /// Enable or disable comments after closing quotes.
    #[must_use]
    pub const fn with_quoted_comments(mut self, value: bool) -> Self {
        self.quoted_comments = value;
        self
    }
    /// Enable or disable decoded newline normalization.
    #[must_use]
    pub const fn with_normalize_newlines(mut self, value: bool) -> Self {
        self.normalize_newlines = value;
        self
    }
    /// Use the given malformed-line policy.
    #[must_use]
    pub const fn with_malformed_lines(mut self, value: MalformedLines) -> Self {
        self.malformed_lines = value;
        self
    }
    /// Enable or disable Node object-key exclusions.
    #[must_use]
    pub const fn with_node_object_keys(mut self, value: bool) -> Self {
        self.node_object_keys = value;
        self
    }
    /// Set the optional per-line byte limit.
    #[must_use]
    pub const fn with_line_byte_limit(mut self, value: Option<usize>) -> Self {
        self.line_byte_limit = value;
        self
    }
}
impl Default for Dialect {
    fn default() -> Self {
        Self::PORTABLE
    }
}
