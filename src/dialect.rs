//! Explicit literal syntax configuration. Presets perform no expansion or I/O.

/// The statement grammar: how a line splits into key, separator and value.
///
/// Each grammar other than [`Portable`](Self::Portable) follows one tool's
/// parser, including its quirks. The other [`Dialect`] fields refine it: for
/// example [`Dialect::keys`] decides which keys are valid and
/// [`Dialect::inline_comments`] where comments start. Each preset pairs its
/// grammar with matching settings, so most callers pick a preset rather than a
/// grammar.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict_with, Dialect, Syntax};
///
/// assert_eq!(Dialect::DOCKER_RAW.syntax, Syntax::Docker);
///
/// // Portable accepts spaces before `=`; the Docker grammar keeps them in the key.
/// let docker = Dialect::PORTABLE.with_syntax(Syntax::Docker);
/// assert_eq!(parse_strict_with("PORT =8080\n", Dialect::PORTABLE)?.get("PORT"), Some("8080"));
/// assert!(parse_strict_with("PORT =8080\n", docker).is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Syntax {
    /// This crate's strict grammar, described on [`Dialect::PORTABLE`].
    ///
    /// It is the only grammar that reports Portable warnings such as
    /// [`DiagnosticCode::NonPosixKey`](crate::DiagnosticCode::NonPosixKey),
    /// [`DiagnosticCode::BareKey`](crate::DiagnosticCode::BareKey) and
    /// [`DiagnosticCode::TightHash`](crate::DiagnosticCode::TightHash).
    Portable,
    /// Node dotenv's default regular-expression parser.
    ///
    /// Its quirks are kept: for example, whitespace and even line breaks may
    /// separate `=` from an opening quote.
    Node,
    /// python-dotenv's stream parser.
    ///
    /// For example, `export # note` is a comment line rather than a malformed
    /// statement.
    Python,
    /// Docker Compose's env-file parser, without interpolation.
    ///
    /// With [`KeySyntax::Compose`], a bare key on a last line that has no line
    /// ending is read as a value with an empty key, as Compose does. In
    /// [`Entry::references`](crate::Entry::references), `$$` is an escaped
    /// dollar.
    Compose,
    /// The Docker CLI `--env-file` parser.
    ///
    /// Only leading whitespace is skipped. A key is everything up to `=`, so
    /// in `A =1` the key is `A ` (with a space), which [`KeySyntax::Docker`]
    /// rejects.
    Docker,
}

/// Where a `#` inside a line starts a comment.
///
/// A line whose first non-blank character is `#` is always a comment. This
/// policy decides when a later `#` starts an inline comment: inside an
/// unquoted value and after a closing quote. A `#` directly after a closing
/// quote, as in `A="x"#note`, also needs [`Dialect::quoted_comments`]. The
/// characters that count as whitespace come from [`Dialect::whitespace`].
///
/// After a bare key, the Portable grammar ([`Syntax::Portable`]) always starts
/// a comment at a `#` that follows whitespace, whatever this policy says: in
/// `DEBUG #note` there is no value for the `#` to belong to.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_with, Dialect, InlineComments};
///
/// let source = "COLOR=#ff0000 # red\n";
/// let (portable, _) = parse_with(source, Dialect::PORTABLE);
/// assert_eq!(portable.get("COLOR"), Some("#ff0000"));
///
/// let any_hash = Dialect::PORTABLE.with_inline_comments(InlineComments::AnyHash);
/// assert_eq!(parse_with(source, any_hash).0.get("COLOR"), Some(""));
///
/// let literal = Dialect::PORTABLE.with_inline_comments(InlineComments::None);
/// assert_eq!(parse_with(source, literal).0.get("COLOR"), Some("#ff0000 # red"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum InlineComments {
    /// A `#` after whitespace starts a comment.
    ///
    /// `A=a #b` reads `a`, and `A=a#b` reads `a#b`. Portable uses this.
    Whitespace,
    /// A `#` after an ASCII space starts a comment, but not at the start of a
    /// value.
    ///
    /// `A=a #b` reads `a`, while `A=a\t#b` and `A= #b` keep the hash.
    /// Compose uses this.
    Space,
    /// Every `#` in an unquoted value starts a comment.
    ///
    /// `A=a#b` reads `a`. Node dotenv uses this.
    AnyHash,
    /// Like [`Whitespace`](Self::Whitespace) in values, but any `#` right after
    /// a bare key starts a comment.
    ///
    /// With the Python grammar, `KEY#note` is the bare key `KEY`. python-dotenv
    /// uses this.
    Python,
    /// A `#` in a value is literal text.
    ///
    /// `A=a #b` reads `a #b`. After a closing quote, `#` is text that
    /// [`Dialect::quoted_tail`] handles, so `A="x" #b` is an error in Portable.
    /// Docker uses this.
    None,
}

/// The set of escape sequences that double-quoted values decode.
///
/// Each constant enables one sequence. Combine them with
/// [`union`](Self::union) and test them with [`contains`](Self::contains). A
/// backslash pair that is not in the set is handled by
/// [`Dialect::unknown_escapes`]. Single-quoted and backtick-quoted values do
/// not use this set.
///
/// | Set | Sequences |
/// |---|---|
/// | [`PORTABLE`](Self::PORTABLE) | `\n` `\r` `\t` `\\` `\"` `\$` |
/// | [`NODE`](Self::NODE) | `\n` `\r` |
/// | [`PYTHON`](Self::PYTHON) | `\n` `\r` `\t` `\\` `\"` `\'` `\a` `\b` `\f` `\v` |
/// | [`COMPOSE`](Self::COMPOSE) | `\n` `\r` `\t` `\\` `\"` `\$` `\a` `\b` `\f` `\v` `\0` |
///
/// Compose's octal escape is `\0` followed by three octal digits, such as
/// `\0101` for `A`. Otherwise, for example with fewer than three octal digits
/// or a number above 255, only the `0` is removed, so `\012` reads `\12`.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_with, Dialect, DoubleEscapes};
///
/// let escapes = DoubleEscapes::PORTABLE.union(DoubleEscapes::BELL);
/// assert!(escapes.contains(DoubleEscapes::LF));
///
/// let dialect = Dialect::PORTABLE.with_double_escapes(escapes);
/// let (doc, _) = parse_with("ALERT=\"ding\\a\"\n", dialect);
/// assert_eq!(doc.get("ALERT"), Some("ding\u{7}"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DoubleEscapes(u16);
impl DoubleEscapes {
    /// No escape sequences: every backslash pair is unknown.
    pub const NONE: Self = Self(0);
    /// `\n` becomes a line feed (LF).
    pub const LF: Self = Self(1);
    /// `\r` becomes a carriage return (CR).
    pub const CR: Self = Self(1 << 1);
    /// `\t` becomes a tab.
    pub const TAB: Self = Self(1 << 2);
    /// `\\` becomes one backslash.
    pub const BACKSLASH: Self = Self(1 << 3);
    /// `\"` becomes a double quote.
    pub const DOUBLE_QUOTE: Self = Self(1 << 4);
    /// `\$` becomes a dollar sign, and the reference after it counts as escaped.
    pub const DOLLAR: Self = Self(1 << 5);
    /// `\'` becomes a single quote.
    pub const SINGLE_QUOTE: Self = Self(1 << 6);
    /// `\a` becomes the bell character (U+0007).
    pub const BELL: Self = Self(1 << 7);
    /// `\b` becomes a backspace (U+0008).
    pub const BACKSPACE: Self = Self(1 << 8);
    /// `\f` becomes a form feed (U+000C).
    pub const FORM_FEED: Self = Self(1 << 9);
    /// `\v` becomes a vertical tab (U+000B).
    pub const VERTICAL_TAB: Self = Self(1 << 10);
    /// Compose's octal escape: `\0` followed by three octal digits.
    pub const OCTAL: Self = Self(1 << 11);
    /// Portable's set: `\n`, `\r`, `\t`, `\\`, `\"` and `\$`.
    pub const PORTABLE: Self = Self(0x3f);
    /// Node dotenv's set: `\n` and `\r`.
    pub const NODE: Self = Self(3);
    /// python-dotenv's set: Portable's without `\$`, plus `\'`, `\a`, `\b`, `\f` and `\v`.
    pub const PYTHON: Self = Self(0x7df);
    /// Compose's set: Portable's plus `\a`, `\b`, `\f`, `\v` and the octal `\0` escape.
    pub const COMPOSE: Self = Self(0xfbf);
    /// Returns the set of sequences enabled in either `self` or `other`.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    /// Checks whether every sequence in `other` is also enabled in `self`.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// What a double-quoted value does with a backslash pair it does not decode.
///
/// A pair is unknown when [`Dialect::double_escapes`] does not include it,
/// such as `\d` in Portable.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_with, Dialect, UnknownEscapes};
///
/// let source = "PATTERN=\"\\d+\"\n";
/// assert_eq!(parse_with(source, Dialect::PORTABLE).0.get("PATTERN"), Some("\\d+"));
///
/// let drop = Dialect::PORTABLE.with_unknown_escapes(UnknownEscapes::DropBackslash);
/// assert_eq!(parse_with(source, drop).0.get("PATTERN"), Some("d+"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum UnknownEscapes {
    /// Both characters stay in the value: `"\q"` reads `\q`.
    ///
    /// Node dotenv, python-dotenv, Compose and Docker use this.
    Preserve,
    /// Both characters stay in the value, with a warning.
    ///
    /// The warning is [`DiagnosticCode::UnknownEscape`](crate::DiagnosticCode::UnknownEscape).
    /// Portable uses this.
    Warn,
    /// The backslash is removed: `"\q"` reads `q`.
    DropBackslash,
    /// Both characters stay, and the entry gets an error.
    ///
    /// The error is [`DiagnosticCode::InvalidEscape`](crate::DiagnosticCode::InvalidEscape),
    /// so [`parse_strict_with`](crate::parse_strict_with) fails.
    Error,
}
/// The escape sequences that single-quoted values decode.
///
/// An escaped quote does not close the value, so with
/// [`Quote`](Self::Quote) the source `'it\'s'` is one value.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict_with, Dialect, SingleEscapes};
///
/// let source = "MESSAGE='it\\'s here'\n";
/// assert!(parse_strict_with(source, Dialect::PORTABLE).is_err());
///
/// let dialect = Dialect::PORTABLE.with_single_escapes(SingleEscapes::Quote);
/// assert_eq!(parse_strict_with(source, dialect)?.get("MESSAGE"), Some("it's here"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SingleEscapes {
    /// Nothing is decoded, and the first `'` closes the value.
    ///
    /// `'a\b'` reads `a\b`. Portable, Node dotenv and Docker use this.
    None,
    /// `\'` becomes `'`.
    ///
    /// `'it\'s'` reads `it's`. Compose uses this.
    Quote,
    /// `\'` becomes `'` and `\\` becomes `\`.
    ///
    /// python-dotenv uses this.
    QuoteAndBackslash,
}
/// Whether a colon can separate a key from its value, as in `KEY: value`.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_with, ColonSeparator, Dialect};
///
/// let colon = Dialect::PORTABLE.with_colon(ColonSeparator::Whitespace);
/// let (doc, _) = parse_with("HOST: localhost\n", colon);
/// assert_eq!(doc.get("HOST"), Some("localhost"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ColonSeparator {
    /// Only `=` separates a key from its value.
    ///
    /// `A: b` is invalid in Portable. Portable, python-dotenv and Docker use
    /// this.
    None,
    /// A colon followed by whitespace also separates.
    ///
    /// `A: b` reads `b`, but `A:b` does not use the colon. Node dotenv uses
    /// this.
    Whitespace,
    /// Any colon separates.
    ///
    /// `A:b` reads `b`. Compose uses this.
    Any,
}
/// Which characters an unquoted key may contain.
///
/// A key that does not match makes its statement malformed; see
/// [`Dialect::malformed_lines`]. A single-quoted key, allowed by
/// [`Dialect::quoted_keys`], is not checked: it only has to be nonempty.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict_with, Dialect, KeySyntax};
///
/// assert!(parse_strict_with("app.port=8080\n", Dialect::PORTABLE).is_ok());
///
/// let posix = Dialect::PORTABLE.with_keys(KeySyntax::Posix);
/// assert!(parse_strict_with("app.port=8080\n", posix).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeySyntax {
    /// A POSIX shell name: an ASCII letter or `_`, then ASCII letters, digits
    /// or `_`.
    ///
    /// `APP_PORT` and `_x` match; `1A` and `app.port` do not.
    Posix,
    /// One or more ASCII letters, digits, `_`, `.` or `-`.
    ///
    /// `app.port` and `1A` match. The Portable grammar accepts them with a
    /// [`DiagnosticCode::NonPosixKey`](crate::DiagnosticCode::NonPosixKey)
    /// warning. Portable and Node dotenv use this.
    Portable,
    /// One or more Unicode 17.0 letters or numbers, `_`, `.` or `-`.
    ///
    /// `ключ` matches.
    Unicode,
    /// One or more characters other than `=` and whitespace.
    ///
    /// Whitespace here is Rust's [`char::is_whitespace`]. `a/b@c` matches.
    NonWhitespace,
    /// One or more characters other than `=`, `#` and
    /// [Python whitespace](Whitespace::Python).
    ///
    /// python-dotenv uses this.
    Python,
    /// Compose's key characters: Unicode letters and numbers, `_`, `.`, `-`,
    /// `[` and `]`.
    ///
    /// [Compose whitespace](Whitespace::Compose) other than an ASCII space is
    /// also allowed inside a key, and the key may be empty, as in `=value`.
    /// Compose uses this.
    Compose,
    /// One or more characters other than an ASCII space, a tab and `=`.
    ///
    /// Other whitespace and control characters are allowed. Docker uses this.
    Docker,
}
/// How lookup treats a bare key: a declaration such as `DEBUG` with no `=`.
///
/// Every policy keeps the declaration as an entry whose
/// [`Entry::value`](crate::Entry::value) is `None` and whose state is
/// [`AssignmentState::Bare`](crate::AssignmentState::Bare); only lookup and
/// diagnostics change. No policy reads the process environment, even where
/// Docker or Compose would.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_with, BareKeys, Dialect};
///
/// let source = "DEBUG=1\nDEBUG\n";
/// assert_eq!(parse_with(source, Dialect::PORTABLE).0.get("DEBUG"), None);
///
/// let ignore = Dialect::PORTABLE.with_bare_keys(BareKeys::Ignore);
/// assert_eq!(parse_with(source, ignore).0.get("DEBUG"), Some("1"));
///
/// let empty = Dialect::PORTABLE.with_bare_keys(BareKeys::Empty);
/// assert_eq!(parse_with(source, empty).0.get("DEBUG"), Some(""));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BareKeys {
    /// The bare key takes part in lookup with no value.
    ///
    /// When it wins, [`Document::get`](crate::Document::get) returns `None`,
    /// even after an earlier `KEY=value`. Portable and python-dotenv use this.
    Preserve,
    /// The bare key does not take part in lookup.
    ///
    /// An earlier `KEY=value` still wins. Node dotenv, Compose and Docker use
    /// this.
    Ignore,
    /// The bare key takes part in lookup and reads as an empty string.
    Empty,
    /// The bare key takes part in lookup like [`Preserve`](Self::Preserve)
    /// and its entry gets an error.
    ///
    /// The error is [`DiagnosticCode::ForbiddenBareKey`](crate::DiagnosticCode::ForbiddenBareKey),
    /// so [`parse_strict_with`](crate::parse_strict_with) fails.
    Error,
}
/// Which characters end a physical line.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_with, Dialect, LineBreaks};
///
/// let source = "A=1\rB=2\n";
/// assert_eq!(parse_with(source, Dialect::PORTABLE).0.get("B"), Some("2"));
///
/// let lf = Dialect::PORTABLE.with_line_breaks(LineBreaks::Lf);
/// assert_eq!(parse_with(source, lf).0.get("A"), Some("1\rB=2"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LineBreaks {
    /// LF, CRLF and a lone CR each end a line.
    ///
    /// Portable, Node dotenv and python-dotenv use this.
    Universal,
    /// Only LF ends a line.
    ///
    /// A CR right before LF, or at the very end of the source, belongs to the
    /// line ending; any other CR is ordinary text. Compose and Docker use this.
    Lf,
}
/// What happens to a UTF-8 byte-order mark (U+FEFF) at the start of the source.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict_with, BomPolicy, DiagnosticCode, Dialect};
///
/// let source = "\u{feff}PORT=8080\n";
/// let doc = parse_strict_with(source, Dialect::PORTABLE)?;
/// assert_eq!(doc.get("PORT"), Some("8080"));
/// assert_eq!(doc.bom_span(), Some(0..3));
///
/// let forbid = Dialect::PORTABLE.with_bom(BomPolicy::Error);
/// let error = parse_strict_with(source, forbid).unwrap_err();
/// assert_eq!(error.code(), DiagnosticCode::ForbiddenByteOrderMark);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BomPolicy {
    /// The mark is set apart from the items.
    ///
    /// It belongs to no item, [`Document::bom_span`](crate::Document::bom_span)
    /// returns its range, and an informational
    /// [`DiagnosticCode::ByteOrderMark`](crate::DiagnosticCode::ByteOrderMark)
    /// is reported. [`Document::source`](crate::Document::source) still
    /// contains it. Every preset uses this.
    Strip,
    /// The mark is ordinary text at the start of the first line.
    ///
    /// In Portable it makes the first key invalid. Where the dialect's
    /// whitespace includes U+FEFF, as in Node dotenv, it is skipped like a
    /// space.
    Keep,
    /// The mark becomes an invalid item of its own, with an error.
    ///
    /// The error is
    /// [`DiagnosticCode::ForbiddenByteOrderMark`](crate::DiagnosticCode::ForbiddenByteOrderMark).
    /// The rest of the first line is read normally.
    Error,
}
/// Which declaration wins when a key is declared more than once.
///
/// Only declarations that take part in lookup count; see
/// [`Entry::participates_in_lookup`](crate::Entry::participates_in_lookup).
/// Every declaration stays in the document, and
/// [`Document::get_all`](crate::Document::get_all) returns them all.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_with, Dialect, Duplicates};
///
/// let source = "PORT=8080\nPORT=9090\n";
/// assert_eq!(parse_with(source, Dialect::PORTABLE).0.get("PORT"), Some("9090"));
///
/// let first = Dialect::PORTABLE.with_duplicates(Duplicates::First);
/// assert_eq!(parse_with(source, first).0.get("PORT"), Some("8080"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Duplicates {
    /// The first declaration wins.
    First,
    /// The last declaration wins.
    ///
    /// Every preset uses this.
    Last,
}
/// What happens to text after a closing quote, as in `A="x"y`.
///
/// Whitespace and an inline comment after the quote are not affected; this
/// policy covers any other text.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict_with, Dialect, QuotedTail};
///
/// let source = "GREETING=\"hello\"world\n";
/// assert!(parse_strict_with(source, Dialect::PORTABLE).is_err());
///
/// let append = Dialect::PORTABLE.with_quoted_tail(QuotedTail::Append);
/// assert_eq!(parse_strict_with(source, append)?.get("GREETING"), Some("helloworld"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum QuotedTail {
    /// The statement is invalid, with an error.
    ///
    /// The error is [`DiagnosticCode::TrailingText`](crate::DiagnosticCode::TrailingText).
    /// Portable and python-dotenv use this.
    Error,
    /// The text stays in the source but not in the value.
    ///
    /// `A="x"y` reads `x`; the `y` is in
    /// [`EntryTokens::quoted_tail`](crate::EntryTokens::quoted_tail).
    Ignore,
    /// The text is added to the value.
    ///
    /// `A="x"y` reads `xy`.
    Append,
    /// The value is read again as unquoted text, starting at the opening quote.
    ///
    /// `A="x"y` reads `"x"y`. A quote that never closes falls back the same
    /// way. Node dotenv uses this.
    LiteralFallback,
    /// The whole statement is skipped, with a warning.
    ///
    /// It becomes an [`ItemKind::Ignored`](crate::ItemKind::Ignored) item with
    /// [`DiagnosticCode::IgnoredStatement`](crate::DiagnosticCode::IgnoredStatement).
    Skip,
    /// The text starts another statement on the same line.
    ///
    /// `A="x"B=y` reads `A` as `x` and `B` as `y`. When only whitespace or a
    /// comment follows the quote, it stays in the quoted entry's item together
    /// with the line ending. Compose uses this.
    NextStatement,
}
/// Which quote styles may contain line breaks.
///
/// When a style may not span lines, its quote must close on the same line. A
/// quote left open is a
/// [`DiagnosticCode::UnterminatedQuote`](crate::DiagnosticCode::UnterminatedQuote)
/// error, or an unquoted value under [`QuotedTail::LiteralFallback`], and the
/// next line starts a new statement.
///
/// Start from [`ALL`](Self::ALL) or [`NONE`](Self::NONE) and change single
/// styles with the `with_*` builders.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_with, Dialect, Multiline};
///
/// let source = "KEY='line one\nline two'\n";
/// assert_eq!(parse_with(source, Dialect::PORTABLE).0.get("KEY"), Some("line one\nline two"));
///
/// let double_only = Multiline::NONE.with_double(true);
/// assert!(double_only.double && !double_only.single);
/// let dialect = Dialect::PORTABLE.with_multiline(double_only);
/// assert_eq!(parse_with(source, dialect).0.get("KEY"), None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Multiline {
    /// Whether single-quoted values may span lines.
    pub single: bool,
    /// Whether double-quoted values may span lines.
    pub double: bool,
    /// Whether backtick-quoted values may span lines, when
    /// [`Dialect::backticks`] is on.
    pub backtick: bool,
}
impl Multiline {
    /// Sets whether single-quoted values may span lines.
    #[must_use]
    pub const fn with_single(mut self, value: bool) -> Self {
        self.single = value;
        self
    }
    /// Sets whether double-quoted values may span lines.
    #[must_use]
    pub const fn with_double(mut self, value: bool) -> Self {
        self.double = value;
        self
    }
    /// Sets whether backtick-quoted values may span lines.
    #[must_use]
    pub const fn with_backtick(mut self, value: bool) -> Self {
        self.backtick = value;
        self
    }
}
impl Multiline {
    /// Every quote style may span lines.
    ///
    /// Portable, Node dotenv, python-dotenv and Compose use this.
    pub const ALL: Self = Self {
        single: true,
        double: true,
        backtick: true,
    };
    /// No quote style may span lines.
    ///
    /// Docker uses this.
    pub const NONE: Self = Self {
        single: false,
        double: false,
        backtick: false,
    };
}
/// Which characters count as whitespace around keys, separators, values and
/// comments.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_with, Dialect, Whitespace};
///
/// let source = "NAME=app\u{a0}\n"; // ends with a no-break space
/// assert_eq!(parse_with(source, Dialect::PORTABLE).0.get("NAME"), Some("app\u{a0}"));
///
/// let unicode = Dialect::PORTABLE.with_whitespace(Whitespace::Unicode);
/// assert_eq!(parse_with(source, unicode).0.get("NAME"), Some("app"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Whitespace {
    /// ASCII space and tab.
    ///
    /// Portable uses this.
    Ascii,
    /// Every character with the Unicode `White_Space` property.
    ///
    /// This is Rust's [`char::is_whitespace`]. Docker uses this.
    Unicode,
    /// JavaScript's whitespace: Unicode `White_Space` without U+0085, plus
    /// U+FEFF.
    ///
    /// Node dotenv uses this.
    EcmaScript,
    /// Python's whitespace: Unicode `White_Space` plus U+001C to U+001F.
    ///
    /// python-dotenv uses this.
    Python,
    /// Compose's whitespace: tab, vertical tab, form feed, CR, space, U+0085
    /// and U+00A0.
    ///
    /// With the Compose grammar, leading whitespace and the end of an unquoted
    /// value are trimmed by [`Unicode`](Self::Unicode) whitespace instead.
    /// Compose uses this.
    Compose,
}
/// What happens to a statement that cannot be read.
///
/// Errors that a policy asks for stay errors under both choices: a forbidden
/// bare key or byte-order mark, a rejected escape, and a line over
/// [`Dialect::line_byte_limit`].
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict_with, Dialect, ItemKind, MalformedLines};
///
/// let source = "NOT A KEY=1\nPORT=8080\n";
/// assert!(parse_strict_with(source, Dialect::PORTABLE).is_err());
///
/// let skip = Dialect::PORTABLE.with_malformed_lines(MalformedLines::Ignore);
/// let doc = parse_strict_with(source, skip)?;
/// assert_eq!(doc.items().next().map(|item| item.kind()), Some(ItemKind::Ignored));
/// assert_eq!(doc.get("PORT"), Some("8080"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MalformedLines {
    /// The statement becomes an [`ItemKind::Invalid`](crate::ItemKind::Invalid)
    /// item with an error.
    ///
    /// [`parse_strict_with`](crate::parse_strict_with) fails. Every preset
    /// except Node dotenv uses this.
    Error,
    /// The statement becomes an [`ItemKind::Ignored`](crate::ItemKind::Ignored)
    /// item with a warning.
    ///
    /// The warning is
    /// [`DiagnosticCode::IgnoredStatement`](crate::DiagnosticCode::IgnoredStatement).
    /// Node dotenv uses this.
    Ignore,
}

/// The complete set of rules for reading a `.env` file and looking up its values.
///
/// There is no single `.env` standard: tools disagree about comments, quotes,
/// escapes, duplicate keys and keys without values. A dialect states every
/// one of these rules. Each [`Document`](crate::Document) keeps the dialect it
/// was read with ([`Document::dialect`](crate::Document::dialect)), and
/// [`Document::reparse`](crate::Document::reparse) reads the same source with
/// another one. Functions without a dialect argument, such as
/// [`parse`](crate::parse), use [`Dialect::PORTABLE`], which is also
/// [`Dialect::default`].
///
/// A dialect covers parsing and literal lookup only. No preset expands
/// `$VAR` references, reads the process environment or loads files; those are
/// separate, explicit steps such as
/// [`Document::resolve`](crate::Document::resolve).
///
/// # Presets
///
/// | Preset | Reads like | Pinned version |
/// |---|---|---|
/// | [`PORTABLE`](Self::PORTABLE) | A strict, literal format for files shared between tools | None |
/// | [`NODE_DOTENV`](Self::NODE_DOTENV) | The default parser of Node `dotenv` | `dotenv` 18.0.8 |
/// | [`PYTHON_DOTENV`](Self::PYTHON_DOTENV) | `python-dotenv`, without interpolation | `python-dotenv` 1.2.4 |
/// | [`COMPOSE`](Self::COMPOSE) | Docker Compose env files, without interpolation | Compose 5.6.0, `compose-go` 2.16.1 |
/// | [`DOCKER_RAW`](Self::DOCKER_RAW) | The Docker CLI `--env-file` parser | Docker CLI 29.8.2 |
///
/// The pinned versions are the upstream releases that this crate's
/// conformance tests were recorded against.
///
/// ```
/// use adotenv::{parse_with, Dialect};
///
/// let source = "A=a#b\nB=`two words`\n";
/// let (node, _) = parse_with(source, Dialect::NODE_DOTENV);
/// assert_eq!(node.get("A"), Some("a"));
/// assert_eq!(node.get("B"), Some("two words"));
///
/// let (docker, _) = node.reparse(Dialect::DOCKER_RAW);
/// assert_eq!(docker.get("A"), Some("a#b"));
/// assert_eq!(docker.get("B"), Some("`two words`"));
/// ```
///
/// # Custom dialects
///
/// Every field is public and has a `const` builder of the same name with a
/// `with_` prefix. The struct is `#[non_exhaustive]`, so start from a preset
/// and change single rules:
///
/// ```
/// use adotenv::{parse_with, BareKeys, Dialect, InlineComments};
///
/// const STRICT_NODE: Dialect = Dialect::NODE_DOTENV
///     .with_inline_comments(InlineComments::None)
///     .with_bare_keys(BareKeys::Empty)
///     .with_line_byte_limit(Some(4096));
///
/// let (doc, _) = parse_with("A=one#two\nBARE\n", STRICT_NODE);
/// assert_eq!(doc.get("A"), Some("one#two"));
/// assert_eq!(doc.get("BARE"), Some(""));
/// ```
///
/// [`Document::portability`](crate::Document::portability) shows the lines
/// that several dialects read differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
// These orthogonal syntax switches are independently overrideable, not states.
#[allow(clippy::struct_excessive_bools)]
#[non_exhaustive]
pub struct Dialect {
    /// The statement grammar: how a line splits into key, separator and value.
    pub syntax: Syntax,
    /// Where a `#` inside a line starts a comment.
    pub inline_comments: InlineComments,
    /// The escape sequences that double-quoted values decode.
    pub double_escapes: DoubleEscapes,
    /// What double quotes do with a backslash pair outside
    /// [`double_escapes`](Self::double_escapes).
    pub unknown_escapes: UnknownEscapes,
    /// The escape sequences that single-quoted values decode.
    pub single_escapes: SingleEscapes,
    /// Whether backticks quote values.
    ///
    /// When `true`, `` A=`two words` `` reads `two words`, with no escapes
    /// decoded. When `false`, backticks are ordinary text.
    pub backticks: bool,
    /// Whether a colon can separate a key from its value.
    pub colon: ColonSeparator,
    /// Which characters an unquoted key may contain.
    pub keys: KeySyntax,
    /// How lookup treats a declaration without `=`.
    pub bare_keys: BareKeys,
    /// Whether the value is every byte after the separator, unchanged.
    ///
    /// When `true`, quotes, escapes, `#` and surrounding whitespace are all
    /// part of the value: `A= "x" # c` reads ` "x" # c`.
    pub raw_values: bool,
    /// Which characters end a physical line.
    pub line_breaks: LineBreaks,
    /// What happens to a byte-order mark at the start of the source.
    pub bom: BomPolicy,
    /// Which declaration wins when a key is declared more than once.
    pub duplicates: Duplicates,
    /// What happens to text after a closing quote, as in `A="x"y`.
    pub quoted_tail: QuotedTail,
    /// Which quote styles may contain line breaks.
    pub multiline: Multiline,
    /// Which characters count as whitespace.
    pub whitespace: Whitespace,
    /// Whether an `export ` prefix before a key is recognized.
    ///
    /// When `false`, `export A=1` does not declare `A`; the presets' key rules
    /// reject it as malformed.
    pub export: bool,
    /// Whether a key may be written in single quotes, as in `'APP NAME'=demo`.
    ///
    /// A quoted key is not checked against [`keys`](Self::keys).
    pub quoted_keys: bool,
    /// Whether a `#` directly after a closing quote starts a comment.
    ///
    /// When `true`, `A="x"#note` reads `x`. When `false`, the comment needs
    /// whitespace before it, as [`inline_comments`](Self::inline_comments)
    /// decides. It has no effect under [`InlineComments::None`].
    pub quoted_comments: bool,
    /// Whether CRLF and a lone CR in a decoded value become LF.
    ///
    /// Spans still address the original bytes.
    pub normalize_newlines: bool,
    /// What happens to a statement that cannot be read.
    pub malformed_lines: MalformedLines,
    /// Whether the key `__proto__` is left out of lookup, as Node dotenv does.
    ///
    /// The entry stays in the document.
    pub node_object_keys: bool,
    /// The longest physical line accepted, in bytes, or `None` for no limit.
    ///
    /// The count leaves out the line's final LF but includes a CR before it
    /// and a lone CR. A leading byte-order mark counts toward the first line.
    /// An item with a longer line becomes invalid, with a
    /// [`DiagnosticCode::LineTooLong`](crate::DiagnosticCode::LineTooLong)
    /// error.
    pub line_byte_limit: Option<usize>,
}
impl Dialect {
    /// Portable rules: a strict, literal format for files shared between tools.
    ///
    /// This is the default dialect. In short:
    ///
    /// - Keys use ASCII letters, digits, `_`, `.` and `-`. A key that is not a
    ///   POSIX shell name, such as `app.port`, gets a warning.
    /// - An `export ` prefix and spaces or tabs around `=` are accepted.
    /// - `#` starts a comment at the start of a line or after whitespace, so
    ///   `A=a#b` reads `a#b`, with a warning.
    /// - Single quotes are literal. Double quotes decode `\n`, `\r`, `\t`,
    ///   `\\`, `\"` and `\$`; other backslash pairs stay as written, with a
    ///   warning.
    /// - Quoted values may span lines. After a closing quote only whitespace
    ///   and a comment may follow, and the comment needs whitespace before it:
    ///   `A="x" #c` is valid, while `A="x"#c` and `A="x"y` are errors.
    /// - Backticks are ordinary text; in an unquoted value they get a warning.
    /// - A bare `KEY` takes part in lookup with no value, with a warning.
    ///   `KEY=` and `KEY=""` are empty.
    /// - The last duplicate wins, with a warning.
    /// - LF, CRLF and a lone CR end lines; a lone CR gets a warning. A leading
    ///   byte-order mark is set apart from the items.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict_with, Dialect};
    ///
    /// let source = "export PORT=8080\nNAME=\"two words\" # note\nTAG=a#b\n";
    /// let doc = parse_strict_with(source, Dialect::PORTABLE)?;
    /// assert_eq!(doc.get("PORT"), Some("8080"));
    /// assert_eq!(doc.get("NAME"), Some("two words"));
    /// assert_eq!(doc.get("TAG"), Some("a#b"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
    /// Node dotenv 18.0.8 parsing rules, from its default parser.
    ///
    /// Differences from Portable:
    ///
    /// - Any `#` in an unquoted value starts a comment: `A=a#b` reads `a`. A
    ///   comment may also follow a closing quote directly, as in `A="x"#note`.
    /// - Backticks quote values, and a colon followed by whitespace works like
    ///   `=`, as in `KEY: value`.
    /// - Double quotes decode only `\n` and `\r`. Other pairs stay as written,
    ///   without a warning.
    /// - Text after a closing quote makes the value literal: `A="x"y` reads
    ///   `"x"y`.
    /// - CRLF and CR inside decoded values become LF.
    /// - Malformed lines are ignored with a warning instead of failing.
    /// - Bare keys and the key `__proto__` stay in the document but take no
    ///   part in lookup.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_with, Dialect};
    ///
    /// let source = "A=a#b\nB=`two words`\nC: three\nD=1\nD\n";
    /// let (doc, _) = parse_with(source, Dialect::NODE_DOTENV);
    /// assert_eq!(doc.get("A"), Some("a"));
    /// assert_eq!(doc.get("B"), Some("two words"));
    /// assert_eq!(doc.get("C"), Some("three"));
    /// assert_eq!(doc.get("D"), Some("1"));
    /// ```
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
    /// python-dotenv 1.2.4 parsing rules, without interpolation.
    ///
    /// Differences from Portable:
    ///
    /// - Double quotes decode `\n`, `\r`, `\t`, `\\`, `\"`, `\'`, `\a`, `\b`,
    ///   `\f` and `\v`, but not `\$`. Other pairs stay as written, without a
    ///   warning.
    /// - Single quotes decode `\'` and `\\`.
    /// - A comment may follow a closing quote directly, as in `A="x"#note`,
    ///   and any `#` right after a bare key starts a comment.
    /// - Keys may contain any character except whitespace, `=` and `#`, and
    ///   may be single-quoted.
    /// - Whitespace follows Python's definition.
    ///
    /// `$VAR` and `${VAR}` stay literal. Use
    /// [`Document::resolve`](crate::Document::resolve) to expand them.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict_with, Dialect};
    ///
    /// let source = "A='it\\'s'\nB=\"x\"#note\nC=${HOME}/app\n";
    /// let doc = parse_strict_with(source, Dialect::PYTHON_DOTENV)?;
    /// assert_eq!(doc.get("A"), Some("it's"));
    /// assert_eq!(doc.get("B"), Some("x"));
    /// assert_eq!(doc.get("C"), Some("${HOME}/app"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
    /// Docker Compose 5.6.0 env-file rules (`compose-go` 2.16.1), without
    /// interpolation.
    ///
    /// Differences from Portable:
    ///
    /// - `#` starts a comment only after an ASCII space: `A=a\t#b` keeps the
    ///   hash.
    /// - Any colon separates a key from its value: `A:b` reads `b`.
    /// - Keys may use Unicode letters and numbers, `[` and `]`, and may be
    ///   empty, as in `=value`.
    /// - Double quotes also decode `\a`, `\b`, `\f`, `\v` and octal `\0`
    ///   escapes. Single quotes decode `\'`.
    /// - Text after a closing quote starts another statement on the same line.
    /// - Only LF ends a line.
    /// - Bare keys take no part in lookup. A bare key on a last line without a
    ///   line ending reads as a value with an empty key, as Compose does.
    ///
    /// `$VAR` stays literal, and no value comes from the process environment.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict_with, Dialect};
    ///
    /// let source = "A:b\nB=\"x\" C=y\nD=a\t#b\n";
    /// let doc = parse_strict_with(source, Dialect::COMPOSE)?;
    /// assert_eq!(doc.get("A"), Some("b"));
    /// assert_eq!(doc.get("B"), Some("x"));
    /// assert_eq!(doc.get("C"), Some("y"));
    /// assert_eq!(doc.get("D"), Some("a\t#b"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
    /// Docker CLI 29.8.2 `--env-file` rules, without reading the process
    /// environment.
    ///
    /// - The value is every byte after `=`, unchanged: quotes, `#`,
    ///   backslashes and spaces are part of it.
    /// - A key is everything before `=`. It may not be empty or contain an
    ///   ASCII space or tab.
    /// - There is no `export` prefix and no multiline value. Only LF ends a
    ///   line.
    /// - Bare keys stay in the document but take no part in lookup.
    /// - A line longer than 65,535 bytes is an error.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict_with, Dialect};
    ///
    /// let source = "GREETING= \"hello\" # kept\nPORT=8080\n";
    /// let doc = parse_strict_with(source, Dialect::DOCKER_RAW)?;
    /// assert_eq!(doc.get("GREETING"), Some(" \"hello\" # kept"));
    /// assert_eq!(doc.get("PORT"), Some("8080"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
    /// Sets [`syntax`](Self::syntax), the statement grammar.
    #[must_use]
    pub const fn with_syntax(mut self, syntax: Syntax) -> Self {
        self.syntax = syntax;
        self
    }
    /// Sets [`inline_comments`](Self::inline_comments): where a `#` starts a comment.
    #[must_use]
    pub const fn with_inline_comments(mut self, value: InlineComments) -> Self {
        self.inline_comments = value;
        self
    }
    /// Sets [`double_escapes`](Self::double_escapes), the escapes double quotes decode.
    #[must_use]
    pub const fn with_double_escapes(mut self, value: DoubleEscapes) -> Self {
        self.double_escapes = value;
        self
    }
    /// Sets [`unknown_escapes`](Self::unknown_escapes): what happens to unknown pairs.
    #[must_use]
    pub const fn with_unknown_escapes(mut self, value: UnknownEscapes) -> Self {
        self.unknown_escapes = value;
        self
    }
    /// Sets [`single_escapes`](Self::single_escapes), the escapes single quotes decode.
    #[must_use]
    pub const fn with_single_escapes(mut self, value: SingleEscapes) -> Self {
        self.single_escapes = value;
        self
    }
    /// Sets [`backticks`](Self::backticks): whether backticks quote values.
    #[must_use]
    pub const fn with_backticks(mut self, value: bool) -> Self {
        self.backticks = value;
        self
    }
    /// Sets [`colon`](Self::colon): whether a colon can separate key and value.
    #[must_use]
    pub const fn with_colon(mut self, value: ColonSeparator) -> Self {
        self.colon = value;
        self
    }
    /// Sets [`keys`](Self::keys), the characters an unquoted key may contain.
    #[must_use]
    pub const fn with_keys(mut self, value: KeySyntax) -> Self {
        self.keys = value;
        self
    }
    /// Sets [`bare_keys`](Self::bare_keys): how lookup treats keys without `=`.
    #[must_use]
    pub const fn with_bare_keys(mut self, value: BareKeys) -> Self {
        self.bare_keys = value;
        self
    }
    /// Sets [`raw_values`](Self::raw_values): whether values are kept byte for byte.
    #[must_use]
    pub const fn with_raw_values(mut self, value: bool) -> Self {
        self.raw_values = value;
        self
    }
    /// Sets [`line_breaks`](Self::line_breaks): which characters end a line.
    #[must_use]
    pub const fn with_line_breaks(mut self, value: LineBreaks) -> Self {
        self.line_breaks = value;
        self
    }
    /// Sets [`bom`](Self::bom): what happens to a leading byte-order mark.
    #[must_use]
    pub const fn with_bom(mut self, value: BomPolicy) -> Self {
        self.bom = value;
        self
    }
    /// Sets [`duplicates`](Self::duplicates): which duplicate declaration wins.
    #[must_use]
    pub const fn with_duplicates(mut self, value: Duplicates) -> Self {
        self.duplicates = value;
        self
    }
    /// Sets [`quoted_tail`](Self::quoted_tail): what happens to text after a quote.
    #[must_use]
    pub const fn with_quoted_tail(mut self, value: QuotedTail) -> Self {
        self.quoted_tail = value;
        self
    }
    /// Sets [`multiline`](Self::multiline): which quote styles may span lines.
    #[must_use]
    pub const fn with_multiline(mut self, value: Multiline) -> Self {
        self.multiline = value;
        self
    }
    /// Sets [`whitespace`](Self::whitespace): which characters count as whitespace.
    #[must_use]
    pub const fn with_whitespace(mut self, value: Whitespace) -> Self {
        self.whitespace = value;
        self
    }
    /// Sets [`export`](Self::export): whether an `export ` prefix is recognized.
    #[must_use]
    pub const fn with_export(mut self, value: bool) -> Self {
        self.export = value;
        self
    }
    /// Sets [`quoted_keys`](Self::quoted_keys): whether keys may be single-quoted.
    #[must_use]
    pub const fn with_quoted_keys(mut self, value: bool) -> Self {
        self.quoted_keys = value;
        self
    }
    /// Sets [`quoted_comments`](Self::quoted_comments): whether `#` may touch a quote.
    #[must_use]
    pub const fn with_quoted_comments(mut self, value: bool) -> Self {
        self.quoted_comments = value;
        self
    }
    /// Sets [`normalize_newlines`](Self::normalize_newlines): whether CR becomes LF.
    #[must_use]
    pub const fn with_normalize_newlines(mut self, value: bool) -> Self {
        self.normalize_newlines = value;
        self
    }
    /// Sets [`malformed_lines`](Self::malformed_lines): errors or ignored lines.
    #[must_use]
    pub const fn with_malformed_lines(mut self, value: MalformedLines) -> Self {
        self.malformed_lines = value;
        self
    }
    /// Sets [`node_object_keys`](Self::node_object_keys): whether to skip `__proto__`.
    #[must_use]
    pub const fn with_node_object_keys(mut self, value: bool) -> Self {
        self.node_object_keys = value;
        self
    }
    /// Sets [`line_byte_limit`](Self::line_byte_limit), the longest line in bytes.
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
