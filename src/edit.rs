use alloc::{
    borrow::Cow,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{error::Error, fmt, ops::Range};

use crate::{encode::encode_native, AssignmentState, Dialect, Document, ItemKind, Quote, Severity};

/// A native byte-range replacement against the original document snapshot.
///
/// Edit and formatting results return their edits as a slice sorted by
/// `range.start`, with ranges that do not overlap. Edits can start at the same
/// offset: an insertion (an empty range) then comes before the replacement
/// that starts there. Apply the edits in reverse order of the slice, from its
/// last element to its first, so each range still addresses unchanged text.
/// Do not re-sort them by offset: a sort by start offset alone can swap edits
/// that share a start and corrupt the result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// Original UTF-8 byte range to replace.
    pub range: Range<usize>,
    /// Replacement text.
    pub replacement: String,
}

/// An edit batch whose complete output has already been parsed and verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedEdits {
    edits: Vec<TextEdit>,
    output: String,
}
impl VerifiedEdits {
    /// Replacements sorted by start offset, without overlaps.
    ///
    /// Apply them in reverse order of this slice; see [`TextEdit`].
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict, Edit};
    ///
    /// let doc = parse_strict("A=1\n").unwrap();
    /// let verified = doc
    ///     .edit(&[
    ///         Edit::Export { entry: 0, enabled: true },
    ///         Edit::Rename { entry: 0, key: "B" },
    ///     ])
    ///     .unwrap();
    /// // Both edits start at offset 0; the insertion comes first.
    /// assert_eq!(verified.edits()[0].range, 0..0);
    /// assert_eq!(verified.edits()[1].range, 0..1);
    ///
    /// let mut text = doc.source().to_string();
    /// for edit in verified.edits().iter().rev() {
    ///     text.replace_range(edit.range.clone(), &edit.replacement);
    /// }
    /// assert_eq!(text, "export B=1\n");
    /// assert_eq!(text, verified.as_str());
    /// ```
    #[must_use]
    pub fn edits(&self) -> &[TextEdit] {
        &self.edits
    }
    /// Consume the result, retaining the native replacements.
    #[must_use]
    pub fn into_edits(self) -> Vec<TextEdit> {
        self.edits
    }
    /// Consume the result, retaining the verified complete text.
    #[must_use]
    pub fn into_string(self) -> String {
        self.output
    }
    /// The verified complete text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.output
    }
}

/// A position for a new declaration in the original snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InsertPosition<'a> {
    /// After the last source item.
    End,
    /// After a uniquely named declaration.
    AfterKey(&'a str),
    /// After an explicitly selected declaration occurrence.
    AfterEntry(usize),
    /// Before the first lexically greater key, or at the end.
    Sorted,
}

/// An operation whose identities refer to the original document snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Edit<'a> {
    /// Assign an exact literal value to one declaration.
    Set {
        /// The snapshot-local declaration identity.
        entry: usize,
        /// The exact new literal value.
        value: &'a str,
    },
    /// Remove exactly one source item, without claiming neighboring comments.
    Remove {
        /// The snapshot-local source item identity.
        item: usize,
    },
    /// Rename one declaration.
    ///
    /// The batch fails with [`EditError::Duplicate`] when the document already
    /// declares the new key and with [`EditError::WouldChangeMeaning`] when two
    /// renames choose the same new key.
    Rename {
        /// The snapshot-local declaration identity.
        entry: usize,
        /// The new literal key name.
        key: &'a str,
    },
    /// Insert an assigned or bare declaration of a new key.
    ///
    /// The key must not already be declared in the output. The batch fails
    /// with [`EditError::Duplicate`] when the document declares the key (any
    /// occurrence, including duplicates and bare declarations) or when another
    /// insertion, rename or uncommented line in the same batch declares it.
    /// Removing, commenting out or renaming every existing declaration of the
    /// key in the same batch frees it. Commented-out declarations are comments
    /// and never collide.
    Insert {
        /// The new literal key name.
        key: &'a str,
        /// An assigned literal value, or `None` for a bare declaration.
        value: Option<&'a str>,
        /// Placement in the original source snapshot.
        position: InsertPosition<'a>,
    },
    /// Prefix every physical line of one item with a comment marker.
    Comment {
        /// The snapshot-local source item identity.
        item: usize,
    },
    /// Remove one comment marker (and one optional following space).
    Uncomment {
        /// The snapshot-local comment item identity.
        item: usize,
    },
    /// Enable or disable a declaration's export prefix.
    Export {
        /// The snapshot-local declaration identity.
        entry: usize,
        /// Whether the declaration should carry `export`.
        enabled: bool,
    },
}

/// Why a native edit could not be produced safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditError {
    /// No valid declaration has the requested key.
    Missing,
    /// A key-only selector matched this many declarations, or a rename or
    /// insertion would leave its key with this many declarations.
    Duplicate(usize),
    /// A snapshot-local identity or item kind is unsuitable.
    InvalidTarget,
    /// The requested name cannot be represented by the selected key grammar.
    InvalidKey,
    /// The value cannot be represented in its selected dialect and source context.
    Unencodable,
    /// Operations overlap or assign the same property more than once.
    ConflictingEdits,
    /// The complete output would reinterpret unselected source or miss the request.
    WouldChangeMeaning,
}
impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("no declaration has the requested key"),
            Self::Duplicate(count) => {
                write!(
                    f,
                    "key has {count} declarations; select one occurrence or use another key"
                )
            }
            Self::InvalidTarget => f.write_str("invalid snapshot-local edit target"),
            Self::InvalidKey => f.write_str("key cannot be represented by the selected dialect"),
            Self::Unencodable => f.write_str("value cannot be represented in the selected dialect"),
            Self::ConflictingEdits => f.write_str("conflicting operations in edit batch"),
            Self::WouldChangeMeaning => {
                f.write_str("edit would change unrelated source interpretation")
            }
        }
    }
}
impl Error for EditError {}

fn key_token(key: &str, dialect: Dialect) -> Option<Cow<'_, str>> {
    if crate::parser::valid_key_in(key, dialect) && !(dialect.quoted_keys && key.starts_with('\''))
    {
        Some(Cow::Borrowed(key))
    } else if dialect.quoted_keys && !key.is_empty() && !key.contains('\'') {
        let mut token = String::with_capacity(key.len() + 2);
        token.push('\'');
        token.push_str(key);
        token.push('\'');
        Some(Cow::Owned(token))
    } else {
        None
    }
}

// Build an index only when a batch performs repeated name lookups. Single-key
// edits retain their allocation-free lookup path. Sorted insertion means the
// earliest source occurrence greater than the requested key, not a key successor.
struct KeyLookup<'doc> {
    document: &'doc Document<'doc>,
    entries: Vec<crate::Entry<'doc>>,
    minimum_starts_rev: Vec<usize>,
}
impl<'doc> KeyLookup<'doc> {
    fn new(document: &'doc Document<'doc>, operations: &[Edit<'_>]) -> Self {
        let lookups = operations
            .iter()
            .copied()
            .filter(|operation| match *operation {
                Edit::Rename { entry, key } => document
                    .entry(entry)
                    .is_some_and(|entry| entry.key() != key),
                Edit::Insert {
                    position: InsertPosition::Sorted | InsertPosition::AfterKey(_),
                    ..
                } => true,
                _ => false,
            })
            .count();
        let mut entries: Vec<_> = if lookups > 1 {
            document.entries().collect()
        } else {
            Vec::new()
        };
        entries.sort_unstable_by(|a, b| a.key().cmp(b.key()));
        let sorted_inserts = operations.iter().any(|operation| {
            matches!(
                *operation,
                Edit::Insert {
                    position: InsertPosition::Sorted,
                    ..
                }
            )
        });
        let mut minimum_starts_rev = Vec::new();
        if sorted_inserts && !entries.is_empty() {
            minimum_starts_rev.reserve(entries.len());
            let mut minimum = document.source().len();
            for entry in entries.iter().rev() {
                minimum = minimum.min(entry.line_span().start);
                minimum_starts_rev.push(minimum);
            }
        }
        Self {
            document,
            entries,
            minimum_starts_rev,
        }
    }

    fn bounds(&self, key: &str) -> Range<usize> {
        self.entries.partition_point(|entry| entry.key() < key)
            ..self.entries.partition_point(|entry| entry.key() <= key)
    }

    fn count(&self, key: &str) -> usize {
        if self.entries.is_empty() {
            self.document.get_all(key).count()
        } else {
            self.bounds(key).len()
        }
    }

    fn unique(&self, key: &str) -> Result<crate::Entry<'doc>, EditError> {
        if self.entries.is_empty() {
            return self.document.unique(key);
        }
        let matches = self.bounds(key);
        match matches.len() {
            0 => Err(EditError::Missing),
            1 => Ok(self.entries[matches.start]),
            count => Err(EditError::Duplicate(count)),
        }
    }

    fn first_greater(&self, key: &str) -> usize {
        if self.entries.is_empty() {
            return self
                .document
                .entries()
                .find(|entry| entry.key() > key)
                .map_or(self.document.source().len(), |entry| {
                    entry.line_span().start
                });
        }
        let start = self.entries.partition_point(|entry| entry.key() <= key);
        if start == self.entries.len() {
            self.document.source().len()
        } else {
            self.minimum_starts_rev[self.entries.len() - start - 1]
        }
    }
}

#[derive(Clone, Copy)]
struct Desired<'a> {
    key: &'a str,
    value: Option<&'a str>,
    quote: Quote,
    exported: bool,
}
impl<'a> Desired<'a> {
    fn from_entry(entry: crate::Entry<'a>) -> Self {
        Self {
            key: entry.key(),
            value: entry.value(),
            quote: entry.quote(),
            exported: entry.exported(),
        }
    }
    fn matches(self, entry: crate::Entry<'_>) -> bool {
        self.key == entry.key()
            && self.value == entry.value()
            && self.quote == entry.quote()
            && self.exported == entry.exported()
            && entry.state()
                == match self.value {
                    None => AssignmentState::Bare,
                    Some("") => AssignmentState::Empty,
                    Some(_) => AssignmentState::Value,
                }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Whole {
    Keep,
    Remove,
    Comment,
    Uncomment,
}
struct ItemPlan<'a> {
    whole: Whole,
    fields: u8,
    desired: Option<Desired<'a>>,
}
impl ItemPlan<'_> {
    fn retains(&self, item: crate::Item<'_>) -> bool {
        match self.whole {
            Whole::Remove => false,
            Whole::Uncomment => {
                item.raw().len()
                    > 1 + usize::from(
                        item.comment_text()
                            .is_some_and(|body| body.starts_with(' ')),
                    )
            }
            _ => true,
        }
    }
}
struct SetTarget<'a> {
    entry: usize,
    value: &'a str,
    range: Range<usize>,
    context: Range<usize>,
}

#[derive(Clone, Copy)]
struct ContextEdits<'a, 'value> {
    base: &'a [TextEdit],
    targets: &'a [SetTarget<'value>],
    sets: &'a [Option<TextEdit>],
}

fn edit_key(edit: &TextEdit) -> (usize, usize) {
    (edit.range.start, edit.range.end)
}

struct Insertion<'a> {
    offset: usize,
    desired: Desired<'a>,
    key_token: Cow<'a, str>,
    leading: String,
    sorted: bool,
}
impl<'a> Insertion<'a> {
    fn append_to(
        &mut self,
        text: &mut String,
        dialect: Dialect,
        ending: &str,
    ) -> Result<(Range<usize>, Desired<'a>), EditError> {
        let start = text.len();
        text.push_str(&self.key_token);
        if let Some(value) = self.desired.value {
            let (token, quote) = encode_native(
                value,
                Quote::None,
                dialect,
                &self.key_token,
                ending,
                true,
                false,
            )
            .map_err(|_| EditError::Unencodable)?;
            text.push('=');
            text.push_str(&token);
            self.desired.quote = quote;
        }
        text.push_str(ending);
        Ok((start..text.len(), self.desired))
    }
}
enum Expect<'a> {
    Original(usize),
    Entry(Desired<'a>),
    Comments,
    Uncommented,
}
struct Region<'a> {
    range: Range<usize>,
    expect: Expect<'a>,
}
fn verify_entry_region<'a>(
    first: crate::Item<'a>,
    desired: Desired<'_>,
    range: &Range<usize>,
    actual: &mut impl Iterator<Item = crate::Item<'a>>,
) -> Result<(), EditError> {
    if first.span().end > range.end
        || first
            .diagnostic()
            .is_some_and(|d| d.severity() == Severity::Error)
        || !first.entry().is_some_and(|entry| desired.matches(entry))
    {
        return Err(EditError::WouldChangeMeaning);
    }
    // A new quote form can expose the selected entry's own whitespace/comment
    // suffix as separate items (Compose). Never consume another declaration.
    let mut end = first.span().end;
    while end < range.end {
        let next = actual.next().ok_or(EditError::WouldChangeMeaning)?;
        if next.span().start != end
            || next.span().end > range.end
            || !matches!(next.kind(), ItemKind::Blank | ItemKind::Comment)
            || next
                .diagnostic()
                .is_some_and(|d| d.severity() == Severity::Error)
        {
            return Err(EditError::WouldChangeMeaning);
        }
        end = next.span().end;
    }
    Ok(())
}
// An inserted key must name exactly one declaration of the verified output.
// Counting the output covers existing declarations that the batch keeps, other
// insertions, renames and uncommented lines, while declarations that the batch
// removes, comments out or renames away no longer collide.
fn verify_inserted_keys(parsed: &Document<'_>, mut keys: Vec<&str>) -> Result<(), EditError> {
    if keys.is_empty() {
        return Ok(());
    }
    keys.sort_unstable();
    keys.dedup();
    let mut counts = vec![0usize; keys.len()];
    for entry in parsed.entries() {
        if let Ok(index) = keys.binary_search(&entry.key()) {
            counts[index] += 1;
        }
    }
    match counts.into_iter().find(|count| *count > 1) {
        Some(count) => Err(EditError::Duplicate(count)),
        None => Ok(()),
    }
}
struct InsertGroup<'a> {
    offset: usize,
    len: usize,
    entries: Vec<(Range<usize>, Desired<'a>)>,
}
fn group_at<'a, 'b>(groups: &'a [InsertGroup<'b>], offset: usize) -> Option<&'a InsertGroup<'b>> {
    groups
        .binary_search_by_key(&offset, |group| group.offset)
        .ok()
        .map(|index| &groups[index])
}

fn claim(plan: &mut ItemPlan<'_>, field: u8) -> Result<(), EditError> {
    if plan.whole != Whole::Keep || plan.fields & field != 0 {
        return Err(EditError::ConflictingEdits);
    }
    plan.fields |= field;
    Ok(())
}
fn whole(plan: &mut ItemPlan<'_>, action: Whole) -> Result<(), EditError> {
    if plan.whole != Whole::Keep || plan.fields != 0 {
        return Err(EditError::ConflictingEdits);
    }
    plan.whole = action;
    Ok(())
}
fn comment_text(text: &str, breaks: crate::LineBreaks) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n'
            || (breaks == crate::LineBreaks::Universal
                && byte == b'\r'
                && text.as_bytes().get(index + 1) != Some(&b'\n'))
        {
            out.push_str("# ");
            out.push_str(&text[start..=index]);
            start = index + 1;
        }
    }
    if start < text.len() {
        out.push_str("# ");
        out.push_str(&text[start..]);
    }
    out
}

// Cumulative replacement lengths let every original item boundary be mapped
// without rescanning the whole batch for each item.
struct Boundaries<'a> {
    edits: &'a [TextEdit],
    removed: Vec<usize>,
    added: Vec<usize>,
}
impl<'a> Boundaries<'a> {
    fn new(edits: &'a [TextEdit]) -> Self {
        let mut removed = vec![0];
        let mut added = vec![0];
        for edit in edits {
            removed.push(removed.last().copied().unwrap_or(0) + edit.range.len());
            added.push(added.last().copied().unwrap_or(0) + edit.replacement.len());
        }
        Self {
            edits,
            removed,
            added,
        }
    }
    fn map(&self, offset: usize) -> usize {
        let count = self.edits.partition_point(|edit| {
            edit.range.end < offset || (edit.range.end == offset && !edit.range.is_empty())
        });
        offset - self.removed[count] + self.added[count]
    }
}

impl Document<'_> {
    /// Plan and verify an entire snapshot-local batch atomically.
    ///
    /// # Errors
    /// Invalid selectors, conflicting operations and unintended reparsing are rejected.
    pub fn edit(&self, operations: &[Edit<'_>]) -> Result<VerifiedEdits, EditError> {
        let keys = KeyLookup::new(self, operations);
        let mut plans: Vec<_> = self
            .items()
            .map(|item| ItemPlan {
                whole: Whole::Keep,
                fields: 0,
                desired: item.entry().map(Desired::from_entry),
            })
            .collect();
        let mut edits = Vec::new();
        let mut insertions = Vec::new();
        let mut end_insertions = vec![0; self.items.len()];
        for operation in operations {
            self.plan_operation(*operation, &mut plans, &mut edits, &mut insertions, &keys)?;
        }
        self.validate_renames(&plans)?;
        insertions.sort_by_key(|insertion| insertion.offset);
        // The dominant line ending scans the whole source; compute it once per
        // batch, and only when a new line can be added.
        let ending = if insertions.is_empty() {
            ""
        } else {
            self.preferred_ending()
        };
        self.finish_sets(
            operations,
            &mut plans,
            &mut edits,
            &insertions,
            &mut end_insertions,
            ending,
        )?;
        let inserted_keys = insertions
            .iter()
            .map(|insertion| insertion.desired.key)
            .collect();
        let groups =
            self.plan_insertions(insertions, &plans, &mut edits, &mut end_insertions, ending)?;
        let edits = Self::normalize_edits(edits)?;
        let output = Self::apply_edits(self.source(), &edits);
        let regions = self.expected_regions(&plans, &end_insertions, &edits, groups)?;
        self.verify_regions(&output, regions, ending, inserted_keys)?;
        Ok(VerifiedEdits { edits, output })
    }

    fn plan_operation<'a>(
        &'a self,
        operation: Edit<'a>,
        plans: &mut [ItemPlan<'a>],
        edits: &mut Vec<TextEdit>,
        insertions: &mut Vec<Insertion<'a>>,
        keys: &KeyLookup<'a>,
    ) -> Result<(), EditError> {
        match operation {
            Edit::Set { entry, value } => {
                self.plan_set(entry, value, plans)?;
            }
            Edit::Rename { entry, key } => self.plan_rename(entry, key, plans, edits, keys)?,
            Edit::Export { entry, enabled } => self.plan_export(entry, enabled, plans, edits)?,
            Edit::Remove { item } => {
                let source_item = self.item(item).ok_or(EditError::InvalidTarget)?;
                whole(&mut plans[item], Whole::Remove)?;
                edits.push(TextEdit {
                    range: source_item.span(),
                    replacement: String::new(),
                });
            }
            Edit::Comment { item } => {
                let source_item = self.item(item).ok_or(EditError::InvalidTarget)?;
                whole(&mut plans[item], Whole::Comment)?;
                edits.push(TextEdit {
                    range: source_item.span(),
                    replacement: comment_text(source_item.raw(), self.dialect().line_breaks),
                });
            }
            Edit::Uncomment { item } => {
                let source_item = self.item(item).ok_or(EditError::InvalidTarget)?;
                if source_item.kind() != ItemKind::Comment {
                    return Err(EditError::InvalidTarget);
                }
                whole(&mut plans[item], Whole::Uncomment)?;
                let span = source_item.span();
                let hash = source_item
                    .raw()
                    .find('#')
                    .ok_or(EditError::InvalidTarget)?
                    + span.start;
                let end =
                    hash + 1 + usize::from(self.source().as_bytes().get(hash + 1) == Some(&b' '));
                edits.push(TextEdit {
                    range: hash..end,
                    replacement: String::new(),
                });
            }
            Edit::Insert {
                key,
                value,
                position,
            } => self.plan_insertion(key, value, position, insertions, keys)?,
        }
        Ok(())
    }

    fn plan_set<'a>(
        &'a self,
        id: usize,
        value: &'a str,
        plans: &mut [ItemPlan<'a>],
    ) -> Result<(), EditError> {
        let entry = self.entry(id).ok_or(EditError::InvalidTarget)?;
        let plan = &mut plans[entry.item_id()];
        claim(plan, 1)?;
        if let Some(desired) = &mut plan.desired {
            desired.value = Some(value);
        }
        Ok(())
    }

    fn finish_sets<'a>(
        &'a self,
        operations: &[Edit<'a>],
        plans: &mut [ItemPlan<'a>],
        edits: &mut Vec<TextEdit>,
        insertions: &[Insertion<'_>],
        end_insertions: &mut [usize],
        ending: &'static str,
    ) -> Result<(), EditError> {
        // Header and whole-item edits are complete before choosing value
        // spellings. Sets on different physical capped lines are independent;
        // retry only within one line when a neighboring edit frees its budget.
        edits.sort_by_key(edit_key);
        let mut targets = Vec::new();
        for operation in operations {
            let Edit::Set { entry: id, value } = *operation else {
                continue;
            };
            let entry = self.entry(id).ok_or(EditError::InvalidTarget)?;
            let span = entry.line_span();
            let range = entry
                .value_span()
                .unwrap_or_else(|| entry.tokens().key.end..entry.tokens().key.end);
            let context = self.value_context(&span, &range);
            targets.push(SetTarget {
                entry: id,
                value,
                range,
                context,
            });
        }
        targets.sort_by_key(|target| (target.range.start, target.range.end));
        let mut set_edits: Vec<Option<TextEdit>> = core::iter::repeat_with(|| None)
            .take(targets.len())
            .collect();
        let mut ending_items = vec![None; targets.len()];
        let mut group_start = 0;
        while group_start < targets.len() {
            let mut group_end = group_start + 1;
            while group_end < targets.len()
                && targets[group_end].context == targets[group_start].context
            {
                group_end += 1;
            }
            let mut remaining = group_end - group_start;
            while remaining != 0 {
                let mut progressed = false;
                for index in group_start..group_end {
                    if set_edits[index].is_some() {
                        continue;
                    }
                    let context_edits = ContextEdits {
                        base: edits,
                        targets: &targets,
                        sets: &set_edits,
                    };
                    match self.finish_set(&targets[index], plans, context_edits, insertions, ending)
                    {
                        Ok((edit, item, ending_field)) => {
                            set_edits[index] = Some(edit);
                            ending_items[index] = ending_field.then_some(item);
                            remaining -= 1;
                            progressed = true;
                        }
                        Err(EditError::Unencodable) => {}
                        Err(error) => return Err(error),
                    }
                }
                if !progressed {
                    return Err(EditError::Unencodable);
                }
            }
            group_start = group_end;
        }
        Self::merge_set_edits(edits, set_edits, ending_items, end_insertions)
    }

    fn merge_set_edits(
        edits: &mut Vec<TextEdit>,
        set_edits: Vec<Option<TextEdit>>,
        ending_items: Vec<Option<usize>>,
        end_insertions: &mut [usize],
    ) -> Result<(), EditError> {
        if set_edits.iter().any(Option::is_none) {
            return Err(EditError::Unencodable);
        }
        let base = core::mem::take(edits);
        edits.reserve(base.len() + set_edits.len());
        let mut base = base.into_iter().peekable();
        let mut sets = set_edits
            .into_iter()
            .zip(ending_items)
            .filter_map(|(edit, item)| edit.map(|edit| (edit, item)))
            .peekable();
        while base.peek().is_some() || sets.peek().is_some() {
            let take_base = match (base.peek(), sets.peek()) {
                (Some(left), Some((right, _))) => edit_key(left) <= edit_key(right),
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            if take_base {
                if let Some(edit) = base.next() {
                    edits.push(edit);
                }
            } else if let Some((edit, ending_item)) = sets.next() {
                edits.push(edit);
                if let Some(item) = ending_item {
                    end_insertions[item] = edits.len();
                }
            }
        }
        Ok(())
    }

    fn finish_set<'a>(
        &'a self,
        target: &SetTarget<'a>,
        plans: &mut [ItemPlan<'a>],
        edits: ContextEdits<'_, 'a>,
        insertions: &[Insertion<'_>],
        preferred_ending: &'static str,
    ) -> Result<(TextEdit, usize, bool), EditError> {
        let entry = self.entry(target.entry).ok_or(EditError::InvalidTarget)?;
        let item = self.item(entry.item_id()).ok_or(EditError::InvalidTarget)?;
        let span = entry.line_span();
        let range = target.range.clone();
        let context = target.context.clone();
        let before = self.edited_context(context.start..range.start, edits);
        let suffix = &self.source()[range.end..span.end];
        let separate_comment = entry
            .tokens()
            .inline_comment
            .as_ref()
            .is_some_and(|comment| comment.start == range.end);
        let encode = |ending: &str, preferred| {
            let after = if ending.is_empty() {
                self.edited_context(range.end..context.end, edits)
            } else if suffix.is_empty() {
                Cow::Borrowed(ending)
            } else {
                let mut combined = String::with_capacity(suffix.len() + ending.len());
                combined.push_str(suffix);
                combined.push_str(ending);
                Cow::Owned(combined)
            };
            encode_native(
                target.value,
                preferred,
                self.dialect(),
                &before,
                &after,
                entry.value_span().is_none(),
                separate_comment,
            )
            .map_err(|_| EditError::Unencodable)
        };
        let original_quote = plans[item.id()]
            .desired
            .ok_or(EditError::InvalidTarget)?
            .quote;
        let mut preferred = original_quote;
        let mut ending = self.following_separator(item, insertions, plans, preferred_ending);
        let mut chosen = None;
        for _ in 0..4 {
            let (candidate, actual_quote) = encode(ending, preferred)?;
            plans[item.id()]
                .desired
                .as_mut()
                .ok_or(EditError::InvalidTarget)?
                .quote = actual_quote;
            let actual_ending = self.following_separator(item, insertions, plans, preferred_ending);
            plans[item.id()]
                .desired
                .as_mut()
                .ok_or(EditError::InvalidTarget)?
                .quote = original_quote;
            if actual_ending == ending {
                chosen = Some((candidate, actual_quote));
                break;
            }
            preferred = actual_quote;
            ending = actual_ending;
        }
        let (mut token, quote) = chosen.ok_or(EditError::Unencodable)?;
        if separate_comment {
            token.push(' ');
        }
        if entry.value_span().is_none() {
            token.insert(0, '=');
        }
        plans[item.id()]
            .desired
            .as_mut()
            .ok_or(EditError::InvalidTarget)?
            .quote = quote;
        let ending_field = range.is_empty() && range.start == span.end;
        Ok((
            TextEdit {
                range,
                replacement: token,
            },
            item.id(),
            ending_field,
        ))
    }

    fn value_context(&self, item: &Range<usize>, value: &Range<usize>) -> Range<usize> {
        if self.dialect().line_byte_limit.is_none() {
            return item.clone();
        }
        let breaks = |c| {
            c == '\n' || (c == '\r' && self.dialect().line_breaks == crate::LineBreaks::Universal)
        };
        let start = self.source()[..item.start]
            .rfind(breaks)
            .map_or(0, |at| at + 1);
        let end = self.source()[value.end..]
            .find(breaks)
            .map_or(self.source().len(), |at| {
                let at = value.end + at;
                at + if self.source().as_bytes()[at] == b'\r'
                    && self.source().as_bytes().get(at + 1) == Some(&b'\n')
                {
                    2
                } else {
                    1
                }
            });
        start..end
    }

    fn edited_context<'a>(
        &'a self,
        range: Range<usize>,
        edits: ContextEdits<'_, '_>,
    ) -> Cow<'a, str> {
        let base_start = edits.base.partition_point(|edit| {
            edit.range.end < range.start
                || (edit.range.end == range.start && !edit.range.is_empty())
        });
        let base_end = edits
            .base
            .partition_point(|edit| edit.range.start < range.end);
        let set_start = edits.targets.partition_point(|target| {
            target.range.end < range.start
                || (target.range.end == range.start && !target.range.is_empty())
        });
        let set_end = edits
            .targets
            .partition_point(|target| target.range.start < range.end);
        let mut base = edits.base[base_start..base_end].iter().peekable();
        let mut sets = edits.sets[set_start..set_end]
            .iter()
            .filter_map(Option::as_ref)
            .peekable();
        if base.peek().is_none() && sets.peek().is_none() {
            return Cow::Borrowed(&self.source()[range]);
        }
        let mut output = String::with_capacity(range.len());
        let mut at = range.start;
        while base.peek().is_some() || sets.peek().is_some() {
            let take_base = match (base.peek(), sets.peek()) {
                (Some(left), Some(right)) => edit_key(left) <= edit_key(right),
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            let edit = if take_base { base.next() } else { sets.next() };
            let Some(edit) = edit else {
                break;
            };
            output.push_str(&self.source()[at..edit.range.start]);
            output.push_str(&edit.replacement);
            at = edit.range.end;
        }
        output.push_str(&self.source()[at..range.end]);
        Cow::Owned(output)
    }

    fn following_separator(
        &self,
        item: crate::Item<'_>,
        insertions: &[Insertion<'_>],
        plans: &[ItemPlan<'_>],
        ending: &'static str,
    ) -> &'static str {
        let next = insertions.partition_point(|insertion| insertion.offset < item.span().end);
        let Some(insertion) = insertions.get(next) else {
            return "";
        };
        // Index from this item rather than skipping an iterator from the
        // first item, which would make a batch of sets quadratic.
        let intervening = (item.id() + 1..plans.len())
            .filter_map(|index| self.item(index))
            .find(|next| plans[next.id()].retains(*next));
        if intervening.is_some_and(|next| next.span().end <= insertion.offset) {
            return "";
        }
        self.insertion_separator(Some(item), ending, plans)
    }

    fn plan_rename<'a>(
        &'a self,
        id: usize,
        key: &'a str,
        plans: &mut [ItemPlan<'a>],
        edits: &mut Vec<TextEdit>,
        keys: &KeyLookup<'a>,
    ) -> Result<(), EditError> {
        let entry = self.entry(id).ok_or(EditError::InvalidTarget)?;
        if key != entry.key() {
            let existing = keys.count(key);
            if existing != 0 {
                return Err(EditError::Duplicate(existing + 1));
            }
        }
        let plan = &mut plans[entry.item_id()];
        claim(plan, 2)?;
        let (range, replacement) =
            if entry.key_span() != entry.tokens().key && !key.is_empty() && !key.contains('\'') {
                (entry.key_span(), key.to_string())
            } else {
                (
                    entry.tokens().key.clone(),
                    key_token(key, self.dialect())
                        .ok_or(EditError::InvalidKey)?
                        .into_owned(),
                )
            };
        edits.push(TextEdit { range, replacement });
        if let Some(desired) = &mut plan.desired {
            desired.key = key;
        }
        Ok(())
    }

    fn plan_export(
        &self,
        id: usize,
        enabled: bool,
        plans: &mut [ItemPlan<'_>],
        edits: &mut Vec<TextEdit>,
    ) -> Result<(), EditError> {
        let entry = self.entry(id).ok_or(EditError::InvalidTarget)?;
        let plan = &mut plans[entry.item_id()];
        claim(plan, 4)?;
        if enabled != entry.exported() {
            let (range, replacement) = if enabled {
                let start = entry.tokens().key.start;
                (start..start, "export ".to_string())
            } else {
                let export = entry
                    .tokens()
                    .export
                    .as_ref()
                    .ok_or(EditError::InvalidTarget)?;
                (
                    export.start..entry.tokens().space_after_export.end,
                    String::new(),
                )
            };
            edits.push(TextEdit { range, replacement });
        }
        if let Some(desired) = &mut plan.desired {
            desired.exported = enabled;
        }
        Ok(())
    }

    fn plan_insertion<'a>(
        &self,
        key: &'a str,
        value: Option<&'a str>,
        position: InsertPosition<'_>,
        insertions: &mut Vec<Insertion<'a>>,
        keys: &KeyLookup<'_>,
    ) -> Result<(), EditError> {
        let key_token = key_token(key, self.dialect()).ok_or(EditError::InvalidKey)?;
        let offset = match position {
            InsertPosition::End => self.source().len(),
            InsertPosition::AfterKey(key) => keys.unique(key)?.line_span().end,
            InsertPosition::AfterEntry(id) => {
                self.entry(id)
                    .ok_or(EditError::InvalidTarget)?
                    .line_span()
                    .end
            }
            InsertPosition::Sorted => keys.first_greater(key),
        };
        insertions.push(Insertion {
            offset,
            desired: Desired {
                key,
                value,
                quote: Quote::None,
                exported: false,
            },
            key_token,
            leading: String::new(),
            sorted: matches!(position, InsertPosition::Sorted),
        });
        Ok(())
    }

    fn validate_renames(&self, plans: &[ItemPlan<'_>]) -> Result<(), EditError> {
        // Original-name collisions are checked while planning. Only two newly
        // renamed declarations can now collide, so index just their new names.
        let mut renamed = plans.iter().enumerate().filter_map(|(index, plan)| {
            if plan.fields & 2 == 0 {
                return None;
            }
            let desired = plan.desired?;
            let original = self.item(index)?.entry()?;
            (original.key() != desired.key).then_some(desired.key)
        });
        let Some(first) = renamed.next() else {
            return Ok(());
        };
        let Some(second) = renamed.next() else {
            return Ok(());
        };
        let mut keys = Vec::from([first, second]);
        keys.extend(renamed);
        keys.sort_unstable();
        if keys.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(EditError::WouldChangeMeaning);
        }
        Ok(())
    }

    fn last_retained_item(&self, plans: &[ItemPlan<'_>]) -> Option<crate::Item<'_>> {
        let mut retained = self
            .items()
            .rev()
            .filter(|item| plans[item.id()].retains(*item));
        let last = retained.next()?;
        // A selected comment can become the output's BOM prelude. A prelude
        // alone is not a physical item and needs no insertion separator.
        let becomes_prelude = !self.has_bom()
            && plans[last.id()].whole == Whole::Uncomment
            && last.line_ending() == crate::LineEnding::None
            && last.raw().starts_with('#')
            && last
                .comment_text()
                .is_some_and(|body| body.strip_prefix(' ').unwrap_or(body) == "\u{feff}");
        if becomes_prelude && retained.next().is_none() {
            None
        } else {
            Some(last)
        }
    }
    fn insertion_separator(
        &self,
        previous: Option<crate::Item<'_>>,
        ending: &'static str,
        plans: &[ItemPlan<'_>],
    ) -> &'static str {
        previous.map_or("", |item| {
            let plan = &plans[item.id()];
            let next_statement = self.dialect().line_byte_limit.is_none()
                && plan.whole == Whole::Keep
                && self.dialect().quoted_tail == crate::QuotedTail::NextStatement
                && plan
                    .desired
                    .is_some_and(|desired| desired.quote != Quote::None);
            let needs_separator = (item.line_ending() == crate::LineEnding::None
                && !next_statement)
                || (self.dialect().line_breaks == crate::LineBreaks::Lf
                    && item.line_ending() == crate::LineEnding::Cr);
            if needs_separator {
                ending
            } else {
                ""
            }
        })
    }

    fn merge_boundary_fields(
        insertions: &mut [Insertion<'_>],
        edits: &mut Vec<TextEdit>,
        end_insertions: &mut [usize],
    ) {
        // Until this pass, nonzero values identify an ending field's edit by
        // one-based index. Convert to byte lengths for region verification.
        for ending in end_insertions {
            let Some(index) = ending.checked_sub(1) else {
                continue;
            };
            let edit = &mut edits[index];
            *ending = edit.replacement.len();
            let at = insertions.partition_point(|insert| insert.offset < edit.range.start);
            if let Some(insertion) = insertions
                .get_mut(at)
                .filter(|insert| insert.offset == edit.range.start)
            {
                insertion.leading = core::mem::take(&mut edit.replacement);
            }
        }
        edits.retain(|edit| !edit.range.is_empty() || !edit.replacement.is_empty());
    }

    fn plan_insertions<'a>(
        &self,
        mut insertions: Vec<Insertion<'a>>,
        plans: &[ItemPlan<'_>],
        edits: &mut Vec<TextEdit>,
        end_insertions: &mut [usize],
        ending: &'static str,
    ) -> Result<Vec<InsertGroup<'a>>, EditError> {
        Self::merge_boundary_fields(&mut insertions, edits, end_insertions);
        if insertions.is_empty() {
            return Ok(Vec::new());
        }
        let last_retained = self.last_retained_item(plans);
        let mut retained = self
            .items()
            .filter(|item| plans[item.id()].retains(*item))
            .peekable();
        let mut previous = None;
        let mut previous_group_item = None;
        let mut groups = Vec::new();
        let mut pending = insertions.as_mut_slice();
        while !pending.is_empty() {
            let offset = pending[0].offset;
            let count = pending.partition_point(|insert| insert.offset == offset);
            let (same, remaining) = pending.split_at_mut(count);
            let mut text = core::mem::take(&mut same[0].leading);
            if same.iter().all(|insert| insert.sorted) {
                same.sort_by(|a, b| a.desired.key.cmp(b.desired.key));
            }
            let at_end = offset == self.source().len();
            while retained
                .peek()
                .is_some_and(|item| item.span().end <= offset)
            {
                previous = retained.next();
            }
            let final_in_output = remaining.is_empty()
                && last_retained.map_or(true, |item| item.span().start < offset);
            let prior = if at_end { last_retained } else { previous };
            let owner = prior.map(crate::Item::id);
            let prefix = if !groups.is_empty() && owner == previous_group_item {
                ""
            } else {
                self.insertion_separator(prior, ending, plans)
            };
            if let Some(owner) = owner {
                end_insertions[owner] += prefix.len();
            }
            previous_group_item = owner;
            text.push_str(prefix);
            let mut entries = Vec::with_capacity(same.len());
            let count = same.len();
            for (index, insertion) in same.iter_mut().enumerate() {
                let suffix = if !final_in_output
                    || self.has_final_newline()
                    || index + 1 != count
                    || (self.dialect().syntax == crate::Syntax::Compose
                        && insertion.desired.value.is_none())
                {
                    ending
                } else {
                    ""
                };
                entries.push(insertion.append_to(&mut text, self.dialect(), suffix)?);
            }
            groups.push(InsertGroup {
                offset,
                len: text.len(),
                entries,
            });
            let insertion = TextEdit {
                range: offset..offset,
                replacement: text,
            };
            edits.push(insertion);
            pending = remaining;
        }
        // Ending fields now belong to their insertion groups; groups precede
        // any remaining fields belonging to the following item at that offset.
        edits.rotate_right(groups.len());
        Ok(groups)
    }

    fn normalize_edits(mut edits: Vec<TextEdit>) -> Result<Vec<TextEdit>, EditError> {
        // Empty boundary insertions precede replacements starting at that boundary.
        edits.sort_by(|a, b| {
            a.range
                .start
                .cmp(&b.range.start)
                .then(a.range.end.cmp(&b.range.end))
        });
        if edits
            .windows(2)
            .any(|pair| pair[1].range.start < pair[0].range.end)
        {
            return Err(EditError::ConflictingEdits);
        }
        edits.dedup_by(|later, earlier| {
            if later.range.is_empty() && earlier.range == later.range {
                earlier.replacement.push_str(&later.replacement);
                true
            } else {
                false
            }
        });
        Ok(edits)
    }

    fn apply_edits(source: &str, edits: &[TextEdit]) -> String {
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

    fn expected_regions<'a>(
        &self,
        plans: &[ItemPlan<'a>],
        end_insertions: &[usize],
        edits: &[TextEdit],
        groups: Vec<InsertGroup<'a>>,
    ) -> Result<Vec<Region<'a>>, EditError> {
        let boundaries = Boundaries::new(edits);
        let mut regions = Vec::with_capacity(self.items.len() + groups.len());
        let mut item = 0;
        while item < self.items.len() {
            let plan = &plans[item];
            if plan.whole == Whole::Remove {
                item += 1;
                continue;
            }
            let first = item;
            let expect = match plan.whole {
                Whole::Uncomment => {
                    let mut next = item + 1;
                    while next < plans.len()
                        && group_at(&groups, self.items[next - 1].span.end).is_none()
                    {
                        match plans[next].whole {
                            Whole::Remove => {}
                            Whole::Uncomment => item = next,
                            _ => break,
                        }
                        next += 1;
                    }
                    Expect::Uncommented
                }
                Whole::Comment => Expect::Comments,
                Whole::Keep if plan.fields != 0 => {
                    Expect::Entry(plan.desired.ok_or(EditError::InvalidTarget)?)
                }
                Whole::Keep => Expect::Original(item),
                Whole::Remove => return Err(EditError::InvalidTarget),
            };
            let end = self.items[item].span.end;
            let start = self.items[first].span.start;
            let inserted_before = group_at(&groups, start).map_or(0, |group| group.len);
            regions.push(Region {
                range: boundaries.map(start) + inserted_before
                    ..boundaries.map(end) + end_insertions[item],
                expect,
            });
            item += 1;
        }
        for group in groups {
            let start = boundaries.map(group.offset);
            for (range, desired) in group.entries {
                regions.push(Region {
                    range: start + range.start..start + range.end,
                    expect: Expect::Entry(desired),
                });
            }
        }
        regions.sort_by_key(|region| region.range.start);
        Ok(regions)
    }

    fn verify_regions(
        &self,
        output: &str,
        mut regions: Vec<Region<'_>>,
        ending: &str,
        inserted_keys: Vec<&str>,
    ) -> Result<(), EditError> {
        let parsed = crate::parser::parse_document(output, self.dialect());
        if parsed.has_bom() != self.has_bom() {
            // Only an explicitly uncommented leading region may expose its own
            // BOM. Removing text before an unrelated BOM must still be refused.
            let selected = regions.first_mut().filter(|region| {
                !self.has_bom()
                    && parsed.has_bom()
                    && region.range.start == 0
                    && region.range.end >= 3
                    && matches!(region.expect, Expect::Uncommented)
            });
            let Some(region) = selected else {
                return Err(EditError::WouldChangeMeaning);
            };
            region.range.start = 3;
        }
        let mut actual = parsed.items();
        for region in regions {
            if region.range.is_empty() && matches!(region.expect, Expect::Uncommented) {
                continue;
            }
            let first = actual.next().ok_or(EditError::WouldChangeMeaning)?;
            if first.span().start != region.range.start {
                return Err(EditError::WouldChangeMeaning);
            }
            match region.expect {
                Expect::Entry(desired) => {
                    verify_entry_region(first, desired, &region.range, &mut actual)?;
                }
                Expect::Original(id) => {
                    let original = self.item(id).ok_or(EditError::InvalidTarget)?;
                    if first.span() != region.range || first.kind() != original.kind() {
                        return Err(EditError::WouldChangeMeaning);
                    }
                    let original_text = original.raw();
                    let new_text = &output[region.range.clone()];
                    if !new_text.starts_with(original_text)
                        || (new_text.len() != original_text.len()
                            && &new_text[original_text.len()..] != ending)
                    {
                        return Err(EditError::WouldChangeMeaning);
                    }
                    if let Some(entry) = original.entry() {
                        if !first
                            .entry()
                            .is_some_and(|after| Desired::from_entry(entry).matches(after))
                        {
                            return Err(EditError::WouldChangeMeaning);
                        }
                    }
                    match (&self.items[id].invalid, &parsed.items[first.id()].invalid) {
                        (None, None) => {}
                        (Some(before), Some(after))
                            if before.code() == after.code()
                                && before.span().start - original.span().start
                                    == after.span().start - region.range.start
                                && before.span().end - original.span().start
                                    == after.span().end - region.range.start => {}
                        _ => return Err(EditError::WouldChangeMeaning),
                    }
                }
                Expect::Comments | Expect::Uncommented => {
                    let comments_only = matches!(region.expect, Expect::Comments);
                    let mut current = first;
                    loop {
                        if current.span().end > region.range.end
                            || current.kind() == ItemKind::Invalid
                            || current
                                .diagnostic()
                                .is_some_and(|diagnostic| diagnostic.severity() == Severity::Error)
                            || (comments_only && current.kind() != ItemKind::Comment)
                        {
                            return Err(EditError::WouldChangeMeaning);
                        }
                        if current.span().end == region.range.end {
                            break;
                        }
                        let next = actual.next().ok_or(EditError::WouldChangeMeaning)?;
                        if next.span().start != current.span().end {
                            return Err(EditError::WouldChangeMeaning);
                        }
                        current = next;
                    }
                }
            }
        }
        if actual.next().is_some() {
            return Err(EditError::WouldChangeMeaning);
        }
        verify_inserted_keys(&parsed, inserted_keys)
    }

    /// Set a uniquely named declaration's exact literal value.
    ///
    /// # Errors
    /// Returns a selector, encoding or verification error.
    pub fn set(&self, key: &str, value: &str) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Set {
            entry: self.unique(key)?.id(),
            value,
        }])
    }
    /// Remove a uniquely named declaration, not its neighboring comments.
    ///
    /// # Errors
    /// Returns a selector or verification error.
    pub fn remove(&self, key: &str) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Remove {
            item: self.unique(key)?.item_id(),
        }])
    }
    /// Rename a uniquely named declaration.
    ///
    /// # Errors
    /// Returns a selector, key, collision or verification error.
    pub fn rename(&self, key: &str, new_key: &str) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Rename {
            entry: self.unique(key)?.id(),
            key: new_key,
        }])
    }
    /// Insert an assigned or bare declaration of a key the document does not
    /// declare.
    ///
    /// # Errors
    /// Returns a selector, key, encoding or verification error, or
    /// [`EditError::Duplicate`] when the document already declares `key`.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict, EditError, InsertPosition};
    ///
    /// let doc = parse_strict("A=1\nB=2\n").unwrap();
    /// let inserted = doc.insert("C", Some("3"), InsertPosition::End).unwrap();
    /// assert_eq!(inserted.as_str(), "A=1\nB=2\nC=3\n");
    /// assert_eq!(
    ///     doc.insert("A", Some("other"), InsertPosition::End),
    ///     Err(EditError::Duplicate(2))
    /// );
    /// ```
    pub fn insert(
        &self,
        key: &str,
        value: Option<&str>,
        position: InsertPosition<'_>,
    ) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Insert {
            key,
            value,
            position,
        }])
    }
    /// Comment every physical line of a uniquely named declaration.
    ///
    /// # Errors
    /// Returns a selector or verification error.
    pub fn comment(&self, key: &str) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Comment {
            item: self.unique(key)?.item_id(),
        }])
    }
    /// Uncomment an explicitly selected comment item.
    ///
    /// # Errors
    /// Returns a target or verification error. Multiline restoration may require a batch.
    pub fn uncomment(&self, item: usize) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Uncomment { item }])
    }
    /// Enable or disable export on a uniquely named declaration.
    ///
    /// # Errors
    /// Returns a selector or verification error.
    pub fn set_export(&self, key: &str, enabled: bool) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Export {
            entry: self.unique(key)?.id(),
            enabled,
        }])
    }
}

/// Set a unique key in recovering input, verifying that unrelated items survive.
///
/// # Errors
/// Returns the native selector, encoding or verification error.
pub fn set(text: &str, key: &str, value: &str) -> Result<String, EditError> {
    crate::parser::parse_document(text, Dialect::PORTABLE)
        .set(key, value)
        .map(VerifiedEdits::into_string)
}
/// Remove a unique key in recovering input, without deleting adjacent comments.
///
/// # Errors
/// Returns the native selector or verification error.
pub fn remove(text: &str, key: &str) -> Result<String, EditError> {
    crate::parser::parse_document(text, Dialect::PORTABLE)
        .remove(key)
        .map(VerifiedEdits::into_string)
}
