//! Cross-dialect literal portability reporting.
//!
//! A report compares the document's exact configuration with caller-selected
//! dialects. Each unique configuration parses the retained text independently;
//! no expansion, lookup, I/O or environment access occurs.

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};
use core::ops::Range;

use crate::{AssignmentState, BareKeys, Dialect, Document, Duplicates, Entry, ItemKind};

/// A declaration as read by one dialect on a physical line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortabilityDeclaration {
    /// Literal key name, excluding any key quotes.
    pub key: String,
    /// Decoded literal value; `None` is a bare declaration.
    pub value: Option<String>,
    /// Bare, empty or non-empty assignment state.
    pub state: AssignmentState,
    /// Whether the declaration has an `export` prefix.
    pub exported: bool,
    /// Whether the declaration participates in effective key lookup.
    pub participates: bool,
    /// Value this declaration contributes to effective lookup.
    ///
    /// This includes `BareKeys::Empty` for bare declarations. It is `None` when
    /// the declaration does not participate, or when a participating bare
    /// declaration has no effective value. A declaration can carry a value
    /// here without winning its key; see [`Self::wins_lookup`].
    pub effective_value: Option<String>,
    /// Whether this declaration wins its key's effective lookup.
    pub wins_lookup: bool,
}

/// One part of a physical line under one dialect.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PortabilityPart {
    /// A recognized declaration.
    Declaration(PortabilityDeclaration),
    /// A blank line.
    Blank,
    /// A whole-line comment.
    Comment,
    /// Malformed syntax retained as invalid.
    Invalid,
    /// Syntax ignored by the selected dialect.
    Ignored,
    /// A continuation line of a multiline item that began earlier.
    Continuation,
}

/// One dialect's reading of a differing physical line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortabilityReading {
    /// The exact configuration that produced this reading.
    pub dialect: Dialect,
    /// Ordered line parts. Multiple statements can share one physical line.
    pub parts: Vec<PortabilityPart>,
}

/// One physical line whose meaning differs across the compared dialects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortabilityDifference {
    /// Exact original-source bytes of the line content, excluding its ending.
    pub span: Range<usize>,
    /// Explicitly one-based physical line number.
    pub first_line: usize,
    /// Explicitly one-based final line number; equal to `first_line` today.
    pub last_line: usize,
    /// One reading per unique compared dialect, in comparison order.
    pub readings: Vec<PortabilityReading>,
}

/// Differences between a document's interpretation and selected dialects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortabilityReport {
    differences: Vec<PortabilityDifference>,
}

impl PortabilityReport {
    /// Whether every compared dialect agreed on every physical line.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.differences.is_empty()
    }
    /// Number of differing physical lines.
    #[must_use]
    pub fn len(&self) -> usize {
        self.differences.len()
    }
    /// Differing physical lines in source order.
    #[must_use]
    pub fn differences(&self) -> &[PortabilityDifference] {
        &self.differences
    }
}

impl Document<'_> {
    /// Compare this document's exact configuration and additional dialects.
    ///
    /// The document's own configuration is always the first reading; duplicate
    /// configurations are ignored. Each unique configuration parses the retained
    /// source once. A difference is reported for every physical line whose
    /// declarations, decoded values, assignment states, export prefixes, lookup
    /// participation, duplicate lookup winner or item grouping differ. Quote
    /// style alone is not a difference. No expansion, lookup, I/O or environment
    /// access occurs.
    ///
    /// Differences are per physical line and ordered by line. A multiline item
    /// contributes a declaration on its first line and [`PortabilityPart::Continuation`]
    /// parts on its later lines, so grouping changes remain visible.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_with, Dialect};
    ///
    /// let (doc, _) = parse_with("A=a#b\n", Dialect::PORTABLE);
    /// let report = doc.portability(&[Dialect::NODE_DOTENV]);
    /// assert_eq!(report.len(), 1);
    /// let difference = &report.differences()[0];
    /// assert_eq!(difference.first_line, 1);
    /// assert_eq!(difference.readings.len(), 2);
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
