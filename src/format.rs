//! Verified whitespace and line-ending formatting over the lossless model.
//!
//! Formatting only changes syntax trivia selected by the caller. It reparses the
//! complete result with the document's exact dialect and verifies that every
//! declaration's meaning is unchanged.

use alloc::{string::String, vec, vec::Vec};
use core::fmt;

use crate::{Document, ItemKind, Quote, TextEdit};

/// Preferred spacing around assignment separators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EqualsSpacing {
    /// Leave existing spacing untouched.
    Preserve,
    /// Use exactly one ASCII space before and after each applicable assignment
    /// separator (`=` or a configured colon form).
    Spaces,
    /// Remove spaces around applicable assignment separators.
    None,
}

/// Final-newline policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FinalNewline {
    /// Leave the existing end-of-file bytes untouched.
    Preserve,
    /// Ensure a final LF when the source does not already end in a line break.
    Ensure,
    /// Remove one final LF, CRLF or lone CR.
    Remove,
}

/// Blank-line grouping policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BlankLines {
    /// Leave every blank item untouched.
    Preserve,
    /// Collapse every run of consecutive blank items to one blank line.
    Collapse,
}

/// Preferred value quoting while formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum QuoteStyle {
    /// Leave the existing quote style untouched.
    Preserve,
    /// Prefer a double-quoted value where the selected dialect can represent it.
    Double,
    /// Prefer a single-quoted value where the selected dialect can represent it.
    Single,
    /// Prefer an unquoted value where it remains unambiguous.
    Unquoted,
}

/// Formatting policy for trivia-only changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormatOptions {
    /// Spacing around assignment separators.
    pub equals_spacing: EqualsSpacing,
    /// Remove trailing spaces and tabs from blank, comment and final entry lines.
    ///
    /// Whitespace inside multiline quoted values and invalid/ignored statements
    /// is never changed.
    pub trim_trailing_whitespace: bool,
    /// Blank-line grouping policy.
    pub blank_lines: BlankLines,
    /// Preferred value quoting.
    pub quote_style: QuoteStyle,
    /// Final-newline policy.
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
    /// Normalize spacing, trim trailing whitespace and ensure a final LF.
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

    /// Normalize spacing and trailing whitespace, collapse blank-line runs and
    /// ensure a final LF.
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

/// A verified trivia-only formatting result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatResult {
    edits: Vec<TextEdit>,
    output: String,
}

impl FormatResult {
    pub(crate) fn from_parts(edits: Vec<TextEdit>, output: String) -> Self {
        Self { edits, output }
    }

    /// Text edits referring to the original source snapshot.
    #[must_use]
    pub fn edits(&self) -> &[TextEdit] {
        &self.edits
    }
    /// Consume the result, retaining its edits.
    #[must_use]
    pub fn into_edits(self) -> Vec<TextEdit> {
        self.edits
    }
    /// The verified formatted text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.output
    }
    /// Consume the result, retaining its text.
    #[must_use]
    pub fn into_string(self) -> String {
        self.output
    }
    /// Whether formatting changed no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

/// A formatting request that could not be verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormatError {
    /// The edits would have changed declaration meaning or structure.
    WouldChangeMeaning,
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("formatting would change document meaning")
    }
}

impl core::error::Error for FormatError {}

impl Document<'_> {
    /// Apply trivia-only formatting and verify that every declaration survives.
    ///
    /// Invalid and ignored statements are not rewritten. Multiline value bodies
    /// are not modified; only final-line trivia, whole-line comments and blank
    /// lines are trimmed. The complete output is reparsed with this document's
    /// exact dialect before it is returned.
    ///
    /// # Errors
    /// Returns [`FormatError::WouldChangeMeaning`] if the complete output does
    /// not preserve declaration keys, values, states, quotes, exports, lookup
    /// participation, inline comments, item kinds or malformed statement
    /// content.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict, EqualsSpacing, FormatOptions};
    ///
    /// let doc = parse_strict("A=1\nB =  2\n").unwrap();
    /// let formatted = doc
    ///     .format(&FormatOptions {
    ///         equals_spacing: EqualsSpacing::Spaces,
    ///         ..FormatOptions::default()
    ///     })
    ///     .unwrap();
    /// assert_eq!(formatted.as_str(), "A = 1\nB = 2\n");
    /// ```
    pub fn format(&self, options: &FormatOptions) -> Result<FormatResult, FormatError> {
        let mut edits = Vec::new();
        self.plan_equals_spacing(*options, &mut edits);
        self.plan_quotes(*options, &mut edits);
        let removed = self.plan_blank_lines(*options, &mut edits);
        if options.trim_trailing_whitespace {
            self.plan_trailing_whitespace(&removed, &mut edits);
        }
        self.plan_final_newline(*options, &mut edits);
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

    fn plan_trailing_whitespace(&self, removed: &[bool], edits: &mut Vec<TextEdit>) {
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
                    if trimmed != content.end {
                        push(self.source(), edits, trimmed..content.end, String::new());
                    }
                }
                ItemKind::Invalid | ItemKind::Ignored => {}
            }
        }
    }

    fn plan_final_newline(&self, options: FormatOptions, edits: &mut Vec<TextEdit>) {
        let source = self.source();
        match options.final_newline {
            FinalNewline::Preserve => {}
            FinalNewline::Ensure => {
                if !source.is_empty()
                    && self.items().next().is_some()
                    && !source.ends_with('\n')
                    && !source.ends_with('\r')
                {
                    push(source, edits, source.len()..source.len(), "\n".into());
                }
            }
            FinalNewline::Remove => {
                // Removing the terminator of a blank final item would remove a
                // blank line (or merge mixed CR/LF boundaries), not just a final
                // newline. Keep that structure.
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
