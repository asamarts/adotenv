use alloc::{
    borrow::Cow,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{error::Error, fmt, ops::Range};

use crate::{AssignmentState, Dialect, Document, ItemKind, Quote, Severity};

/// A native byte-range replacement against the original document snapshot.
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
    /// Replacements in ascending, non-overlapping source order.
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
    Rename {
        /// The snapshot-local declaration identity.
        entry: usize,
        /// The new literal key name.
        key: &'a str,
    },
    /// Insert an assigned or bare declaration.
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
    /// A key-only selector matched this many declarations.
    Duplicate(usize),
    /// A snapshot-local identity or item kind is unsuitable.
    InvalidTarget,
    /// The requested name cannot be represented by the selected key grammar.
    InvalidKey,
    /// The value contains unsupported control characters.
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
                write!(f, "key has {count} declarations; select an occurrence")
            }
            Self::InvalidTarget => f.write_str("invalid snapshot-local edit target"),
            Self::InvalidKey => f.write_str("key cannot be represented by the selected dialect"),
            Self::Unencodable => f.write_str("value contains an unsupported control character"),
            Self::ConflictingEdits => f.write_str("conflicting operations in edit batch"),
            Self::WouldChangeMeaning => {
                f.write_str("edit would change unrelated source interpretation")
            }
        }
    }
}
impl Error for EditError {}

fn supported(value: &str) -> bool {
    value
        .chars()
        .all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
}
fn unquoted(value: &str) -> bool {
    !value.starts_with(['\'', '"'])
        && !value
            .chars()
            .any(|c| c.is_whitespace() || c == '#' || c.is_control())
}
fn double_quoted(value: &str) -> String {
    let mut token = String::with_capacity(value.len() + 2);
    token.push('"');
    for c in value.chars() {
        match c {
            '\\' => token.push_str("\\\\"),
            '"' => token.push_str("\\\""),
            '\n' => token.push_str("\\n"),
            '\r' => token.push_str("\\r"),
            '\t' => token.push_str("\\t"),
            _ => token.push(c),
        }
    }
    token.push('"');
    token
}
fn encode_with_style(value: &str, quote: Quote) -> Option<(String, Quote)> {
    if !supported(value) {
        return None;
    }
    match quote {
        Quote::Single if !value.contains('\'') => {
            let mut token = String::with_capacity(value.len() + 2);
            token.push('\'');
            token.push_str(value);
            token.push('\'');
            Some((token, Quote::Single))
        }
        Quote::Backtick if !value.contains('`') => {
            let mut token = String::with_capacity(value.len() + 2);
            token.push('`');
            token.push_str(value);
            token.push('`');
            Some((token, Quote::Backtick))
        }
        Quote::Double => Some((double_quoted(value), Quote::Double)),
        _ if unquoted(value) => Some((value.to_string(), Quote::None)),
        _ => Some((double_quoted(value), Quote::Double)),
    }
}

fn encode_in(value: &str, quote: Quote, dialect: Dialect) -> Option<(String, Quote)> {
    if dialect.raw_values {
        supported(value).then(|| (value.to_string(), Quote::None))
    } else {
        encode_with_style(value, quote)
    }
}

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

/// Encode one exact standalone Portable value token, without interpolation.
///
/// Unsupported control characters return `None`; CR, LF and tab are escaped.
#[must_use]
pub fn encode_value(value: &str) -> Option<String> {
    encode_with_style(value, Quote::None).map(|(token, _)| token)
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
struct Insertion<'a> {
    offset: usize,
    desired: Desired<'a>,
    token: Option<String>,
    key_token: Cow<'a, str>,
    sorted: bool,
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
struct InsertGroup<'a> {
    offset: usize,
    prefix: usize,
    before: usize,
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
fn comment_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' || (byte == b'\r' && text.as_bytes().get(index + 1) != Some(&b'\n')) {
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
            self.plan_operation(
                *operation,
                &mut plans,
                &mut edits,
                &mut insertions,
                &mut end_insertions,
                &keys,
            )?;
        }
        self.validate_renames(&plans)?;
        let (groups, ending) = self.plan_insertions(insertions, &plans, &mut edits);
        let edits = Self::normalize_edits(edits)?;
        let output = Self::apply_edits(self.source(), &edits);
        let regions = self.expected_regions(&plans, &end_insertions, &edits, groups)?;
        self.verify_regions(&output, regions, ending)?;
        Ok(VerifiedEdits { edits, output })
    }

    fn plan_operation<'a>(
        &'a self,
        operation: Edit<'a>,
        plans: &mut [ItemPlan<'a>],
        edits: &mut Vec<TextEdit>,
        insertions: &mut Vec<Insertion<'a>>,
        end_insertions: &mut [usize],
        keys: &KeyLookup<'a>,
    ) -> Result<(), EditError> {
        match operation {
            Edit::Set { entry, value } => {
                self.plan_set(entry, value, plans, edits, end_insertions)?;
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
                    replacement: comment_text(source_item.raw()),
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
        edits: &mut Vec<TextEdit>,
        end_insertions: &mut [usize],
    ) -> Result<(), EditError> {
        let entry = self.entry(id).ok_or(EditError::InvalidTarget)?;
        let plan = &mut plans[entry.item_id()];
        claim(plan, 1)?;
        let retained_tail = self.dialect().quoted_tail == crate::QuotedTail::Ignore
            && entry.tokens().quoted_tail.is_some();
        let style = match entry.quote() {
            Quote::Single if retained_tail && value.contains('\'') => Quote::Double,
            Quote::Backtick if retained_tail && value.contains('`') => Quote::Double,
            style => style,
        };
        let (mut token, quote) =
            encode_in(value, style, self.dialect()).ok_or(EditError::Unencodable)?;
        let range = entry
            .value_span()
            .unwrap_or_else(|| entry.tokens().key.end..entry.tokens().key.end);
        if entry
            .tokens()
            .inline_comment
            .as_ref()
            .is_some_and(|comment| comment.start == range.end)
        {
            token.push(' ');
        }
        if entry.value_span().is_none() {
            token.insert(0, '=');
        }
        if range.is_empty() && range.start == entry.line_span().end {
            end_insertions[entry.item_id()] += token.len();
        }
        edits.push(TextEdit {
            range,
            replacement: token,
        });
        if let Some(desired) = &mut plan.desired {
            desired.value = Some(value);
            desired.quote = quote;
        }
        Ok(())
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
        let (token, quote) = match value {
            Some(value) => {
                let (token, quote) =
                    encode_in(value, Quote::None, self.dialect()).ok_or(EditError::Unencodable)?;
                (Some(token), quote)
            }
            None => (None, Quote::None),
        };
        insertions.push(Insertion {
            offset,
            desired: Desired {
                key,
                value,
                quote,
                exported: false,
            },
            token,
            key_token,
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
    ) -> &'static str {
        previous.map_or("", |item| {
            let next_statement = self.dialect().quoted_tail == crate::QuotedTail::NextStatement
                && item
                    .entry()
                    .is_some_and(|entry| entry.quote() != Quote::None);
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

    fn plan_insertions<'a>(
        &self,
        mut insertions: Vec<Insertion<'a>>,
        plans: &[ItemPlan<'_>],
        edits: &mut Vec<TextEdit>,
    ) -> (Vec<InsertGroup<'a>>, &'static str) {
        if insertions.is_empty() {
            return (Vec::new(), "");
        }
        let ending = self.preferred_ending();
        let last_retained = self.last_retained_item(plans);
        let mut retained = self
            .items()
            .filter(|item| plans[item.id()].retains(*item))
            .peekable();
        let mut previous = None;
        insertions.sort_by_key(|insert| insert.offset);
        let mut groups = Vec::new();
        let mut preceding_edits = 0;
        let mut final_edit = None;
        let mut pending = insertions.as_mut_slice();
        while !pending.is_empty() {
            let offset = pending[0].offset;
            let count = pending.partition_point(|insert| insert.offset == offset);
            let (same, remaining) = pending.split_at_mut(count);
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
            let prefix =
                self.insertion_separator(if at_end { last_retained } else { previous }, ending);
            let mut text = String::from(prefix);
            let mut entries = Vec::with_capacity(same.len());
            for (index, insertion) in same.iter().enumerate() {
                let start = text.len();
                text.push_str(&insertion.key_token);
                if let Some(token) = &insertion.token {
                    text.push('=');
                    text.push_str(token);
                }
                if !final_in_output || self.has_final_newline() || index + 1 != same.len() {
                    text.push_str(ending);
                }
                entries.push((start..text.len(), insertion.desired));
            }
            let before = if at_end {
                edits
                    .iter()
                    .filter(|edit| edit.range == (offset..offset))
                    .map(|edit| edit.replacement.len())
                    .sum()
            } else {
                0
            };
            groups.push(InsertGroup {
                offset,
                prefix: prefix.len(),
                before,
                len: text.len(),
                entries,
            });
            let insertion = TextEdit {
                range: offset..offset,
                replacement: text,
            };
            if at_end {
                final_edit = Some(insertion);
            } else {
                edits.push(insertion);
                preceding_edits += 1;
            }
            pending = remaining;
        }
        // Non-EOF insertions precede field insertions at the same boundary;
        // EOF insertions follow them. Rotate the appended groups once in the
        // existing buffer rather than allocate a staging vector or shift per group.
        edits.rotate_right(preceding_edits);
        if let Some(edit) = final_edit {
            edits.push(edit);
        }
        (groups, ending)
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
            let suffix = group_at(&groups, end).map_or(0, |group| group.prefix);
            let start = self.items[first].span.start;
            let inserted_before = group_at(&groups, start).map_or(0, |group| group.len);
            regions.push(Region {
                range: boundaries.map(start) + inserted_before
                    ..boundaries.map(end) + end_insertions[item] + suffix,
                expect,
            });
            item += 1;
        }
        for group in groups {
            let start = boundaries.map(group.offset) + group.before;
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
                    if first.span() != region.range
                        || first
                            .diagnostic()
                            .is_some_and(|diagnostic| diagnostic.severity() == Severity::Error)
                        || !first.entry().is_some_and(|entry| desired.matches(entry))
                    {
                        return Err(EditError::WouldChangeMeaning);
                    }
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
        Ok(())
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
    /// Insert an assigned or bare declaration.
    ///
    /// # Errors
    /// Returns a selector, key, encoding or verification error.
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
