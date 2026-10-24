//! Key-based comparison of two literal-document interpretations.
//!
//! Comparison is independent of source order and formatting. It uses each
//! document's effective lookup entry, so duplicate-key policy and bare-key
//! participation are honored. No expansion, environment access or I/O occurs.

use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
    vec::Vec,
};

use crate::{AssignmentState, Document};

/// The effective value side of one compared key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ComparedValue {
    /// Whether the declaration is bare, empty or assigned a non-empty value.
    pub state: AssignmentState,
    /// Decoded literal value; `None` for a bare declaration.
    pub value: Option<String>,
    /// Whether the declaration carries an `export` prefix.
    pub exported: bool,
}

/// Why a key appears in a [`Comparison`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DifferenceKind {
    /// The key has no effective declaration in the right document.
    OnlyLeft,
    /// The key has no effective declaration in the left document.
    OnlyRight,
    /// Both sides declare the key, but bare/empty/value states differ.
    State,
    /// Both sides have the same state but different decoded values.
    Value,
    /// The decoded value is the same but export participation differs.
    Export,
}

/// One changed key and its effective values on both sides.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ComparisonEntry {
    /// The declaration key.
    pub key: String,
    /// The effective left declaration, or `None` when absent.
    pub left: Option<ComparedValue>,
    /// The effective right declaration, or `None` when absent.
    pub right: Option<ComparedValue>,
    /// The primary difference category.
    pub kind: DifferenceKind,
}

/// Changed keys between two documents in stable key order.
///
/// Keys present on the left appear in left source order, followed by keys that
/// occur only on the right in right source order. Keys with identical effective
/// values are omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    differences: Vec<ComparisonEntry>,
}

impl Comparison {
    /// Changed entries in stable key order.
    #[must_use]
    pub fn differences(&self) -> &[ComparisonEntry] {
        &self.differences
    }
    /// Whether the effective declarations are identical.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.differences.is_empty()
    }
    /// Number of changed keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.differences.len()
    }
}

impl Document<'_> {
    /// Compare effective declarations by key.
    ///
    /// Each side contributes only declarations that participate in lookup.
    /// Duplicate resolution uses the document's own `Duplicates` policy. A
    /// bare declaration, an assigned empty value and an absent key are distinct
    /// states; decoded values are compared literally. Export differences are
    /// reported only when state and value are otherwise equal.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict, DifferenceKind};
    ///
    /// let left = parse_strict("A=1\nB=\nC=3\n").unwrap();
    /// let right = parse_strict("A=2\nB\nD=4\n").unwrap();
    /// let comparison = left.compare(&right);
    /// let kinds: Vec<_> = comparison
    ///     .differences()
    ///     .iter()
    ///     .map(|entry| (entry.key.as_str(), entry.kind))
    ///     .collect();
    /// assert_eq!(
    ///     kinds,
    ///     [
    ///         ("A", DifferenceKind::Value),
    ///         ("B", DifferenceKind::State),
    ///         ("C", DifferenceKind::OnlyLeft),
    ///         ("D", DifferenceKind::OnlyRight),
    ///     ]
    /// );
    /// ```
    #[must_use]
    pub fn compare<'a>(&'a self, other: &'a Document<'_>) -> Comparison {
        let mut keys = Vec::new();
        let mut seen = BTreeSet::new();
        collect_keys(self, &mut keys, &mut seen);
        collect_keys(other, &mut keys, &mut seen);

        let left_index = self.lookup_index();
        let right_index = other.lookup_index();
        let mut differences = Vec::new();
        for key in keys {
            let left = if key.is_empty() {
                self.get_entry(&key)
            } else {
                left_index.get_entry(&key)
            }
            .map(snapshot);
            let right = if key.is_empty() {
                other.get_entry(&key)
            } else {
                right_index.get_entry(&key)
            }
            .map(snapshot);
            let kind = match (&left, &right) {
                (None, Some(_)) => DifferenceKind::OnlyRight,
                (Some(_), None) => DifferenceKind::OnlyLeft,
                (Some(left), Some(right)) if left.state != right.state => DifferenceKind::State,
                (Some(left), Some(right)) if left.value != right.value => DifferenceKind::Value,
                (Some(left), Some(right)) if left.exported != right.exported => {
                    DifferenceKind::Export
                }
                (Some(_), Some(_)) | (None, None) => continue,
            };
            differences.push(ComparisonEntry {
                key,
                left,
                right,
                kind,
            });
        }
        Comparison { differences }
    }
}

fn collect_keys(document: &Document<'_>, keys: &mut Vec<String>, seen: &mut BTreeSet<String>) {
    for entry in document.entries() {
        if !entry.participates_in_lookup() {
            continue;
        }
        let key = entry.key();
        if !seen.contains(key) {
            seen.insert(key.to_string());
            keys.push(key.to_string());
        }
    }
}

fn snapshot(entry: crate::Entry<'_>) -> ComparedValue {
    ComparedValue {
        state: entry.state(),
        value: entry.value().map(ToString::to_string),
        exported: entry.exported(),
    }
}
