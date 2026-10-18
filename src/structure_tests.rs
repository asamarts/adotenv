use alloc::{string::String, vec::Vec};
use proptest::prelude::*;

use crate::{parse, ItemKind};

#[test]
fn groups_split_on_blank_lines_and_keep_comments() {
    let source = "# header\n# second\n\n# group\nA=1\n# A note\nB=2\n\nC=3\n";
    let (doc, _) = parse(source);
    let groups: Vec<_> = doc.groups().collect();
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].items().count(), 2);
    assert_eq!(groups[0].entries().count(), 0);
    assert_eq!(groups[1].items().count(), 4);
    assert_eq!(groups[1].entries().count(), 2);
    assert_eq!(groups[2].items().count(), 1);
    assert_eq!(groups[2].entries().count(), 1);
    assert_eq!(groups[1].leading_comments().count(), 1);
    assert_eq!(&source[groups[1].span()], "# group\nA=1\n# A note\nB=2\n");
    let b = doc.unique("B").unwrap();
    let comments: Vec<_> = b.leading_comments().collect();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].raw(), "# A note\n");
    let a = doc.unique("A").unwrap();
    let comments: Vec<_> = a.leading_comments().collect();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].raw(), "# group\n");
}

#[test]
fn header_is_the_structural_prefix_before_the_first_entry() {
    let source = "# header\n\n# more\nA=1\n";
    let (doc, _) = parse(source);
    let header = doc.header().unwrap();
    assert_eq!(header.items().count(), 3);
    assert_eq!(header.entries().count(), 0);
    assert_eq!(&source[header.span()], "# header\n\n# more\n");
    let leading: Vec<_> = doc.unique("A").unwrap().leading_comments().collect();
    assert_eq!(leading.len(), 1);
    assert_eq!(leading[0].raw(), "# more\n");

    let (doc, _) = parse("A=1\n");
    assert!(doc.header().is_none());
    let (doc, _) = parse("");
    assert!(doc.header().is_none());
    assert_eq!(doc.groups().count(), 0);
}

#[test]
fn blank_lines_break_comment_attachment() {
    let source = "# detached\n\n# attached\nA=1\n";
    let (doc, _) = parse(source);
    let comments: Vec<_> = doc.unique("A").unwrap().leading_comments().collect();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].raw(), "# attached\n");
}

#[test]
fn multiline_entries_stay_inside_one_group() {
    let source = "A=\"one\nB=two\"\nC=x\n\nD=y\n";
    let (doc, _) = parse(source);
    let groups: Vec<_> = doc.groups().collect();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].items().count(), 2);
    assert_eq!(groups[0].entries().count(), 2);
    assert_eq!(groups[1].entries().count(), 1);
}

#[test]
fn bom_prelude_is_not_part_of_the_header_view() {
    let source = "\u{feff}# header\nA=1\n";
    let (doc, _) = parse(source);
    let header = doc.header().unwrap();
    assert_eq!(header.items().count(), 1);
    assert_eq!(header.items().next().unwrap().raw(), "# header\n");
    assert!(!header.span().contains(&0));
}

proptest! {
    #[test]
    fn groups_partition_nonblank_items_and_spans_are_ordered(
        chars in prop::collection::vec(
            prop_oneof![
                Just('A'), Just('='), Just('1'), Just('#'), Just(' '), Just('\t'),
                Just('\n'), Just('\r'), Just('"'), any::<char>(),
            ],
            0..200,
        )
    ) {
        let source: String = chars.into_iter().collect();
        let (doc, _) = parse(&source);
        let groups: Vec<_> = doc.groups().collect();
        let mut expected = 0usize;
        let mut previous_end = 0usize;
        for group in groups {
            let span = group.span();
            prop_assert!(span.start >= previous_end);
            prop_assert!(span.start < span.end);
            previous_end = span.end;
            let mut count = 0usize;
            for item in group.items() {
                prop_assert!(item.kind() != ItemKind::Blank);
                count += 1;
            }
            prop_assert!(count > 0);
            expected += count;
        }
        let nonblank = doc.items().filter(|item| item.kind() != ItemKind::Blank).count();
        prop_assert_eq!(expected, nonblank);
        for entry in doc.entries() {
            let comments: Vec<_> = entry.leading_comments().collect();
            for (index, comment) in comments.iter().enumerate() {
                prop_assert_eq!(comment.kind(), ItemKind::Comment);
                if index != 0 {
                    prop_assert!(comments[index - 1].id() + 1 == comment.id());
                }
            }
            if let Some(comment) = comments.last() {
                prop_assert!(comment.id() + 1 == entry.item_id());
            }
        }
    }
}
