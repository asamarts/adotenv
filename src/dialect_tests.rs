use alloc::{
    format,
    string::{String, ToString},
    vec,
};
use proptest::prelude::*;

use crate::{
    parse_owned_with, parse_strict_with, parse_with, AssignmentState, BareKeys, BomPolicy,
    ColonSeparator, DiagnosticCode as Code, Dialect, DoubleEscapes, Duplicates, EditError,
    InlineComments, InsertPosition, ItemKind, KeySyntax, LineBreaks, MalformedLines, Multiline,
    Quote, QuotedTail, Severity, SingleEscapes, Syntax, UnknownEscapes, Whitespace,
};

const PRESETS: [Dialect; 5] = [
    Dialect::PORTABLE,
    Dialect::NODE_DOTENV,
    Dialect::PYTHON_DOTENV,
    Dialect::COMPOSE,
    Dialect::DOCKER_RAW,
];

#[test]
fn reinterpretation_changes_grouping_without_losing_declarations_or_source() {
    let text = "A=`one\nINNER=inside\ntwo`\nBARE\nEMPTY=\nDUP=first\nDUP\n";
    let (portable, _) = parse_with(text, Dialect::PORTABLE);
    let (node, _) = portable.reparse(Dialect::NODE_DOTENV);
    assert_eq!(portable.get("INNER"), Some("inside"));
    assert_eq!(node.get("INNER"), None);
    assert_eq!(node.get("A"), Some("one\nINNER=inside\ntwo"));
    assert_eq!(node.unique("A").unwrap().quote(), Quote::Backtick);
    assert_eq!(node.get("DUP"), Some("first"));
    assert_eq!(node.get_all("DUP").count(), 2);
    assert_eq!(node.unique("DUP").unwrap_err(), EditError::Duplicate(2));
    assert_eq!(node.unique("BARE").unwrap().state(), AssignmentState::Bare);
    assert_eq!(node.unique("BARE").unwrap().value(), None);
    assert!(!node.unique("BARE").unwrap().participates_in_lookup());
    assert_eq!(
        node.unique("EMPTY").unwrap().state(),
        AssignmentState::Empty
    );
    assert_eq!(node.source(), text);
    assert_eq!(portable.source(), text);
    assert_eq!(node.into_owned().dialect(), Dialect::NODE_DOTENV);
}

#[test]
fn bare_lookup_policy_never_erases_syntax_or_performs_inheritance() {
    let source = "A=first\nA\nB\n";
    let mut dialect = Dialect::PORTABLE;
    assert_eq!(parse_with(source, dialect).0.get("A"), None);
    dialect.bare_keys = BareKeys::Ignore;
    assert_eq!(parse_with(source, dialect).0.get("A"), Some("first"));
    dialect.bare_keys = BareKeys::Empty;
    let (doc, _) = parse_with(source, dialect);
    assert_eq!(doc.get("A"), Some(""));
    assert_eq!(doc.get("B"), Some(""));
    assert_eq!(doc.get_entry("A").unwrap().value(), None);
    assert_eq!(doc.get_entry("A").unwrap().state(), AssignmentState::Bare);
    dialect.duplicates = Duplicates::First;
    assert_eq!(parse_with(source, dialect).0.get("A"), Some("first"));
    dialect.bare_keys = BareKeys::Error;
    let (doc, diagnostics) = parse_with(source, dialect);
    let error = diagnostics
        .into_iter()
        .find(|d| d.severity() == Severity::Error)
        .unwrap();
    assert_eq!(error.code(), Code::ForbiddenBareKey);
    assert_eq!(doc.get_all("A").count(), 2);
    assert_eq!(parse_strict_with(source, dialect), Err(error));
}

#[test]
fn comment_overrides_apply_to_each_family_independently_of_raw_mode() {
    for mut dialect in PRESETS {
        dialect.raw_values = false;
        dialect.inline_comments = InlineComments::None;
        assert_eq!(
            parse_with("A=one#two\n", dialect).0.get("A"),
            Some("one#two")
        );
        dialect.inline_comments = InlineComments::AnyHash;
        assert_eq!(parse_with("A=one#two\n", dialect).0.get("A"), Some("one"));
    }
    let mut dialect = Dialect::PORTABLE;
    dialect.raw_values = true;
    dialect.inline_comments = InlineComments::AnyHash;
    assert_eq!(
        parse_with("A=  'x' # literal \n", dialect).0.get("A"),
        Some("  'x' # literal ")
    );
}

#[test]
fn escape_sets_and_unknown_policy_control_values_and_strict_errors() {
    let source = "A=\"x\\n\\t\\q\"\n";
    let mut dialect = Dialect::PORTABLE;
    dialect.double_escapes = DoubleEscapes::LF;
    dialect.unknown_escapes = UnknownEscapes::Preserve;
    assert_eq!(parse_with(source, dialect).0.get("A"), Some("x\n\\t\\q"));
    dialect.unknown_escapes = UnknownEscapes::DropBackslash;
    assert_eq!(parse_with(source, dialect).0.get("A"), Some("x\ntq"));
    dialect.unknown_escapes = UnknownEscapes::Error;
    let (doc, diagnostics) = parse_with(source, dialect);
    let error = diagnostics
        .into_iter()
        .find(|d| d.severity() == Severity::Error)
        .unwrap();
    assert_eq!(error.code(), Code::InvalidEscape);
    assert_eq!(&source[error.span()], "\\t");
    assert_eq!(parse_strict_with(source, dialect), Err(error));
    assert_eq!(doc.unique("A").unwrap().value(), Some("x\n\\t\\q"));
    // A successful native edit must not introduce a fatal selected-policy error.
    if let Ok(edit) = doc.set("A", "\\t") {
        assert_eq!(
            parse_strict_with(edit.as_str(), dialect).unwrap().get("A"),
            Some("\\t")
        );
    }
}

#[test]
fn single_quote_escape_switch_changes_closing_and_decoding_rules() {
    let source = "A='a\\'b\\\\c'\n";
    let mut dialect = Dialect::PORTABLE;
    assert_eq!(
        parse_strict_with(source, dialect).unwrap_err().code(),
        Code::TrailingText
    );
    dialect.single_escapes = SingleEscapes::Quote;
    assert_eq!(
        parse_strict_with(source, dialect).unwrap().get("A"),
        Some("a'b\\\\c")
    );
    dialect.single_escapes = SingleEscapes::QuoteAndBackslash;
    assert_eq!(
        parse_strict_with(source, dialect).unwrap().get("A"),
        Some("a'b\\c")
    );
}

#[test]
fn colon_and_key_alphabet_switches_have_real_lexical_effects() {
    let mut dialect = Dialect::PORTABLE;
    dialect.colon = ColonSeparator::Whitespace;
    let doc = parse_strict_with("A: value\n", dialect).unwrap();
    let separator = doc.unique("A").unwrap().tokens().eq.clone().unwrap();
    assert_eq!(&doc.source()[separator], ":");
    assert!(parse_strict_with("A:value\n", dialect).is_err());
    dialect.colon = ColonSeparator::Any;
    assert_eq!(
        parse_strict_with("A:value\n", dialect).unwrap().get("A"),
        Some("value")
    );
    dialect.keys = KeySyntax::Posix;
    assert!(parse_strict_with("1A=x\n", dialect).is_err());
    dialect.keys = KeySyntax::Unicode;
    assert_eq!(
        parse_strict_with("CAFÉ=x\n", dialect).unwrap().get("CAFÉ"),
        Some("x")
    );
    assert!(parse_strict_with("\u{0345}=x\n", dialect).is_err());
    dialect.keys = KeySyntax::NonWhitespace;
    assert_eq!(
        parse_strict_with("A/B=x\n", dialect).unwrap().get("A/B"),
        Some("x")
    );
}

#[test]
fn bom_policy_is_explicit_and_errors_survive_ignored_line_policy() {
    let source = "\u{feff}A=x\n";
    let mut dialect = Dialect::PORTABLE;
    let (stripped, _) = parse_with(source, dialect);
    assert_eq!(stripped.bom_span(), Some(0..3));
    assert_eq!(stripped.get("A"), Some("x"));
    dialect.bom = BomPolicy::Keep;
    dialect.keys = KeySyntax::NonWhitespace;
    let (kept, _) = parse_with(source, dialect);
    assert_eq!(kept.bom_span(), None);
    assert_eq!(kept.get("\u{feff}A"), Some("x"));
    dialect = Dialect::NODE_DOTENV;
    dialect.bom = BomPolicy::Error;
    let (doc, diagnostics) = parse_with(source, dialect);
    assert_eq!(doc.item(0).unwrap().span(), 0..3);
    assert_eq!(doc.get("A"), Some("x"));
    let error = diagnostics
        .into_iter()
        .find(|d| d.severity() == Severity::Error)
        .unwrap();
    assert_eq!(error.code(), Code::ForbiddenByteOrderMark);
    assert_eq!(parse_strict_with(source, dialect), Err(error));
}

#[test]
fn disabling_multiline_restores_following_statement_recovery() {
    let source = "A=\"one\nB=two\nend\"\nNEXT=kept\n";
    let mut dialect = Dialect::PORTABLE;
    assert_eq!(parse_with(source, dialect).0.get("B"), None);
    dialect.multiline = Multiline::NONE;
    let (doc, _) = parse_with(source, dialect);
    assert_eq!(doc.get("B"), Some("two"));
    assert_eq!(doc.get("NEXT"), Some("kept"));
    assert_eq!(
        parse_strict_with(source, dialect).unwrap_err().code(),
        Code::UnterminatedQuote
    );
}

#[test]
fn quote_tail_policies_preserve_tokens_and_exact_appended_value_mappings() {
    let source = "A=\"x\"tail # note\nNEXT=kept\n";
    let mut dialect = Dialect::PORTABLE;
    for (policy, expected) in [
        (QuotedTail::Ignore, "x"),
        (QuotedTail::Append, "xtail"),
        (QuotedTail::LiteralFallback, "\"x\"tail"),
    ] {
        dialect.quoted_tail = policy;
        let doc = parse_strict_with(source, dialect).unwrap();
        assert_eq!(doc.get("A"), Some(expected));
        assert_eq!(doc.get("NEXT"), Some("kept"));
        let entry = doc.unique("A").unwrap();
        if policy != QuotedTail::LiteralFallback {
            assert_eq!(&source[entry.tokens().quoted_tail.clone().unwrap()], "tail");
        }
        if policy == QuotedTail::Append {
            assert_eq!(entry.decoded_offset_to_raw(expected.len()), Ok(9));
            let edit = doc.set("A", "z").unwrap();
            assert_eq!(
                parse_strict_with(edit.as_str(), dialect).unwrap().get("A"),
                Some("z")
            );
            assert_eq!(
                parse_strict_with(edit.as_str(), dialect)
                    .unwrap()
                    .get("NEXT"),
                Some("kept")
            );
        }
    }
    dialect.quoted_tail = QuotedTail::Skip;
    let doc = parse_strict_with(source, dialect).unwrap();
    assert_eq!(doc.item(0).unwrap().kind(), ItemKind::Ignored);
    assert_eq!(doc.get("A"), None);
    dialect.quoted_tail = QuotedTail::NextStatement;
    let doc = parse_strict_with("A=\"x\"B=y\n", dialect).unwrap();
    assert_eq!(doc.get("A"), Some("x"));
    assert_eq!(doc.get("B"), Some("y"));
}

#[test]
fn raw_line_breaks_and_normalization_remain_independent() {
    let source = "A=  \"x\" # raw \rB=y\r\n";
    let mut dialect = Dialect::DOCKER_RAW;
    assert_eq!(
        parse_strict_with(source, dialect).unwrap().get("A"),
        Some("  \"x\" # raw \rB=y")
    );
    dialect.normalize_newlines = true;
    assert_eq!(
        parse_strict_with(source, dialect).unwrap().get("A"),
        Some("  \"x\" # raw \nB=y")
    );
    dialect.normalize_newlines = false;
    dialect.line_breaks = LineBreaks::Universal;
    let doc = parse_strict_with(source, dialect).unwrap();
    assert_eq!(doc.get("A"), Some("  \"x\" # raw "));
    assert_eq!(doc.get("B"), Some("y"));
}

#[test]
fn normalized_and_deleted_syntax_maps_use_original_character_boundaries() {
    let source = "A=x\u{2028}'a'\u{2028}\"b\"\n";
    let doc = parse_strict_with(source, Dialect::NODE_DOTENV).unwrap();
    let entry = doc.unique("A").unwrap();
    assert_eq!(entry.value(), Some("x\u{2028}a\u{2028}b"));
    for (decoded, raw) in [(0, 2), (1, 3), (4, 7), (5, 9), (8, 13), (9, 15)] {
        assert_eq!(entry.decoded_offset_to_raw(decoded), Ok(raw));
    }
    let source = "A=\"é\r\nz\"\r\n";
    let doc = parse_strict_with(source, Dialect::NODE_DOTENV).unwrap();
    let entry = doc.unique("A").unwrap();
    assert_eq!(entry.value(), Some("é\nz"));
    for (decoded, raw) in [(0, 3), (2, 5), (3, 7), (4, 8)] {
        assert_eq!(entry.decoded_offset_to_raw(decoded), Ok(raw));
    }
    assert_eq!(
        entry.decoded_offset_to_raw(1),
        Err(crate::PositionError::InvalidUtf8Boundary)
    );
}

#[test]
fn scanner_limit_is_checked_before_parsing_and_is_not_a_skipped_line() {
    let mut source = format!("A={}\n", "x".repeat(65533));
    assert_eq!(
        parse_strict_with(&source, Dialect::DOCKER_RAW)
            .unwrap()
            .get("A")
            .unwrap()
            .len(),
        65533
    );
    source.insert(source.len() - 1, 'x');
    assert_eq!(
        parse_strict_with(&source, Dialect::DOCKER_RAW)
            .unwrap_err()
            .code(),
        Code::LineTooLong
    );
    let mut dialect = Dialect::NODE_DOTENV;
    dialect.line_byte_limit = Some(4);
    assert_eq!(
        parse_strict_with("A=long\n", dialect).unwrap_err().code(),
        Code::LineTooLong
    );
}

#[test]
fn selected_dialect_verifies_raw_backtick_and_quoted_key_native_edits() {
    let node = parse_strict_with("A=`old`\nKEEP=ok\n", Dialect::NODE_DOTENV).unwrap();
    let edit = node.set("A", "x # y").unwrap();
    let result = parse_strict_with(edit.as_str(), node.dialect()).unwrap();
    assert_eq!(result.get("A"), Some("x # y"));
    assert_eq!(result.get("KEEP"), Some("ok"));
    assert_eq!(result.unique("A").unwrap().quote(), Quote::Backtick);

    let raw = parse_strict_with("A=old\nKEEP=ok\n", Dialect::DOCKER_RAW).unwrap();
    let value = " leading # \"quoted\" trailing ";
    let edit = raw.set("A", value).unwrap();
    assert_eq!(
        parse_strict_with(edit.as_str(), raw.dialect())
            .unwrap()
            .get("A"),
        Some(value)
    );
    assert!(raw.set("A", "x\nINJECTED=y").is_err());

    let source = "\u{feff}'A B'\r\nKEEP='é'\r\n";
    let python = parse_strict_with(source, Dialect::PYTHON_DOTENV).unwrap();
    let entry = python.unique("A B").unwrap();
    assert_eq!(&source[entry.key_span()], "A B");
    assert_eq!(&source[entry.tokens().key.clone()], "'A B'");
    let edit = python
        .edit(&[
            crate::Edit::Set {
                entry: entry.id(),
                value: "new",
            },
            crate::Edit::Rename {
                entry: entry.id(),
                key: "C D",
            },
            crate::Edit::Export {
                entry: entry.id(),
                enabled: true,
            },
        ])
        .unwrap();
    assert_eq!(edit.as_str(), "\u{feff}export 'C D'=new\r\nKEEP='é'\r\n");
    let result = parse_strict_with(edit.as_str(), python.dialect()).unwrap();
    assert_eq!(result.get("C D"), Some("new"));
    assert_eq!(result.get("KEEP"), Some("é"));
    let inserted = result
        .insert("E F", Some("extra"), InsertPosition::End)
        .unwrap();
    assert_eq!(
        parse_strict_with(inserted.as_str(), python.dialect())
            .unwrap()
            .get("E F"),
        Some("extra")
    );
}

#[test]
fn native_edits_preserve_ignored_statement_metadata() {
    let source = "not valid!\nA=x\n";
    let (doc, _) = parse_with(source, Dialect::NODE_DOTENV);
    assert_eq!(doc.edit(&[]).unwrap().as_str(), source);
    let edited = doc.set("A", "y").unwrap();
    assert_eq!(edited.as_str(), "not valid!\nA=y\n");
    assert_eq!(
        parse_with(edited.as_str(), doc.dialect()).0.get("A"),
        Some("y")
    );
}

#[test]
fn quoted_key_rename_can_change_to_an_unquoted_spelling() {
    let doc = parse_strict_with("'old key'=v\nKEEP=ok\n", Dialect::PYTHON_DOTENV).unwrap();
    let edited = doc.rename("old key", "O'NEIL").unwrap();
    assert_eq!(edited.as_str(), "O'NEIL=v\nKEEP=ok\n");
    let after = parse_strict_with(edited.as_str(), doc.dialect()).unwrap();
    assert_eq!(after.get("O'NEIL"), Some("v"));
    assert_eq!(after.get("old key"), None);
    assert_eq!(after.get("KEEP"), Some("ok"));
}

#[test]
fn raw_value_carriage_returns_do_not_vote_for_generated_endings() {
    let doc = parse_strict_with("A=x\ry\rz\r\n", Dialect::DOCKER_RAW).unwrap();
    let edited = doc.insert("B", Some("x"), InsertPosition::End).unwrap();
    assert_eq!(edited.as_str(), "A=x\ry\rz\r\nB=x\r\n");
    let after = parse_strict_with(edited.as_str(), doc.dialect()).unwrap();
    assert_eq!(after.get("A"), Some("x\ry\rz"));
    assert_eq!(after.get("B"), Some("x"));
}

#[test]
fn set_keeps_a_delimiter_before_ignored_quote_tails() {
    let dialect = Dialect {
        quoted_tail: QuotedTail::Ignore,
        ..Dialect::PORTABLE
    };
    let doc = parse_strict_with("A='x'tail\nKEEP=ok\n", dialect).unwrap();
    let edited = doc.set("A", "can't").unwrap();
    assert_eq!(edited.as_str(), "A=\"can't\"tail\nKEEP=ok\n");
    let after = parse_strict_with(edited.as_str(), dialect).unwrap();
    assert_eq!(after.get("A"), Some("can't"));
    assert_eq!(after.get("KEEP"), Some("ok"));
    assert_eq!(
        after
            .unique("A")
            .unwrap()
            .tokens()
            .quoted_tail
            .as_ref()
            .map(|span| &edited.as_str()[span.clone()]),
        Some("tail")
    );
}

#[test]
fn bare_python_empty_assignment_keeps_a_tight_comment() {
    let doc = parse_strict_with("A# note\nKEEP=ok\n", Dialect::PYTHON_DOTENV).unwrap();
    let edited = doc.set("A", "").unwrap();
    assert_eq!(edited.as_str(), "A= # note\nKEEP=ok\n");
    let after = parse_strict_with(edited.as_str(), doc.dialect()).unwrap();
    assert_eq!(after.get("A"), Some(""));
    assert_eq!(after.get("KEEP"), Some("ok"));
    assert_eq!(after.unique("A").unwrap().inline_comment(), Some("# note"));
}

#[test]
fn lf_only_insertion_completes_a_terminal_cr_separator() {
    let doc = parse_strict_with("A=x\ry\r", Dialect::DOCKER_RAW).unwrap();
    let edited = doc.insert("B", Some("z"), InsertPosition::End).unwrap();
    assert_eq!(edited.as_str(), "A=x\ry\r\nB=z\n");
    let after = parse_strict_with(edited.as_str(), doc.dialect()).unwrap();
    assert_eq!(after.get("A"), Some("x\ry"));
    assert_eq!(after.get("B"), Some("z"));
}

#[test]
fn native_insert_cannot_introduce_a_forbidden_bare_declaration() {
    let dialect = Dialect {
        bare_keys: BareKeys::Error,
        ..Dialect::PORTABLE
    };
    let doc = parse_strict_with("A=x\n", dialect).unwrap();
    assert_eq!(
        doc.insert("B", None, InsertPosition::End).unwrap_err(),
        EditError::WouldChangeMeaning
    );
    let edited = doc.insert("B", Some(""), InsertPosition::End).unwrap();
    assert_eq!(
        parse_strict_with(edited.as_str(), dialect)
            .unwrap()
            .get("B"),
        Some("")
    );
}

#[test]
fn insertion_after_a_node_regex_boundary_keeps_both_neighbors() {
    let source = "A=\"x\"\u{2028}KEEP=y\n";
    let doc = parse_strict_with(source, Dialect::NODE_DOTENV).unwrap();
    let edited = doc
        .insert("B", Some("z"), InsertPosition::AfterKey("A"))
        .unwrap();
    let after = parse_strict_with(edited.as_str(), doc.dialect()).unwrap();
    assert_eq!(after.get("A"), Some("x"));
    assert_eq!(after.get("B"), Some("z"));
    assert_eq!(after.get("KEEP"), Some("y"));
    assert_eq!(edited.as_str(), "A=\"x\"\nB=z\n\u{2028}KEEP=y\n");
}

#[test]
fn insertion_after_a_compose_quote_uses_its_next_statement_boundary() {
    let doc = parse_strict_with("A=\"x\"", Dialect::COMPOSE).unwrap();
    let edited = doc.insert("B", Some("z"), InsertPosition::End).unwrap();
    let after = parse_strict_with(edited.as_str(), doc.dialect()).unwrap();
    assert_eq!(after.get("A"), Some("x"));
    assert_eq!(after.get("B"), Some("z"));
    assert_eq!(edited.as_str(), "A=\"x\"B=z");
}

#[test]
fn lf_only_comment_restoration_keeps_internal_carriage_returns() {
    let source = "A=x\ry\nKEEP=z\n";
    let doc = parse_strict_with(source, Dialect::DOCKER_RAW).unwrap();
    let commented = doc.comment("A").unwrap();
    assert_eq!(commented.as_str(), "# A=x\ry\nKEEP=z\n");
    let comments = parse_strict_with(commented.as_str(), doc.dialect()).unwrap();
    let restored = comments
        .edit(&[crate::Edit::Uncomment { item: 0 }])
        .unwrap();
    assert_eq!(restored.as_str(), source);
}

#[test]
fn inserting_after_a_commented_compose_quote_adds_a_separator() {
    let doc = parse_strict_with("A=\"x\"", Dialect::COMPOSE).unwrap();
    let edited = doc
        .edit(&[
            crate::Edit::Comment { item: 0 },
            crate::Edit::Insert {
                key: "B",
                value: Some("z"),
                position: InsertPosition::End,
            },
        ])
        .unwrap();
    assert_eq!(edited.as_str(), "# A=\"x\"\nB=z");
    let after = parse_strict_with(edited.as_str(), doc.dialect()).unwrap();
    assert_eq!(after.get("A"), None);
    assert_eq!(after.get("B"), Some("z"));
}

#[test]
fn a_preceding_bare_field_precedes_a_node_boundary_insertion() {
    let doc = parse_strict_with("A\u{2028}B=y\n", Dialect::NODE_DOTENV).unwrap();
    let edited = doc
        .edit(&[
            crate::Edit::Set {
                entry: 0,
                value: "x",
            },
            crate::Edit::Insert {
                key: "C",
                value: Some("z"),
                position: InsertPosition::AfterEntry(0),
            },
        ])
        .unwrap();
    assert_eq!(edited.as_str(), "A=x\nC=z\n\u{2028}B=y\n");
    let after = parse_strict_with(edited.as_str(), doc.dialect()).unwrap();
    assert_eq!(after.get("A"), Some("x"));
    assert_eq!(after.get("B"), Some("y"));
    assert_eq!(after.get("C"), Some("z"));
}

#[test]
fn a_final_compose_bare_insertion_cannot_become_an_eof_value() {
    let doc = parse_strict_with("A=x", Dialect::COMPOSE).unwrap();
    let edited = doc.insert("B", None, InsertPosition::End).unwrap();
    assert_eq!(edited.as_str(), "A=x\nB\n");
    let after = parse_strict_with(edited.as_str(), doc.dialect()).unwrap();
    let bare = after.unique("B").unwrap();
    assert_eq!(bare.value(), None);
    assert_eq!(bare.state(), AssignmentState::Bare);
    assert!(!bare.participates_in_lookup());
    assert_eq!(after.get(""), None);
}

#[test]
fn insertion_separator_ownership_survives_removed_source() {
    let doc = parse_strict_with("A\u{2028}REMOVE=y\n", Dialect::NODE_DOTENV).unwrap();
    let edited = doc
        .edit(&[
            crate::Edit::Set {
                entry: 0,
                value: "x",
            },
            crate::Edit::Remove { item: 1 },
            crate::Edit::Insert {
                key: "B",
                value: Some("z"),
                position: InsertPosition::End,
            },
        ])
        .unwrap();
    assert_eq!(edited.as_str(), "A=x\nB=z\n");
}

#[test]
fn removed_statement_gaps_do_not_repeat_insertion_separators() {
    let doc =
        parse_strict_with("A\u{2028}R1=\"y\"\u{2028}R2=\"z\"\n", Dialect::NODE_DOTENV).unwrap();
    let edited = doc
        .edit(&[
            crate::Edit::Set {
                entry: 0,
                value: "x",
            },
            crate::Edit::Remove { item: 1 },
            crate::Edit::Remove { item: 2 },
            crate::Edit::Insert {
                key: "B",
                value: Some("one"),
                position: InsertPosition::AfterEntry(0),
            },
            crate::Edit::Insert {
                key: "C",
                value: Some("two"),
                position: InsertPosition::AfterEntry(1),
            },
        ])
        .unwrap();
    assert_eq!(edited.as_str(), "A=x\nB=one\nC=two\n");
}

#[test]
fn python_single_quote_backslash_pairs_are_non_overlapping() {
    let source = format!("A='{}'\n", "\\".repeat(4));
    let doc = parse_strict_with(&source, Dialect::PYTHON_DOTENV).unwrap();
    let entry = doc.unique("A").unwrap();
    assert_eq!(entry.value(), Some("\\\\"));
    assert_eq!(entry.decoded_offset_to_raw(1).unwrap(), 5);
}

#[test]
fn a_plain_node_item_rejects_the_whole_original_oversized_line() {
    let dialect = Dialect {
        line_byte_limit: Some(6),
        ..Dialect::NODE_DOTENV
    };
    let source = "\n#x\u{2028}A=1\n";
    let (doc, diagnostics) = parse_with(source, dialect);
    assert_eq!(doc.get("A"), None);
    // The leading blank line is its own item. The oversized line is rejected
    // whole, although its comment alone would end at the LS anchor.
    assert_eq!(
        item_layout(source, dialect),
        [
            (ItemKind::Blank, "\n"),
            (ItemKind::Invalid, "#x\u{2028}A=1\n")
        ]
    );
    let item = doc.item(1).unwrap();
    assert_eq!(item.invalid_code(), Some(Code::LineTooLong));
    let error = diagnostics
        .iter()
        .find(|error| error.code() == Code::LineTooLong)
        .unwrap();
    assert!(item.span().start <= error.span().start && item.span().end >= error.span().end);
}

#[test]
fn compose_malformed_octal_deletion_preserves_every_decoded_boundary() {
    let doc = parse_strict_with("A=\"\\012\"\n", Dialect::COMPOSE).unwrap();
    let entry = doc.unique("A").unwrap();
    assert_eq!(entry.value(), Some("\\12"));
    for (offset, raw) in [3, 5, 6, 7].into_iter().enumerate() {
        assert_eq!(entry.decoded_offset_to_raw(offset), Ok(raw));
    }
}

#[test]
fn dropping_an_escape_still_normalizes_source_crlf() {
    let dialect = Dialect {
        normalize_newlines: true,
        unknown_escapes: UnknownEscapes::DropBackslash,
        ..Dialect::PORTABLE
    };
    let doc = parse_strict_with("A=\"\\\r\nz\"\n", dialect).unwrap();
    let entry = doc.unique("A").unwrap();
    assert_eq!(entry.value(), Some("\nz"));
    for (offset, raw) in [4, 6, 7].into_iter().enumerate() {
        assert_eq!(entry.decoded_offset_to_raw(offset), Ok(raw));
    }
}

#[test]
fn tight_quoted_comments_require_the_explicit_switch() {
    for base in PRESETS {
        let mut dialect = Dialect {
            inline_comments: InlineComments::AnyHash,
            quoted_comments: false,
            quoted_tail: QuotedTail::Error,
            raw_values: false,
            malformed_lines: crate::MalformedLines::Error,
            ..base
        };
        assert_eq!(
            parse_strict_with("A=\"x\"#tail\n", dialect)
                .unwrap_err()
                .code(),
            Code::TrailingText
        );
        assert_eq!(
            parse_strict_with("A=\"x\" #tail\n", dialect)
                .unwrap()
                .get("A"),
            Some("x")
        );
        dialect.quoted_comments = true;
        assert_eq!(
            parse_strict_with("A=\"x\"#tail\n", dialect)
                .unwrap()
                .get("A"),
            Some("x")
        );
    }
}

#[test]
fn multiline_line_limit_recovery_cannot_reenter_a_rejected_line() {
    let dialect = Dialect {
        line_byte_limit: Some(4),
        ..Dialect::COMPOSE
    };
    let source = "A=\"x\n\"B=yy\nC=z\n";
    let (doc, diagnostics) = parse_with(source, dialect);
    assert_eq!(doc.get("A"), None);
    assert_eq!(doc.get("B"), None);
    assert_eq!(doc.get("C"), Some("z"));
    let invalid = doc.items().next().unwrap();
    assert_eq!(invalid.kind(), ItemKind::Invalid);
    assert_eq!(invalid.raw(), "A=\"x\n\"B=yy\n");
    assert_eq!(diagnostics[0].code(), Code::LineTooLong);
    assert_eq!(diagnostics[0].span(), 5..10);
    assert_eq!(diagnostics[0].line(), 2);
    assert_eq!(
        parse_strict_with(source, dialect).unwrap_err(),
        diagnostics[0]
    );
}

#[test]
fn bare_comment_headers_honor_custom_anchor_policies() {
    for base in [Dialect::NODE_DOTENV, Dialect::PYTHON_DOTENV] {
        let dialect = Dialect {
            inline_comments: InlineComments::Space,
            bare_keys: BareKeys::Empty,
            malformed_lines: crate::MalformedLines::Error,
            ..base
        };
        let (doc, diagnostics) = parse_with("A#note\nB #note\n", dialect);
        assert!(doc.get_entry("A").is_none());
        assert_eq!(doc.get("B"), Some(""));
        assert!(diagnostics.iter().any(|d| d.severity() == Severity::Error));
    }
    let python = parse_strict_with("A#note\nB=x#literal\n", Dialect::PYTHON_DOTENV).unwrap();
    assert_eq!(
        python.unique("A").unwrap().state(),
        crate::AssignmentState::Bare
    );
    assert_eq!(python.get("B"), Some("x#literal"));
}

fn physical_line_at(source: &str, offset: usize) -> usize {
    let mut line = 1;
    let mut previous_cr = false;
    for ch in source[..offset].chars() {
        if ch == '\r' || ch == '\n' && !previous_cr {
            line += 1;
        }
        previous_cr = ch == '\r';
    }
    line
}

fn assert_document_invariants(source: &str, dialect: Dialect) {
    let (doc, diagnostics) = parse_with(source, dialect);
    assert_eq!(doc.source(), source);
    assert_eq!(doc.dialect(), dialect);
    let mut cursor = doc.bom_span().map_or(0, |span| span.end);
    for item in doc.items() {
        let span = item.span();
        assert_eq!(span.start, cursor);
        assert!(span.end > span.start);
        assert!(source.get(span.clone()).is_some());
        cursor = span.end;
        if let Some(diagnostic) = item.diagnostic() {
            assert!(span.start <= diagnostic.span().start && diagnostic.span().end <= span.end);
        }
        if let Some(entry) = item.entry() {
            assert_eq!(
                entry.line(),
                physical_line_at(source, entry.tokens().key.start)
            );
            assert_eq!(entry.line_span(), span.clone());
            let t = entry.tokens();
            let mut end = span.start;
            for token in [
                Some(&t.indent),
                t.export.as_ref(),
                Some(&t.space_after_export),
                Some(&t.key),
                Some(&t.space_before_eq),
                t.eq.as_ref(),
                Some(&t.space_after_eq),
                t.value.as_ref(),
                t.quoted_tail.as_ref(),
                Some(&t.trailing_space),
                t.inline_comment.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                assert_eq!(token.start, end, "{source:?} {dialect:?} {t:?}");
                assert!(source.get(token.clone()).is_some());
                end = token.end;
            }
            assert_eq!(end, item.content_span().end);
            if let Some(body) = &t.body {
                let value = t.value.as_ref().unwrap();
                assert!(value.start <= body.start && body.end <= value.end);
            }
            assert_eq!(
                entry.state(),
                match entry.value() {
                    None => AssignmentState::Bare,
                    Some("") => AssignmentState::Empty,
                    Some(_) => AssignmentState::Value,
                }
            );
            if let Some(value) = entry.value() {
                let token = entry.value_span().unwrap();
                let mut last = token.start;
                for offset in value
                    .char_indices()
                    .map(|(at, _)| at)
                    .chain(core::iter::once(value.len()))
                {
                    let raw = entry.decoded_offset_to_raw(offset).unwrap();
                    assert!(source.is_char_boundary(raw));
                    assert!(last <= raw && raw <= token.end);
                    last = raw;
                }
            }
        }
    }
    assert_eq!(cursor, source.len());
    for diagnostic in &diagnostics {
        assert!(source.get(diagnostic.span()).is_some());
        assert_eq!(
            diagnostic.line(),
            physical_line_at(source, diagnostic.span().start)
        );
        if let Some(span) = diagnostic.secondary_span() {
            assert!(source.get(span).is_some());
        }
    }
    assert!(diagnostics
        .windows(2)
        .all(|pair| pair[0].span().start <= pair[1].span().start));
    let first = diagnostics
        .into_iter()
        .find(|d| d.severity() == Severity::Error);
    assert_eq!(
        parse_strict_with(source, dialect),
        first.map_or(Ok(doc.clone()), Err)
    );
    assert_eq!(
        parse_owned_with(String::from(source), dialect).0,
        doc.clone().into_owned()
    );
}

proptest! {
    #[test]
    fn every_preset_and_custom_configuration_preserves_source_and_error_contracts(
        fragments in prop::collection::vec(prop_oneof![
            Just(String::from("A=\"x\\n\\q\"\n")), Just(String::from("A=one#two\r")),
            Just(String::from("'A B'=x\n")), Just(String::from("# hi\r\n")),
            Just(String::from("\u{feff}")), Just(String::from("A: value\n")),
            Just(String::from("A\n")), Just(String::from("A=`x\nB=y\n`\n")),
            Just(String::from("A=\"\\012\"\n")), Just(String::from("A=\"\\\r\nz\"\n")),
            any::<char>().prop_map(|c| c.to_string())
        ], 0..32),
        selection in any::<u32>(),
    ) {
        let source: String = fragments.concat();
        for dialect in PRESETS { assert_document_invariants(&source, dialect); }
        let mut dialect = PRESETS[selection as usize % PRESETS.len()];
        dialect.raw_values = selection & 1 != 0;
        dialect.bom = [BomPolicy::Strip, BomPolicy::Keep, BomPolicy::Error][(selection >> 1) as usize % 3];
        dialect.bare_keys = [BareKeys::Preserve, BareKeys::Ignore, BareKeys::Empty, BareKeys::Error][(selection >> 3) as usize % 4];
        dialect.unknown_escapes = [UnknownEscapes::Preserve, UnknownEscapes::Warn, UnknownEscapes::DropBackslash, UnknownEscapes::Error][(selection >> 5) as usize % 4];
        dialect.quoted_tail = [QuotedTail::Error, QuotedTail::Ignore, QuotedTail::Append, QuotedTail::LiteralFallback, QuotedTail::Skip, QuotedTail::NextStatement][(selection >> 7) as usize % 6];
        dialect.quoted_keys = selection & 512 != 0;
        dialect.backticks = selection & 1024 != 0;
        dialect.normalize_newlines = selection & 2048 != 0;
        dialect.multiline = if selection & 4096 != 0 { Multiline::ALL } else { Multiline::NONE };
        dialect.line_breaks = if selection & 8192 != 0 { LineBreaks::Universal } else { LineBreaks::Lf };
        dialect.line_byte_limit = if selection & 16384 != 0 {
            Some(((selection >> 15) % 32) as usize)
        } else { None };
        dialect.inline_comments = [
            InlineComments::Whitespace, InlineComments::Space, InlineComments::AnyHash,
            InlineComments::None, InlineComments::Python,
        ][(selection >> 20) as usize % 5];
        dialect.quoted_comments = selection & (1 << 23) != 0;
        assert_document_invariants(&source, dialect);
    }
}

#[test]
fn docker_grammar_keeps_raw_values_comments_and_eof_lines() {
    let source = "   # comment\n\nA= value \nB=hash#literal\nC=\"quoted\"\nD=last";
    let doc = parse_strict_with(source, Dialect::DOCKER_RAW).unwrap();
    assert_eq!(doc.get("A"), Some(" value "));
    assert_eq!(doc.get("B"), Some("hash#literal"));
    assert_eq!(doc.get("C"), Some("\"quoted\""));
    assert_eq!(doc.get("D"), Some("last"));
    assert_eq!(doc.item(0).unwrap().kind(), ItemKind::Comment);
    assert_eq!(doc.item(1).unwrap().kind(), ItemKind::Blank);
}

#[test]
fn docker_quoted_keys_and_invalid_boundaries_are_reported() {
    let dialect = Dialect {
        quoted_keys: true,
        ..Dialect::DOCKER_RAW
    };
    let doc = parse_strict_with("'A B'=value\nKEEP=ok\n", dialect).unwrap();
    assert_eq!(doc.get("A B"), Some("value"));
    assert_eq!(doc.get("KEEP"), Some("ok"));

    for source in ["'unterminated=value\n", "'A B'value\n"] {
        let error = parse_strict_with(source, dialect).unwrap_err();
        assert_eq!(error.code(), Code::InvalidKey, "{source:?}");
    }
}

#[test]
fn docker_key_boundaries_and_empty_keys_are_rejected() {
    for source in ["A B=value\n", "A =value\n", "=value\n", "BAD KEY\n"] {
        let error = parse_strict_with(source, Dialect::DOCKER_RAW).unwrap_err();
        assert_eq!(error.code(), Code::InvalidKey, "{source:?}");
    }
}

#[test]
fn docker_export_and_colon_overrides_keep_raw_values() {
    let export = Dialect {
        export: true,
        ..Dialect::DOCKER_RAW
    };
    let doc = parse_strict_with(" export A= value \n", export).unwrap();
    assert_eq!(doc.get("A"), Some(" value "));
    assert_eq!(
        doc.unique("A")
            .unwrap()
            .tokens()
            .export
            .as_ref()
            .map(|span| &doc.source()[span.clone()]),
        Some("export")
    );

    let any_colon = Dialect {
        colon: ColonSeparator::Any,
        ..Dialect::DOCKER_RAW
    };
    let doc = parse_strict_with("A:value\n", any_colon).unwrap();
    assert_eq!(doc.get("A"), Some("value"));

    let spaced_colon = Dialect {
        colon: ColonSeparator::Whitespace,
        ..Dialect::DOCKER_RAW
    };
    let doc = parse_strict_with("A: value\nB:value\n", spaced_colon).unwrap();
    assert_eq!(doc.get("A"), Some(" value"));
    assert_eq!(doc.get("B"), None);
    assert_eq!(doc.unique("B:value").unwrap().key(), "B:value");
}

#[test]
fn every_dialect_builder_sets_only_its_documented_field() {
    let base = Dialect::PORTABLE;

    let dialect = base.with_syntax(Syntax::Docker);
    assert_eq!(dialect.syntax, Syntax::Docker);
    assert_eq!(dialect.keys, base.keys);

    let dialect = base.with_inline_comments(InlineComments::None);
    assert_eq!(dialect.inline_comments, InlineComments::None);
    let dialect = base.with_double_escapes(DoubleEscapes::NONE);
    assert_eq!(dialect.double_escapes, DoubleEscapes::NONE);
    let dialect = base.with_unknown_escapes(UnknownEscapes::Preserve);
    assert_eq!(dialect.unknown_escapes, UnknownEscapes::Preserve);
    let dialect = base.with_single_escapes(SingleEscapes::Quote);
    assert_eq!(dialect.single_escapes, SingleEscapes::Quote);
    let dialect = base.with_backticks(true);
    assert!(dialect.backticks);
    let dialect = base.with_colon(ColonSeparator::Any);
    assert_eq!(dialect.colon, ColonSeparator::Any);
    let dialect = base.with_keys(KeySyntax::Docker);
    assert_eq!(dialect.keys, KeySyntax::Docker);
    let dialect = base.with_bare_keys(BareKeys::Ignore);
    assert_eq!(dialect.bare_keys, BareKeys::Ignore);
    let dialect = base.with_raw_values(true);
    assert!(dialect.raw_values);
    let dialect = base.with_line_breaks(LineBreaks::Lf);
    assert_eq!(dialect.line_breaks, LineBreaks::Lf);
    let dialect = base.with_bom(BomPolicy::Keep);
    assert_eq!(dialect.bom, BomPolicy::Keep);
    let dialect = base.with_duplicates(Duplicates::First);
    assert_eq!(dialect.duplicates, Duplicates::First);
    let dialect = base.with_quoted_tail(QuotedTail::Append);
    assert_eq!(dialect.quoted_tail, QuotedTail::Append);
    let dialect = base.with_multiline(Multiline::NONE);
    assert_eq!(dialect.multiline, Multiline::NONE);
    let dialect = base.with_whitespace(Whitespace::Unicode);
    assert_eq!(dialect.whitespace, Whitespace::Unicode);
    let dialect = base.with_export(false);
    assert!(!dialect.export);
    let dialect = base.with_quoted_keys(true);
    assert!(dialect.quoted_keys);
    let dialect = base.with_quoted_comments(true);
    assert!(dialect.quoted_comments);
    let dialect = base.with_normalize_newlines(true);
    assert!(dialect.normalize_newlines);
    let dialect = base.with_malformed_lines(MalformedLines::Ignore);
    assert_eq!(dialect.malformed_lines, MalformedLines::Ignore);
    let dialect = base.with_node_object_keys(true);
    assert!(dialect.node_object_keys);
    let dialect = base.with_line_byte_limit(Some(10));
    assert_eq!(dialect.line_byte_limit, Some(10));
}

#[test]
fn escape_sets_and_multiline_flags_compose_predictably() {
    assert!(DoubleEscapes::PORTABLE.contains(DoubleEscapes::LF));
    assert!(!DoubleEscapes::NONE.contains(DoubleEscapes::LF));
    let combined = DoubleEscapes::LF.union(DoubleEscapes::CR);
    assert!(combined.contains(DoubleEscapes::LF));
    assert!(combined.contains(DoubleEscapes::CR));
    assert!(!combined.contains(DoubleEscapes::TAB));

    let all = Multiline::ALL;
    assert!(all.single && all.double && all.backtick);
    let none = Multiline::NONE;
    assert!(!none.single && !none.double && !none.backtick);
    let single = Multiline::NONE.with_single(true);
    assert!(single.single && !single.double && !single.backtick);
    let double = Multiline::NONE.with_double(true);
    assert!(!double.single && double.double && !double.backtick);
    let backtick = Multiline::NONE.with_backtick(true);
    assert!(!backtick.single && !backtick.double && backtick.backtick);
}

#[test]
fn node_export_retry_colon_and_cross_line_paths() {
    let (doc, _) = parse_with("export =x\n", Dialect::NODE_DOTENV);
    assert_eq!(doc.get("export"), Some("x"));

    let (doc, diagnostics) = parse_with("A: value\nB:value\n", Dialect::NODE_DOTENV);
    assert_eq!(doc.get("A"), Some("value"));
    assert_eq!(doc.get("B"), None);
    assert!(doc.entries().all(|entry| entry.key() != "B"));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code() == Code::IgnoredStatement));

    let (doc, _) = parse_with("A= \n\"value\"\n", Dialect::NODE_DOTENV);
    assert_eq!(doc.get("A"), Some("value"));

    let source = "\u{2028}\u{2029}A=1\n";
    let (doc, _) = parse_with(source, Dialect::NODE_DOTENV);
    assert_eq!(doc.get("A"), Some("1"));
    assert_eq!(doc.source(), source);
}

#[test]
fn node_quoted_keys_and_invalid_headers_are_bounded() {
    let quoted = Dialect {
        quoted_keys: true,
        ..Dialect::NODE_DOTENV
    };
    let (doc, _) = parse_with("'A B'=value\n", quoted);
    assert_eq!(doc.get("A B"), Some("value"));

    for source in ["''=value\n", "'A B'value\n"] {
        let (doc, diagnostics) = parse_with(source, quoted);
        assert!(doc.entries().next().is_none(), "{source:?}");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code() == Code::IgnoredStatement),
            "{source:?}"
        );
    }
    let (doc, diagnostics) = parse_with("BAD KEY=value\n", Dialect::NODE_DOTENV);
    assert!(doc.entries().next().is_none());
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code() == Code::IgnoredStatement));
}

#[test]
fn compose_custom_whitespace_export_and_eof_quirks() {
    let ascii = Dialect {
        whitespace: Whitespace::Ascii,
        ..Dialect::COMPOSE
    };
    let doc = parse_strict_with("export\tA=1\n", ascii).unwrap();
    assert_eq!(doc.get("A"), Some("1"));

    let doc = parse_strict_with("export A=1\n", Dialect::COMPOSE).unwrap();
    assert_eq!(doc.get("A"), Some("1"));

    for source in ["A", "A\r", "A B"] {
        let (doc, _) = parse_with(source, Dialect::COMPOSE);
        assert_eq!(
            doc.get(""),
            Some(source.trim_end_matches('\r')),
            "{source:?}"
        );
        assert_eq!(doc.get("A"), None, "{source:?}");
    }
}

#[test]
fn compose_quoted_keys_and_invalid_eof_boundaries_are_bounded() {
    let quoted = Dialect {
        quoted_keys: true,
        ..Dialect::COMPOSE
    };
    let doc = parse_strict_with("'A B'=value\n", quoted).unwrap();
    assert_eq!(doc.get("A B"), Some("value"));

    for source in ["'unterminated=value\n", "'A B'value\n"] {
        let error = parse_strict_with(source, quoted).unwrap_err();
        assert_eq!(error.code(), Code::InvalidKey, "{source:?}");
    }
}

#[test]
fn non_whitespace_and_compose_key_policies_reject_literal_spaces() {
    let non_whitespace = Dialect {
        keys: KeySyntax::NonWhitespace,
        ..Dialect::PORTABLE
    };
    let error = parse_strict_with("A B=1\n", non_whitespace).unwrap_err();
    assert_eq!(error.code(), Code::InvalidKey);

    let error = parse_strict_with("A B=1\n", Dialect::COMPOSE).unwrap_err();
    assert_eq!(error.code(), Code::InvalidKey);
}

fn item_layout(source: &str, dialect: Dialect) -> vec::Vec<(ItemKind, &str)> {
    let (doc, _) = parse_with(source, dialect);
    doc.items()
        .map(|item| (item.kind(), &source[item.span()]))
        .collect()
}

#[test]
fn blank_and_whitespace_only_lines_are_blank_items_in_every_preset() {
    for source in [
        "B=2\n\nA=1\n",
        "B=2\n  \t\n\n  A=1\n",
        "B=2\r\n\r\n \r\nA=1\r\n",
        "\n\n# c\n\nA=1\n\n\n",
        "A=1\n \n",
        "A=1\n\n  ",
    ] {
        let expected = item_layout(source, Dialect::PORTABLE);
        for dialect in PRESETS {
            assert_eq!(
                item_layout(source, dialect),
                expected,
                "{:?} {source:?}",
                dialect.syntax
            );
        }
    }
    let expected = [
        (ItemKind::Entry, "B=2\n"),
        (ItemKind::Blank, "  \t\n"),
        (ItemKind::Blank, "\n"),
        (ItemKind::Entry, "  A=1\n"),
    ];
    for dialect in [Dialect::NODE_DOTENV, Dialect::PYTHON_DOTENV] {
        let source = "B=2\n  \t\n\n  A=1\n";
        assert_eq!(item_layout(source, dialect), expected);
        let doc = parse_strict_with(source, dialect).unwrap();
        let entry = doc.unique("A").unwrap();
        assert_eq!(&source[entry.tokens().indent.clone()], "  ");
        assert_eq!(entry.line(), 4);
    }
}

#[test]
fn blank_lines_keep_edits_sorting_and_portability_consistent() {
    let source = "B=2\nA=1\n\n# section\nD=4\nC=3\n";
    for dialect in PRESETS {
        let name = dialect.syntax;
        let doc = parse_strict_with(source, dialect).unwrap();
        assert_eq!(
            doc.sort().unwrap().as_str(),
            "A=1\nB=2\n\n# section\nC=3\nD=4\n",
            "{name:?}"
        );
        assert_eq!(
            doc.remove("D").unwrap().as_str(),
            "B=2\nA=1\n\n# section\nC=3\n",
            "{name:?}"
        );
        assert_eq!(
            doc.remove("A").unwrap().as_str(),
            "B=2\n\n# section\nD=4\nC=3\n",
            "{name:?}"
        );
        let collapsed = parse_strict_with("A=1\n\n  \n\nB=2\n", dialect)
            .unwrap()
            .format(
                &crate::FormatOptions::default()
                    .with_trim_trailing_whitespace(true)
                    .with_blank_lines(crate::BlankLines::Collapse),
            )
            .unwrap();
        assert_eq!(collapsed.as_str(), "A=1\n\nB=2\n", "{name:?}");
    }
    for dialect in PRESETS {
        let (doc, _) = parse_with(source, dialect);
        assert!(doc.portability(&PRESETS).is_empty(), "{:?}", dialect.syntax);
    }
}

#[test]
fn compose_quoted_entries_own_the_rest_of_their_physical_line() {
    assert_eq!(
        item_layout("A=\"1\"\n", Dialect::COMPOSE),
        [(ItemKind::Entry, "A=\"1\"\n")]
    );
    let source = "B=\"2\" # note\nA='1'  \nC=\"3\"D=\"4\" # d\nE=\"5\"  F=6\nG=\"7\"#g\r\n";
    assert_eq!(
        item_layout(source, Dialect::COMPOSE),
        [
            (ItemKind::Entry, "B=\"2\" # note\n"),
            (ItemKind::Entry, "A='1'  \n"),
            (ItemKind::Entry, "C=\"3\""),
            (ItemKind::Entry, "D=\"4\" # d\n"),
            (ItemKind::Entry, "E=\"5\""),
            (ItemKind::Entry, "  F=6\n"),
            (ItemKind::Entry, "G=\"7\"#g\r\n"),
        ]
    );
    let doc = parse_strict_with(source, Dialect::COMPOSE).unwrap();
    for (key, value) in [
        ("A", "1"),
        ("B", "2"),
        ("C", "3"),
        ("D", "4"),
        ("E", "5"),
        ("F", "6"),
        ("G", "7"),
    ] {
        assert_eq!(doc.get(key), Some(value), "{key}");
    }
    assert_eq!(doc.unique("B").unwrap().inline_comment(), Some("# note"));
    assert_eq!(doc.unique("D").unwrap().inline_comment(), Some("# d"));
    assert_eq!(doc.unique("G").unwrap().inline_comment(), Some("#g"));
    assert_eq!(doc.unique("C").unwrap().inline_comment(), None);
    let a = doc.unique("A").unwrap();
    assert_eq!(&source[a.tokens().trailing_space.clone()], "  ");
    assert_eq!(a.inline_comment(), None);

    let multiline = "A=\"1\n2\" # c\r\nB=x\n";
    assert_eq!(
        item_layout(multiline, Dialect::COMPOSE),
        [
            (ItemKind::Entry, "A=\"1\n2\" # c\r\n"),
            (ItemKind::Entry, "B=x\n")
        ]
    );
}

#[test]
fn compose_quoted_entries_keep_line_structure_under_edits() {
    let doc = parse_strict_with(
        "B=\"2\" # note for B\nA=\"1\" # note for A\n",
        Dialect::COMPOSE,
    )
    .unwrap();
    assert_eq!(
        doc.sort().unwrap().as_str(),
        "A=\"1\" # note for A\nB=\"2\" # note for B\n"
    );

    let doc = parse_strict_with("A=\"1\" # c\nB=2\n", Dialect::COMPOSE).unwrap();
    let inserted = doc
        .insert("C", Some("z"), InsertPosition::AfterKey("A"))
        .unwrap();
    assert_eq!(inserted.as_str(), "A=\"1\" # c\nC=z\nB=2\n");

    let doc = parse_strict_with("A=\"1\"\nB=2\n", Dialect::COMPOSE).unwrap();
    assert_eq!(doc.remove("A").unwrap().as_str(), "B=2\n");

    let doc = parse_strict_with("A=\"1\" # c\nB=\"2\"\n", Dialect::COMPOSE).unwrap();
    assert_eq!(doc.example(), "A=\"\" # c\nB=\n");
}

#[test]
fn compose_quoted_entries_keep_blank_line_groups_when_formatting() {
    let doc = parse_strict_with("A=\"1\"   \n\n\nB=\"2\" # b  \n", Dialect::COMPOSE).unwrap();
    let options = crate::FormatOptions::default()
        .with_trim_trailing_whitespace(true)
        .with_blank_lines(crate::BlankLines::Collapse);
    let formatted = doc.format(&options).unwrap();
    assert_eq!(formatted.as_str(), "A=\"1\"\n\nB=\"2\" # b\n");
}

#[test]
fn quoted_key_diagnostics_count_line_breaks_in_leading_whitespace() {
    let mut wrong = vec::Vec::new();
    for preset in PRESETS {
        let mut dialect = preset
            .with_quoted_keys(true)
            .with_line_breaks(LineBreaks::Lf);
        if preset.whitespace == Whitespace::Ascii {
            dialect = dialect.with_whitespace(Whitespace::Unicode);
        }
        let name = preset.syntax;
        let source = "\r'unterminated=value\n";
        let (_, diagnostics) = parse_with(source, dialect);
        let error = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.span().start == 1)
            .unwrap();
        let position = crate::LineIndex::new(source)
            .to_position(error.span().start, crate::ColumnEncoding::Utf8)
            .unwrap();
        assert_eq!(position.line + 1, 2, "{name:?}");
        let valid = parse_strict_with("\r'valid'=value\n", dialect).unwrap();
        assert_eq!(valid.unique("valid").unwrap().line(), 2, "{name:?}");
        if error.line() != 2 {
            wrong.push((name, error.line()));
        }
    }
    assert_eq!(wrong, []);
}

#[test]
fn double_escape_docs_show_the_escape_sequences_verbatim() {
    let source = include_str!("dialect.rs");
    let start = source.find("impl DoubleEscapes {").unwrap();
    let source = &source[start..];
    let mut wrong = vec::Vec::new();
    for (name, sequence) in [
        ("LF", r"`\n`"),
        ("CR", r"`\r`"),
        ("TAB", r"`\t`"),
        ("BACKSLASH", r"`\\`"),
        ("DOUBLE_QUOTE", r#"`\"`"#),
        ("DOLLAR", r"`\$`"),
        ("SINGLE_QUOTE", r"`\'`"),
        ("BELL", r"`\a`"),
        ("BACKSPACE", r"`\b`"),
        ("FORM_FEED", r"`\f`"),
        ("VERTICAL_TAB", r"`\v`"),
        ("OCTAL", r"`\0`"),
    ] {
        let declaration = source.find(&format!("pub const {name}: Self")).unwrap();
        let doc = source[..declaration]
            .trim_end()
            .lines()
            .last()
            .unwrap()
            .trim();
        let doubled = format!("`\\{}", &sequence[1..]);
        if !doc.starts_with("///") || !doc.contains(sequence) || doc.contains(&doubled) {
            wrong.push((name, doc));
        }
    }
    assert_eq!(wrong, []);
}

#[test]
fn statements_sharing_one_physical_line_scan_each_byte_a_bounded_number_of_times() {
    let next_statement = Dialect {
        quoted_tail: QuotedTail::NextStatement,
        ..Dialect::NODE_DOTENV
    };
    let limited = |dialect: Dialect| Dialect {
        line_byte_limit: Some(65_535),
        ..dialect
    };
    let mut excessive = vec::Vec::new();
    for (statement, dialect) in [
        ("A=\"x\"", Dialect::COMPOSE),
        ("A=\"x\"", limited(Dialect::COMPOSE)),
        ("A=\"x\"\u{2028}", limited(Dialect::NODE_DOTENV)),
        ("A=\"x\" ", Dialect::COMPOSE),
        ("A=\"x\"\u{2028}", Dialect::NODE_DOTENV),
        ("A=1 #\u{2028}", Dialect::NODE_DOTENV),
        ("A=\"x\"", next_statement),
        ("A=\"x\"\n", Dialect::PORTABLE),
        ("A=\"x\"\n", Dialect::PYTHON_DOTENV),
        ("A=x\n", Dialect::DOCKER_RAW),
    ] {
        let source = statement.repeat(4_000);
        let scanned = crate::parser::scanned_bytes(&source, dialect);
        if scanned > 4 * source.len() {
            excessive.push((dialect.syntax, statement, scanned, source.len()));
        }
    }
    assert_eq!(excessive, []);
}

#[test]
fn node_and_python_blank_lines_format_and_sort_within_groups() {
    let trim = crate::FormatOptions::default().with_trim_trailing_whitespace(true);
    for dialect in [Dialect::NODE_DOTENV, Dialect::PYTHON_DOTENV] {
        let name = dialect.syntax;
        let (doc, _) = parse_with(" \n ", dialect);
        let trimmed = doc.format(&trim).unwrap();
        assert_eq!(trimmed.as_str(), "\n ", "{name:?}");
        let (again, _) = parse_with(trimmed.as_str(), dialect);
        assert!(again.format(&trim).unwrap().is_empty(), "{name:?}");

        let (doc, _) = parse_with("A=1\n\n\n   ", dialect);
        let collapsed = doc
            .format(&crate::FormatOptions::normalize_groups())
            .unwrap();
        assert_eq!(collapsed.as_str(), "A = 1\n\n", "{name:?}");

        let (doc, _) = parse_with("C=1\nB=2\n\nZ=1\nA=1", dialect);
        assert_eq!(
            doc.sort().unwrap().as_str(),
            "B=2\nC=1\n\nA=1\nZ=1",
            "{name:?}"
        );
    }
}

#[test]
fn compose_quoted_entries_sort_when_each_owns_its_line() {
    let (doc, _) = parse_with("B=\"2\"\nA=\"1\"", Dialect::COMPOSE);
    assert_eq!(doc.sort().unwrap().as_str(), "A=\"1\"\nB=\"2\"");
}
