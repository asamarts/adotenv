//! Compact storage for effective `.env` values.
//!
//! [`CompactValues`] keeps the same keys and values as
//! [`parse_values`](crate::parse_values) but stores them in one contiguous
//! string and one fixed-size entry per key, instead of one map node and two
//! string allocations per key.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::{AssignmentState, Duplicates};

const NO_VALUE: usize = usize::MAX;

/// A compact, sorted map of effective `.env` values.
///
/// Returned by [`parse_values_compact`](crate::parse_values_compact) and its
/// `_with` variants. It contains the same keys and values as
/// [`parse_values`](crate::parse_values), with the same duplicate-key and
/// bare-key rules, but stores the text in one buffer plus one fixed-size
/// entry per key. Lookup is `O(log n)` and iteration is ordered by key.
///
/// Values stay literal: references are not expanded. Empty keys are omitted,
/// as they are from [`parse_values`](crate::parse_values).
///
/// # Examples
///
/// ```
/// let values = adotenv::parse_values_compact("PORT=8080\nPORT=9090\nDEBUG\n")?;
/// assert_eq!(values.len(), 1);
/// assert_eq!(values.get("PORT"), Some("9090"));
/// assert_eq!(values.iter().collect::<Vec<_>>(), [("PORT", "9090")]);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone)]
pub struct CompactValues {
    data: String,
    entries: Box<[CompactEntry]>,
}

impl CompactValues {
    /// Returns the number of effective values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether there are no effective values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the value of `key`, or `None` if it has no effective value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .binary_search_by(|entry| entry.key(&self.data).cmp(key))
            .ok()
            .and_then(|index| self.entries[index].value(&self.data))
    }

    /// Returns whether `key` has an effective value.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Returns an iterator over key/value pairs, ordered by key.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.entries.iter().filter_map(|entry| {
            entry
                .value(&self.data)
                .map(|value| (entry.key(&self.data), value))
        })
    }

    /// Returns an iterator over keys, ordered lexicographically.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn keys(&self) -> impl Iterator<Item = &str> + '_ {
        self.entries.iter().map(|entry| entry.key(&self.data))
    }

    /// Returns an iterator over values in key order.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn values(&self) -> impl Iterator<Item = &str> + '_ {
        self.entries
            .iter()
            .filter_map(|entry| entry.value(&self.data))
    }

    /// Converts to a map sorted by key, allocating one map node and two strings
    /// per value.
    #[must_use]
    pub fn into_btreemap(self) -> BTreeMap<String, String> {
        self.iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect()
    }
}

impl CompactValues {
    /// Merges two compact maps, with `later` winning on equal keys.
    #[cfg(feature = "load")]
    pub(crate) fn merge(self, later: &Self) -> Self {
        let mut builder =
            CompactBuilder::with_capacity(self.data.len().saturating_add(later.data.len()));
        let mut left = 0;
        let mut right = 0;
        while left < self.entries.len() || right < later.entries.len() {
            let take_later = match (left < self.entries.len(), right < later.entries.len()) {
                (false, true) => true,
                (true, false) => false,
                (true, true) => {
                    let left_key = self.entries[left].key(&self.data);
                    let right_key = later.entries[right].key(&later.data);
                    match left_key.cmp(right_key) {
                        core::cmp::Ordering::Less => false,
                        core::cmp::Ordering::Equal => {
                            left += 1;
                            true
                        }
                        core::cmp::Ordering::Greater => true,
                    }
                }
                (false, false) => break,
            };
            if take_later {
                let entry = later.entries[right];
                builder.push(entry.key(&later.data), entry.value(&later.data));
                right += 1;
            } else {
                let entry = self.entries[left];
                builder.push(entry.key(&self.data), entry.value(&self.data));
                left += 1;
            }
        }
        builder.finish(Duplicates::Last, true)
    }
}

impl Default for CompactValues {
    fn default() -> Self {
        Self {
            data: String::new(),
            entries: Vec::new().into_boxed_slice(),
        }
    }
}

impl PartialEq for CompactValues {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl Eq for CompactValues {}

impl fmt::Debug for CompactValues {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_map().entries(self.iter()).finish()
    }
}

/// A compact, sorted map of winning `.env` entries that preserves bare
/// declarations.
///
/// Returned by [`parse_entries_compact`](crate::parse_entries_compact) and its
/// `_with` variants. Unlike [`CompactValues`], which drops winning bare
/// declarations, `CompactEntries` keeps them as `None` when the dialect
/// retains bare declarations in lookup (`BareKeys::Preserve`), allowing
/// callers to distinguish bare declarations (`KEY` -> `Some(None)`), empty
/// assignments (`KEY=` -> `Some(Some(""))`), non-empty assignments
/// (`KEY=1` -> `Some(Some("1"))`), and absent keys (`None`) in one contiguous
/// buffer without allocating a lossless [`Document`](crate::Document).
///
/// Lookup is `O(log n)` and iteration is ordered by key. Values stay literal:
/// references are not expanded. Empty keys are omitted.
///
/// # Examples
///
/// ```
/// use adotenv::AssignmentState;
///
/// let entries = adotenv::parse_entries_compact("PORT=8080\nPORT=9090\nDEBUG\nEMPTY=\n")?;
/// assert_eq!(entries.len(), 3);
/// assert_eq!(entries.get("PORT"), Some(Some("9090")));
/// assert_eq!(entries.get("EMPTY"), Some(Some("")));
/// assert_eq!(entries.get("DEBUG"), Some(None));
/// assert_eq!(entries.get("MISSING"), None);
/// assert_eq!(entries.get_state("DEBUG"), Some(AssignmentState::Bare));
/// assert_eq!(entries.get_state("EMPTY"), Some(AssignmentState::Empty));
/// assert_eq!(entries.get_value("PORT"), Some("9090"));
/// assert_eq!(entries.get_value("DEBUG"), None);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone)]
pub struct CompactEntries {
    data: String,
    entries: Box<[CompactEntry]>,
}

impl CompactEntries {
    /// Returns the number of winning entries, including bare declarations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether there are no winning entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the winning entry for `key`:
    ///
    /// - `None` when `key` has no participating declaration.
    /// - `Some(None)` when the winning declaration is bare under
    ///   [`BareKeys::Preserve`](crate::BareKeys::Preserve).
    /// - `Some(Some(value))` when the winning declaration has an effective
    ///   value (including `""` for `KEY=` or bare keys under
    ///   [`BareKeys::Empty`](crate::BareKeys::Empty)).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Option<&str>> {
        self.entries
            .binary_search_by(|entry| entry.key(&self.data).cmp(key))
            .ok()
            .map(|index| self.entries[index].value(&self.data))
    }

    /// Returns the effective value of `key`, or `None` if it is absent or bare.
    #[must_use]
    pub fn get_value(&self, key: &str) -> Option<&str> {
        self.get(key).flatten()
    }

    /// Returns the effective assignment state of `key`, or `None` if it has no
    /// winning entry.
    ///
    /// Under [`BareKeys::Empty`](crate::BareKeys::Empty), bare keys have an
    /// effective empty value (`Some(Some(""))`) and therefore report
    /// [`AssignmentState::Empty`].
    #[must_use]
    pub fn get_state(&self, key: &str) -> Option<AssignmentState> {
        self.get(key).map(|value| match value {
            None => AssignmentState::Bare,
            Some("") => AssignmentState::Empty,
            Some(_) => AssignmentState::Value,
        })
    }

    /// Returns whether `key` has a winning entry, including a bare declaration.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Returns an iterator over `(key, value)` pairs ordered by key, where
    /// winning bare declarations yield `None`.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn iter(&self) -> impl Iterator<Item = (&str, Option<&str>)> + '_ {
        self.entries
            .iter()
            .map(|entry| (entry.key(&self.data), entry.value(&self.data)))
    }

    /// Returns an iterator over keys, ordered lexicographically.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn keys(&self) -> impl Iterator<Item = &str> + '_ {
        self.entries.iter().map(|entry| entry.key(&self.data))
    }

    /// Returns an iterator over optional values in key order.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn values(&self) -> impl Iterator<Item = Option<&str>> + '_ {
        self.entries.iter().map(|entry| entry.value(&self.data))
    }

    /// Converts to a sorted map preserving `None` for bare declarations.
    #[must_use]
    pub fn into_btreemap(self) -> BTreeMap<String, Option<String>> {
        self.iter()
            .map(|(key, value)| (key.to_owned(), value.map(ToOwned::to_owned)))
            .collect()
    }
}

impl Default for CompactEntries {
    fn default() -> Self {
        Self {
            data: String::new(),
            entries: Vec::new().into_boxed_slice(),
        }
    }
}

impl PartialEq for CompactEntries {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl Eq for CompactEntries {}

impl fmt::Debug for CompactEntries {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_map().entries(self.iter()).finish()
    }
}

#[derive(Clone, Copy)]
struct CompactEntry {
    key_start: usize,
    key_len: usize,
    value_start: usize,
    value_len: usize,
}

impl CompactEntry {
    fn key<'a>(&self, data: &'a str) -> &'a str {
        &data[self.key_start..self.key_start + self.key_len]
    }

    fn value<'a>(&self, data: &'a str) -> Option<&'a str> {
        (self.value_start != NO_VALUE)
            .then(|| &data[self.value_start..self.value_start + self.value_len])
    }
}

pub(crate) struct CompactBuilder {
    data: String,
    entries: Vec<CompactEntry>,
}

impl CompactBuilder {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            data: String::with_capacity(capacity),
            entries: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, key: &str, value: Option<&str>) {
        let key_start = self.data.len();
        self.data.push_str(key);
        let value_start = match value {
            Some(value) => {
                let start = self.data.len();
                self.data.push_str(value);
                start
            }
            None => NO_VALUE,
        };
        self.entries.push(CompactEntry {
            key_start,
            key_len: key.len(),
            value_start,
            value_len: value.map_or(0, str::len),
        });
    }

    pub(crate) fn finish(self, duplicates: Duplicates, shrink: bool) -> CompactValues {
        let (data, entries) = self.finish_raw(duplicates, shrink, false);
        CompactValues { data, entries }
    }

    pub(crate) fn finish_entries(self, duplicates: Duplicates, shrink: bool) -> CompactEntries {
        let (data, entries) = self.finish_raw(duplicates, shrink, true);
        CompactEntries { data, entries }
    }

    fn finish_raw(
        self,
        duplicates: Duplicates,
        shrink: bool,
        keep_bare: bool,
    ) -> (String, Box<[CompactEntry]>) {
        let Self {
            mut data,
            mut entries,
        } = self;
        if entries.is_empty() {
            data.shrink_to_fit();
            return (data, Vec::new().into_boxed_slice());
        }

        entries.sort_by(|left, right| left.key(&data).cmp(right.key(&data)));

        let mut selected = Vec::with_capacity(entries.len());
        let mut start = 0;
        while start < entries.len() {
            let key = entries[start].key(&data);
            let mut end = start + 1;
            while end < entries.len() && entries[end].key(&data) == key {
                end += 1;
            }
            let winner = match duplicates {
                Duplicates::First => entries[start],
                Duplicates::Last => entries[end - 1],
            };
            if keep_bare || winner.value(&data).is_some() {
                selected.push(winner);
            }
            start = end;
        }

        if selected.len() == entries.len() {
            if shrink {
                data.shrink_to_fit();
            }
            return (data, selected.into_boxed_slice());
        }

        let capacity = selected
            .iter()
            .map(|entry| entry.key_len + entry.value_len)
            .sum();
        let mut compact = String::with_capacity(capacity);
        let mut rebuilt = Vec::with_capacity(selected.len());
        for mut entry in selected {
            let key = entry.key(&data);
            entry.key_start = compact.len();
            compact.push_str(key);
            entry.key_len = key.len();
            if let Some(value) = entry.value(&data) {
                entry.value_start = compact.len();
                compact.push_str(value);
                entry.value_len = value.len();
            } else {
                entry.value_start = NO_VALUE;
                entry.value_len = 0;
            }
            rebuilt.push(entry);
        }

        (compact, rebuilt.into_boxed_slice())
    }
}
