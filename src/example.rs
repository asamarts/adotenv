//! Safe example-text generation.
//!
//! An example keeps comments, blank lines, keys and formatting while removing
//! assigned values. Malformed or ignored statements are converted to comments so
//! a generated example cannot leak a value from a line the parser could not
//! interpret.

use alloc::string::String;

use crate::{Document, InlineComments, ItemKind};

impl Document<'_> {
    /// Generate a new example text with every assigned value removed.
    ///
    /// Whole-line comments, blank lines, keys, export prefixes, spacing, line
    /// endings and the BOM are retained. Bare declarations remain bare.
    /// Assigned values become `Empty`; an empty quoted placeholder keeps an
    /// inline comment attached when the grammar requires a value body before it.
    /// Invalid or ignored statements, and entries with an empty key, become
    /// indentation-preserving `#` comments so their raw text cannot leak a
    /// value. No expansion, environment access or I/O is performed.
    ///
    /// # Panics
    /// Panics only if an internal document-snapshot invariant is broken; an
    /// `Entry` item always has an entry view.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::parse;
    ///
    /// let source = "# app\nA=1\nB=\"x\" # note\nBARE\nBAD KEY=secret\n";
    /// let (doc, _) = parse(source);
    /// let example = doc.example();
    /// assert_eq!(example, "# app\nA=\nB= # note\nBARE\n#\n");
    /// ```
    #[must_use]
    pub fn example(&self) -> String {
        let source = self.source();
        let mut output = String::with_capacity(source.len());
        let mut cursor = 0;
        let mut items = self.items().peekable();
        while let Some(item) = items.next() {
            let span = item.span();
            let mut moved_comment = false;
            output.push_str(&source[cursor..span.start]);
            match item.kind() {
                ItemKind::Entry => {
                    let entry = item.entry().expect("entry item has an entry view");
                    if entry.key().is_empty() {
                        push_commented(&mut output, source, span.clone(), item.content_span());
                    } else if let Some(value) = entry.value_span() {
                        // Keep only the declaration prefix, an optional safe
                        // placeholder, an inline comment and the final line
                        // ending. This drops multiline value bodies and any
                        // trailing text without exposing it as a new value.
                        output.push_str(&source[span.start..value.start]);
                        let tokens = entry.tokens();
                        let colon_separator = tokens
                            .eq
                            .as_ref()
                            .is_some_and(|eq| &source[eq.clone()] == ":");
                        let multiline_value = source[value.clone()]
                            .bytes()
                            .any(|byte| matches!(byte, b'\n' | b'\r'));
                        let same_line_next =
                            item.line_ending() == crate::LineEnding::None && items.peek().is_some();
                        let quoted_comment =
                            entry.inline_comment().is_some() && self.dialect().quoted_comments;
                        if !self.dialect().raw_values
                            && (colon_separator
                                || multiline_value
                                || same_line_next
                                || quoted_comment)
                        {
                            output.push_str("\"\"");
                        }
                        let ending = item.line_ending();
                        if let Some(comment) = &tokens.inline_comment {
                            if self.dialect().inline_comments == InlineComments::Space
                                && !self.dialect().quoted_comments
                            {
                                // This custom grammar cannot attach a comment to
                                // an empty value; retain it on the next line.
                                moved_comment = true;
                                output.push_str(if ending == crate::LineEnding::None {
                                    "\n"
                                } else {
                                    ending.as_str()
                                });
                                output.push_str(&source[comment.clone()]);
                                output.push_str(if ending == crate::LineEnding::None {
                                    "\n"
                                } else {
                                    ending.as_str()
                                });
                            } else {
                                output.push_str(&source[tokens.trailing_space.clone()]);
                                output.push_str(&source[comment.clone()]);
                                output.push_str(ending.as_str());
                            }
                        } else {
                            output.push_str(ending.as_str());
                        }
                    } else {
                        output.push_str(item.raw());
                    }
                }
                ItemKind::Invalid | ItemKind::Ignored => {
                    push_commented(&mut output, source, span.clone(), item.content_span());
                }
                ItemKind::Blank | ItemKind::Comment => output.push_str(item.raw()),
            }
            if !moved_comment
                && item.line_ending() == crate::LineEnding::None
                && items.peek().is_some()
            {
                // A parser can split multiple statements without a line ending
                // between them. Keep them on separate lines in the example.
                output.push('\n');
            }
            cursor = span.end;
        }
        output.push_str(&source[cursor..]);
        output
    }
}

fn push_commented(
    output: &mut String,
    source: &str,
    span: core::ops::Range<usize>,
    content: core::ops::Range<usize>,
) {
    let indent = source[content.clone()]
        .bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    output.push_str(&source[span.start..content.start + indent]);
    output.push('#');
    output.push_str(&source[content.end..span.end]);
}
