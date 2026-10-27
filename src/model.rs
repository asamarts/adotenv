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

/// Quoting used by a literal value in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quote {
    /// An unquoted value.
    None,
    /// A single-quoted value, with the selected dialect's escape rules.
    Single,
    /// A double-quoted value, with the selected dialect's escape rules.
    Double,
    /// A backtick-quoted value.
    Backtick,
}

/// A declaration's assignment state, independent of any environment lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AssignmentState {
    /// A key-only declaration, with no assigned value.
    Bare,
    /// An assigned empty string; quoting is recorded separately.
    Empty,
    /// An assigned non-empty literal value.
    Value,
}

/// A physical source line ending, retained exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LineEnding {
    /// No terminator at EOF.
    None,
    /// A line feed.
    Lf,
    /// A carriage return followed by a line feed.
    CrLf,
    /// A lone carriage return, diagnosed by Portable parsing.
    Cr,
}
impl LineEnding {
    /// The original ending bytes as text.
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

/// The kind of a lossless source item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ItemKind {
    /// A blank physical line.
    Blank,
    /// A comment physical line.
    Comment,
    /// An assignment or bare declaration, possibly spanning physical lines.
    Entry,
    /// A malformed statement retained verbatim.
    Invalid,
    /// Syntax ignored by the selected dialect, retained with its diagnostic.
    Ignored,
}

/// Raw token ranges into a document's source; absent syntax is represented by `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EntryTokens {
    /// Leading whitespace, as recognized by the selected dialect.
    pub indent: Range<usize>,
    /// The optional `export` keyword, excluding its separator.
    pub export: Option<Range<usize>>,
    /// Whitespace after the export keyword.
    pub space_after_export: Range<usize>,
    /// The complete raw key token, including any key quotes.
    pub key: Range<usize>,
    /// Whitespace between the key and assignment separator (or bare tail).
    pub space_before_eq: Range<usize>,
    /// The assignment separator (`=` or an enabled `:`); absent for bare keys.
    pub eq: Option<Range<usize>>,
    /// Whitespace between the separator and the value.
    pub space_after_eq: Range<usize>,
    /// The raw value token, including any quotes; absent for bare declarations.
    pub value: Option<Range<usize>>,
    /// The raw value body, excluding quotes.
    pub body: Option<Range<usize>>,
    /// Text after a closing quote, retained when ignored or appended by policy.
    pub quoted_tail: Option<Range<usize>>,
    /// Whitespace after the value and before any comment.
    pub trailing_space: Range<usize>,
    /// An inline comment, including its hash.
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

/// A lossless document interpreted under one explicit literal syntax configuration.
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
    /// The complete, unmodified input, including the BOM and malformed syntax.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
    /// The exact syntax and literal lookup policy used for this interpretation.
    #[must_use]
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }
    /// Interpret the retained text again, borrowing it without copying source bytes.
    #[must_use]
    pub fn reparse(&self, dialect: Dialect) -> (Document<'_>, Vec<Diagnostic>) {
        crate::parse_with(self.source(), dialect)
    }
    /// All source items in order; a leading BOM is the separate prelude.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn items(&self) -> impl DoubleEndedIterator<Item = Item<'_>> + ExactSizeIterator {
        (0..self.items.len()).map(|index| Item {
            document: self,
            index,
        })
    }
    /// One source item by its zero-based snapshot-local identity.
    #[must_use]
    pub fn item(&self, index: usize) -> Option<Item<'_>> {
        (index < self.items.len()).then_some(Item {
            document: self,
            index,
        })
    }
    /// Valid declarations in source order, including duplicates and bare keys.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn entries(&self) -> impl DoubleEndedIterator<Item = Entry<'_>> + ExactSizeIterator {
        (0..self.entries.len()).map(|index| Entry {
            document: self,
            index,
        })
    }
    /// One declaration by its zero-based snapshot-local identity.
    #[must_use]
    pub fn entry(&self, index: usize) -> Option<Entry<'_>> {
        (index < self.entries.len()).then_some(Entry {
            document: self,
            index,
        })
    }
    /// Every occurrence of a key in source order.
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
    /// The winning participating declaration according to the dialect's policy.
    ///
    /// Empty keys remain representable for dialects such as Compose; they are
    /// intentionally excluded from [`LookupIndex`] and effective-entry lookup.
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
    /// The effective literal lookup value, without environment inheritance.
    ///
    /// `BareKeys::Empty` returns `Some("")` for a winning bare declaration,
    /// while that declaration's [`Entry::value`] and syntax state remain bare.
    /// A dialect-specific empty key remains directly addressable here but is
    /// excluded from [`LookupIndex`] and effective-entry iteration.
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
    /// Effective declarations in source order, with their effective values.
    ///
    /// Duplicate selection follows the dialect. Nonparticipating and empty-key
    /// declarations are omitted. A bare declaration contributes an empty value
    /// only under `BareKeys::Empty`; otherwise that key is omitted, matching
    /// [`Document::get`] for nonempty keys. The returned entries borrow this
    /// document.
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

    /// Build an optional key lookup index using the dialect's duplicate policy.
    ///
    /// Index construction is explicit and stores one winner per participating,
    /// nonempty key in a compact open-addressed table. If probing exceeds a
    /// bounded budget, construction falls back to an ordered map so adversarial
    /// collision sets cannot force unbounded lookup work. The index borrows this
    /// document and returns views with the same snapshot lifetime. It does not
    /// change lookup semantics: bare declarations remain bare, and
    /// `BareKeys::Empty` remains an effective empty value.
    ///
    /// ```
    /// use adotenv::parse_strict;
    ///
    /// let document = parse_strict("PORT=8080\nHOST=localhost\n").unwrap();
    /// let index = document.lookup_index();
    /// assert_eq!(index.get("PORT"), Some("8080"));
    /// assert_eq!(index.len(), 2);
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

    /// Select a unique declaration without guessing between duplicate occurrences.
    ///
    /// # Errors
    /// Returns [`EditError::Missing`] or [`EditError::Duplicate`].
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
    /// Whether one leading UTF-8 BOM is retained as a separate source prelude.
    #[must_use]
    pub const fn has_bom(&self) -> bool {
        self.bom
    }
    /// The BOM byte range, when present.
    #[must_use]
    pub fn bom_span(&self) -> Option<Range<usize>> {
        self.bom.then_some(0..3)
    }
    /// Whether the source ends in LF, CRLF or lone CR.
    #[must_use]
    pub fn has_final_newline(&self) -> bool {
        self.source.ends_with(['\n', '\r'])
    }
    /// Build an explicit source index without imposing an index allocation on parsing.
    #[must_use]
    pub fn line_index(&self) -> LineIndex<'_> {
        LineIndex::new(self.source())
    }
    /// Retain all metadata and consume owned source, copying borrowed source only here.
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

/// A borrowed view of a declaration; identities belong to this document snapshot.
#[derive(Debug, Clone, Copy)]
pub struct Entry<'doc> {
    pub(crate) document: &'doc Document<'doc>,
    pub(crate) index: usize,
}
/// A borrowed lookup index over one document snapshot.
///
/// The index honors the document's duplicate policy and excludes
/// nonparticipating or empty keys. It is built explicitly with
/// [`Document::lookup_index`] and uses a compact open-addressed table, with a
/// bounded-probe fallback to a tree for pathological key sets.
#[derive(Debug, Clone)]
pub struct LookupIndex<'doc> {
    document: &'doc Document<'doc>,
    winners: Winners<'doc>,
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
        let entries = document.entries().len();
        let capacity = entries
            .saturating_mul(2)
            .max(8)
            .checked_next_power_of_two()?;
        let mask = capacity - 1;
        let mut slots = alloc::vec![None; capacity];
        let mut len = 0;
        let mut probes = 0;
        for entry in document.entries() {
            if !entry.participates_in_lookup() || entry.key().is_empty() {
                continue;
            }
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
    /// The winning declaration for a key, if it participates in lookup.
    #[must_use]
    pub fn get_entry(&self, key: &str) -> Option<Entry<'doc>> {
        self.winners
            .get(key)
            .and_then(|index| self.document.entry(index))
    }
    /// The effective literal value for a key, applying bare-key policy.
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
    /// Number of participating, nonempty keys in the index.
    #[must_use]
    pub fn len(&self) -> usize {
        self.winners.len()
    }
    /// Whether the index contains no keys.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.winners.is_empty()
    }
}

/// One effective declaration and its effective literal value.
///
/// This is the value-aware companion to [`Entry`]. It exists only for a
/// participating declaration that wins duplicate selection and has an
/// effective value. A bare declaration under `BareKeys::Empty` is represented
/// with `""` while its underlying entry remains bare.
#[derive(Debug, Clone, Copy)]
pub struct EffectiveEntry<'doc> {
    entry: Entry<'doc>,
    value: &'doc str,
}

impl<'doc> EffectiveEntry<'doc> {
    /// The winning declaration.
    #[must_use]
    pub const fn entry(self) -> Entry<'doc> {
        self.entry
    }
    /// The literal key.
    #[must_use]
    pub fn key(self) -> &'doc str {
        self.entry.key()
    }
    /// The effective literal value.
    #[must_use]
    pub const fn value(self) -> &'doc str {
        self.value
    }
    /// One-based source line of the winning declaration.
    #[must_use]
    pub fn line(self) -> usize {
        self.entry.line()
    }
}

impl<'doc> Entry<'doc> {
    fn data(self) -> &'doc EntryData {
        &self.document.entries[self.index]
    }
    /// Zero-based declaration identity for explicit native edits on this snapshot.
    #[must_use]
    pub const fn id(self) -> usize {
        self.index
    }
    /// The source item containing this declaration.
    #[must_use]
    pub fn item_id(self) -> usize {
        self.data().item
    }
    /// The literal key name, excluding any key quotes, borrowed without copying.
    #[must_use]
    pub fn key(self) -> &'doc str {
        &self.document.source[self.key_span()]
    }
    /// Literal decoded value; `None` is a bare declaration, not an empty string.
    #[must_use]
    pub fn value(self) -> Option<&'doc str> {
        let data = self.data();
        data.tokens.body.as_ref().map(|body| {
            data.decoded
                .as_deref()
                .unwrap_or(&self.document.source[body.clone()])
        })
    }
    /// Exact original value token, including quotes and any appended quoted tail.
    #[must_use]
    pub fn raw_value(self) -> Option<&'doc str> {
        self.value_span().map(|span| &self.document.source[span])
    }
    /// Whether this source declaration participates in literal key lookup.
    ///
    /// Ignored bare declarations and Node's `__proto__` object-assignment quirk
    /// remain inspectable and editable even when they do not participate.
    #[must_use]
    pub fn participates_in_lookup(self) -> bool {
        self.data().participates
    }
    /// How the assigned value is quoted.
    #[must_use]
    pub fn quote(self) -> Quote {
        self.data().quote
    }
    /// Whether this declaration has the export prefix.
    #[must_use]
    pub fn exported(self) -> bool {
        self.data().tokens.export.is_some()
    }
    /// Bare/empty/non-empty state, without implicit environment interpretation.
    #[must_use]
    pub fn state(self) -> AssignmentState {
        self.data().state
    }
    /// Explicitly one-based physical line of the key.
    #[must_use]
    pub fn line(self) -> usize {
        self.data().line
    }
    /// Whole statement, including its final line ending, excluding the BOM prelude.
    #[must_use]
    pub fn line_span(self) -> Range<usize> {
        self.document.items[self.item_id()].span.clone()
    }
    /// Original key-name byte range, excluding any key quotes.
    #[must_use]
    pub fn key_span(self) -> Range<usize> {
        let range = self.data().tokens.key.clone();
        if self.data().key_quoted {
            range.start + 1..range.end - 1
        } else {
            range
        }
    }
    /// Original value token range, including a tail only when it is appended.
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
    /// All raw token metadata.
    #[must_use]
    pub fn tokens(self) -> &'doc EntryTokens {
        &self.data().tokens
    }
    /// An inline comment including its marker.
    #[must_use]
    pub fn inline_comment(self) -> Option<&'doc str> {
        self.data()
            .tokens
            .inline_comment
            .as_ref()
            .map(|span| &self.document.source[span.clone()])
    }
    /// Map a decoded-value byte boundary back into original source bytes.
    ///
    /// Lookup is logarithmic in the number of decoded escape segments.
    ///
    /// # Errors
    /// Rejects absent values, out-of-range offsets and interior UTF-8 boundaries.
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
    /// Map a decoded-value byte range to its exact original source region.
    ///
    /// # Errors
    /// Rejects reversed ranges and invalid decoded boundaries.
    pub fn decoded_span_to_raw(self, span: Range<usize>) -> Result<Range<usize>, PositionError> {
        if span.start > span.end {
            return Err(PositionError::OutOfBounds);
        }
        Ok(self.decoded_offset_to_raw(span.start)?..self.decoded_offset_to_raw(span.end)?)
    }
}

/// A borrowed view of a raw source item.
#[derive(Debug, Clone, Copy)]
pub struct Item<'doc> {
    pub(crate) document: &'doc Document<'doc>,
    pub(crate) index: usize,
}
impl<'doc> Item<'doc> {
    fn data(self) -> &'doc ItemData {
        &self.document.items[self.index]
    }
    /// Zero-based identity, valid only for this document snapshot.
    #[must_use]
    pub const fn id(self) -> usize {
        self.index
    }
    /// The item's syntax classification.
    #[must_use]
    pub fn kind(self) -> ItemKind {
        self.data().kind
    }
    /// Complete raw byte range, including its final line ending.
    #[must_use]
    pub fn span(self) -> Range<usize> {
        self.data().span.clone()
    }
    /// Original source text, without normalization.
    #[must_use]
    pub fn raw(self) -> &'doc str {
        &self.document.source[self.span()]
    }
    /// The final physical line ending of the item.
    #[must_use]
    pub fn line_ending(self) -> LineEnding {
        self.data().ending
    }
    /// Item bytes excluding the final line ending.
    #[must_use]
    pub fn content_span(self) -> Range<usize> {
        let span = self.span();
        span.start..span.end - self.line_ending().as_str().len()
    }
    /// Declaration view when this is an entry item.
    #[must_use]
    pub fn entry(self) -> Option<Entry<'doc>> {
        let index = self.data().entry;
        (index != NO_ENTRY).then_some(Entry {
            document: self.document,
            index,
        })
    }
    /// Comment body after `#`, preserving its spaces.
    #[must_use]
    pub fn comment_text(self) -> Option<&'doc str> {
        self.data()
            .comment
            .as_ref()
            .map(|span| &self.document.source[span.clone()])
    }
    /// The primary syntax or policy diagnostic retained on this item, if any.
    #[must_use]
    pub fn diagnostic(self) -> Option<&'doc Diagnostic> {
        self.data().invalid.as_deref()
    }
    /// Stable primary diagnostic identity, convenient for source-preserving checks.
    #[must_use]
    pub fn invalid_code(self) -> Option<DiagnosticCode> {
        self.diagnostic().map(Diagnostic::code)
    }
}
