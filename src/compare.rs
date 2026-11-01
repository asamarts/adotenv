//! Key-based comparison of the effective declarations of two documents.
//!
//! Comparison is independent of source order and formatting. It uses the
//! declaration that each document's lookup returns, so duplicate-key policy
//! and bare-key participation are honored. No expansion, environment access
//! or I/O occurs.

use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
    vec::Vec,
};

use crate::{AssignmentState, Document};

/// The state, value and `export` prefix of one side's effective declaration.
///
/// [`ComparisonEntry::left`] and [`ComparisonEntry::right`] hold these values,
/// copied from the declaration that lookup returns in each document.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ComparedValue {
    /// Whether the declaration is bare, assigned an empty value or assigned a
    /// non-empty value.
    pub state: AssignmentState,
    /// The decoded literal value; `None` for a bare declaration.
    pub value: Option<String>,
    /// Whether the declaration has the `export` prefix.
    pub exported: bool,
}

/// The kind of difference that [`Document::compare`] found for one key.
///
/// The variants are listed in priority order: each [`ComparisonEntry`]
/// reports the first kind that applies. A key whose value and `export` prefix
/// both differ reports [`Value`](Self::Value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DifferenceKind {
    /// Only the left document has a declaration of the key that takes part in
    /// lookup.
    OnlyLeft,
    /// Only the right document has a declaration of the key that takes part in
    /// lookup.
    OnlyRight,
    /// The assignment states differ: a non-empty value, an empty value or a
    /// bare declaration.
    State,
    /// The decoded values differ.
    Value,
    /// Only the `export` prefix differs.
    Export,
}

/// One key whose effective declarations differ, with both sides.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ComparisonEntry {
    /// The compared key.
    pub key: String,
    /// The effective declaration in the left document, or `None` when it has
    /// none.
    pub left: Option<ComparedValue>,
    /// The effective declaration in the right document, or `None` when it has
    /// none.
    pub right: Option<ComparedValue>,
    /// The first kind of difference that applies; see [`DifferenceKind`].
    pub kind: DifferenceKind,
}

/// The keys whose effective declarations differ between two documents.
///
/// [`Document::compare`] returns this type. Keys that take part in lookup in
/// the left document come first, in the order of their first declaration that
/// takes part in lookup, followed by the keys that take part in lookup only in
/// the right document, in the same order. Keys that agree are left out.
///
/// # Examples
///
/// ```
/// use adotenv::{AssignmentState, DifferenceKind};
///
/// let left = adotenv::parse_strict("PORT=8080\nexport HOST=localhost\n")?;
/// let right = adotenv::parse_strict("HOST=localhost\nPORT=8080\n")?;
/// let comparison = left.compare(&right);
/// assert_eq!(comparison.len(), 1);
///
/// let entry = &comparison.differences()[0];
/// assert_eq!(entry.key, "HOST");
/// assert_eq!(entry.kind, DifferenceKind::Export);
/// let left_side = entry.left.as_ref().unwrap();
/// assert_eq!(left_side.state, AssignmentState::Value);
/// assert_eq!(left_side.value.as_deref(), Some("localhost"));
/// assert!(left_side.exported);
///
/// assert!(left.compare(&left).is_empty());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    differences: Vec<ComparisonEntry>,
}

impl Comparison {
    /// Returns one entry for each differing key, left keys first.
    #[must_use]
    pub fn differences(&self) -> &[ComparisonEntry] {
        &self.differences
    }
    /// Returns `true` when the two documents agree on every key.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.differences.is_empty()
    }
    /// Returns the number of differing keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.differences.len()
    }
}

impl Document<'_> {
    /// Compares the effective declarations of this document and `other` by
    /// key.
    ///
    /// This document is the left side and `other` the right side. For each
    /// key, the declaration that lookup returns ([`Document::get_entry`]) on
    /// each side is compared: its assignment state, decoded value and `export`
    /// prefix. Each document uses its own dialect for duplicate keys and
    /// lookup participation, so a
    /// [`Dialect::NODE_DOTENV`](crate::Dialect::NODE_DOTENV) file can be
    /// compared with a Portable one.
    ///
    /// A bare declaration, an assigned empty value and an absent key are
    /// distinct. A bare declaration that does not take part in lookup, as under
    /// [`BareKeys::Ignore`](crate::BareKeys::Ignore) in Node, Compose and
    /// Docker, counts as absent. Quoting, spacing, comments and source order are
    /// not compared. Empty keys, which some dialects accept, are compared too.
    /// See [`DifferenceKind`] for how each difference is classified and
    /// [`Comparison`] for the order of the result.
    ///
    /// Comparison is literal: it does not expand values, read files, inspect
    /// the environment or run commands.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict, DifferenceKind};
    ///
    /// let left = parse_strict("A=1\nB=\nC=3\n")?;
    /// let right = parse_strict("A=2\nB\nD=4\n")?;
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
    /// # Ok::<(), Box<dyn std::error::Error>>(())
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
