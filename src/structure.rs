//! Comment and blank-line structure over the lossless item model.

use core::ops::Range;

use crate::{Document, Entry, Item, ItemKind};

/// A contiguous run of nonblank items, separated by blank lines.
///
/// A group may contain comments, declarations and malformed statements. Blank
/// items are separators and are not part of any group.
#[derive(Debug, Clone, Copy)]
pub struct Group<'doc> {
    document: &'doc Document<'doc>,
    start: usize,
    end: usize,
}

impl<'doc> Group<'doc> {
    /// Items in source order.
    ///
    /// # Panics
    /// Panics only if an internal document-snapshot invariant is broken; groups
    /// are always created from valid item ranges.
    #[must_use]
    pub fn items(self) -> impl DoubleEndedIterator<Item = Item<'doc>> + ExactSizeIterator + 'doc {
        (self.start..self.end).map(move |index| {
            self.document
                .item(index)
                .expect("group item belongs to document")
        })
    }
    /// Declarations in source order.
    pub fn entries(self) -> impl Iterator<Item = Entry<'doc>> + 'doc {
        self.items().filter_map(Item::entry)
    }
    /// Whole-line comments in source order.
    pub fn comments(self) -> impl Iterator<Item = Item<'doc>> + 'doc {
        self.items().filter(|item| item.kind() == ItemKind::Comment)
    }
    /// Contiguous comments at the start of this group, before its first entry.
    pub fn leading_comments(self) -> impl Iterator<Item = Item<'doc>> + 'doc {
        self.items()
            .take_while(|item| item.kind() == ItemKind::Comment)
    }
    /// Exact source range from the first item's start to the last item's end.
    ///
    /// # Panics
    /// Panics only if an internal document-snapshot invariant is broken; groups
    /// are never empty.
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

/// Blank-line-separated groups in source order.
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
    /// Blank-line-separated groups in source order.
    #[must_use]
    pub fn groups(&self) -> Groups<'_> {
        Groups {
            document: self,
            next: 0,
        }
    }

    /// Items before the first declaration, when any exist.
    ///
    /// This is a structural header view, not a separate syntax kind. It can
    /// contain comments and blank lines and excludes the BOM prelude.
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
    /// Contiguous whole-line comments immediately before this declaration.
    ///
    /// A blank line breaks attachment. Inline comments are available through
    /// [`Entry::inline_comment`](crate::Entry::inline_comment).
    ///
    /// # Panics
    /// Panics only if an internal document-snapshot invariant is broken; leading
    /// comment indexes are derived from the same document.
    #[must_use]
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
