use alloc::{string::String, vec::Vec};

use proptest::prelude::*;

use crate::{parse, parse_strict, parse_with, AssignmentState, Dialect, InlineComments, ItemKind};

#[test]
fn values_are_blanked_and_comments_and_layout_are_kept() {
    let source = "# app\n\nA=1   \nexport B=\"two words\" # note\nBARE\nC=''\n";
    let doc = parse_strict(source).unwrap();
    assert_eq!(doc.example(), "# app\n\nA=\nexport B= # note\nBARE\nC=\n");
    let example_text = doc.example();
    let example = parse_strict(&example_text).unwrap();
    assert_eq!(example.get("A"), Some(""));
    assert_eq!(example.get("B"), Some(""));
    assert_eq!(example.get("BARE"), None);
    assert_eq!(example.get("C"), Some(""));
    assert_eq!(
        example.unique("B").unwrap().inline_comment(),
        Some("# note")
    );
}

#[test]
fn multiline_values_are_removed_without_leaving_body_lines() {
    let source = "A=\"first\nsecond\"\nB=2\n";
    let doc = parse_strict(source).unwrap();
    assert_eq!(doc.example(), "A=\"\"\nB=\n");
    let example_text = doc.example();
    let example = parse_strict(&example_text).unwrap();
    assert_eq!(example.get("A"), Some(""));
    assert_eq!(example.get("B"), Some(""));
}

#[test]
fn malformed_and_ignored_statements_cannot_leak_values() {
    let source = "BAD KEY=secret\n  ALSO BAD=other\n";
    let (doc, _) = parse(source);
    assert_eq!(doc.example(), "#\n  #\n");
    let example_text = doc.example();
    let (example, _) = parse(&example_text);
    assert_eq!(example.entries().count(), 0);
}

#[test]
fn compose_inline_comments_keep_an_empty_quoted_placeholder() {
    let source = "B= value   # note\n";
    let (doc, _) = parse_with(source, Dialect::COMPOSE);
    let example = doc.example();
    assert_eq!(example, "B= \"\"   # note\n");
    let (after, _) = parse_with(&example, Dialect::COMPOSE);
    let entry = after.unique("B").unwrap();
    assert_eq!(entry.value(), Some(""));
    assert_eq!(entry.inline_comment(), Some("# note"));
    assert_eq!(after.items().count(), 1);
}

#[test]
fn compose_quoted_trailing_comments_are_retained_as_comments() {
    let source = "B=\"value\"   # note\n";
    let (doc, _) = parse_with(source, Dialect::COMPOSE);
    let example = doc.example();
    assert_eq!(example, "B=\"\"   # note\n");
    let (after, _) = parse_with(&example, Dialect::COMPOSE);
    let entry = after.unique("B").unwrap();
    assert_eq!(entry.value(), Some(""));
    assert_eq!(entry.inline_comment(), Some("# note"));
    assert_eq!(after.items().count(), 1);
}

#[test]
fn empty_key_entries_are_commented_without_retaining_their_value() {
    let source = "= value\n";
    let (doc, _) = parse_with(source, Dialect::COMPOSE);
    let example = doc.example();
    assert_eq!(example, "#\n");
    let (after, _) = parse_with(&example, Dialect::COMPOSE);
    assert_eq!(after.entries().count(), 0);
}

#[test]
fn python_adjacent_quoted_comments_keep_an_empty_placeholder() {
    let source = "A=\"value\"#note\n";
    let (doc, _) = parse_with(source, Dialect::PYTHON_DOTENV);
    let example = doc.example();
    assert_eq!(example, "A=\"\"#note\n");
    let (after, _) = parse_with(&example, Dialect::PYTHON_DOTENV);
    assert_eq!(after.unique("A").unwrap().value(), Some(""));
    assert_eq!(after.unique("A").unwrap().inline_comment(), Some("#note"));
}

#[test]
fn node_colon_values_keep_an_empty_placeholder() {
    let source = "A: value\n";
    let (doc, _) = parse_with(source, Dialect::NODE_DOTENV);
    let example = doc.example();
    assert_eq!(example, "A: \"\"\n");
    let (after, _) = parse_with(&example, Dialect::NODE_DOTENV);
    assert_eq!(after.unique("A").unwrap().value(), Some(""));
}

#[test]
fn custom_space_comments_move_to_a_safe_comment_line() {
    let dialect = Dialect {
        inline_comments: InlineComments::Space,
        quoted_comments: false,
        ..Dialect::PORTABLE
    };
    let source = "A=value # note\n";
    let (doc, _) = parse_with(source, dialect);
    let example = doc.example();
    assert_eq!(example, "A=\n# note\n");
    let (after, _) = parse_with(&example, dialect);
    assert_eq!(after.unique("A").unwrap().value(), Some(""));
    assert!(after
        .items()
        .any(|item| item.kind() == ItemKind::Comment && item.raw().contains("# note")));
}

#[test]
fn example_keeps_the_bom_under_every_policy() {
    let source = "\u{feff}A=secret\n";
    let (kept, _) = parse_with(source, Dialect::PORTABLE.with_bom(crate::BomPolicy::Keep));
    assert_eq!(kept.example(), "\u{feff}#\n");

    let (error, _) = parse_with(source, Dialect::PORTABLE.with_bom(crate::BomPolicy::Error));
    assert_eq!(error.example(), "\u{feff}#\nA=\n");

    let (stripped, _) = parse_with(source, Dialect::PORTABLE);
    assert_eq!(stripped.example(), "\u{feff}A=\n");

    let (node, _) = parse_with(
        source,
        Dialect::NODE_DOTENV.with_bom(crate::BomPolicy::Keep),
    );
    assert_eq!(node.example(), "\u{feff}A=\n");
}

#[test]
fn raw_values_and_bom_are_handled() {
    let source = "\u{feff}A= raw value  \n";
    let (doc, _) = parse_with(source, Dialect::DOCKER_RAW);
    assert_eq!(doc.example(), "\u{feff}A=\n");
}

#[test]
fn example_preserves_keys_and_replaces_all_assigned_states() {
    let source = "A=1\nB=\nC\n";
    let doc = parse_strict(source).unwrap();
    let example_text = doc.example();
    let example = parse_strict(&example_text).unwrap();
    let original: Vec<_> = doc.entries().map(crate::Entry::key).collect();
    let generated: Vec<_> = example.entries().map(crate::Entry::key).collect();
    assert_eq!(original, generated);
    assert_eq!(
        example
            .entries()
            .map(|entry| (entry.key(), entry.state()))
            .collect::<Vec<_>>(),
        [
            ("A", AssignmentState::Empty),
            ("B", AssignmentState::Empty),
            ("C", AssignmentState::Bare),
        ]
    );
}

#[test]
fn comments_are_kept_even_when_they_contain_value_like_text() {
    let source = "# A=secret\nA=value\n";
    let doc = parse_strict(source).unwrap();
    assert_eq!(doc.example(), "# A=secret\nA=\n");
}

proptest! {
    #[test]
    fn example_preserves_entry_keys_and_never_retains_assigned_values(
        source in "\\PC{0,240}",
    ) {
        let (doc, _) = parse(&source);
        let example_text: String = doc.example();
        let (example, _) = parse(&example_text);
        let expected: Vec<_> = doc
            .entries()
            .filter(|entry| !entry.key().is_empty())
            .collect();
        prop_assert_eq!(expected.len(), example.entries().len());
        for (left, right) in expected.into_iter().zip(example.entries()) {
            prop_assert_eq!(left.key(), right.key());
            if left.state() == AssignmentState::Bare {
                prop_assert_eq!(right.state(), AssignmentState::Bare);
                prop_assert_eq!(right.value(), None);
            } else {
                prop_assert_eq!(right.state(), AssignmentState::Empty);
                prop_assert_eq!(right.value(), Some(""));
            }
            prop_assert_eq!(left.inline_comment(), right.inline_comment());
        }
        for item in doc.items() {
            if item.kind() == ItemKind::Comment {
                prop_assert!(example_text.contains(item.raw()));
            }
        }
    }
}

#[test]
fn example_keeps_moved_comment_separate_on_lf_only_trailing_cr() {
    let dialect = crate::Dialect::COMPOSE.with_quoted_comments(false);
    let (doc, _) = parse_with("A=1\r\nB=2 # note\r", dialect);
    let example_text = doc.example();
    let (example, _) = parse_with(&example_text, dialect);
    assert_eq!(example.get("A"), Some(""));
    assert_eq!(example.get("B"), Some(""));
    assert!(example
        .items()
        .any(|item| item.kind() == ItemKind::Comment && item.comment_text() == Some(" note")));

    // LF-only CRLF line keeps CRLF breaks when moving an inline comment.
    let (crlf_doc, _) = parse_with("A=1 # note\r\n", dialect);
    assert_eq!(crlf_doc.example(), "A=\r\n# note\r\n");

    // Universal line-break dialect preserves a CR ending when moving an inline comment.
    let universal = dialect.with_line_breaks(crate::LineBreaks::Universal);
    let (cr_doc, _) = parse_with("A=1 # note\r", universal);
    assert_eq!(cr_doc.example(), "A=\r# note\r");
}

#[test]
fn compose_post_quote_bare_text_becomes_a_comment_while_assignments_stay() {
    for (source, expected) in [
        ("A=\"secret\"tail\n", "A=\"\"\n#\n"),
        ("A=\"secret\"   tail\n", "A=\"\"\n   #\n"),
        ("A='secret'tail\n", "A=\"\"\n#\n"),
        ("A=\"secret\"tail", "A=\"\"\n#"),
        ("A=\"secret\"export tail\n", "A=\"\"\n#\n"),
        ("A=\"1\" B=\"2\"tail\n", "A=\"\"\n B=\"\"\n#\n"),
        ("A=\"secret\"B=secret2\n", "A=\"\"\nB=\n"),
        ("A=\"secret\" B=secret2\n", "A=\"\"\n B=\n"),
        ("A=\"secret\" # note\n", "A=\"\" # note\n"),
        ("A=\"secret\"#note\n", "A=\"\"#note\n"),
        (
            "BARE\nA=\"secret\"tail\nBARE2\n",
            "BARE\nA=\"\"\n#\nBARE2\n",
        ),
    ] {
        let (doc, _) = parse_with(source, Dialect::COMPOSE);
        let example = doc.example();
        assert_eq!(example, expected, "{source:?}");
        assert!(
            !example.contains("secret") && !example.contains("tail"),
            "{source:?} leaked into {example:?}"
        );
        let (after, _) = parse_with(&example, Dialect::COMPOSE);
        assert!(
            !after
                .items()
                .any(|item| matches!(item.kind(), ItemKind::Invalid | ItemKind::Ignored)),
            "{source:?} produced invalid example {example:?}"
        );
    }

    // BomPolicy::Error produces a LineEnding::None invalid BOM item before a
    // standalone bare key; that bare key is not post-quote text and stays bare.
    let bom_err = Dialect::COMPOSE.with_bom(crate::BomPolicy::Error);
    let (doc, _) = parse_with("\u{feff}BARE\nA=\"secret\"tail\n", bom_err);
    assert_eq!(doc.example(), "\u{feff}#\nBARE\nA=\"\"\n#\n");

    // In custom Node + NextStatement dialects, LS/PS (\u{2028}/\u{2029}) ends
    // the line with LineEnding::None, so a bare key on the next line stays bare.
    let node_next = Dialect::NODE_DOTENV.with_quoted_tail(crate::QuotedTail::NextStatement);
    for (source, expected) in [
        (
            "A=\"secret\" # note\u{2028}BARE\n",
            "A=\"\" # note\n\u{2028}BARE\n",
        ),
        ("A=\"secret\"\u{2028}BARE\n", "A=\"\"\n\u{2028}BARE\n"),
        (
            "A=\"secret\"tail\u{2029}BARE\n",
            "A=\"\"\n#\n\u{2029}BARE\n",
        ),
    ] {
        let (doc, _) = parse_with(source, node_next);
        let example = doc.example();
        assert_eq!(example, expected, "{source:?}");
        assert!(!example.contains("secret") && !example.contains("tail"));
        let (after, _) = parse_with(&example, node_next);
        assert_eq!(after.unique("A").unwrap().value(), Some(""));
        assert_eq!(after.unique("BARE").unwrap().state(), AssignmentState::Bare);
    }
}
