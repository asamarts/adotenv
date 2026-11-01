//! Safe example-text generation.
//!
//! An example keeps comments, blank lines, keys and layout while removing
//! assigned values. Invalid or ignored statements become comments, so a
//! generated example cannot leak a value from a line the parser could not
//! read.

use alloc::string::String;

use crate::{Document, InlineComments, ItemKind};

impl Document<'_> {
    /// Returns a copy of the source with every assigned value removed.
    ///
    /// The result is meant for files such as `.env.example`. It keeps:
    ///
    /// - whole-line comments and blank lines;
    /// - keys, `export` prefixes and the spacing before each value;
    /// - inline comments;
    /// - line endings and a BOM;
    /// - bare declarations, which stay bare.
    ///
    /// Every assigned value becomes empty
    /// ([`AssignmentState::Empty`](crate::AssignmentState::Empty)), and the
    /// rest of its line is dropped unless it is an inline comment. Where an
    /// empty value would change how the line reads, the value becomes `""`
    /// instead of nothing: when the separator is `:`, when the value spans
    /// lines, when another statement follows on the same line, or when the
    /// entry has an inline comment in a dialect that lets comments follow
    /// quotes (Node, Python and Compose). Raw-value dialects such as Docker
    /// never get a placeholder. In a custom dialect where an inline comment
    /// cannot follow an empty value, the comment moves to its own line.
    ///
    /// Invalid statements, ignored statements and entries with an empty key
    /// become `#` comments that keep only their indentation and line ending, so
    /// no value text from them can leak into the output. Statements that shared
    /// one physical line are put on separate lines.
    ///
    /// Comments are kept verbatim, including anything secret written in them.
    /// The result is meant for examples and documentation, not for use as a
    /// runtime configuration. No expansion, environment access or I/O is
    /// performed.
    ///
    /// # Panics
    ///
    /// Does not panic in practice: the method expects every entry item to have
    /// a declaration, which the parser guarantees.
    ///
    /// # Examples
    ///
    /// ```
    /// let source = "# app\nA=1\nB=\"two\" # note\nBARE\nBAD KEY=secret\n";
    /// let (doc, _) = adotenv::parse(source);
    /// assert_eq!(doc.example(), "# app\nA=\nB= # note\nBARE\n#\n");
    ///
    /// // Node lets a comment follow quotes, so the empty value keeps them.
    /// let (doc, _) = adotenv::parse_with("PORT=8080 # note\n", adotenv::Dialect::NODE_DOTENV);
    /// assert_eq!(doc.example(), "PORT=\"\" # note\n");
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
