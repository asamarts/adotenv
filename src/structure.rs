//! Comment and blank-line structure over the lossless item model.

use core::ops::Range;

use crate::{Document, Entry, Item, ItemKind};

/// A run of items between blank lines.
///
/// [`Document::groups`] splits a document's items at blank lines. A group
/// holds one run of comments, entries and invalid or ignored statements with
/// no blank line between them; the blank lines themselves belong to no group.
/// A multiline value stays in one group, even if it contains empty lines.
/// [`Document::header`] also returns a `Group`, and is the exception: it can
/// contain blank lines.
///
/// A group is a small `Copy` view of the document's items; it does not move or
/// change any text. [`Document::sort`] sorts the declarations within each
/// group.
///
/// # Examples
///
/// ```
/// let source = "# database\nDB_HOST=localhost\nDB_PORT=5432\n\n# cache\nCACHE_TTL=60\n";
/// let (doc, _) = adotenv::parse(source);
/// let groups: Vec<_> = doc.groups().collect();
///
/// assert_eq!(groups.len(), 2);
/// let keys: Vec<_> = groups[0].entries().map(|entry| entry.key()).collect();
/// assert_eq!(keys, ["DB_HOST", "DB_PORT"]);
/// assert_eq!(&source[groups[1].span()], "# cache\nCACHE_TTL=60\n");
///
/// let title: Vec<_> = groups[1]
///     .leading_comments()
///     .filter_map(|item| item.comment_text())
///     .collect();
/// assert_eq!(title, [" cache"]);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Group<'doc> {
    document: &'doc Document<'doc>,
    start: usize,
    end: usize,
}

impl<'doc> Group<'doc> {
    /// Returns an iterator over the group's items in source order.
    ///
    /// # Panics
    ///
    /// Panics only if an internal invariant is broken; a group always refers
    /// to items of its own document.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn items(self) -> impl DoubleEndedIterator<Item = Item<'doc>> + ExactSizeIterator + 'doc {
        (self.start..self.end).map(move |index| {
            self.document
                .item(index)
                .expect("group item belongs to document")
        })
    }
    /// Returns an iterator over the group's declarations in source order.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn entries(self) -> impl Iterator<Item = Entry<'doc>> + 'doc {
        self.items().filter_map(Item::entry)
    }
    /// Returns an iterator over the group's whole-line comments in source
    /// order.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn comments(self) -> impl Iterator<Item = Item<'doc>> + 'doc {
        self.items().filter(|item| item.kind() == ItemKind::Comment)
    }
    /// Returns an iterator over the comments at the start of the group.
    ///
    /// The run ends at the first item that is not a comment, such as the first
    /// entry or an invalid statement. These comments usually describe the
    /// whole group.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn leading_comments(self) -> impl Iterator<Item = Item<'doc>> + 'doc {
        self.items()
            .take_while(|item| item.kind() == ItemKind::Comment)
    }
    /// Returns the byte range from the start of the group's first item to the
    /// end of its last item, including that item's line ending.
    ///
    /// # Panics
    ///
    /// Panics only if an internal invariant is broken; a group is never empty.
    #[must_use]
    pub fn span(self) -> Range<usize> {
        let start = self
            .document
            .item(self.start)
            .expect("nonempty group")
            .span()
            .start;
        let end = self
            .document
            .item(self.end - 1)
            .expect("nonempty group")
            .span()
            .end;
        start..end
    }
}

/// An iterator over the [`Group`]s of a document, in source order.
///
/// Returned by [`Document::groups`].
#[derive(Debug, Clone)]
pub struct Groups<'doc> {
    document: &'doc Document<'doc>,
    next: usize,
}

impl<'doc> Iterator for Groups<'doc> {
    type Item = Group<'doc>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next < self.document.items().len()
            && self
                .document
                .item(self.next)
                .expect("group cursor belongs to document")
                .kind()
                == ItemKind::Blank
        {
            self.next += 1;
        }
        if self.next >= self.document.items().len() {
            return None;
        }
        let start = self.next;
        while self.next < self.document.items().len()
            && self
                .document
                .item(self.next)
                .expect("group cursor belongs to document")
                .kind()
                != ItemKind::Blank
        {
            self.next += 1;
        }
        Some(Group {
            document: self.document,
            start,
            end: self.next,
        })
    }
}

impl Document<'_> {
    /// Returns an iterator over the groups of items between blank lines.
    ///
    /// Each [`Group`] is a run of items with no blank line in it. A document
    /// with no items, or only blank lines, has no groups. See [`Group`] for an
    /// example.
    #[must_use]
    pub fn groups(&self) -> Groups<'_> {
        Groups {
            document: self,
            next: 0,
        }
    }

    /// Returns the items before the first entry, or `None` if there are none.
    ///
    /// The header is the text at the top of the file: comments, blank lines and
    /// invalid or ignored statements up to the first declaration. Unlike the
    /// groups from [`Document::groups`], it can contain blank lines. It
    /// includes the comments directly above the first entry, which
    /// [`Entry::leading_comments`] also returns. A document without entries is
    /// all header. A stripped byte-order mark is not part of it.
    ///
    /// # Examples
    ///
    /// ```
    /// let source = "# Settings for the web app\n\n# database\nDB_HOST=localhost\n";
    /// let (doc, _) = adotenv::parse(source);
    /// let header = doc.header().unwrap();
    ///
    /// assert_eq!(&source[header.span()], "# Settings for the web app\n\n# database\n");
    /// assert_eq!(header.items().count(), 3);
    ///
    /// let (doc, _) = adotenv::parse("PORT=8080\n");
    /// assert!(doc.header().is_none());
    /// ```
    #[must_use]
    pub fn header(&self) -> Option<Group<'_>> {
        let end = self
            .items()
            .position(|item| item.kind() == ItemKind::Entry)
            .unwrap_or(self.items().len());
        (end != 0).then_some(Group {
            document: self,
            start: 0,
            end,
        })
    }
}

impl<'doc> Entry<'doc> {
    /// Returns an iterator over the whole-line comments directly above this
    /// declaration, in source order.
    ///
    /// The run of comments ends at the first item above that is not a
    /// comment: a blank line, an entry or an invalid statement. So a comment
    /// separated from the declaration by a blank line is not attached to it.
    /// The inline comment on the declaration's own line is available through
    /// [`Entry::inline_comment`].
    ///
    /// # Panics
    ///
    /// Panics only if an internal invariant is broken; the comments always
    /// come from the declaration's own document.
    ///
    /// # Examples
    ///
    /// ```
    /// let source = "# detached\n\n# HTTP port\nPORT=8080 # default\n";
    /// let (doc, _) = adotenv::parse(source);
    /// let entry = doc.get_entry("PORT").unwrap();
    ///
    /// let comments: Vec<_> = entry.leading_comments().map(|item| item.raw()).collect();
    /// assert_eq!(comments, ["# HTTP port\n"]);
    /// assert_eq!(entry.inline_comment(), Some("# default"));
    /// ```
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn leading_comments(
        self,
    ) -> impl DoubleEndedIterator<Item = Item<'doc>> + ExactSizeIterator + 'doc {
        let end = self.item_id();
        let mut start = end;
        while start != 0 {
            let Some(previous) = self.document.item(start - 1) else {
                break;
            };
            if previous.kind() != ItemKind::Comment {
                break;
            }
            start -= 1;
        }
        (start..end).map(move |index| {
            self.document
                .item(index)
                .expect("leading comment belongs to document")
        })
    }
}
