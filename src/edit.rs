use alloc::{
    borrow::Cow,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{error::Error, fmt, ops::Range};

use crate::{encode::encode_native, AssignmentState, Dialect, Document, ItemKind, Quote, Severity};

/// A replacement of one byte range of the original source.
///
/// [`VerifiedEdits::edits`] and [`FormatResult::edits`](crate::FormatResult::edits)
/// return these for editors and language servers that apply a change to their
/// own copy of the text. `range` is a byte range into the
/// [`Document::source`] that produced the edits, never into the output, so the
/// edits are only valid for that exact text. An editor whose buffer may have
/// changed since it was parsed must check the buffer version first.
///
/// The edits of one result are sorted by `range.start` and do not overlap.
/// Two edits can start at the same offset: an insertion (an empty range) then
/// comes before the replacement that starts there. Apply the edits in reverse
/// order of the slice, from its last element to its first, so each range
/// still addresses unchanged text. Do not re-sort them by offset: a sort by
/// start offset alone can swap edits that share a start and corrupt the
/// result. See [`VerifiedEdits::edits`] for an example.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// The byte range of the original source to replace; empty for an
    /// insertion.
    pub range: Range<usize>,
    /// The text that replaces `range`; empty for a deletion.
    pub replacement: String,
}

/// The verified result of an edit: the complete output text and the text edits
/// that produce it.
///
/// Every edit method of [`Document`] returns this type. Before it is returned,
/// the complete output was parsed again with the document's dialect and
/// checked: the requested changes happened, and unrelated declarations,
/// comments, item kinds and invalid statements did not change.
///
/// Use [`as_str`](Self::as_str) or [`into_string`](Self::into_string) for the
/// new text, for example to write a file back. Use [`edits`](Self::edits) or
/// [`into_edits`](Self::into_edits) to apply the change to a buffer that still
/// holds the original source. The [`Document`] itself is not modified; parse
/// the output again to make further edits.
///
/// # Examples
///
/// ```
/// let doc = adotenv::parse_strict("# server\nPORT=8080\n")?;
/// let verified = doc.set("PORT", "9090")?;
/// assert_eq!(verified.as_str(), "# server\nPORT=9090\n");
/// assert_eq!(verified.edits()[0].range, 14..18);
/// assert_eq!(verified.edits()[0].replacement, "9090");
///
/// // Parse the output again to keep editing.
/// let doc = adotenv::parse_strict(verified.as_str())?;
/// assert_eq!(doc.get("PORT"), Some("9090"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedEdits {
    edits: Vec<TextEdit>,
    output: String,
}
impl VerifiedEdits {
    /// Returns the text edits that turn the original source into the output.
    ///
    /// The edits are sorted by start offset and do not overlap. Their ranges
    /// refer to the original [`Document::source`]. Apply them in reverse order
    /// of this slice, from the last element to the first; see [`TextEdit`]. An
    /// empty batch returns no edits.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::Edit;
    ///
    /// let doc = adotenv::parse_strict("A=1\n")?;
    /// let verified = doc.edit(&[
    ///     Edit::Export { entry: 0, enabled: true },
    ///     Edit::Rename { entry: 0, key: "B" },
    /// ])?;
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
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn edits(&self) -> &[TextEdit] {
        &self.edits
    }
    /// Returns the text edits, consuming the result.
    ///
    /// The edits are in the same order as [`edits`](Self::edits) returns them.
    #[must_use]
    pub fn into_edits(self) -> Vec<TextEdit> {
        self.edits
    }
    /// Returns the complete verified output, consuming the result.
    #[must_use]
    pub fn into_string(self) -> String {
        self.output
    }
    /// Returns the complete verified output.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.output
    }
}

/// Where [`Edit::Insert`] and [`Document::insert`] place a new declaration.
///
/// Positions refer to the original source. A new declaration goes on its own
/// line and uses the line ending that is most common in the document, or LF
/// when it has none. At the end of a file without a final line ending, the
/// line break goes before the new declaration and the output still has no
/// final line ending, except where the dialect needs one: a bare key added at
/// the end of a Compose file gets a line ending. An empty file counts as a
/// file without a final line ending, so inserting `PORT=8080` into it gives
/// `PORT=8080` with no line ending.
///
/// In a dialect that reads text after a closing quote as the next statement
/// ([`QuotedTail::NextStatement`](crate::QuotedTail::NextStatement), as in
/// Compose), a declaration inserted right after a quoted value that has no
/// line ending follows the closing quote on the same line: inserting `B=1`
/// at the end of `A="x"` gives `A="x"B=1`.
///
/// Several insertions at the same offset keep their order in the batch. When
/// all of them use [`Sorted`](Self::Sorted), they are ordered by key instead.
///
/// # Examples
///
/// ```
/// use adotenv::InsertPosition;
///
/// let doc = adotenv::parse_strict("# app\nDEBUG=false\nPORT=8080\n")?;
/// let sorted = doc.insert("HOST", Some("localhost"), InsertPosition::Sorted)?;
/// assert_eq!(sorted.as_str(), "# app\nDEBUG=false\nHOST=localhost\nPORT=8080\n");
///
/// let after = doc.insert("TRACE", None, InsertPosition::AfterKey("DEBUG"))?;
/// assert_eq!(after.as_str(), "# app\nDEBUG=false\nTRACE\nPORT=8080\n");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InsertPosition<'a> {
    /// After the last item of the document.
    End,
    /// After the only declaration of this key.
    ///
    /// The new declaration goes after the whole declaration, including a
    /// multiline value, and before any comment that follows it. The batch
    /// fails with [`EditError::Missing`] when no declaration has the key and
    /// with [`EditError::Duplicate`] when several do; use
    /// [`AfterEntry`](Self::AfterEntry) to pick one of them.
    AfterKey(&'a str),
    /// After one declaration, selected by its entry ID ([`Entry::id`]).
    ///
    /// The batch fails with [`EditError::InvalidTarget`] when the ID is out of
    /// range.
    ///
    /// [`Entry::id`]: crate::Entry::id
    AfterEntry(usize),
    /// Before the first declaration, in source order, whose key sorts after
    /// the new key, or at the end when there is none.
    ///
    /// Keys compare by bytes, so uppercase letters sort before lowercase ones.
    /// The new line goes directly above that declaration, below any comments
    /// over it. `Sorted` does not sort the file: in an unsorted file the new
    /// declaration still goes before the first greater key it finds. Use
    /// [`Document::sort`] to sort a file.
    Sorted,
}

/// One operation of a [`Document::edit`] batch.
///
/// Operations select their targets by entry ID ([`Entry::id`]) or item ID
/// ([`Item::id`]) of the document that runs the batch, so a batch can change
/// one of several declarations of the same key. Every ID and position refers
/// to the original document, not to the output of earlier operations in the
/// batch. The batch either applies every operation or fails as a whole.
///
/// The single-edit methods such as [`Document::set`] select a declaration by
/// key and run a batch of one operation.
///
/// # Examples
///
/// ```
/// use adotenv::Edit;
///
/// let doc = adotenv::parse_strict("PORT=8080\nPORT=9090\n")?;
/// // `set` refuses to guess between duplicates; an entry ID picks one.
/// assert!(doc.set("PORT", "3000").is_err());
/// let second = doc.get_all("PORT").nth(1).unwrap();
/// let verified = doc.edit(&[Edit::Set { entry: second.id(), value: "3000" }])?;
/// assert_eq!(verified.as_str(), "PORT=8080\nPORT=3000\n");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// [`Entry::id`]: crate::Entry::id
/// [`Item::id`]: crate::Item::id
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Edit<'a> {
    /// A new literal value for one declaration.
    ///
    /// The value is written so that it reads back as exactly `value` in the
    /// document's dialect, quoted or escaped as needed. A reference such as
    /// `${HOST}` is written as typed, not expanded, and it is not escaped
    /// against later expansion, so [`Document::resolve`] can still expand it.
    /// The declaration keeps its quote style when that style can spell the
    /// value. A bare declaration gets `=` and the value. The batch fails with
    /// [`EditError::Unencodable`] when the dialect cannot spell the value in
    /// this position.
    Set {
        /// The entry ID ([`Entry::id`](crate::Entry::id)) of the declaration.
        entry: usize,
        /// The new literal value.
        value: &'a str,
    },
    /// The removal of one item, keeping the comments around it.
    ///
    /// The item's whole text is removed, including its line ending and every
    /// line of a multiline value. Any item can be removed: a declaration,
    /// comment, blank line, invalid statement or ignored statement.
    Remove {
        /// The item ID ([`Item::id`](crate::Item::id)) of the item.
        item: usize,
    },
    /// A new key for one declaration.
    ///
    /// The dialect must be able to spell the key, or the batch fails with
    /// [`EditError::InvalidKey`]. In a dialect with quoted keys, such as
    /// [`Dialect::PYTHON_DOTENV`], a quoted key keeps its quotes and a key
    /// that needs quotes gets single quotes.
    ///
    /// The new key follows the same rule as [`Edit::Insert`]: it must name
    /// exactly one declaration of the output. The batch fails with
    /// [`EditError::Duplicate`] when the document keeps another declaration of
    /// the new key or when another rename, insertion or uncommented line in the
    /// same batch declares it. Removing, commenting out or renaming away every
    /// existing declaration of the key in the same batch frees it, so a batch
    /// can swap two keys.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{Edit, EditError};
    ///
    /// let doc = adotenv::parse_strict("A=1\nB=2\n")?;
    /// assert_eq!(doc.rename("A", "B"), Err(EditError::Duplicate(2)));
    ///
    /// let swapped = doc.edit(&[
    ///     Edit::Rename { entry: 0, key: "B" },
    ///     Edit::Rename { entry: 1, key: "A" },
    /// ])?;
    /// assert_eq!(swapped.as_str(), "B=1\nA=2\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    Rename {
        /// The entry ID ([`Entry::id`](crate::Entry::id)) of the declaration.
        entry: usize,
        /// The new literal key.
        key: &'a str,
    },
    /// A new declaration of a key that is not declared yet.
    ///
    /// The new declaration has no `export` prefix, and its value is written
    /// without quotes when the dialect allows it; see [`InsertPosition`] for
    /// where it goes. The dialect must be able to spell the key and the value,
    /// or the batch fails with [`EditError::InvalidKey`] or
    /// [`EditError::Unencodable`].
    ///
    /// The new declaration must be the only declaration of the key in the
    /// output. The batch fails with [`EditError::Duplicate`] when the document
    /// keeps a declaration of the key (any occurrence, including duplicates
    /// and bare declarations) or when another insertion, rename or uncommented
    /// line in the same batch declares it. Removing, commenting out or
    /// renaming away every existing declaration of the key in the same batch
    /// frees it. Commented-out declarations are comments and never collide.
    Insert {
        /// The new literal key.
        key: &'a str,
        /// The literal value, or `None` for a bare declaration.
        value: Option<&'a str>,
        /// Where the new declaration goes in the original source.
        position: InsertPosition<'a>,
    },
    /// A `# ` prefix on every physical line of one item.
    ///
    /// Any item can be commented out. A multiline value becomes one comment
    /// item per line, so restoring it takes one [`Edit::Uncomment`] per line
    /// in one batch.
    Comment {
        /// The item ID ([`Item::id`](crate::Item::id)) of the item.
        item: usize,
    },
    /// The removal of the comment marker from one comment item.
    ///
    /// The first `#` of the comment is removed, together with one space after
    /// it if there is one. The item must be a comment, or the batch fails with
    /// [`EditError::InvalidTarget`].
    ///
    /// The uncommented text must parse as valid items that end where the
    /// selected comments end; otherwise the batch fails with
    /// [`EditError::WouldChangeMeaning`]. That happens when the text is not
    /// valid syntax, or when it opens a multiline value whose other lines stay
    /// commented out: uncomment every line of such a value in one batch.
    ///
    /// A restored declaration must be the only declaration of its key in the
    /// output, as for [`Edit::Insert`]: the batch fails with
    /// [`EditError::Duplicate`] when the document keeps another declaration of
    /// the key, unless the same batch removes, comments out or renames it
    /// away.
    Uncomment {
        /// The item ID ([`Item::id`](crate::Item::id)) of the comment.
        item: usize,
    },
    /// An added or removed `export` prefix on one declaration.
    ///
    /// Adding writes `export ` before the key, after any indentation. Removing
    /// deletes `export` and the whitespace after it. Nothing changes when the
    /// declaration already has the requested state. In a dialect without
    /// `export`, such as [`Dialect::DOCKER_RAW`], adding it fails with
    /// [`EditError::WouldChangeMeaning`].
    Export {
        /// The entry ID ([`Entry::id`](crate::Entry::id)) of the declaration.
        entry: usize,
        /// `true` to add the `export` prefix, `false` to remove it.
        enabled: bool,
    },
}

/// The reason an edit was refused.
///
/// Edits never return partial results: when any operation of a batch fails,
/// the whole batch returns one of these errors.
///
/// # Examples
///
/// ```
/// use adotenv::EditError;
///
/// let doc = adotenv::parse_strict("PORT=8080\nPORT=9090\n")?;
/// match doc.set("PORT", "3000") {
///     Err(EditError::Duplicate(count)) => assert_eq!(count, 2),
///     other => panic!("unexpected result: {other:?}"),
/// }
/// assert_eq!(doc.set("HOST", "localhost"), Err(EditError::Missing));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditError {
    /// No declaration has the requested key.
    ///
    /// Returned by the methods that select a declaration by key, such as
    /// [`Document::set`] and [`adotenv::set`](crate::set), and for
    /// [`InsertPosition::AfterKey`]. Invalid statements are not declarations,
    /// so a key that appears only on an invalid line is missing.
    Missing,
    /// The key has this many declarations where exactly one is needed.
    ///
    /// Either a method that selects a declaration by key, or
    /// [`InsertPosition::AfterKey`], found this many declarations of the key
    /// (select one by entry ID with [`Document::edit`] instead), or an
    /// inserted or renamed key would have this many declarations in the
    /// output (see [`Edit::Insert`] and [`Edit::Rename`]). Every declaration
    /// counts, including bare ones and ones that do not take part in lookup.
    Duplicate(usize),
    /// An entry or item ID is out of range, or names the wrong kind of item.
    ///
    /// For example, [`Edit::Uncomment`] on an item that is not a comment, or
    /// an [`InsertPosition::AfterEntry`] ID past the last declaration. IDs are
    /// valid only for the document they came from.
    InvalidTarget,
    /// The dialect cannot spell the key.
    ///
    /// Returned for an inserted or renamed key that the dialect cannot write,
    /// not even in quotes where it accepts quoted keys. Examples are an empty
    /// key and a key with a space in [`Dialect::PORTABLE`].
    InvalidKey,
    /// The dialect cannot spell the value in this position.
    ///
    /// For example, a line break in a [`Dialect::DOCKER_RAW`] value, which
    /// always ends at the end of its line.
    Unencodable,
    /// Operations of one batch overlap or change the same property twice.
    ///
    /// The value, key and `export` prefix of a declaration can each be changed
    /// once per batch. An item that is removed, commented out or uncommented
    /// cannot be changed in any other way in the same batch.
    ConflictingEdits,
    /// The output, parsed again, would change unrelated source or miss the
    /// requested change.
    ///
    /// The edit was planned but failed verification. For example, uncommenting
    /// one line of a commented-out multiline value, adding `export` in a dialect
    /// without it, or removing a line so that a byte order mark on the next
    /// line would start the file.
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
            Self::InvalidTarget => {
                f.write_str("edit target is out of range or has the wrong item kind")
            }
            Self::InvalidKey => f.write_str("key cannot be represented by the selected dialect"),
            Self::Unencodable => f.write_str("value cannot be represented in the selected dialect"),
            Self::ConflictingEdits => f.write_str("conflicting operations in edit batch"),
            Self::WouldChangeMeaning => {
                f.write_str("edit would change other parts of the document or miss its change")
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
            .filter(|operation| {
                matches!(
                    **operation,
                    Edit::Insert {
                        position: InsertPosition::Sorted | InsertPosition::AfterKey(_),
                        ..
                    }
                )
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
// An inserted or renamed key must name exactly one declaration of the verified
// output. Counting the output covers existing declarations that the batch keeps,
// other insertions, renames and uncommented lines, while declarations that the
// batch removes, comments out or renames away no longer collide.
fn verify_new_keys(parsed: &Document<'_>, mut keys: Vec<&str>) -> Result<(), EditError> {
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
    /// Applies a batch of edits and verifies the complete output once.
    ///
    /// Every operation refers to this document's source and IDs, not to the
    /// output of earlier operations in the batch. The planned output is parsed
    /// again with the same dialect and checked as a whole: every requested
    /// change must come out as planned, and unrelated declarations, comments,
    /// item kinds and invalid statements must not change. The batch either
    /// succeeds completely or returns an error. The document is not modified;
    /// the result holds the new text and the text edits. An empty batch returns
    /// the source unchanged, with no text edits.
    ///
    /// Inserted, renamed and uncommented keys must each name exactly one
    /// declaration of the output; removing, commenting out or renaming away the
    /// existing declarations in the same batch frees a key. See
    /// [`Edit::Insert`], [`Edit::Rename`] and [`Edit::Uncomment`].
    ///
    /// # Errors
    ///
    /// - [`EditError::InvalidTarget`] when an entry or item ID is out of range,
    ///   or the target of [`Edit::Uncomment`] is not a comment.
    /// - [`EditError::Missing`] or [`EditError::Duplicate`] when the key of an
    ///   [`InsertPosition::AfterKey`] is not declared exactly once.
    /// - [`EditError::Duplicate`] when an inserted, renamed or uncommented key
    ///   would have more than one declaration in the output.
    /// - [`EditError::InvalidKey`] when the dialect cannot spell an inserted or
    ///   renamed key.
    /// - [`EditError::Unencodable`] when the dialect cannot spell a value in its
    ///   position.
    /// - [`EditError::ConflictingEdits`] when two operations change the same
    ///   property of one declaration, or an item that is removed, commented out
    ///   or uncommented is also changed in another way.
    /// - [`EditError::WouldChangeMeaning`] when the parsed output differs from
    ///   the plan.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{Edit, InsertPosition};
    ///
    /// let source = "# database\nDB_HOST=localhost\nDB_PORT=5432\nDEBUG=true\n";
    /// let doc = adotenv::parse_strict(source)?;
    /// let host = doc.unique("DB_HOST")?;
    /// let verified = doc.edit(&[
    ///     Edit::Set { entry: host.id(), value: "db.internal" },
    ///     Edit::Export { entry: host.id(), enabled: true },
    ///     Edit::Remove { item: doc.unique("DEBUG")?.item_id() },
    ///     Edit::Insert {
    ///         key: "DB_NAME",
    ///         value: Some("app"),
    ///         position: InsertPosition::AfterKey("DB_PORT"),
    ///     },
    /// ])?;
    /// assert_eq!(
    ///     verified.as_str(),
    ///     "# database\nexport DB_HOST=db.internal\nDB_PORT=5432\nDB_NAME=app\n"
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
        let new_keys = self.new_keys(&plans, &insertions);
        let groups =
            self.plan_insertions(insertions, &plans, &mut edits, &mut end_insertions, ending)?;
        let edits = Self::normalize_edits(edits)?;
        let output = Self::apply_edits(self.source(), &edits);
        let regions = self.expected_regions(&plans, &end_insertions, &edits, groups)?;
        self.verify_regions(&output, regions, ending, new_keys)?;
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
            Edit::Rename { entry, key } => self.plan_rename(entry, key, plans, edits)?,
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
        if separate_comment && (!token.is_empty() || !before.ends_with(char::is_whitespace)) {
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
    ) -> Result<(), EditError> {
        let entry = self.entry(id).ok_or(EditError::InvalidTarget)?;
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

    /// Keys that the batch newly declares: inserted keys and the new names of
    /// renamed declarations. The verified output must declare each only once.
    fn new_keys<'a>(&self, plans: &[ItemPlan<'a>], insertions: &[Insertion<'a>]) -> Vec<&'a str> {
        let renamed = plans.iter().enumerate().filter_map(|(index, plan)| {
            if plan.fields & 2 == 0 {
                return None;
            }
            let desired = plan.desired?;
            let original = self.item(index)?.entry()?;
            (original.key() != desired.key).then_some(desired.key)
        });
        insertions
            .iter()
            .map(|insertion| insertion.desired.key)
            .chain(renamed)
            .collect()
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
        new_keys: Vec<&str>,
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
        // Uncommented declarations follow the collision rule of insertions.
        let mut new_keys = new_keys;
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
                            || (!comments_only && current.kind() == ItemKind::Ignored)
                            || current
                                .diagnostic()
                                .is_some_and(|diagnostic| diagnostic.severity() == Severity::Error)
                            || (comments_only && current.kind() != ItemKind::Comment)
                        {
                            return Err(EditError::WouldChangeMeaning);
                        }
                        if let Some(entry) = current.entry().filter(|_| !comments_only) {
                            new_keys.push(entry.key());
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
        verify_new_keys(&parsed, new_keys)
    }

    /// Sets the literal value of the only declaration of `key`.
    ///
    /// The value is written so that it reads back as exactly `value` in the
    /// document's dialect, quoted or escaped as needed. A reference such as
    /// `${HOST}` is written as typed, not expanded, and it is not escaped
    /// against later expansion, so [`Document::resolve`] can still expand it.
    /// The declaration keeps its quote style when that style can spell the
    /// value. A bare declaration becomes an assignment. Every other part of the
    /// file stays as it is.
    ///
    /// To change one of several declarations of a key, use [`Document::edit`]
    /// with [`Edit::Set`].
    ///
    /// # Errors
    ///
    /// - [`EditError::Missing`] when no declaration has `key`.
    /// - [`EditError::Duplicate`] when `key` has more than one declaration.
    /// - [`EditError::Unencodable`] when the dialect cannot spell `value` here,
    ///   such as a line break in a [`Dialect::DOCKER_RAW`] value.
    /// - [`EditError::WouldChangeMeaning`] when the output fails verification.
    ///
    /// # Examples
    ///
    /// ```
    /// let doc = adotenv::parse_strict("# server\nPORT=8080\nHOST='localhost'\n")?;
    /// let verified = doc.set("PORT", "9090")?;
    /// assert_eq!(verified.as_str(), "# server\nPORT=9090\nHOST='localhost'\n");
    ///
    /// // The value is quoted when it needs to be; single quotes are kept.
    /// let verified = doc.set("PORT", "a #b")?;
    /// assert_eq!(verified.as_str(), "# server\nPORT=\"a #b\"\nHOST='localhost'\n");
    /// let verified = doc.set("HOST", "my host")?;
    /// assert_eq!(verified.as_str(), "# server\nPORT=8080\nHOST='my host'\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn set(&self, key: &str, value: &str) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Set {
            entry: self.unique(key)?.id(),
            value,
        }])
    }
    /// Removes the only declaration of `key`, keeping the comments around it.
    ///
    /// The whole declaration is removed, including its line ending and every
    /// line of a multiline value. To remove a comment, a blank line or one of
    /// several declarations of a key, use [`Document::edit`] with
    /// [`Edit::Remove`].
    ///
    /// # Errors
    ///
    /// - [`EditError::Missing`] when no declaration has `key`.
    /// - [`EditError::Duplicate`] when `key` has more than one declaration.
    /// - [`EditError::WouldChangeMeaning`] when the output fails verification,
    ///   for example when a byte order mark on the next line would start the
    ///   file.
    ///
    /// # Examples
    ///
    /// ```
    /// let source = "# database\nDATABASE_URL=postgres://localhost/app\nPORT=8080\n";
    /// let doc = adotenv::parse_strict(source)?;
    /// let verified = doc.remove("DATABASE_URL")?;
    /// assert_eq!(verified.as_str(), "# database\nPORT=8080\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn remove(&self, key: &str) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Remove {
            item: self.unique(key)?.item_id(),
        }])
    }
    /// Renames the only declaration of `key` to `new_key`.
    ///
    /// Only the key changes; the value, `export` prefix, spacing and comments
    /// stay. The new key must not be declared elsewhere in the document. To
    /// swap two keys or rename one of several declarations, use
    /// [`Document::edit`] with [`Edit::Rename`].
    ///
    /// # Errors
    ///
    /// - [`EditError::Missing`] when no declaration has `key`.
    /// - [`EditError::Duplicate`] when `key` has more than one declaration, or
    ///   when `new_key` is already declared.
    /// - [`EditError::InvalidKey`] when the dialect cannot spell `new_key`.
    /// - [`EditError::WouldChangeMeaning`] when the output fails verification.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::EditError;
    ///
    /// let source = "export DB_URL=postgres://localhost/app # main\nPORT=8080\n";
    /// let doc = adotenv::parse_strict(source)?;
    /// let verified = doc.rename("DB_URL", "DATABASE_URL")?;
    /// assert_eq!(
    ///     verified.as_str(),
    ///     "export DATABASE_URL=postgres://localhost/app # main\nPORT=8080\n"
    /// );
    /// assert_eq!(doc.rename("DB_URL", "PORT"), Err(EditError::Duplicate(2)));
    /// assert_eq!(doc.rename("DB_URL", "DB URL"), Err(EditError::InvalidKey));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn rename(&self, key: &str, new_key: &str) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Rename {
            entry: self.unique(key)?.id(),
            key: new_key,
        }])
    }
    /// Inserts a declaration of a key that the document does not declare yet.
    ///
    /// `value` is the literal value, or `None` for a bare declaration (`KEY`
    /// without `=`). The new declaration has no `export` prefix, and its value
    /// is written without quotes when the dialect allows it. `position` says
    /// where it goes; see [`InsertPosition`]. Every existing declaration of
    /// `key` counts, including duplicates and bare declarations, while a
    /// commented-out declaration does not. To replace a key in one step,
    /// remove its declarations and insert it in one [`Document::edit`] batch.
    ///
    /// # Errors
    ///
    /// - [`EditError::InvalidKey`] when the dialect cannot spell `key`.
    /// - [`EditError::Duplicate`] when the document already declares `key`;
    ///   the count includes the new declaration.
    /// - [`EditError::Missing`] or [`EditError::Duplicate`] when the key of an
    ///   [`InsertPosition::AfterKey`] is not declared exactly once.
    /// - [`EditError::InvalidTarget`] when an [`InsertPosition::AfterEntry`] ID
    ///   is out of range.
    /// - [`EditError::Unencodable`] when the dialect cannot spell `value` here.
    /// - [`EditError::WouldChangeMeaning`] when the output fails verification,
    ///   for example when the new text would close an unterminated quote on an
    ///   earlier line.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{EditError, InsertPosition};
    ///
    /// let doc = adotenv::parse_strict("PORT=8080\nHOST=localhost\n")?;
    /// let inserted = doc.insert("DEBUG", Some("true"), InsertPosition::End)?;
    /// assert_eq!(inserted.as_str(), "PORT=8080\nHOST=localhost\nDEBUG=true\n");
    ///
    /// assert_eq!(
    ///     doc.insert("PORT", Some("9090"), InsertPosition::End),
    ///     Err(EditError::Duplicate(2))
    /// );
    /// # Ok::<(), Box<dyn std::error::Error>>(())
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
    /// Comments out the only declaration of `key`, line by line.
    ///
    /// Every physical line of the declaration gets a `# ` prefix, so a
    /// multiline value becomes one comment item per line. Restore it with
    /// [`Document::uncomment`], or with one [`Edit::Uncomment`] per line in one
    /// batch.
    ///
    /// # Errors
    ///
    /// - [`EditError::Missing`] when no declaration has `key`.
    /// - [`EditError::Duplicate`] when `key` has more than one declaration.
    /// - [`EditError::WouldChangeMeaning`] when the output fails verification.
    ///
    /// # Examples
    ///
    /// ```
    /// let doc = adotenv::parse_strict("DEBUG=true\nKEY='line one\nline two'\n")?;
    /// assert_eq!(doc.comment("DEBUG")?.as_str(), "# DEBUG=true\nKEY='line one\nline two'\n");
    /// assert_eq!(doc.comment("KEY")?.as_str(), "DEBUG=true\n# KEY='line one\n# line two'\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn comment(&self, key: &str) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Comment {
            item: self.unique(key)?.item_id(),
        }])
    }
    /// Removes the comment marker from the comment item with ID `item`.
    ///
    /// The first `#` of the comment is removed, together with one space after
    /// it if there is one. `item` is an item ID ([`Item::id`]); comments have
    /// no key to select them by. A commented-out multiline value spans several
    /// comment items: uncomment all of them in one [`Document::edit`] batch of
    /// [`Edit::Uncomment`] operations.
    ///
    /// The restored declaration must be the only declaration of its key in the
    /// output: uncommenting `# PORT=8080` while `PORT=3000` stays fails. To
    /// swap the two, remove or comment out the other declaration in the same
    /// [`Document::edit`] batch.
    ///
    /// # Errors
    ///
    /// - [`EditError::InvalidTarget`] when `item` is out of range or is not a
    ///   comment.
    /// - [`EditError::Duplicate`] when the document still declares the key of
    ///   the restored declaration.
    /// - [`EditError::WouldChangeMeaning`] when the uncommented text is not
    ///   valid syntax or would continue onto a line that stays commented out,
    ///   such as the first line of a multiline value.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{Edit, EditError};
    ///
    /// let doc = adotenv::parse_strict("# DEBUG=true\n# KEY='line one\n# line two'\n")?;
    /// assert_eq!(doc.uncomment(0)?.as_str(), "DEBUG=true\n# KEY='line one\n# line two'\n");
    ///
    /// // One line of a multiline value cannot be restored on its own.
    /// assert_eq!(doc.uncomment(1), Err(EditError::WouldChangeMeaning));
    /// let restored = doc.edit(&[Edit::Uncomment { item: 1 }, Edit::Uncomment { item: 2 }])?;
    /// assert_eq!(restored.as_str(), "# DEBUG=true\nKEY='line one\nline two'\n");
    ///
    /// // A restored key must not be declared twice.
    /// let doc = adotenv::parse_strict("PORT=3000\n# PORT=8080\n")?;
    /// assert_eq!(doc.uncomment(1), Err(EditError::Duplicate(2)));
    /// let swapped = doc.edit(&[Edit::Comment { item: 0 }, Edit::Uncomment { item: 1 }])?;
    /// assert_eq!(swapped.as_str(), "# PORT=3000\nPORT=8080\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// [`Item::id`]: crate::Item::id
    pub fn uncomment(&self, item: usize) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Uncomment { item }])
    }
    /// Adds or removes the `export` prefix of the only declaration of `key`.
    ///
    /// Adding writes `export ` before the key, after any indentation. Removing
    /// deletes `export` and the whitespace after it. Nothing changes when the
    /// declaration already has the requested state.
    ///
    /// # Errors
    ///
    /// - [`EditError::Missing`] when no declaration has `key`.
    /// - [`EditError::Duplicate`] when `key` has more than one declaration.
    /// - [`EditError::WouldChangeMeaning`] when the output fails verification,
    ///   for example when adding `export` in a dialect without it, such as
    ///   [`Dialect::DOCKER_RAW`].
    ///
    /// # Examples
    ///
    /// ```
    /// let doc = adotenv::parse_strict("PORT=8080\nexport HOST=localhost\n")?;
    /// let exported = doc.set_export("PORT", true)?;
    /// assert_eq!(exported.as_str(), "export PORT=8080\nexport HOST=localhost\n");
    /// let plain = doc.set_export("HOST", false)?;
    /// assert_eq!(plain.as_str(), "PORT=8080\nHOST=localhost\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn set_export(&self, key: &str, enabled: bool) -> Result<VerifiedEdits, EditError> {
        self.edit(&[Edit::Export {
            entry: self.unique(key)?.id(),
            enabled,
        }])
    }
}

/// Sets the literal value of the only declaration of `key` in `text`.
///
/// Parses `text` with the recovering [`Dialect::PORTABLE`] parser, as
/// [`parse`](crate::parse) does, applies [`Document::set`] and returns the
/// verified output. Invalid lines are allowed and stay as they are, but their
/// reading must not change. For another dialect, or to make several changes,
/// parse the text yourself and use the [`Document`] methods.
///
/// # Errors
///
/// - [`EditError::Missing`] when no declaration has `key`; a key on an invalid
///   line is not a declaration.
/// - [`EditError::Duplicate`] when `key` has more than one declaration.
/// - [`EditError::Unencodable`] when Portable cannot spell `value` here.
/// - [`EditError::WouldChangeMeaning`] when the output fails verification.
///
/// # Examples
///
/// ```
/// let text = "BAD LINE\nPORT=8080\n";
/// assert_eq!(adotenv::set(text, "PORT", "9090")?, "BAD LINE\nPORT=9090\n");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn set(text: &str, key: &str, value: &str) -> Result<String, EditError> {
    crate::parser::parse_document(text, Dialect::PORTABLE)
        .set(key, value)
        .map(VerifiedEdits::into_string)
}
/// Removes the only declaration of `key` from `text`, keeping the comments
/// around it.
///
/// Parses `text` with the recovering [`Dialect::PORTABLE`] parser, as
/// [`parse`](crate::parse) does, applies [`Document::remove`] and returns the
/// verified output. Invalid lines are allowed and stay as they are, but their
/// reading must not change. For another dialect, or to make several changes,
/// parse the text yourself and use the [`Document`] methods.
///
/// # Errors
///
/// - [`EditError::Missing`] when no declaration has `key`; a key on an invalid
///   line is not a declaration.
/// - [`EditError::Duplicate`] when `key` has more than one declaration.
/// - [`EditError::WouldChangeMeaning`] when the output fails verification,
///   for example when a byte order mark on the next line would start the
///   file.
///
/// # Examples
///
/// ```
/// let text = "# server\nPORT=8080\nBAD LINE\n";
/// assert_eq!(adotenv::remove(text, "PORT")?, "# server\nBAD LINE\n");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn remove(text: &str, key: &str) -> Result<String, EditError> {
    crate::parser::parse_document(text, Dialect::PORTABLE)
        .remove(key)
        .map(VerifiedEdits::into_string)
}
