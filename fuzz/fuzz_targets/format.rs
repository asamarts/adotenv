#![no_main]
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use adotenv::{
    parse_with, BlankLines, Dialect, EqualsSpacing, ExpandOptions, ExpandScope, FinalNewline,
    FormatOptions, InlineComments, ItemKind, QuoteStyle, TextEdit,
};
use libfuzzer_sys::fuzz_target;

type GroupValues = BTreeMap<(usize, String), Vec<Option<String>>>;

// Resolved values keyed by group and key; sorting moves entries only within
// their group.
fn resolved_by_group(
    document: &adotenv::Document<'_>,
    options: &ExpandOptions,
) -> Result<GroupValues, adotenv::ResolveError> {
    let resolved = document.resolve(|_| None, options)?;
    let mut values = GroupValues::new();
    for (group, items) in document.groups().enumerate() {
        for entry in items.entries() {
            values
                .entry((group, entry.key().to_owned()))
                .or_default()
                .push(resolved.entries()[entry.id()].value.clone());
        }
    }
    Ok(values)
}

// Sorting must not change which declaration a reference sees. Whole-file
// resolution caches dependencies in source order, so only its successful
// results are compared: a depth limit can depend on that order.
fn check_sort_resolution(document: &adotenv::Document<'_>, after: &adotenv::Document<'_>) {
    for base in [ExpandOptions::portable(), ExpandOptions::compose()] {
        for scope in [ExpandScope::Sequential, ExpandScope::WholeFile] {
            let options = base
                .with_scope(scope)
                .with_max_depth(8)
                .with_max_value_bytes(1024);
            match (
                resolved_by_group(document, &options),
                resolved_by_group(after, &options),
            ) {
                (Ok(before), Ok(sorted)) => assert_eq!(before, sorted, "{scope:?}"),
                (before, sorted) if scope == ExpandScope::Sequential => {
                    assert_eq!(before.is_ok(), sorted.is_ok(), "{scope:?}");
                }
                _ => {}
            }
        }
    }
}

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

fn check_sort(document: &adotenv::Document<'_>) {
    let Ok(sorted) = document.sort() else {
        return;
    };
    let (after, _) = parse_with(sorted.as_str(), document.dialect());
    assert_eq!(document.groups().count(), after.groups().count());
    assert_eq!(document.entries().len(), after.entries().len());
    for (before, after) in document.groups().zip(after.groups()) {
        let mut left: Vec<_> = before
            .entries()
            .map(|entry| {
                format!(
                    "{:?}|{:?}|{:?}|{:?}|{:?}",
                    entry.key(),
                    entry.value(),
                    entry.state(),
                    entry.quote(),
                    entry.exported()
                )
            })
            .collect();
        let mut right: Vec<_> = after
            .entries()
            .map(|entry| {
                format!(
                    "{:?}|{:?}|{:?}|{:?}|{:?}",
                    entry.key(),
                    entry.value(),
                    entry.state(),
                    entry.quote(),
                    entry.exported()
                )
            })
            .collect();
        left.sort_unstable();
        right.sort_unstable();
        assert_eq!(left, right);
        let mut left_comments: Vec<_> = before.comments().map(|item| item.raw()).collect();
        let mut right_comments: Vec<_> = after.comments().map(|item| item.raw()).collect();
        left_comments.sort_unstable();
        right_comments.sort_unstable();
        assert_eq!(left_comments, right_comments);
    }
    if !sorted.is_empty() {
        check_sort_resolution(document, &after);
    }
    let again = after.sort().expect("verified sort is representable again");
    assert!(again.is_empty());
    assert_eq!(again.as_str(), sorted.as_str());
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
        if options.quote_style == QuoteStyle::Preserve {
            assert_eq!(left.quote(), right.quote());
        }
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
    let mut reversed = source.to_owned();
    for edit in formatted.edits().iter().rev() {
        reversed.replace_range(edit.range.clone(), &edit.replacement);
    }
    assert_eq!(reversed, formatted.as_str());

    if options.final_newline != FinalNewline::Remove {
        let again = after
            .format(options)
            .expect("verified formatting is representable again");
        assert!(again.is_empty());
        assert_eq!(again.as_str(), formatted.as_str());
    }
}

// One option combination derived from the input, so a run covers every
// combination of the five policies without formatting each input with all of
// them. FNV-1a keeps the choice stable for a given input.
fn derived_options(bytes: &[u8]) -> FormatOptions {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let mut pick = |choices: u64| {
        let choice = hash % choices;
        hash /= choices;
        usize::try_from(choice).expect("choice is below the choice count")
    };
    FormatOptions::default()
        .with_equals_spacing(
            [
                EqualsSpacing::Preserve,
                EqualsSpacing::Spaces,
                EqualsSpacing::None,
            ][pick(3)],
        )
        .with_trim_trailing_whitespace(pick(2) == 1)
        .with_blank_lines([BlankLines::Preserve, BlankLines::Collapse][pick(2)])
        .with_quote_style(
            [
                QuoteStyle::Preserve,
                QuoteStyle::Double,
                QuoteStyle::Single,
                QuoteStyle::Unquoted,
            ][pick(4)],
        )
        .with_final_newline(
            [
                FinalNewline::Preserve,
                FinalNewline::Ensure,
                FinalNewline::Remove,
            ][pick(3)],
        )
}

fuzz_target!(|bytes: &[u8]| {
    let Ok(source) = std::str::from_utf8(bytes) else {
        return;
    };
    let options = [
        derived_options(bytes),
        // Trimming while the final newline is left as is.
        FormatOptions::default().with_trim_trailing_whitespace(true),
        // Blank-line collapsing with a quote style.
        FormatOptions::normalize_groups().with_quote_style(QuoteStyle::Double),
        // Unquoting runs only when no other trivia rule is selected.
        FormatOptions::default().with_quote_style(QuoteStyle::Unquoted),
        FormatOptions::normalize(),
        FormatOptions::normalize_groups(),
        FormatOptions::normalize().with_quote_style(QuoteStyle::Double),
        FormatOptions::normalize().with_quote_style(QuoteStyle::Single),
        FormatOptions::normalize().with_quote_style(QuoteStyle::Unquoted),
        FormatOptions::default()
            .with_equals_spacing(EqualsSpacing::Spaces)
            .with_trim_trailing_whitespace(false),
        FormatOptions::default()
            .with_equals_spacing(EqualsSpacing::None)
            .with_trim_trailing_whitespace(true)
            .with_final_newline(FinalNewline::Ensure),
        FormatOptions::default()
            .with_equals_spacing(EqualsSpacing::Spaces)
            .with_trim_trailing_whitespace(true)
            .with_blank_lines(BlankLines::Collapse)
            .with_final_newline(FinalNewline::Remove),
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
        check_sort(&document);
        for options in &options {
            check_result(source, &document, options);
        }
    }
});
