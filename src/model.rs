use alloc::{
    borrow::Cow,
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use core::{fmt, ops::Range};

use crate::{
    BareKeys, Diagnostic, DiagnosticCode, Dialect, Duplicates, EditError, LineIndex, PositionError,
    QuotedTail,
};

/// The quote style of a value as written in the source.
///
/// Returned by [`Entry::quote`]. The decoded [`Entry::value`] never includes
/// the quotes; [`Entry::raw_value`] does.
///
/// # Examples
///
/// ```
/// use adotenv::Quote;
///
/// let doc = adotenv::parse_strict("A=plain\nB='single'\nC=\"double\"\n")?;
/// assert_eq!(doc.get_entry("A").unwrap().quote(), Quote::None);
/// assert_eq!(doc.get_entry("B").unwrap().quote(), Quote::Single);
/// assert_eq!(doc.get_entry("C").unwrap().quote(), Quote::Double);
/// assert_eq!(doc.get("C"), Some("double"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quote {
    /// An unquoted value, or no value at all (a bare declaration).
    None,
    /// A single-quoted value.
    ///
    /// Portable decodes no escapes inside single quotes; some dialects decode a
    /// few ([`Dialect::single_escapes`]).
    Single,
    /// A double-quoted value, whose escapes the dialect decodes
    /// ([`Dialect::double_escapes`]).
    Double,
    /// A backtick-quoted value.
    ///
    /// Only dialects that allow backticks ([`Dialect::backticks`]), such as
    /// [`Dialect::NODE_DOTENV`], read them as quotes. Portable reads a backtick
    /// as literal text.
    Backtick,
}

/// Whether a declaration is bare, empty or has a value.
///
/// A *bare key* such as `DEBUG` has no `=` and no value, so its
/// [`Entry::value`] is `None`. An empty assignment such as `NAME=` or `NAME=""`
/// has the value `Some("")`. The state describes the source only: it does not
/// depend on lookup rules, and a bare key never reads the process environment.
/// [`BareKeys::Empty`] makes lookup read a bare key as `""` without changing
/// its state.
///
/// # Examples
///
/// ```
/// use adotenv::AssignmentState;
///
/// let doc = adotenv::parse_strict("DEBUG\nNAME=\nPORT=8080\n")?;
/// assert_eq!(doc.get_entry("DEBUG").unwrap().state(), AssignmentState::Bare);
/// assert_eq!(doc.get("DEBUG"), None);
/// assert_eq!(doc.get_entry("NAME").unwrap().state(), AssignmentState::Empty);
/// assert_eq!(doc.get("NAME"), Some(""));
/// assert_eq!(doc.get_entry("PORT").unwrap().state(), AssignmentState::Value);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AssignmentState {
    /// A bare key: a declaration without `=` and without a value.
    Bare,
    /// An assignment whose decoded value is empty, quoted or not.
    Empty,
    /// An assignment whose decoded value is not empty.
    Value,
}

/// A line ending as written in the source.
///
/// Returned by [`Item::line_ending`]. Parsing keeps line endings exactly and
/// never converts one kind into another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LineEnding {
    /// No line ending.
    ///
    /// The item ends at the end of the source, or another statement follows
    /// on the same line.
    None,
    /// A line feed (`\n`).
    Lf,
    /// A carriage return followed by a line feed (`\r\n`).
    CrLf,
    /// A carriage return without a line feed (`\r`).
    ///
    /// Dialects with [`LineBreaks::Universal`](crate::LineBreaks::Universal),
    /// such as Portable, end a line at any lone CR, and Portable reports
    /// [`DiagnosticCode::LoneCarriageReturn`]. Dialects with
    /// [`LineBreaks::Lf`](crate::LineBreaks::Lf), such as Compose and Docker,
    /// keep a lone CR inside the line and use this ending only for a CR at the
    /// very end of the source.
    Cr,
}
impl LineEnding {
    /// Returns the line ending's text: `""`, `"\n"`, `"\r\n"` or `"\r"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
        }
    }
}

/// What kind of source text an [`Item`] holds.
///
/// Returned by [`Item::kind`]. Every byte of the source belongs to exactly one
/// item, apart from a leading byte-order mark that the dialect strips
/// ([`Document::bom_span`]).
///
/// # Examples
///
/// ```
/// use adotenv::ItemKind;
///
/// let (doc, _) = adotenv::parse("# app\nPORT=8080\n\nBAD KEY=1\n");
/// let kinds: Vec<_> = doc.items().map(|item| item.kind()).collect();
/// assert_eq!(
///     kinds,
///     [ItemKind::Comment, ItemKind::Entry, ItemKind::Blank, ItemKind::Invalid]
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ItemKind {
    /// An empty line, or a line with only whitespace.
    Blank,
    /// A whole-line comment: a line whose first text after any indentation
    /// is `#`.
    Comment,
    /// A declaration: `KEY=value` or a bare `KEY`.
    ///
    /// [`Item::entry`] returns it as an [`Entry`]. A multiline quoted value
    /// makes one item of several lines.
    Entry,
    /// A statement with an error, kept exactly as written.
    ///
    /// [`Item::diagnostic`] returns the error, which has
    /// [`Severity::Error`](crate::Severity::Error).
    Invalid,
    /// A statement the dialect skips, kept exactly as written.
    ///
    /// [`Item::diagnostic`] returns a [`DiagnosticCode::IgnoredStatement`]
    /// warning. Dialects that ignore malformed lines, such as
    /// [`Dialect::NODE_DOTENV`], produce these instead of [`ItemKind::Invalid`].
    Ignored,
}

/// The byte ranges of each part of a declaration's source text.
///
/// Returned by [`Entry::tokens`]. Every range is a span into
/// [`Document::source`], so `&doc.source()[range]` returns the text of that
/// part. Whitespace ranges are always present and may be empty; parts that can
/// be missing, such as `export` or the value, are `None` when absent.
///
/// # Examples
///
/// ```
/// let source = "export PORT = \"8080\" # http\n";
/// let doc = adotenv::parse_strict(source)?;
/// let tokens = doc.get_entry("PORT").unwrap().tokens();
///
/// assert_eq!(&source[tokens.export.clone().unwrap()], "export");
/// assert_eq!(&source[tokens.key.clone()], "PORT");
/// assert_eq!(&source[tokens.space_before_eq.clone()], " ");
/// assert_eq!(&source[tokens.value.clone().unwrap()], "\"8080\"");
/// assert_eq!(&source[tokens.body.clone().unwrap()], "8080");
/// assert_eq!(&source[tokens.inline_comment.clone().unwrap()], "# http");
/// assert!(tokens.indent.is_empty());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EntryTokens {
    /// Whitespace before the statement.
    pub indent: Range<usize>,
    /// The `export` keyword, without the whitespace after it.
    pub export: Option<Range<usize>>,
    /// Whitespace between `export` and the key; empty without `export`.
    pub space_after_export: Range<usize>,
    /// The key as written, including key quotes where the dialect allows them
    /// ([`Dialect::quoted_keys`]).
    pub key: Range<usize>,
    /// Whitespace between the key and the separator, or after a bare key.
    pub space_before_eq: Range<usize>,
    /// The separator: `=`, or `:` where the dialect allows it
    /// ([`Dialect::colon`]); `None` for a bare key.
    pub eq: Option<Range<usize>>,
    /// Whitespace between the separator and the value.
    pub space_after_eq: Range<usize>,
    /// The value as written, including its quotes; `None` for a bare key.
    ///
    /// An empty assignment such as `KEY=` has an empty range here. Under
    /// [`QuotedTail::Append`], this is the quoted token only; the appended text
    /// is [`quoted_tail`](Self::quoted_tail), and
    /// [`Entry::value_span`] covers both.
    pub value: Option<Range<usize>>,
    /// The value without its quotes; `None` for a bare key.
    ///
    /// For an unquoted value this is the same range as `value`. Under
    /// [`QuotedTail::Append`], this excludes the appended tail, which is
    /// [`quoted_tail`](Self::quoted_tail).
    pub body: Option<Range<usize>>,
    /// Text after the closing quote that the dialect keeps.
    ///
    /// Present only under [`QuotedTail::Ignore`], which keeps the text out of
    /// the value, and [`QuotedTail::Append`], which appends it to the value.
    pub quoted_tail: Option<Range<usize>>,
    /// Whitespace after the value, before any inline comment.
    pub trailing_space: Range<usize>,
    /// The inline comment, including its `#`.
    pub inline_comment: Option<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecodeSegment {
    pub decoded: Range<usize>,
    pub raw: Range<usize>,
}

pub(crate) const NO_ENTRY: usize = usize::MAX;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntryData {
    pub item: usize,
    pub line: usize,
    pub tokens: EntryTokens,
    pub quote: Quote,
    pub state: AssignmentState,
    pub key_quoted: bool,
    pub participates: bool,
    pub decoded: Option<Box<str>>,
    pub map: Option<Box<[DecodeSegment]>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ItemData {
    pub kind: ItemKind,
    pub span: Range<usize>,
    pub ending: LineEnding,
    pub entry: usize,
    pub comment: Option<Range<usize>>,
    pub invalid: Option<Box<Diagnostic>>,
}

/// A parsed `.env` document that keeps every byte of its source.
///
/// A document holds the source text and the [items](Item) that one
/// [`Dialect`] reads from it: entries, comments, blank lines, and statements
/// the dialect rejects or skips. Parsing never changes the source:
/// [`Document::source`] and the `Display` output return it byte for byte,
/// malformed lines included. Values are literal: `${HOST}` stays as written,
/// and only [`Document::resolve`] substitutes references, in a separate
/// result.
///
/// Create a document with [`parse`](crate::parse) or one of its siblings. A
/// `Document<'src>` borrows its source; [`parse_owned`](crate::parse_owned)
/// and [`Document::into_owned`] give a `Document<'static>`. A document never
/// changes after parsing: editing methods return new text, which you can parse
/// again. Item and entry ids are positions in one document, so a document
/// from [`Document::reparse`] or from edited text has its own.
///
/// # Examples
///
/// ```
/// let source = "# database\nDATABASE_URL=postgres://localhost/app\nPORT=8080\n";
/// let (doc, diagnostics) = adotenv::parse(source);
///
/// assert!(diagnostics.is_empty());
/// assert_eq!(doc.get("PORT"), Some("8080"));
/// assert_eq!(doc.entries().count(), 2);
/// assert_eq!(doc.to_string(), source);
///
/// let edited = doc.set("PORT", "9090")?;
/// assert_eq!(
///     edited.as_str(),
///     "# database\nDATABASE_URL=postgres://localhost/app\nPORT=9090\n"
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Methods by task
///
/// - **Lookup**: [`get`](Document::get) returns the *effective value* of a
///   key, after the dialect's duplicate-key and bare-key rules, and
///   [`get_entry`](Document::get_entry) the declaration that wins lookup.
///   [`get_all`](Document::get_all) yields every declaration of a key, and
///   [`unique`](Document::unique) returns the only one or fails.
///   [`effective_entries`](Document::effective_entries) yields the winning
///   declaration of each key that has an effective value. For many lookups,
///   build a [`LookupIndex`] once with
///   [`lookup_index`](Document::lookup_index).
/// - **Items and entries**: [`items`](Document::items) and
///   [`item`](Document::item) give every piece of the source as an [`Item`];
///   [`entries`](Document::entries) and [`entry`](Document::entry) give the
///   declarations as [`Entry`] values.
/// - **Source text**: [`source`](Document::source),
///   [`has_bom`](Document::has_bom), [`bom_span`](Document::bom_span) and
///   [`has_final_newline`](Document::has_final_newline) describe the text, and
///   [`into_owned`](Document::into_owned) makes the document own it.
/// - **Structure**: [`groups`](Document::groups) splits the items at blank
///   lines, and [`header`](Document::header) returns the items before the
///   first entry. [`Entry::leading_comments`] returns the comments above a
///   declaration.
/// - **Positions**: [`line_index`](Document::line_index) builds a
///   [`LineIndex`] that converts byte offsets to lines and columns.
/// - **Dialect**: [`dialect`](Document::dialect) returns the dialect the
///   document was parsed with, and [`reparse`](Document::reparse) reads the
///   same source with another one.
/// - **Editing**: [`set`](Document::set), [`remove`](Document::remove),
///   [`rename`](Document::rename), [`comment`](Document::comment) and
///   [`set_export`](Document::set_export) change the only declaration of a
///   key; [`insert`](Document::insert) adds a declaration and
///   [`uncomment`](Document::uncomment) removes the `#` of a comment item.
///   [`edit`](Document::edit) applies a batch of [`Edit`](crate::Edit)
///   operations at once. Each returns [`VerifiedEdits`](crate::VerifiedEdits):
///   the new text, *verified* by parsing it again with the same dialect, and
///   the byte-range edits that produce it.
/// - **Formatting and sorting**: [`format`](Document::format) changes
///   spacing, quoting, blank lines and the final line ending as set by
///   [`FormatOptions`](crate::FormatOptions), never the values, and
///   [`sort`](Document::sort) sorts the declarations of each group by key.
///   Both verify their output.
/// - **Comparison and examples**: [`compare`](Document::compare) reports the
///   keys that are missing or differ in state, value or `export` between two
///   documents, and
///   [`example`](Document::example) returns the text with every value
///   removed, for an `.env.example` file.
/// - **Portability**: [`portability`](Document::portability) reports the
///   lines that other dialects read differently.
/// - **References and resolution**: [`Entry::references`] lists the `$VAR`,
///   `${VAR:-default}` and `$(command)` forms in a value without resolving
///   them. [`resolve`](Document::resolve) substitutes variables using a lookup
///   function you provide, and
///   [`resolve_with_commands`](Document::resolve_with_commands) also runs
///   commands through a callback you provide.
/// - **Serde** (`serde` feature): [`deserialize`](Document::deserialize) reads
///   the effective values into a type that implements `serde::Deserialize`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document<'src> {
    pub(crate) source: Cow<'src, str>,
    pub(crate) items: Vec<ItemData>,
    pub(crate) entries: Vec<EntryData>,
    pub(crate) bom: bool,
    pub(crate) dialect: Dialect,
    pub(crate) has_duplicate_keys: bool,
}

impl Document<'_> {
    /// Returns the complete source text, unchanged.
    ///
    /// The text includes a leading byte-order mark and every malformed
    /// statement. `doc.to_string()` returns the same text.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
    /// Returns the dialect this document was parsed with.
    ///
    /// The dialect sets both the syntax and the lookup rules, such as which
    /// duplicate wins.
    #[must_use]
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }
    /// Parses this document's source again with another dialect.
    ///
    /// The new document borrows the same source without copying it, and comes
    /// with its own diagnostics and its own item and entry ids. Use it to see
    /// how another tool reads the same file; [`Document::portability`] reports
    /// the differences line by line.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::Dialect;
    ///
    /// let (doc, _) = adotenv::parse("COLOR=blue#green\n");
    /// assert_eq!(doc.get("COLOR"), Some("blue#green"));
    ///
    /// // Node dotenv starts a comment at any `#`.
    /// let (node, _) = doc.reparse(Dialect::NODE_DOTENV);
    /// assert_eq!(node.get("COLOR"), Some("blue"));
    /// assert_eq!(node.dialect(), Dialect::NODE_DOTENV);
    /// ```
    #[must_use]
    pub fn reparse(&self, dialect: Dialect) -> (Document<'_>, Vec<Diagnostic>) {
        crate::parse_with(self.source(), dialect)
    }
    /// Returns an iterator over every item in source order.
    ///
    /// The items cover the source with no gaps or overlaps, except a leading
    /// byte-order mark that the dialect strips ([`Document::bom_span`]), so
    /// joining their [`Item::raw`] text rebuilds the source without that mark.
    /// The zero-based position of an item in this iterator is its
    /// [`Item::id`].
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::ItemKind;
    ///
    /// let source = "# app\nPORT=8080\n\nBAD KEY=1\n";
    /// let (doc, _) = adotenv::parse(source);
    ///
    /// let invalid: Vec<_> = doc
    ///     .items()
    ///     .filter(|item| item.kind() == ItemKind::Invalid)
    ///     .map(|item| item.raw())
    ///     .collect();
    /// assert_eq!(invalid, ["BAD KEY=1\n"]);
    ///
    /// let rebuilt: String = doc.items().map(|item| item.raw()).collect();
    /// assert_eq!(rebuilt, source);
    /// ```
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn items(&self) -> impl DoubleEndedIterator<Item = Item<'_>> + ExactSizeIterator {
        (0..self.items.len()).map(|index| Item {
            document: self,
            index,
        })
    }
    /// Returns the item with the given zero-based id, or `None` if there is
    /// none.
    ///
    /// Ids are positions in [`Document::items`] of this document only.
    #[must_use]
    pub fn item(&self, index: usize) -> Option<Item<'_>> {
        (index < self.items.len()).then_some(Item {
            document: self,
            index,
        })
    }
    /// Returns an iterator over every declaration in source order.
    ///
    /// It includes duplicates, bare keys, declarations that do not take part in
    /// lookup, and declarations with a policy error such as
    /// [`DiagnosticCode::ForbiddenBareKey`]. Invalid and ignored statements are
    /// not entries. The zero-based position of an entry in this iterator is
    /// its [`Entry::id`].
    ///
    /// # Examples
    ///
    /// ```
    /// let doc = adotenv::parse_strict("PORT=8080\nDEBUG\nPORT=9090\n")?;
    /// let keys: Vec<_> = doc.entries().map(|entry| entry.key()).collect();
    /// assert_eq!(keys, ["PORT", "DEBUG", "PORT"]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn entries(&self) -> impl DoubleEndedIterator<Item = Entry<'_>> + ExactSizeIterator {
        (0..self.entries.len()).map(|index| Entry {
            document: self,
            index,
        })
    }
    /// Returns the declaration with the given zero-based id, or `None` if
    /// there is none.
    ///
    /// Ids are positions in [`Document::entries`] of this document only.
    #[must_use]
    pub fn entry(&self, index: usize) -> Option<Entry<'_>> {
        (index < self.entries.len()).then_some(Entry {
            document: self,
            index,
        })
    }
    /// Returns an iterator over every declaration of `key`, in source order.
    ///
    /// It includes duplicates, bare keys and declarations that do not take
    /// part in lookup. Use [`Document::get_entry`] for the one that wins
    /// lookup.
    ///
    /// # Examples
    ///
    /// ```
    /// let doc = adotenv::parse_strict("PORT=8080\nHOST=localhost\nPORT=9090\n")?;
    /// let lines: Vec<_> = doc.get_all("PORT").map(|entry| entry.line()).collect();
    /// assert_eq!(lines, [1, 3]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn get_all<'doc, 'key>(
        &'doc self,
        key: &'key str,
    ) -> impl DoubleEndedIterator<Item = Entry<'doc>> + 'key
    where
        'doc: 'key,
    {
        self.entries().filter(move |entry| entry.key() == key)
    }
    /// Returns the declaration that wins lookup for `key`.
    ///
    /// Only declarations that take part in lookup
    /// ([`Entry::participates_in_lookup`]) count, and the dialect's duplicate
    /// rule ([`Dialect::duplicates`]) picks the winner; in Portable the last
    /// one wins. The winner can be a bare key, which has no value: use
    /// [`Document::get`] for the effective value.
    ///
    /// A dialect that allows an empty key, such as Compose, can look it up
    /// here with `""`. [`LookupIndex`] and [`Document::effective_entries`]
    /// leave empty keys out.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::Dialect;
    ///
    /// let doc = adotenv::parse_strict("PORT=8080\nPORT=9090\n")?;
    /// assert_eq!(doc.get_entry("PORT").unwrap().line(), 2);
    ///
    /// // Node dotenv ignores the bare `PORT`, so line 1 wins.
    /// let (doc, _) = adotenv::parse_with("PORT=8080\nPORT\n", Dialect::NODE_DOTENV);
    /// assert_eq!(doc.get_entry("PORT").unwrap().line(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn get_entry(&self, key: &str) -> Option<Entry<'_>> {
        let mut entries = self
            .entries()
            .filter(|entry| entry.participates_in_lookup() && entry.key() == key);
        match self.dialect.duplicates {
            Duplicates::First => entries.next(),
            Duplicates::Last => entries.next_back(),
        }
    }
    /// Returns the effective value of `key`, or `None` if it has none.
    ///
    /// This is the decoded [`Entry::value`] of the declaration that wins
    /// lookup ([`Document::get_entry`]). A bare key that wins has no value, so
    /// this returns `None`, unless the dialect uses [`BareKeys::Empty`], which
    /// reads it as `Some("")` while the entry itself stays bare. Values are
    /// literal and never come from the process environment.
    ///
    /// Each call scans the entries; for many lookups, build a [`LookupIndex`]
    /// with [`Document::lookup_index`]. A dialect that allows an empty key,
    /// such as Compose, can look it up here with `""`.
    ///
    /// # Examples
    ///
    /// ```
    /// let doc = adotenv::parse_strict("PORT=8080\nPORT=9090\nDEBUG\nNAME=\n")?;
    ///
    /// assert_eq!(doc.get("PORT"), Some("9090"));
    /// assert_eq!(doc.get("NAME"), Some(""));
    /// assert_eq!(doc.get("DEBUG"), None);
    /// assert_eq!(doc.get("MISSING"), None);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.get_entry(key).and_then(|entry| {
            entry.value().or_else(|| {
                (entry.state() == AssignmentState::Bare
                    && self.dialect.bare_keys == BareKeys::Empty)
                    .then_some("")
            })
        })
    }
    /// Returns an iterator over the winning declaration of each key, with its
    /// effective value.
    ///
    /// It yields one [`EffectiveEntry`] for every non-empty key that
    /// [`Document::get`] returns a value for, ordered by the position of the
    /// winning declaration. A key whose winner is a bare key is left out,
    /// unless the dialect uses [`BareKeys::Empty`]. Declarations that do not
    /// take part in lookup never win.
    ///
    /// # Examples
    ///
    /// ```
    /// let doc = adotenv::parse_strict("A=1\nA=2\nB=3\nDEBUG\n")?;
    /// let winners: Vec<_> = doc
    ///     .effective_entries()
    ///     .map(|entry| (entry.key(), entry.value(), entry.line()))
    ///     .collect();
    /// assert_eq!(winners, [("A", "2", 2), ("B", "3", 3)]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn effective_entries(&self) -> impl Iterator<Item = EffectiveEntry<'_>> {
        let mut effective = Vec::new();
        if self.has_duplicate_keys {
            let mut seen = BTreeSet::new();
            let indices: alloc::boxed::Box<dyn Iterator<Item = usize>> =
                match self.dialect.duplicates {
                    Duplicates::First => alloc::boxed::Box::new(0..self.entries.len()),
                    Duplicates::Last => alloc::boxed::Box::new((0..self.entries.len()).rev()),
                };
            for index in indices {
                let Some(entry) = self.entry(index) else {
                    continue;
                };
                let key = entry.key();
                if !entry.participates_in_lookup() || key.is_empty() || !seen.insert(key) {
                    continue;
                }
                let value = entry.value().or_else(|| {
                    (entry.state() == AssignmentState::Bare
                        && self.dialect.bare_keys == BareKeys::Empty)
                        .then_some("")
                });
                if let Some(value) = value {
                    effective.push((index, EffectiveEntry { entry, value }));
                }
            }
            effective.sort_unstable_by_key(|(index, _)| *index);
        } else {
            for index in 0..self.entries.len() {
                let Some(entry) = self.entry(index) else {
                    continue;
                };
                if !entry.participates_in_lookup() || entry.key().is_empty() {
                    continue;
                }
                let value = entry.value().or_else(|| {
                    (entry.state() == AssignmentState::Bare
                        && self.dialect.bare_keys == BareKeys::Empty)
                        .then_some("")
                });
                if let Some(value) = value {
                    effective.push((index, EffectiveEntry { entry, value }));
                }
            }
        }
        effective.into_iter().map(|(_, entry)| entry)
    }

    /// Builds a [`LookupIndex`] for fast repeated lookups.
    ///
    /// [`Document::get`] scans the entries on every call. The index stores the
    /// winning declaration of every non-empty key once, in a hash table, and
    /// returns the same results as [`Document::get`] and
    /// [`Document::get_entry`] for those keys. If many keys collide in the
    /// table, it uses an ordered map instead, so a crafted file cannot make
    /// lookups slow. The index borrows this document.
    ///
    /// # Examples
    ///
    /// ```
    /// let doc = adotenv::parse_strict("PORT=8080\nHOST=localhost\nPORT=9090\n")?;
    /// let index = doc.lookup_index();
    ///
    /// assert_eq!(index.get("PORT"), Some("9090"));
    /// assert_eq!(index.get("HOST"), Some("localhost"));
    /// assert_eq!(index.len(), 2);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn lookup_index(&self) -> LookupIndex<'_> {
        LookupIndex {
            document: self,
            winners: Winners::build(self, self.dialect.duplicates),
        }
    }

    #[cfg(test)]
    pub(crate) fn lookup_index_with_probe_limit(&self, probe_limit: usize) -> LookupIndex<'_> {
        LookupIndex {
            document: self,
            winners: Winners::build_with_limit(self, self.dialect.duplicates, probe_limit),
        }
    }

    /// Returns the only declaration of `key`, failing if there are none or
    /// several.
    ///
    /// Every declaration of the key counts, including bare keys and
    /// declarations that do not take part in lookup. The editing helpers, such
    /// as [`Document::set`], select their target this way, so they never pick
    /// one of several duplicates by guessing.
    ///
    /// # Errors
    ///
    /// - [`EditError::Missing`] when no declaration has the key.
    /// - [`EditError::Duplicate`] with the number of declarations when the key
    ///   is declared more than once.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::EditError;
    ///
    /// let doc = adotenv::parse_strict("PORT=8080\nPORT=9090\nHOST=localhost\n")?;
    ///
    /// assert_eq!(doc.unique("HOST")?.value(), Some("localhost"));
    /// assert_eq!(doc.unique("PORT").unwrap_err(), EditError::Duplicate(2));
    /// assert_eq!(doc.unique("MISSING").unwrap_err(), EditError::Missing);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn unique(&self, key: &str) -> Result<Entry<'_>, EditError> {
        let mut entries = self.entries().filter(|entry| entry.key() == key);
        let first = entries.next().ok_or(EditError::Missing)?;
        let remaining = entries.count();
        if remaining == 0 {
            Ok(first)
        } else {
            Err(EditError::Duplicate(remaining + 1))
        }
    }
    /// Returns whether the source starts with a UTF-8 byte-order mark that is
    /// kept outside the items.
    ///
    /// This is `true` only when the dialect strips the mark
    /// ([`BomPolicy::Strip`](crate::BomPolicy::Strip), used by every preset).
    /// The three bytes stay in [`Document::source`] but belong to no item, and
    /// parsing reports [`DiagnosticCode::ByteOrderMark`]. Under
    /// [`BomPolicy::Error`](crate::BomPolicy::Error) the mark is an
    /// [`ItemKind::Invalid`] item instead, and under
    /// [`BomPolicy::Keep`](crate::BomPolicy::Keep) it is part of the first
    /// statement.
    ///
    /// # Examples
    ///
    /// ```
    /// let doc = adotenv::parse_strict("\u{feff}PORT=8080\n")?;
    ///
    /// assert!(doc.has_bom());
    /// assert_eq!(doc.bom_span(), Some(0..3));
    /// assert_eq!(doc.items().next().unwrap().span(), 3..13);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub const fn has_bom(&self) -> bool {
        self.bom
    }
    /// Returns the byte range `0..3` of the stripped byte-order mark, or
    /// `None` if [`Document::has_bom`] is `false`.
    #[must_use]
    pub fn bom_span(&self) -> Option<Range<usize>> {
        self.bom.then_some(0..3)
    }
    /// Returns whether the source ends with a line ending (LF, CRLF or a lone
    /// CR).
    #[must_use]
    pub fn has_final_newline(&self) -> bool {
        self.source.ends_with(['\n', '\r'])
    }
    /// Builds a [`LineIndex`] that converts byte offsets in the source to
    /// lines and columns.
    ///
    /// Parsing does not build one. Each call scans the source again, so keep
    /// the index when converting many positions.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{ColumnEncoding, Position};
    ///
    /// let source = "HOST=localhost\nPORT=8080\n";
    /// let doc = adotenv::parse_strict(source)?;
    /// let index = doc.line_index();
    ///
    /// let value = doc.get_entry("PORT").unwrap().value_span().unwrap();
    /// let position = index.to_position(value.start, ColumnEncoding::Utf8)?;
    /// assert_eq!(position, Position { line: 1, column: 5 });
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn line_index(&self) -> LineIndex<'_> {
        LineIndex::new(self.source())
    }
    /// Converts the document into one that owns its source, so it can outlive
    /// the borrowed text.
    ///
    /// The source is copied only if the document still borrows it. Items,
    /// entries and their ids stay the same.
    ///
    /// # Examples
    ///
    /// ```
    /// fn load() -> adotenv::Document<'static> {
    ///     let text = String::from("PORT=8080\n");
    ///     let (doc, _) = adotenv::parse(&text);
    ///     doc.into_owned()
    /// }
    ///
    /// assert_eq!(load().get("PORT"), Some("8080"));
    /// ```
    #[must_use]
    pub fn into_owned(self) -> Document<'static> {
        Document {
            source: Cow::Owned(self.source.into_owned()),
            items: self.items,
            entries: self.entries,
            bom: self.bom,
            dialect: self.dialect,
            has_duplicate_keys: self.has_duplicate_keys,
        }
    }
    /// The dominant source line ending, or LF for a document without endings.
    pub(crate) fn preferred_ending(&self) -> &'static str {
        let mut counts = [0usize; 3];
        let bytes = self.source.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            match bytes[at] {
                b'\r' if bytes.get(at + 1) == Some(&b'\n') => {
                    counts[1] += 1;
                    at += 1;
                }
                b'\r' if self.dialect.line_breaks == crate::LineBreaks::Universal => counts[2] += 1,
                b'\n' => counts[0] += 1,
                _ => {}
            }
            at += 1;
        }
        if counts[1] > counts[0] && counts[1] >= counts[2] {
            "\r\n"
        } else if self.dialect.line_breaks == crate::LineBreaks::Universal
            && counts[2] > counts[0]
            && counts[2] > counts[1]
        {
            "\r"
        } else {
            "\n"
        }
    }
}

impl fmt::Display for Document<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.source())
    }
}

/// A declaration in a [`Document`]: a `KEY=value` or bare `KEY` item.
///
/// An entry is a small `Copy` view that borrows its document. Get one from
/// [`Document::entries`], [`Document::get_entry`], [`Document::unique`] or
/// [`Item::entry`].
///
/// [`Entry::value`] is the decoded value: quotes removed and escapes decoded
/// as the dialect defines, without the inline comment or the whitespace around
/// it. It stays literal, so `$HOME` is not expanded. [`Entry::raw_value`],
/// [`Entry::tokens`] and the span methods give the source text behind it. A
/// bare key has no value (`None`), which is different from an empty
/// assignment such as `KEY=` (`Some("")`); see [`AssignmentState`].
///
/// Every declaration is an entry, including duplicates and declarations that
/// do not take part in lookup. The *effective value* of a key, after the
/// dialect's duplicate-key and bare-key rules, comes from [`Document::get`].
///
/// # Examples
///
/// ```
/// use adotenv::{AssignmentState, Quote};
///
/// let source = "export GREETING=\"hello\\nworld\" # shown at login\n";
/// let doc = adotenv::parse_strict(source)?;
/// let entry = doc.unique("GREETING")?;
///
/// assert_eq!(entry.value(), Some("hello\nworld"));
/// assert_eq!(entry.raw_value(), Some("\"hello\\nworld\""));
/// assert_eq!(entry.quote(), Quote::Double);
/// assert_eq!(entry.state(), AssignmentState::Value);
/// assert!(entry.exported());
/// assert_eq!(entry.inline_comment(), Some("# shown at login"));
/// assert_eq!(&source[entry.key_span()], "GREETING");
/// assert_eq!(entry.line(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Entry<'doc> {
    pub(crate) document: &'doc Document<'doc>,
    pub(crate) index: usize,
}
/// A prebuilt key lookup table for one [`Document`].
///
/// [`Document::get`] scans the entries on every call. Build this index once
/// with [`Document::lookup_index`] when you look up many keys. It holds the
/// declaration that wins lookup for every non-empty key, using the document's
/// dialect rules, so [`LookupIndex::get`] and [`LookupIndex::get_entry`]
/// return the same results as [`Document::get`] and [`Document::get_entry`].
/// The only difference is the empty key, which some dialects (such as Compose)
/// allow: the index leaves it out.
///
/// The index borrows its document. Editing methods return new text, so after
/// an edit, parse that text and build a new index.
///
/// # Examples
///
/// ```
/// let doc = adotenv::parse_strict("PORT=8080\nDEBUG\nPORT=9090\n")?;
/// let index = doc.lookup_index();
///
/// assert_eq!(index.get("PORT"), Some("9090"));
/// assert_eq!(index.get_entry("PORT").unwrap().line(), 3);
/// // `DEBUG` is a bare key: it is in the index but has no value.
/// assert_eq!(index.get("DEBUG"), None);
/// assert_eq!(index.len(), 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone)]
pub struct LookupIndex<'doc> {
    document: &'doc Document<'doc>,
    winners: Winners<'doc>,
}

#[cfg(test)]
impl LookupIndex<'_> {
    pub(crate) fn hash_slots(&self) -> Option<usize> {
        match &self.winners {
            Winners::Hash(winners) => Some(winners.slots.len()),
            Winners::Tree(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
enum Winners<'doc> {
    Hash(HashWinners<'doc>),
    Tree(BTreeMap<&'doc str, usize>),
}

impl<'doc> Winners<'doc> {
    fn build(document: &'doc Document<'doc>, policy: Duplicates) -> Self {
        let entries = document.entries().len();
        let probe_limit = entries.saturating_mul(4).max(64);
        Self::build_with_limit(document, policy, probe_limit)
    }

    fn build_with_limit(
        document: &'doc Document<'doc>,
        policy: Duplicates,
        probe_limit: usize,
    ) -> Self {
        if probe_limit == 0 {
            return Self::Tree(tree_winners(document, policy));
        }
        HashWinners::build(document, policy, probe_limit)
            .map_or_else(|| Self::Tree(tree_winners(document, policy)), Self::Hash)
    }

    fn get(&self, key: &str) -> Option<usize> {
        match self {
            Self::Hash(winners) => winners.get(key),
            Self::Tree(winners) => winners.get(key).copied(),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Hash(winners) => winners.len,
            Self::Tree(winners) => winners.len(),
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Copy)]
struct HashSlot<'doc> {
    key: &'doc str,
    index: usize,
}

#[derive(Debug, Clone)]
struct HashWinners<'doc> {
    slots: Vec<Option<HashSlot<'doc>>>,
    len: usize,
}

impl<'doc> HashWinners<'doc> {
    fn build(
        document: &'doc Document<'doc>,
        policy: Duplicates,
        probe_limit: usize,
    ) -> Option<Self> {
        // Start small and grow with participating keys. Sizing from all entries
        // allocates a large table for declarations a dialect excludes from
        // lookup, such as Node bare keys.
        let mut slots = alloc::vec![None; 8];
        let mut len: usize = 0;
        let mut probes = 0;
        for entry in document.entries() {
            if !entry.participates_in_lookup() || entry.key().is_empty() {
                continue;
            }
            if len.saturating_add(1) > slots.len().saturating_mul(3) / 4 {
                let capacity = slots.len().checked_mul(2)?;
                slots = rehash(slots, capacity);
            }
            let mask = slots.len() - 1;
            let key = entry.key();
            let mut at = hash_key(key) & mask;
            loop {
                match slots[at] {
                    None => {
                        slots[at] = Some(HashSlot {
                            key,
                            index: entry.id(),
                        });
                        len += 1;
                        break;
                    }
                    Some(slot) if slot.key == key => {
                        if policy == Duplicates::Last {
                            slots[at] = Some(HashSlot {
                                key,
                                index: entry.id(),
                            });
                        }
                        break;
                    }
                    Some(_) => {
                        probes += 1;
                        if probes > probe_limit {
                            return None;
                        }
                        at = (at + 1) & mask;
                    }
                }
            }
        }
        Some(Self { slots, len })
    }

    fn get(&self, key: &str) -> Option<usize> {
        let mask = self.slots.len() - 1;
        let mut at = hash_key(key) & mask;
        loop {
            match self.slots[at] {
                None => return None,
                Some(slot) if slot.key == key => return Some(slot.index),
                Some(_) => at = (at + 1) & mask,
            }
        }
    }
}

fn rehash(slots: Vec<Option<HashSlot<'_>>>, capacity: usize) -> Vec<Option<HashSlot<'_>>> {
    let mask = capacity - 1;
    let mut grown = alloc::vec![None; capacity];
    for slot in slots.into_iter().flatten() {
        let mut at = hash_key(slot.key) & mask;
        while grown[at].is_some() {
            at = (at + 1) & mask;
        }
        grown[at] = Some(slot);
    }
    grown
}

fn tree_winners<'doc>(
    document: &'doc Document<'doc>,
    policy: Duplicates,
) -> BTreeMap<&'doc str, usize> {
    let mut winners = BTreeMap::new();
    for (index, entry) in document.entries().enumerate() {
        if !entry.participates_in_lookup() || entry.key().is_empty() {
            continue;
        }
        match policy {
            Duplicates::First => {
                winners.entry(entry.key()).or_insert(index);
            }
            Duplicates::Last => {
                winners.insert(entry.key(), index);
            }
        }
    }
    winners
}

#[allow(clippy::cast_possible_truncation)]
fn hash_key(key: &str) -> usize {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash as usize
}

impl<'doc> LookupIndex<'doc> {
    /// Returns the declaration that wins lookup for `key`.
    ///
    /// The result is the same as [`Document::get_entry`], except that the
    /// empty key is never found.
    #[must_use]
    pub fn get_entry(&self, key: &str) -> Option<Entry<'doc>> {
        self.winners
            .get(key)
            .and_then(|index| self.document.entry(index))
    }
    /// Returns the effective value of `key`, or `None` if it has none.
    ///
    /// The result is the same as [`Document::get`], except that the empty key
    /// is never found. A bare key that wins has no value unless the dialect
    /// uses [`BareKeys::Empty`].
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&'doc str> {
        self.get_entry(key).and_then(|entry| {
            entry.value().or_else(|| {
                (entry.state() == AssignmentState::Bare
                    && self.document.dialect().bare_keys == BareKeys::Empty)
                    .then_some("")
            })
        })
    }
    /// Returns the number of keys in the index.
    ///
    /// It counts every non-empty key that has a declaration taking part in
    /// lookup, including keys whose winner is a bare key without a value. It
    /// can therefore be larger than the count of
    /// [`Document::effective_entries`].
    #[must_use]
    pub fn len(&self) -> usize {
        self.winners.len()
    }
    /// Returns whether the index holds no keys.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.winners.is_empty()
    }
}

/// The declaration that wins lookup for a key, with its effective value.
///
/// [`Document::effective_entries`] yields these. [`EffectiveEntry::value`] is
/// what [`Document::get`] returns for the key. It differs from the entry's own
/// [`Entry::value`] only for a bare key under [`BareKeys::Empty`]: the
/// effective value is `""` while the entry stays bare.
///
/// # Examples
///
/// ```
/// use adotenv::{AssignmentState, BareKeys, Dialect};
///
/// let dialect = Dialect::PORTABLE.with_bare_keys(BareKeys::Empty);
/// let (doc, _) = adotenv::parse_with("DEBUG\nPORT=8080\n", dialect);
/// let debug = doc.effective_entries().next().unwrap();
///
/// assert_eq!((debug.key(), debug.value(), debug.line()), ("DEBUG", "", 1));
/// assert_eq!(debug.entry().value(), None);
/// assert_eq!(debug.entry().state(), AssignmentState::Bare);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct EffectiveEntry<'doc> {
    entry: Entry<'doc>,
    value: &'doc str,
}

impl<'doc> EffectiveEntry<'doc> {
    /// Returns the winning declaration.
    #[must_use]
    pub const fn entry(self) -> Entry<'doc> {
        self.entry
    }
    /// Returns the key.
    #[must_use]
    pub fn key(self) -> &'doc str {
        self.entry.key()
    }
    /// Returns the effective value, as [`Document::get`] returns it.
    #[must_use]
    pub const fn value(self) -> &'doc str {
        self.value
    }
    /// Returns the one-based line of the winning declaration's key.
    #[must_use]
    pub fn line(self) -> usize {
        self.entry.line()
    }
}

impl<'doc> Entry<'doc> {
    fn data(self) -> &'doc EntryData {
        &self.document.entries[self.index]
    }
    /// Returns the zero-based id of this declaration: its position in
    /// [`Document::entries`].
    ///
    /// Ids belong to this document only. [`Edit`](crate::Edit) operations
    /// select declarations by this id.
    #[must_use]
    pub const fn id(self) -> usize {
        self.index
    }
    /// Returns the zero-based id of the item that holds this declaration: its
    /// position in [`Document::items`].
    #[must_use]
    pub fn item_id(self) -> usize {
        self.data().item
    }
    /// Returns the key, without any key quotes.
    ///
    /// The key is borrowed from the source. Dialects that allow quoted keys
    /// ([`Dialect::quoted_keys`]) drop the quotes here; [`EntryTokens::key`]
    /// keeps them.
    #[must_use]
    pub fn key(self) -> &'doc str {
        &self.document.source[self.key_span()]
    }
    /// Returns the decoded value, or `None` for a bare key.
    ///
    /// Decoding removes the quotes and decodes escapes as the dialect defines;
    /// the inline comment and the whitespace around the value are not part of
    /// it. The value stays literal: `$HOME` and `${HOME}` are not expanded. An
    /// empty assignment such as `KEY=` returns `Some("")`, not `None`. The
    /// value is borrowed from the source when no decoding was needed.
    ///
    /// This is the declaration's own value. The *effective value* of a key,
    /// after duplicate-key and bare-key rules, comes from [`Document::get`].
    ///
    /// # Examples
    ///
    /// ```
    /// let doc = adotenv::parse_strict("A=\"tab\\there\"\nB='$HOME'\nC=\nD\n")?;
    ///
    /// assert_eq!(doc.unique("A")?.value(), Some("tab\there"));
    /// assert_eq!(doc.unique("B")?.value(), Some("$HOME"));
    /// assert_eq!(doc.unique("C")?.value(), Some(""));
    /// assert_eq!(doc.unique("D")?.value(), None);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn value(self) -> Option<&'doc str> {
        let data = self.data();
        data.tokens.body.as_ref().map(|body| {
            data.decoded
                .as_deref()
                .unwrap_or(&self.document.source[body.clone()])
        })
    }
    /// Returns the value exactly as written, including its quotes, or `None`
    /// for a bare key.
    ///
    /// Text after the closing quote is included only when the dialect appends
    /// it to the value ([`QuotedTail::Append`]). This is the text of
    /// [`Entry::value_span`].
    #[must_use]
    pub fn raw_value(self) -> Option<&'doc str> {
        self.value_span().map(|span| &self.document.source[span])
    }
    /// Returns whether this declaration takes part in lookup.
    ///
    /// Every declaration takes part in Portable. A dialect can leave some out:
    /// a bare key under [`BareKeys::Ignore`] (used by the Node dotenv, Compose
    /// and Docker presets), and the key `__proto__` in Node dotenv
    /// ([`Dialect::node_object_keys`]). Such declarations still appear in
    /// [`Document::entries`], [`Document::get_all`] and [`Document::unique`]
    /// and can be edited, but [`Document::get`] skips them.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::Dialect;
    ///
    /// let (doc, _) = adotenv::parse_with("PORT=8080\nPORT\n", Dialect::NODE_DOTENV);
    /// let bare = doc.get_all("PORT").last().unwrap();
    ///
    /// assert!(!bare.participates_in_lookup());
    /// assert_eq!(doc.get("PORT"), Some("8080"));
    /// ```
    #[must_use]
    pub fn participates_in_lookup(self) -> bool {
        self.data().participates
    }
    /// Returns the quote style of the value; [`Quote::None`] for a bare key.
    #[must_use]
    pub fn quote(self) -> Quote {
        self.data().quote
    }
    /// Returns whether the declaration starts with `export`.
    #[must_use]
    pub fn exported(self) -> bool {
        self.data().tokens.export.is_some()
    }
    /// Returns whether the declaration is bare, empty or has a value.
    ///
    /// See [`AssignmentState`]. The state depends on the source only, not on
    /// lookup rules.
    #[must_use]
    pub fn state(self) -> AssignmentState {
        self.data().state
    }
    /// Returns the one-based line of the key.
    ///
    /// Lines are counted as [`LineIndex`] counts them: LF, CRLF and a lone CR
    /// each end a line, in every dialect. So in Compose and Docker, where a
    /// lone CR does not end a statement, it still starts a new line number.
    #[must_use]
    pub fn line(self) -> usize {
        self.data().line
    }
    /// Returns the byte range of the whole statement, including its line
    /// ending.
    ///
    /// This is the span of the item that holds the declaration
    /// ([`Item::span`]). It covers every line of a multiline value. In
    /// dialects that read several statements on one line, such as Compose, it
    /// can be part of a line.
    #[must_use]
    pub fn line_span(self) -> Range<usize> {
        self.document.items[self.item_id()].span.clone()
    }
    /// Returns the byte range of the key, without any key quotes.
    #[must_use]
    pub fn key_span(self) -> Range<usize> {
        let range = self.data().tokens.key.clone();
        if self.data().key_quoted {
            range.start + 1..range.end - 1
        } else {
            range
        }
    }
    /// Returns the byte range of the value as written, including its quotes,
    /// or `None` for a bare key.
    ///
    /// Text after the closing quote is included only when the dialect appends
    /// it to the value ([`QuotedTail::Append`]).
    #[must_use]
    pub fn value_span(self) -> Option<Range<usize>> {
        self.data().tokens.value.as_ref().map(|range| {
            range.start..if self.document.dialect.quoted_tail == QuotedTail::Append {
                self.data()
                    .tokens
                    .quoted_tail
                    .as_ref()
                    .map_or(range.end, |tail| tail.end)
            } else {
                range.end
            }
        })
    }
    /// Returns the byte ranges of every part of the declaration, such as the
    /// key, separator, value and inline comment.
    #[must_use]
    pub fn tokens(self) -> &'doc EntryTokens {
        &self.data().tokens
    }
    /// Returns the inline comment, including its `#`, or `None` if there is
    /// none.
    ///
    /// Whole-line comments above the declaration are available through
    /// [`Entry::leading_comments`].
    #[must_use]
    pub fn inline_comment(self) -> Option<&'doc str> {
        self.data()
            .tokens
            .inline_comment
            .as_ref()
            .map(|span| &self.document.source[span.clone()])
    }
    /// Maps a byte offset in the decoded value to a byte offset in the source.
    ///
    /// Escapes make the decoded value shorter than its source text, so offsets
    /// differ: in `A="a\nb"` the decoded `b` is at offset 2 of the value but at
    /// byte 6 of the source. An offset at the start of a decoded escape maps
    /// to the start of the escape in the source. The offset equal to the value
    /// length is valid too. The time taken grows with the logarithm of the
    /// number of escapes.
    ///
    /// # Errors
    ///
    /// - [`PositionError::OutOfBounds`] when the declaration is a bare key or
    ///   `offset` is greater than the length of the decoded value.
    /// - [`PositionError::InvalidUtf8Boundary`] when `offset` is inside a
    ///   UTF-8 character of the decoded value.
    ///
    /// # Examples
    ///
    /// ```
    /// let source = "A=\"a\\nb\"\n";
    /// let doc = adotenv::parse_strict(source)?;
    /// let entry = doc.unique("A")?;
    ///
    /// assert_eq!(entry.value(), Some("a\nb"));
    /// assert_eq!(entry.decoded_offset_to_raw(1)?, 4);
    /// assert_eq!(entry.decoded_offset_to_raw(2)?, 6);
    /// assert_eq!(&source[4..6], "\\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn decoded_offset_to_raw(self, offset: usize) -> Result<usize, PositionError> {
        let value = self.value().ok_or(PositionError::OutOfBounds)?;
        if offset > value.len() {
            return Err(PositionError::OutOfBounds);
        }
        if !value.is_char_boundary(offset) {
            return Err(PositionError::InvalidUtf8Boundary);
        }
        let data = self.data();
        let body = data
            .tokens
            .body
            .as_ref()
            .ok_or(PositionError::OutOfBounds)?;
        let segments = data.map.as_deref().unwrap_or(&[]);
        let count = segments.partition_point(|segment| segment.decoded.start <= offset);
        let Some(index) = count.checked_sub(1) else {
            return Ok(body.start + offset);
        };
        let segment = &segments[index];
        if offset == segment.decoded.start {
            return Ok(segment.raw.start);
        }
        if offset < segment.decoded.end {
            return Err(PositionError::InvalidUtf8Boundary);
        }
        Ok(segment.raw.end + offset - segment.decoded.end)
    }
    /// Maps a byte range in the decoded value to a byte range in the source.
    ///
    /// Both ends are mapped with [`Entry::decoded_offset_to_raw`], so a range
    /// that covers a decoded escape covers the whole escape in the source.
    ///
    /// # Errors
    ///
    /// - [`PositionError::OutOfBounds`] when the range is reversed, the
    ///   declaration is a bare key, or an end is past the decoded value.
    /// - [`PositionError::InvalidUtf8Boundary`] when an end is inside a UTF-8
    ///   character of the decoded value.
    ///
    /// # Examples
    ///
    /// ```
    /// let source = "export A=\"two words\" # note\n";
    /// let doc = adotenv::parse_strict(source)?;
    /// let entry = doc.unique("A")?;
    ///
    /// let raw = entry.decoded_span_to_raw(0..3)?;
    /// assert_eq!(raw, 10..13);
    /// assert_eq!(&source[raw], "two");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn decoded_span_to_raw(self, span: Range<usize>) -> Result<Range<usize>, PositionError> {
        if span.start > span.end {
            return Err(PositionError::OutOfBounds);
        }
        Ok(self.decoded_offset_to_raw(span.start)?..self.decoded_offset_to_raw(span.end)?)
    }
}

/// One piece of a [`Document`]'s source: an entry, comment, blank line, or
/// invalid or ignored statement.
///
/// An item is a small `Copy` view that borrows its document. Get one from
/// [`Document::items`] or [`Document::item`]; [`Item::kind`] says what it
/// holds.
///
/// The items cover the source in order, with no gaps or overlaps, except a
/// leading byte-order mark that the dialect strips ([`Document::bom_span`]).
/// Most items are one line including its line ending. A multiline quoted
/// value makes one item of several lines, and dialects that read several
/// statements on one line, such as Compose, can split a line into several
/// items.
///
/// # Examples
///
/// ```
/// use adotenv::{DiagnosticCode, ItemKind, LineEnding};
///
/// let (doc, _) = adotenv::parse("# HTTP settings\nBAD KEY=1\r\n");
/// let comment = doc.item(0).unwrap();
/// let invalid = doc.item(1).unwrap();
///
/// assert_eq!(comment.kind(), ItemKind::Comment);
/// assert_eq!(comment.comment_text(), Some(" HTTP settings"));
/// assert_eq!(invalid.kind(), ItemKind::Invalid);
/// assert_eq!(invalid.raw(), "BAD KEY=1\r\n");
/// assert_eq!(invalid.line_ending(), LineEnding::CrLf);
/// assert_eq!(invalid.content_span(), 16..25);
/// assert_eq!(invalid.invalid_code(), Some(DiagnosticCode::InvalidKey));
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Item<'doc> {
    pub(crate) document: &'doc Document<'doc>,
    pub(crate) index: usize,
}
impl<'doc> Item<'doc> {
    fn data(self) -> &'doc ItemData {
        &self.document.items[self.index]
    }
    /// Returns the zero-based id of this item: its position in
    /// [`Document::items`].
    ///
    /// Ids belong to this document only. [`Edit`](crate::Edit) operations
    /// select items by this id.
    #[must_use]
    pub const fn id(self) -> usize {
        self.index
    }
    /// Returns what kind of item this is.
    #[must_use]
    pub fn kind(self) -> ItemKind {
        self.data().kind
    }
    /// Returns the byte range of the item, including its line ending.
    #[must_use]
    pub fn span(self) -> Range<usize> {
        self.data().span.clone()
    }
    /// Returns the item's source text exactly as written, including its line
    /// ending.
    #[must_use]
    pub fn raw(self) -> &'doc str {
        &self.document.source[self.span()]
    }
    /// Returns the line ending at the end of the item.
    ///
    /// [`LineEnding::None`] means the item ends at the end of the source or
    /// another statement follows on the same line.
    #[must_use]
    pub fn line_ending(self) -> LineEnding {
        self.data().ending
    }
    /// Returns the byte range of the item without its final line ending.
    #[must_use]
    pub fn content_span(self) -> Range<usize> {
        let span = self.span();
        span.start..span.end - self.line_ending().as_str().len()
    }
    /// Returns the declaration when this item is an [`ItemKind::Entry`], and
    /// `None` otherwise.
    #[must_use]
    pub fn entry(self) -> Option<Entry<'doc>> {
        let index = self.data().entry;
        (index != NO_ENTRY).then_some(Entry {
            document: self.document,
            index,
        })
    }
    /// Returns the text after the `#` of a whole-line comment, or `None` for
    /// other items.
    ///
    /// The text keeps its spaces and excludes the line ending: `# note\n`
    /// gives `" note"`. Inline comments of entries are available through
    /// [`Entry::inline_comment`].
    #[must_use]
    pub fn comment_text(self) -> Option<&'doc str> {
        self.data()
            .comment
            .as_ref()
            .map(|span| &self.document.source[span.clone()])
    }
    /// Returns the diagnostic attached to this item, if any.
    ///
    /// Invalid items carry their error, and ignored items carry a
    /// [`DiagnosticCode::IgnoredStatement`] warning. An entry carries one when
    /// it breaks a dialect policy, such as [`DiagnosticCode::ForbiddenBareKey`]
    /// or [`DiagnosticCode::InvalidEscape`]. Other warnings, such as
    /// [`DiagnosticCode::DuplicateKey`], are not attached to items: they are
    /// only in the list that [`parse`](crate::parse) returns.
    #[must_use]
    pub fn diagnostic(self) -> Option<&'doc Diagnostic> {
        self.data().invalid.as_deref()
    }
    /// Returns the code of the diagnostic attached to this item, if any.
    ///
    /// This is a shortcut for `item.diagnostic().map(Diagnostic::code)`.
    #[must_use]
    pub fn invalid_code(self) -> Option<DiagnosticCode> {
        self.diagnostic().map(Diagnostic::code)
    }
}
