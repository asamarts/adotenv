//! Verified layout formatting: separator spacing, trailing whitespace, blank
//! lines, value quoting and the final line ending.
//!
//! Formatting changes only the layout that the caller selects. It parses the
//! complete result again with the document's dialect and checks that every
//! declaration's meaning is unchanged.

use alloc::{string::String, vec, vec::Vec};
use core::fmt;

use crate::{Document, ItemKind, Quote, TextEdit};

/// The spacing that [`Document::format`] puts around assignment separators.
///
/// The spacing applies to the separator (`=`, or `:` in dialects that accept
/// it) of every assignment; bare declarations have none. Declarations with an
/// empty key and every declaration in a raw-value dialect such as
/// [`Dialect::DOCKER_RAW`](crate::Dialect::DOCKER_RAW) keep their spacing. The
/// space after the separator also stays when the value is empty or starts
/// with `#` and no inline comment follows, because changing it could change
/// the value.
///
/// # Examples
///
/// ```
/// use adotenv::{EqualsSpacing, FormatOptions};
///
/// let doc = adotenv::parse_strict("PORT =  8080\nHOST=localhost\n")?;
/// let spaces = FormatOptions::default().with_equals_spacing(EqualsSpacing::Spaces);
/// assert_eq!(doc.format(&spaces)?.as_str(), "PORT = 8080\nHOST = localhost\n");
/// let none = FormatOptions::default().with_equals_spacing(EqualsSpacing::None);
/// assert_eq!(doc.format(&none)?.as_str(), "PORT=8080\nHOST=localhost\n");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EqualsSpacing {
    /// The spacing stays as it is.
    Preserve,
    /// Exactly one space before and one after the separator.
    Spaces,
    /// No space before or after the separator.
    ///
    /// On a line with an inline comment, the space after the separator stays.
    None,
}

/// What [`Document::format`] does with the line ending at the end of the file.
///
/// # Examples
///
/// ```
/// use adotenv::{FinalNewline, FormatOptions};
///
/// let doc = adotenv::parse_strict("PORT=8080")?;
/// let ensure = FormatOptions::default().with_final_newline(FinalNewline::Ensure);
/// assert_eq!(doc.format(&ensure)?.as_str(), "PORT=8080\n");
///
/// let doc = adotenv::parse_strict("PORT=8080\r\n")?;
/// let remove = FormatOptions::default().with_final_newline(FinalNewline::Remove);
/// assert_eq!(doc.format(&remove)?.as_str(), "PORT=8080");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FinalNewline {
    /// The end of the file stays as it is.
    Preserve,
    /// A final line ending: an LF is added when the output does not end with a
    /// line break.
    ///
    /// The LF is added even in a CRLF file. Nothing is added to an empty
    /// document, or when collapsing a trailing run of blank lines already
    /// leaves the output ending in a line break. With trimming, a
    /// whitespace-only last line becomes an empty blank line.
    Ensure,
    /// No final line ending: one final LF, CRLF or lone CR is removed.
    ///
    /// Nothing is removed when the last item is a blank line, because removing
    /// its line ending would remove the blank line.
    Remove,
}

/// What [`Document::format`] does with runs of blank lines.
///
/// # Examples
///
/// ```
/// use adotenv::{BlankLines, FormatOptions};
///
/// let doc = adotenv::parse_strict("PORT=8080\n\n\n\nHOST=localhost\n")?;
/// let options = FormatOptions::default().with_blank_lines(BlankLines::Collapse);
/// assert_eq!(doc.format(&options)?.as_str(), "PORT=8080\n\nHOST=localhost\n");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BlankLines {
    /// Blank lines stay as they are.
    Preserve,
    /// Each run of blank lines becomes one blank line.
    ///
    /// The first blank line of each run is kept, also for runs at the start
    /// and the end of the file, so the blank-line groups of the document stay
    /// the same.
    Collapse,
}

/// The quoting that [`Document::format`] prefers for values.
///
/// A value is written in the preferred style only when its decoded text stays
/// exactly the same; a value that the style cannot spell keeps its current
/// quoting. Multiline values are included, so a style change can move a
/// multiline value onto one line with an escaped line break, or the other way
/// around. Empty values, bare declarations and declarations with an empty key
/// keep their form, and values in raw-value dialects such as
/// [`Dialect::DOCKER_RAW`](crate::Dialect::DOCKER_RAW) are never re-quoted.
///
/// # Examples
///
/// ```
/// use adotenv::{FormatOptions, QuoteStyle};
///
/// let doc = adotenv::parse_strict("A='x\ny'\nB=plain\nC=''\n")?;
/// let options = FormatOptions::default().with_quote_style(QuoteStyle::Double);
/// assert_eq!(doc.format(&options)?.as_str(), "A=\"x\\ny\"\nB=\"plain\"\nC=''\n");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum QuoteStyle {
    /// Values keep their current quoting.
    Preserve,
    /// Double quotes, where the dialect can spell the value in them.
    Double,
    /// Single quotes, where the dialect can spell the value in them.
    Single,
    /// No quotes, where the value reads back the same without them.
    ///
    /// Removing quotes changes where a declaration ends, so this style applies
    /// only when every other option keeps its default (`Preserve` or `false`).
    /// Combined with any other change, it is skipped.
    Unquoted,
}

/// The layout changes that [`Document::format`] makes.
///
/// [`FormatOptions::default`] changes nothing. Each field turns on one kind of
/// change and has a `with_*` builder; [`normalize`](Self::normalize) and
/// [`normalize_groups`](Self::normalize_groups) are ready-made presets.
/// Whatever the options, formatting never changes keys, decoded values,
/// `export` prefixes, indentation, the order of items, or invalid and ignored
/// statements; see [`Document::format`].
///
/// # Examples
///
/// ```
/// use adotenv::{BlankLines, EqualsSpacing, FinalNewline, FormatOptions};
///
/// let options = FormatOptions::default()
///     .with_equals_spacing(EqualsSpacing::Spaces)
///     .with_trim_trailing_whitespace(true)
///     .with_blank_lines(BlankLines::Collapse)
///     .with_final_newline(FinalNewline::Ensure);
/// assert_eq!(options, FormatOptions::normalize_groups());
///
/// let doc = adotenv::parse_strict("# app   \nPORT=8080\n\n\nHOST =  localhost")?;
/// let formatted = doc.format(&options)?;
/// assert_eq!(formatted.as_str(), "# app\nPORT = 8080\n\nHOST = localhost\n");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct FormatOptions {
    /// The spacing around assignment separators; see [`EqualsSpacing`].
    pub equals_spacing: EqualsSpacing,
    /// Whether to remove spaces and tabs at the end of blank lines, comment
    /// lines and the last line of each declaration.
    ///
    /// Whitespace inside multiline values and in invalid or ignored statements
    /// is never changed. Declaration lines in raw-value dialects such as
    /// [`Dialect::DOCKER_RAW`](crate::Dialect::DOCKER_RAW) keep their
    /// whitespace, because it belongs to the value. A whitespace-only last
    /// line without a line ending is kept as is unless [`FinalNewline::Ensure`]
    /// is selected, because trimming it would remove the line. Lines that end
    /// with or follow a lone CR line ending, and an empty or bare declaration
    /// followed by an indented line (together with that line), are also left
    /// alone, so the next parse reads the same lines.
    pub trim_trailing_whitespace: bool,
    /// What happens to runs of blank lines; see [`BlankLines`].
    pub blank_lines: BlankLines,
    /// The preferred quoting of values; see [`QuoteStyle`].
    pub quote_style: QuoteStyle,
    /// What happens to the line ending at the end of the file; see
    /// [`FinalNewline`].
    pub final_newline: FinalNewline,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            equals_spacing: EqualsSpacing::Preserve,
            trim_trailing_whitespace: false,
            blank_lines: BlankLines::Preserve,
            quote_style: QuoteStyle::Preserve,
            final_newline: FinalNewline::Preserve,
        }
    }
}

impl FormatOptions {
    /// Sets [`equals_spacing`](Self::equals_spacing) and returns the options.
    #[must_use]
    pub const fn with_equals_spacing(mut self, value: EqualsSpacing) -> Self {
        self.equals_spacing = value;
        self
    }
    /// Sets [`trim_trailing_whitespace`](Self::trim_trailing_whitespace) and
    /// returns the options.
    #[must_use]
    pub const fn with_trim_trailing_whitespace(mut self, value: bool) -> Self {
        self.trim_trailing_whitespace = value;
        self
    }
    /// Sets [`blank_lines`](Self::blank_lines) and returns the options.
    #[must_use]
    pub const fn with_blank_lines(mut self, value: BlankLines) -> Self {
        self.blank_lines = value;
        self
    }
    /// Sets [`quote_style`](Self::quote_style) and returns the options.
    #[must_use]
    pub const fn with_quote_style(mut self, value: QuoteStyle) -> Self {
        self.quote_style = value;
        self
    }
    /// Sets [`final_newline`](Self::final_newline) and returns the options.
    #[must_use]
    pub const fn with_final_newline(mut self, value: FinalNewline) -> Self {
        self.final_newline = value;
        self
    }
}
impl FormatOptions {
    /// A preset that puts one space around separators, trims trailing
    /// whitespace and ensures a final line ending.
    ///
    /// It uses [`EqualsSpacing::Spaces`], trailing-whitespace trimming and
    /// [`FinalNewline::Ensure`]. Blank lines and quotes stay as they are.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::FormatOptions;
    ///
    /// let doc = adotenv::parse_strict("# app   \n\n\nA=1   \nB =  2 # note   ")?;
    /// let formatted = doc.format(&FormatOptions::normalize())?;
    /// assert_eq!(formatted.as_str(), "# app\n\n\nA = 1\nB = 2 # note\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub const fn normalize() -> Self {
        Self {
            equals_spacing: EqualsSpacing::Spaces,
            trim_trailing_whitespace: true,
            blank_lines: BlankLines::Preserve,
            quote_style: QuoteStyle::Preserve,
            final_newline: FinalNewline::Ensure,
        }
    }

    /// The [`normalize`](Self::normalize) preset plus collapsed runs of blank
    /// lines.
    ///
    /// It adds [`BlankLines::Collapse`] to [`normalize`](Self::normalize).
    /// This is the profile that `adotenv-cli fmt` uses.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::FormatOptions;
    ///
    /// let doc = adotenv::parse_strict("# app\n\n\nA=1   \n\n\nB =  2")?;
    /// let formatted = doc.format(&FormatOptions::normalize_groups())?;
    /// assert_eq!(formatted.as_str(), "# app\n\nA = 1\n\nB = 2\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub const fn normalize_groups() -> Self {
        Self {
            equals_spacing: EqualsSpacing::Spaces,
            trim_trailing_whitespace: true,
            blank_lines: BlankLines::Collapse,
            quote_style: QuoteStyle::Preserve,
            final_newline: FinalNewline::Ensure,
        }
    }
}

/// The verified output of [`Document::format`] or [`Document::sort`], with the
/// text edits that produce it.
///
/// Before it is returned, the output was parsed again with the document's
/// dialect and checked. Use [`as_str`](Self::as_str) or
/// [`into_string`](Self::into_string) for the new text, and
/// [`edits`](Self::edits) or [`into_edits`](Self::into_edits) to apply the
/// change to a buffer that still holds the original source.
/// [`is_empty`](Self::is_empty) tells whether anything changed.
///
/// # Examples
///
/// ```
/// use adotenv::FormatOptions;
///
/// let doc = adotenv::parse_strict("PORT=8080   \nHOST =localhost\n")?;
/// let formatted = doc.format(&FormatOptions::normalize())?;
/// assert_eq!(formatted.as_str(), "PORT = 8080\nHOST = localhost\n");
///
/// let mut text = doc.source().to_string();
/// for edit in formatted.edits().iter().rev() {
///     text.replace_range(edit.range.clone(), &edit.replacement);
/// }
/// assert_eq!(text, formatted.as_str());
///
/// // Formatting the output again changes nothing.
/// let again = adotenv::parse_strict(formatted.as_str())?;
/// assert!(again.format(&FormatOptions::normalize())?.is_empty());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatResult {
    edits: Vec<TextEdit>,
    output: String,
}

impl FormatResult {
    pub(crate) fn from_parts(edits: Vec<TextEdit>, output: String) -> Self {
        Self { edits, output }
    }

    /// Returns the text edits that turn the original source into the output.
    ///
    /// The edits are sorted by start offset and do not overlap. Their ranges
    /// refer to the original [`Document::source`]. Apply them in reverse order
    /// of this slice, from the last element to the first; see [`TextEdit`].
    /// [`Document::sort`] returns one edit for each reordered group.
    #[must_use]
    pub fn edits(&self) -> &[TextEdit] {
        &self.edits
    }
    /// Returns the text edits, consuming the result.
    ///
    /// The edits are in the same order as [`edits`](Self::edits) returns them.
    #[must_use]
    pub fn into_edits(self) -> Vec<TextEdit> {
        self.edits
    }
    /// Returns the complete verified output.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.output
    }
    /// Returns the complete verified output, consuming the result.
    #[must_use]
    pub fn into_string(self) -> String {
        self.output
    }
    /// Returns `true` when nothing changed: there are no edits and the output
    /// equals the source.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

/// The reason [`Document::format`] or [`Document::sort`] refused to return a
/// result.
///
/// Both methods refuse rather than return output that reads differently from
/// the source. They skip the changes they know to be risky, so this error is
/// rare; it comes from inputs such as an unterminated quote that a quote
/// change would close.
///
/// # Examples
///
/// ```
/// use adotenv::{FormatError, FormatOptions, QuoteStyle};
///
/// // The recovering parser keeps the unterminated quote as an invalid line.
/// // A double quote added to the next value would close it.
/// let (doc, _) = adotenv::parse("BROKEN=\"\nPORT='8080'\n");
/// let options = FormatOptions::default().with_quote_style(QuoteStyle::Double);
/// match doc.format(&options) {
///     Err(FormatError::WouldChangeMeaning) => {}
///     other => panic!("unexpected result: {other:?}"),
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormatError {
    /// The output, parsed again, would change a declaration, a comment or the
    /// item structure.
    WouldChangeMeaning,
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("formatting would change document meaning")
    }
}

impl core::error::Error for FormatError {}

impl Document<'_> {
    /// Formats the layout of this document as `options` select and verifies the
    /// output.
    ///
    /// Formatting changes only these parts of the layout, each when its option
    /// asks for it:
    ///
    /// - the spacing around separators ([`FormatOptions::equals_spacing`]);
    /// - trailing spaces and tabs ([`FormatOptions::trim_trailing_whitespace`]);
    /// - runs of blank lines ([`FormatOptions::blank_lines`]);
    /// - the quoting of values ([`FormatOptions::quote_style`]);
    /// - the line ending at the end of the file
    ///   ([`FormatOptions::final_newline`]).
    ///
    /// It never changes keys, decoded values, assignment states, `export`
    /// prefixes, lookup participation, indentation, the order of items, the
    /// kind of line endings, the BOM, or the text of comments and inline
    /// comments apart from trailing whitespace. Invalid and ignored statements
    /// are never rewritten. Trimming never touches whitespace inside multiline
    /// values. A quote style other than [`QuoteStyle::Preserve`] re-encodes a
    /// value, multiline values included, only when its decoded text stays the
    /// same. In raw-value dialects such as
    /// [`Dialect::DOCKER_RAW`](crate::Dialect::DOCKER_RAW), declaration lines
    /// stay untouched because every byte after `=` is part of the value;
    /// comments and blank lines are still formatted.
    ///
    /// The complete output is parsed again with this document's dialect before
    /// it is returned. Formatting does not reorder declarations; use
    /// [`Document::sort`] for that.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::WouldChangeMeaning`] if the parsed output does not
    /// keep the keys, values, assignment states, `export` prefixes, lookup
    /// participation and inline comments of every declaration, the kind of
    /// every item and the text of invalid and ignored statements, and also the
    /// quoting when the quote style is [`QuoteStyle::Preserve`]. The formatter
    /// skips the changes it knows to be risky, so this is rare; see
    /// [`FormatError`] for an example.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{EqualsSpacing, FormatOptions};
    ///
    /// let doc = adotenv::parse_strict("# server   \nPORT=8080\nHOST =  localhost\n")?;
    /// let options = FormatOptions::default().with_equals_spacing(EqualsSpacing::Spaces);
    /// let formatted = doc.format(&options)?;
    /// assert_eq!(formatted.as_str(), "# server   \nPORT = 8080\nHOST = localhost\n");
    ///
    /// let formatted = doc.format(&FormatOptions::normalize())?;
    /// assert_eq!(formatted.as_str(), "# server\nPORT = 8080\nHOST = localhost\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn format(&self, options: &FormatOptions) -> Result<FormatResult, FormatError> {
        let mut edits = Vec::new();
        self.plan_equals_spacing(*options, &mut edits);
        self.plan_quotes(*options, &mut edits);
        let removed = self.plan_blank_lines(*options, &mut edits);
        if options.trim_trailing_whitespace {
            self.plan_trailing_whitespace(
                &removed,
                options.final_newline == FinalNewline::Ensure,
                &mut edits,
            );
        }
        self.plan_final_newline(*options, &removed, &mut edits);
        edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
        if edits
            .windows(2)
            .any(|pair| pair[1].range.start < pair[0].range.end)
        {
            return Err(FormatError::WouldChangeMeaning);
        }
        let output = apply(self.source(), &edits);
        let (formatted, _) = crate::parse_with(&output, self.dialect());
        if !same_meaning(
            self,
            &formatted,
            options.blank_lines,
            options.quote_style == QuoteStyle::Preserve,
        ) {
            return Err(FormatError::WouldChangeMeaning);
        }
        Ok(FormatResult { edits, output })
    }

    fn plan_equals_spacing(&self, options: FormatOptions, edits: &mut Vec<TextEdit>) {
        if options.equals_spacing == EqualsSpacing::Preserve {
            return;
        }
        for item in self.items() {
            let Some(entry) = item.entry() else {
                continue;
            };
            let tokens = entry.tokens();
            if tokens.eq.is_none() || entry.key().is_empty() || self.dialect().raw_values {
                continue;
            }
            let before = tokens.space_before_eq.clone();
            match options.equals_spacing {
                EqualsSpacing::Preserve => {}
                EqualsSpacing::Spaces => push(self.source(), edits, before, " ".into()),
                EqualsSpacing::None => push(self.source(), edits, before, String::new()),
            }
            let after = tokens.space_after_eq.clone();
            let content_end = item.content_span().end;
            let inline_comment = tokens.inline_comment.is_some();
            let empty_value = entry.value().map_or(true, str::is_empty);
            if empty_value && !inline_comment {
                // Whitespace after an empty assignment can become indentation
                // for the next physical line in some grammars.
                continue;
            }
            let risky_hash_value =
                !inline_comment && entry.value().is_some_and(|value| value.starts_with('#'));
            if risky_hash_value {
                continue;
            }
            match options.equals_spacing {
                EqualsSpacing::Preserve => {}
                EqualsSpacing::Spaces => {
                    // Trailing whitespace is owned by the trim rule when enabled.
                    if !(options.trim_trailing_whitespace && after.end == content_end) {
                        push(self.source(), edits, after, " ".into());
                    }
                }
                EqualsSpacing::None => {
                    if !(inline_comment
                        || options.trim_trailing_whitespace && after.end == content_end)
                    {
                        push(self.source(), edits, after, String::new());
                    }
                }
            }
        }
    }

    fn plan_quotes(&self, options: FormatOptions, edits: &mut Vec<TextEdit>) {
        let preferred = match options.quote_style {
            QuoteStyle::Preserve => return,
            QuoteStyle::Double => Quote::Double,
            QuoteStyle::Single => Quote::Single,
            QuoteStyle::Unquoted => Quote::None,
        };
        if preferred == Quote::None
            && (options.equals_spacing != EqualsSpacing::Preserve
                || options.trim_trailing_whitespace
                || options.blank_lines != BlankLines::Preserve
                || options.final_newline != FinalNewline::Preserve)
        {
            // Removing quotes changes the declaration boundary; keep that
            // conservative operation separate from other trivia rewrites.
            return;
        }
        if self.dialect().raw_values {
            return;
        }
        for item in self.items() {
            let Some(entry) = item.entry() else {
                continue;
            };
            if entry.key().is_empty() {
                continue;
            }
            let Some(value) = entry.value_span() else {
                continue;
            };
            let Some(decoded) = entry.value() else {
                continue;
            };
            if decoded.is_empty() {
                // Empty values interact with the after-`=` spacing policy and
                // comment separators; preserve them in this slice.
                continue;
            }
            let before = &self.source()[item.span().start..value.start];
            let after = &self.source()[value.end..item.span().end];
            let Ok((token, actual)) = crate::encode::encode_native(
                decoded,
                preferred,
                self.dialect(),
                before,
                after,
                false,
                false,
            ) else {
                continue;
            };
            if actual == preferred && !token.is_empty() && token != self.source()[value.clone()] {
                push(self.source(), edits, value, token);
            }
        }
    }

    fn plan_blank_lines(&self, options: FormatOptions, edits: &mut Vec<TextEdit>) -> Vec<bool> {
        if options.blank_lines == BlankLines::Preserve {
            return Vec::new();
        }
        let mut removed = vec![false; self.items().len()];
        let mut previous_blank = false;
        for item in self.items() {
            if item.kind() != ItemKind::Blank {
                previous_blank = false;
                continue;
            }
            if previous_blank {
                removed[item.id()] = true;
                push(self.source(), edits, item.span(), String::new());
            } else {
                previous_blank = true;
            }
        }
        removed
    }

    fn plan_trailing_whitespace(
        &self,
        removed: &[bool],
        ensure_final_newline: bool,
        edits: &mut Vec<TextEdit>,
    ) {
        let kept: Vec<_> = self
            .items()
            .enumerate()
            .filter(|(index, _)| !removed.get(*index).copied().unwrap_or(false))
            .map(|(_, item)| item)
            .collect();
        let mut previous_ending = crate::LineEnding::None;
        let mut protect_next = false;
        let mut items = kept.into_iter().peekable();
        while let Some(item) = items.next() {
            let ending = item.line_ending();
            if protect_next {
                protect_next = false;
                previous_ending = ending;
                continue;
            }
            if ending == crate::LineEnding::Cr || previous_ending == crate::LineEnding::Cr {
                previous_ending = ending;
                continue;
            }
            previous_ending = ending;
            let next_indented = items
                .peek()
                .is_some_and(|next| matches!(next.raw().as_bytes().first(), Some(b' ' | b'\t')));
            if item.kind() == ItemKind::Entry {
                if self.dialect().raw_values {
                    continue;
                }
                let bare_or_empty = item.entry().is_some_and(|entry| {
                    entry.value().map_or(true, str::is_empty) && entry.inline_comment().is_none()
                });
                if bare_or_empty && next_indented {
                    // Trailing spaces on an empty assignment can become the
                    // next line's indentation in continuation-capable grammars.
                    // Keep that next item's indentation as well, or a later
                    // pass would see a different layout.
                    protect_next = true;
                    continue;
                }
            }
            match item.kind() {
                ItemKind::Blank | ItemKind::Comment | ItemKind::Entry => {
                    let content = item.content_span();
                    let trimmed = trim_end(self.source(), content.clone());
                    if item.kind() == ItemKind::Blank
                        && ending == crate::LineEnding::None
                        && trimmed == content.start
                        && !ensure_final_newline
                    {
                        // A whitespace-only last line without a line ending
                        // would disappear entirely. Keep the line unless a
                        // final newline turns it into an empty blank line.
                        continue;
                    }
                    if trimmed != content.end {
                        push(self.source(), edits, trimmed..content.end, String::new());
                    }
                }
                ItemKind::Invalid | ItemKind::Ignored => {}
            }
        }
    }

    fn plan_final_newline(
        &self,
        options: FormatOptions,
        removed: &[bool],
        edits: &mut Vec<TextEdit>,
    ) {
        let source = self.source();
        match options.final_newline {
            FinalNewline::Preserve => {}
            FinalNewline::Ensure => {
                // Collapsing a trailing blank run can expose an earlier line
                // ending at the end of the output; adding another one would
                // add a blank line or join a lone CR into a CRLF.
                let end = self
                    .items()
                    .rev()
                    .find(|item| !removed.get(item.id()).copied().unwrap_or(false))
                    .map_or(source.len(), |item| item.span().end);
                let output_end = &source[..end];
                if !source.is_empty()
                    && self.items().next().is_some()
                    && !output_end.ends_with('\n')
                    && !output_end.ends_with('\r')
                {
                    push(source, edits, source.len()..source.len(), "\n".into());
                }
            }
            FinalNewline::Remove => {
                // Removing the terminator of a blank final item would remove a
                // blank line (or merge mixed CR/LF boundaries) rather than only
                // a final newline. Keep that structure.
                if self
                    .items()
                    .next_back()
                    .is_some_and(|item| item.kind() != ItemKind::Blank)
                {
                    let remove = if source.ends_with("\r\n") {
                        2
                    } else {
                        usize::from(source.ends_with('\n') || source.ends_with('\r'))
                    };
                    if remove != 0 {
                        push(
                            source,
                            edits,
                            source.len() - remove..source.len(),
                            String::new(),
                        );
                    }
                }
            }
        }
    }
}

fn push(
    source: &str,
    edits: &mut Vec<TextEdit>,
    range: core::ops::Range<usize>,
    replacement: String,
) {
    if source[range.clone()] != replacement {
        edits.push(TextEdit { range, replacement });
    }
}

fn trim_end(source: &str, content: core::ops::Range<usize>) -> usize {
    let mut end = content.end;
    while end > content.start && matches!(source.as_bytes()[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    // Exposing a CR that was followed by horizontal whitespace would change how
    // the next parse recognizes a mixed CR/LF line ending. Leave that line
    // untouched instead of making the formatter non-idempotent.
    if end != content.end && end > content.start && source.as_bytes()[end - 1] == b'\r' {
        return content.end;
    }
    end
}

fn apply(source: &str, edits: &[TextEdit]) -> String {
    let removed: usize = edits.iter().map(|edit| edit.range.len()).sum();
    let added: usize = edits.iter().map(|edit| edit.replacement.len()).sum();
    let mut output = String::with_capacity(source.len() - removed + added);
    let mut cursor = 0;
    for edit in edits {
        output.push_str(&source[cursor..edit.range.start]);
        output.push_str(&edit.replacement);
        cursor = edit.range.end;
    }
    output.push_str(&source[cursor..]);
    output
}

fn same_meaning(
    left: &Document<'_>,
    right: &Document<'_>,
    blank_lines: BlankLines,
    preserve_quote: bool,
) -> bool {
    if left.has_bom() != right.has_bom() {
        return false;
    }
    match blank_lines {
        BlankLines::Preserve => {
            left.items().len() == right.items().len()
                && left
                    .items()
                    .zip(right.items())
                    .all(|(left, right)| same_item(left, right, preserve_quote))
        }
        BlankLines::Collapse => {
            let mut left_items = left.items().filter(|item| item.kind() != ItemKind::Blank);
            let mut right_items = right.items().filter(|item| item.kind() != ItemKind::Blank);
            let same_nonblank = loop {
                match (left_items.next(), right_items.next()) {
                    (Some(left), Some(right)) if same_item(left, right, preserve_quote) => {}
                    (None, None) => break true,
                    _ => break false,
                }
            };
            same_nonblank
                && left.groups().count() == right.groups().count()
                && !right
                    .items()
                    .zip(right.items().skip(1))
                    .any(|(left, right)| {
                        left.kind() == ItemKind::Blank && right.kind() == ItemKind::Blank
                    })
        }
    }
}

fn same_item(left: crate::Item<'_>, right: crate::Item<'_>, preserve_quote: bool) -> bool {
    if left.kind() != right.kind()
        || left.diagnostic().map(crate::Diagnostic::code)
            != right.diagnostic().map(crate::Diagnostic::code)
    {
        return false;
    }
    match left.kind() {
        ItemKind::Blank => true,
        ItemKind::Comment => {
            trim_horizontal(&left.document.source()[left.content_span()])
                == trim_horizontal(&right.document.source()[right.content_span()])
        }
        ItemKind::Invalid | ItemKind::Ignored => {
            left.document.source()[left.content_span()]
                == right.document.source()[right.content_span()]
        }
        ItemKind::Entry => {
            let Some(left) = left.entry() else {
                return false;
            };
            let Some(right) = right.entry() else {
                return false;
            };
            left.key() == right.key()
                && left.value() == right.value()
                && left.state() == right.state()
                && (!preserve_quote || left.quote() == right.quote())
                && left.exported() == right.exported()
                && left.participates_in_lookup() == right.participates_in_lookup()
                && left.inline_comment().map(trim_horizontal)
                    == right.inline_comment().map(trim_horizontal)
        }
    }
}

fn trim_horizontal(text: &str) -> &str {
    text.trim_end_matches([' ', '\t'])
}

#[cfg(test)]
mod tests {
    use super::{same_meaning, BlankLines};
    use crate::parse_strict;

    #[test]
    fn same_meaning_rejects_different_items_with_equal_counts() {
        let left = parse_strict("A=1\n").unwrap();
        let right = parse_strict("A=2\n").unwrap();
        assert!(!same_meaning(&left, &right, BlankLines::Preserve, true));
    }

    #[test]
    fn same_meaning_rejects_different_nonblank_items_when_collapsing() {
        let left = parse_strict("A=1\n\nB=2\n").unwrap();
        let right = parse_strict("A=1\n\nB=3\n").unwrap();
        assert!(!same_meaning(&left, &right, BlankLines::Collapse, true));
    }

    #[test]
    fn same_meaning_rejects_group_count_changes_when_collapsing() {
        let left = parse_strict("A=1\n\nB=2\n").unwrap();
        let right = parse_strict("A=1\nB=2\n").unwrap();
        assert!(!same_meaning(&left, &right, BlankLines::Collapse, true));
    }

    #[test]
    fn same_meaning_rejects_consecutive_blank_items_when_collapsing() {
        let left = parse_strict("A=1\n\nB=2\n").unwrap();
        let right = parse_strict("A=1\n\n\nB=2\n").unwrap();
        assert!(!same_meaning(&left, &right, BlankLines::Collapse, true));
    }
}
