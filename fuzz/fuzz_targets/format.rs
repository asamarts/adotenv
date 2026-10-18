#![no_main]
#![forbid(unsafe_code)]

use adotenv::{
    parse_with, BlankLines, Dialect, EqualsSpacing, FinalNewline, FormatOptions, InlineComments,
    ItemKind, TextEdit,
};
use libfuzzer_sys::fuzz_target;

fn trim_horizontal(text: &str) -> &str {
    text.trim_end_matches([' ', '\t'])
}

fn check_edit(source: &str, edit: &TextEdit, previous_end: &mut usize) {
    assert!(edit.range.start <= edit.range.end);
    assert!(edit.range.start >= *previous_end);
    assert!(source.get(edit.range.clone()).is_some());
    assert!(source.is_char_boundary(edit.range.start));
    assert!(source.is_char_boundary(edit.range.end));
    *previous_end = edit.range.end;
}

fn check_structure(source: &str, document: &adotenv::Document<'_>) {
    let mut previous_group_end = document.bom_span().map_or(0, |span| span.end);
    let mut grouped = 0usize;
    for group in document.groups() {
        let span = group.span();
        assert!(span.start >= previous_group_end);
        assert!(span.start < span.end);
        assert!(source.get(span.clone()).is_some());
        previous_group_end = span.end;
        let mut cursor = span.start;
        for item in group.items() {
            assert_ne!(item.kind(), ItemKind::Blank);
            assert_eq!(item.span().start, cursor);
            cursor = item.span().end;
            grouped += 1;
        }
        assert_eq!(cursor, span.end);
        let _ = group.comments().count();
        let _ = group.leading_comments().count();
        let _ = group.entries().count();
    }
    assert_eq!(
        grouped,
        document
            .items()
            .filter(|item| item.kind() != ItemKind::Blank)
            .count()
    );
    if let Some(header) = document.header() {
        let first_entry = document
            .items()
            .position(|item| item.kind() == ItemKind::Entry)
            .unwrap_or(document.items().len());
        assert_eq!(header.items().count(), first_entry);
        assert_eq!(header.entries().count(), 0);
    }
    for entry in document.entries() {
        let comments: Vec<_> = entry.leading_comments().collect();
        for (index, comment) in comments.iter().enumerate() {
            assert_eq!(comment.kind(), ItemKind::Comment);
            if index != 0 {
                assert_eq!(comments[index - 1].id() + 1, comment.id());
            }
        }
        if let Some(last) = comments.last() {
            assert_eq!(last.id() + 1, entry.item_id());
        }
    }
}

fn check_compare(source: &str, document: &adotenv::Document<'_>) {
    assert!(document.compare(document).is_empty());
    let (again, _) = parse_with(source, document.dialect());
    assert!(document.compare(&again).is_empty());
}

fn check_example(document: &adotenv::Document<'_>) {
    let example = document.example();
    let (after, _) = parse_with(&example, document.dialect());
    assert_eq!(document.has_bom(), after.has_bom());
    let expected: Vec<_> = document
        .entries()
        .filter(|entry| !entry.key().is_empty())
        .collect();
    assert_eq!(expected.len(), after.entries().len());
    for (left, right) in expected.into_iter().zip(after.entries()) {
        assert_eq!(left.key(), right.key());
        if left.state() == adotenv::AssignmentState::Bare {
            assert_eq!(right.state(), adotenv::AssignmentState::Bare);
            assert_eq!(right.value(), None);
        } else {
            assert_eq!(right.state(), adotenv::AssignmentState::Empty);
            assert_eq!(right.value(), Some(""));
        }
        if let Some(comment) = left.inline_comment() {
            if document.dialect().inline_comments == InlineComments::Space {
                assert!(example.contains(comment));
            } else {
                assert_eq!(right.inline_comment(), Some(comment));
            }
        }
    }
    assert!(!after
        .items()
        .any(|item| matches!(item.kind(), ItemKind::Invalid | ItemKind::Ignored)));
    for item in document
        .items()
        .filter(|item| item.kind() == ItemKind::Comment)
    {
        assert!(example.contains(item.raw()));
    }
}

fn check_result(source: &str, document: &adotenv::Document<'_>, options: &FormatOptions) {
    let Ok(formatted) = document.format(options) else {
        return;
    };
    let mut previous_end = 0;
    for edit in formatted.edits() {
        check_edit(source, edit, &mut previous_end);
    }
    let (after, _) = parse_with(formatted.as_str(), document.dialect());
    assert_eq!(document.has_bom(), after.has_bom());
    if options.blank_lines == BlankLines::Preserve {
        assert_eq!(document.items().len(), after.items().len());
    } else {
        assert_eq!(document.groups().count(), after.groups().count());
        assert!(!after
            .items()
            .zip(after.items().skip(1))
            .any(|(left, right)| {
                left.kind() == ItemKind::Blank && right.kind() == ItemKind::Blank
            }));
    }
    assert_eq!(
        document
            .items()
            .filter(|item| item.kind() != ItemKind::Blank)
            .count(),
        after
            .items()
            .filter(|item| item.kind() != ItemKind::Blank)
            .count()
    );
    let before_items = document
        .items()
        .filter(|item| item.kind() != ItemKind::Blank);
    let after_items = after.items().filter(|item| item.kind() != ItemKind::Blank);
    for (left, right) in before_items.zip(after_items) {
        assert_eq!(left.kind(), right.kind());
        if left.kind() == ItemKind::Comment {
            assert_eq!(
                trim_horizontal(&source[left.content_span()]),
                trim_horizontal(&formatted.as_str()[right.content_span()])
            );
        }
        if matches!(left.kind(), ItemKind::Invalid | ItemKind::Ignored) {
            assert_eq!(
                &source[left.content_span()],
                &formatted.as_str()[right.content_span()]
            );
        }
    }
    assert_eq!(document.entries().len(), after.entries().len());
    for (left, right) in document.entries().zip(after.entries()) {
        assert_eq!(left.key(), right.key());
        assert_eq!(left.value(), right.value());
        assert_eq!(left.state(), right.state());
        assert_eq!(left.quote(), right.quote());
        assert_eq!(left.exported(), right.exported());
        assert_eq!(
            left.participates_in_lookup(),
            right.participates_in_lookup()
        );
        assert_eq!(
            left.inline_comment().map(trim_horizontal),
            right.inline_comment().map(trim_horizontal)
        );
    }
    let invalid = |document: &adotenv::Document<'_>| {
        document
            .items()
            .filter(|item| matches!(item.kind(), ItemKind::Invalid | ItemKind::Ignored))
            .count()
    };
    assert_eq!(invalid(document), invalid(&after));

    let mut replayed = String::new();
    let mut cursor = 0;
    for edit in formatted.edits() {
        replayed.push_str(&source[cursor..edit.range.start]);
        replayed.push_str(&edit.replacement);
        cursor = edit.range.end;
    }
    replayed.push_str(&source[cursor..]);
    assert_eq!(replayed, formatted.as_str());

    if options.final_newline != FinalNewline::Remove {
        let again = after
            .format(options)
            .expect("verified formatting is representable again");
        assert!(again.is_empty());
        assert_eq!(again.as_str(), formatted.as_str());
    }
}

fuzz_target!(|bytes: &[u8]| {
    let Ok(source) = std::str::from_utf8(bytes) else {
        return;
    };
    let options = [
        FormatOptions::normalize(),
        FormatOptions::normalize_groups(),
        FormatOptions {
            equals_spacing: EqualsSpacing::Spaces,
            trim_trailing_whitespace: false,
            blank_lines: BlankLines::Preserve,
            final_newline: FinalNewline::Preserve,
        },
        FormatOptions {
            equals_spacing: EqualsSpacing::None,
            trim_trailing_whitespace: true,
            blank_lines: BlankLines::Preserve,
            final_newline: FinalNewline::Ensure,
        },
        FormatOptions {
            equals_spacing: EqualsSpacing::Spaces,
            trim_trailing_whitespace: true,
            blank_lines: BlankLines::Collapse,
            final_newline: FinalNewline::Remove,
        },
    ];
    for dialect in [
        Dialect::PORTABLE,
        Dialect::NODE_DOTENV,
        Dialect::PYTHON_DOTENV,
        Dialect::COMPOSE,
        Dialect::DOCKER_RAW,
    ] {
        let (document, _) = parse_with(source, dialect);
        check_structure(source, &document);
        check_compare(source, &document);
        check_example(&document);
        for options in &options {
            check_result(source, &document, options);
        }
    }
});
