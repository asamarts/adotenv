//! Cross-dialect literal portability reporting.
//!
//! A report compares the document's exact configuration with caller-selected
//! dialects. Each unique configuration parses the retained text independently;
//! no expansion, lookup, I/O or environment access occurs.

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};
use core::ops::Range;

use crate::{AssignmentState, BareKeys, Dialect, Document, Duplicates, Entry, ItemKind};

/// A declaration as one dialect reads it, inside a [`PortabilityReading`].
///
/// Two readings of a line differ when any field differs. The quote style and
/// the spelling of the value are not recorded, so `A=x` and `A="x"` read the
/// same.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PortabilityDeclaration {
    /// The key, without the quotes of a quoted key.
    pub key: String,
    /// The decoded value, or `None` for a bare key.
    pub value: Option<String>,
    /// Whether the declaration is bare, empty or has a value.
    pub state: AssignmentState,
    /// Whether the declaration has an `export` prefix.
    pub exported: bool,
    /// Whether the declaration takes part in lookup.
    ///
    /// See [`Entry::participates_in_lookup`].
    pub participates: bool,
    /// The value lookup returns for this declaration if it wins.
    ///
    /// It is `None` when the declaration does not take part in lookup, or when
    /// it is a bare key and the dialect gives bare keys no value. Under
    /// [`BareKeys::Empty`] a bare key has `Some("")` here. A declaration can
    /// have a value here and still lose to a duplicate; see
    /// [`wins_lookup`](Self::wins_lookup).
    pub effective_value: Option<String>,
    /// Whether this declaration wins lookup for its key, under the dialect's
    /// duplicate rule.
    pub wins_lookup: bool,
}

/// One item, or the part of one, on a physical line as one dialect reads it.
///
/// A line can hold several parts when statements share it, as with
/// [`QuotedTail::NextStatement`](crate::QuotedTail::NextStatement).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PortabilityPart {
    /// A declaration, with the details that lookup depends on.
    Declaration(PortabilityDeclaration),
    /// A blank line.
    Blank,
    /// A whole-line comment.
    Comment,
    /// A malformed statement, kept as an [`ItemKind::Invalid`] item.
    Invalid,
    /// A statement the dialect skips, kept as an [`ItemKind::Ignored`] item.
    Ignored,
    /// A later line of an item that started on an earlier line, such as a
    /// multiline quoted value.
    Continuation,
}

/// How one dialect reads one differing physical line.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PortabilityReading {
    /// The dialect that produced this reading.
    pub dialect: Dialect,
    /// The parts of the line in source order.
    pub parts: Vec<PortabilityPart>,
}

/// A physical line that the compared dialects read differently.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PortabilityDifference {
    /// The byte range of the line in the source, without its line ending.
    ///
    /// On line 1 it includes a leading byte-order mark.
    pub span: Range<usize>,
    /// The one-based line number.
    pub first_line: usize,
    /// The one-based number of the last line covered, which is always
    /// [`first_line`](Self::first_line) today.
    pub last_line: usize,
    /// One reading per compared dialect, in comparison order.
    ///
    /// The first reading is the document's own dialect.
    pub readings: Vec<PortabilityReading>,
}

/// The lines of a document that several dialects read differently.
///
/// [`Document::portability`] returns it. An empty report means that every
/// compared dialect reads every line the same way.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict, Dialect, PortabilityPart};
///
/// let doc = parse_strict("PORT=8080\nTAG=v1#beta\n")?;
/// let report = doc.portability(&[Dialect::NODE_DOTENV]);
/// assert_eq!(report.len(), 1);
///
/// let difference = &report.differences()[0];
/// assert_eq!(difference.first_line, 2);
/// assert_eq!(&doc.source()[difference.span.clone()], "TAG=v1#beta");
///
/// // Portable reads the hash as text; Node dotenv starts a comment there.
/// let values: Vec<_> = difference
///     .readings
///     .iter()
///     .map(|reading| match &reading.parts[..] {
///         [PortabilityPart::Declaration(declaration)] => declaration.value.as_deref(),
///         _ => None,
///     })
///     .collect();
/// assert_eq!(values, [Some("v1#beta"), Some("v1")]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortabilityReport {
    differences: Vec<PortabilityDifference>,
}

impl PortabilityReport {
    /// Returns `true` when every compared dialect reads every line the same way.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.differences.is_empty()
    }
    /// Returns the number of lines that differ.
    #[must_use]
    pub fn len(&self) -> usize {
        self.differences.len()
    }
    /// Returns the lines that differ, in source order.
    #[must_use]
    pub fn differences(&self) -> &[PortabilityDifference] {
        &self.differences
    }
}

impl Document<'_> {
    /// Compares how this document's dialect and other dialects read each line.
    ///
    /// The document's own dialect is always the first reading, followed by
    /// `dialects` in order; a dialect equal to one already compared is
    /// skipped. Each dialect parses the document's source again.
    ///
    /// The report has one [`PortabilityDifference`] per physical line whose
    /// readings differ in the items on the line (blank, comment, invalid,
    /// ignored or declaration) or, for a declaration, in its key, decoded
    /// value, assignment state, `export` prefix, lookup participation,
    /// effective value or duplicate winner. Quote style alone is not a
    /// difference: `A=x` and `A="x"` read the same.
    ///
    /// Lines are split at LF, CRLF and a lone CR, whatever the dialects' line
    /// break rules. A multiline item gives a declaration on its first line and
    /// [`PortabilityPart::Continuation`] parts on its later lines, so a change
    /// in how lines group into items shows on every line involved.
    ///
    /// The comparison is literal: nothing is expanded, and no environment or
    /// file is read.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict, Dialect};
    ///
    /// let doc = parse_strict("A=a#b\nDEBUG\nPORT=8080\n")?;
    /// let report = doc.portability(&[Dialect::NODE_DOTENV, Dialect::DOCKER_RAW]);
    ///
    /// // Line 1: Node dotenv starts a comment at `#`.
    /// // Line 2: Node dotenv and Docker leave bare keys out of lookup.
    /// let lines: Vec<_> = report.differences().iter().map(|d| d.first_line).collect();
    /// assert_eq!(lines, [1, 2]);
    /// assert_eq!(report.differences()[0].readings.len(), 3);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn portability(&self, dialects: &[Dialect]) -> PortabilityReport {
        let mut configurations = Vec::new();
        for dialect in core::iter::once(self.dialect()).chain(dialects.iter().copied()) {
            if !configurations.contains(&dialect) {
                configurations.push(dialect);
            }
        }

        let ranges = physical_lines(self.source());
        let mut readings = Vec::with_capacity(configurations.len());
        for dialect in configurations {
            let (document, _) = crate::parse_with(self.source(), dialect);
            readings.push((dialect, line_parts(&document, &ranges)));
        }

        let mut differences = Vec::new();
        if let Some((_, baseline)) = readings.first() {
            for (line, span) in ranges.iter().enumerate() {
                let parts = &baseline[line];
                if readings.iter().any(|(_, lines)| &lines[line] != parts) {
                    differences.push(PortabilityDifference {
                        span: span.clone(),
                        first_line: line + 1,
                        last_line: line + 1,
                        readings: readings
                            .iter()
                            .map(|(dialect, lines)| PortabilityReading {
                                dialect: *dialect,
                                parts: lines[line].clone(),
                            })
                            .collect(),
                    });
                }
            }
        }
        PortabilityReport { differences }
    }
}

fn line_parts(document: &Document<'_>, ranges: &[Range<usize>]) -> Vec<Vec<PortabilityPart>> {
    let winners = lookup_winners(document);
    let mut lines = vec![Vec::new(); ranges.len()];
    for item in document.items() {
        let content = item.content_span();
        let first = line_of(ranges, content.start);
        let last = line_of(ranges, content.end);
        let part = match item.kind() {
            ItemKind::Blank => PortabilityPart::Blank,
            ItemKind::Comment => PortabilityPart::Comment,
            ItemKind::Entry => item.entry().map_or(PortabilityPart::Invalid, |entry| {
                PortabilityPart::Declaration(PortabilityDeclaration {
                    key: String::from(entry.key()),
                    value: entry.value().map(String::from),
                    state: entry.state(),
                    exported: entry.exported(),
                    participates: entry.participates_in_lookup(),
                    effective_value: effective_value(entry, document.dialect()),
                    wins_lookup: winners[entry.id()],
                })
            }),
            ItemKind::Invalid => PortabilityPart::Invalid,
            ItemKind::Ignored => PortabilityPart::Ignored,
        };
        if let Some(parts) = lines.get_mut(first) {
            parts.push(part);
        }
        for line in first + 1..=last {
            if let Some(parts) = lines.get_mut(line) {
                parts.push(PortabilityPart::Continuation);
            }
        }
    }
    lines
}

fn effective_value(entry: Entry<'_>, dialect: Dialect) -> Option<String> {
    if !entry.participates_in_lookup() {
        return None;
    }
    match entry.state() {
        AssignmentState::Bare => (dialect.bare_keys == BareKeys::Empty).then(String::new),
        AssignmentState::Empty => Some(String::new()),
        AssignmentState::Value => entry.value().map(String::from),
    }
}

fn lookup_winners(document: &Document<'_>) -> Vec<bool> {
    let mut first = BTreeMap::new();
    let mut last = BTreeMap::new();
    for entry in document.entries() {
        if entry.participates_in_lookup() {
            first.entry(entry.key()).or_insert(entry.id());
            last.insert(entry.key(), entry.id());
        }
    }
    let mut winners = vec![false; document.entries().len()];
    match document.dialect().duplicates {
        Duplicates::First => {
            for id in first.values() {
                winners[*id] = true;
            }
        }
        Duplicates::Last => {
            for id in last.values() {
                winners[*id] = true;
            }
        }
    }
    winners
}

fn physical_lines(source: &str) -> Vec<Range<usize>> {
    let bytes = source.as_bytes();
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\r' | b'\n' => {
                ranges.push(start..at);
                at += if bytes[at] == b'\r' && bytes.get(at + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                start = at;
            }
            _ => at += 1,
        }
    }
    ranges.push(start..source.len());
    ranges
}

fn line_of(ranges: &[Range<usize>], offset: usize) -> usize {
    ranges
        .partition_point(|range| range.start <= offset)
        .saturating_sub(1)
}
