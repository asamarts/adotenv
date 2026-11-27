use crate::{
    parse, parse_with, remove, set, AssignmentState, DiagnosticCode, Edit, EditError,
    InsertPosition, ItemKind, Quote, Severity, TextEdit,
};
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::fmt::Write as _;
use proptest::prelude::*;

// The documented contract: apply the returned slice from its last element to
// its first, each against the text produced so far.
fn apply_in_reverse(source: &str, edits: &[TextEdit]) -> String {
    let mut text = source.to_string();
    for edit in edits.iter().rev() {
        text.replace_range(edit.range.clone(), &edit.replacement);
    }
    text
}

fn replay(source: &str, result: &crate::VerifiedEdits) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    for edit in result.edits() {
        assert!(edit.range.start >= cursor);
        assert!(source.is_char_boundary(edit.range.start));
        assert!(source.is_char_boundary(edit.range.end));
        output.push_str(&source[cursor..edit.range.start]);
        output.push_str(&edit.replacement);
        cursor = edit.range.end;
    }
    output.push_str(&source[cursor..]);
    assert_eq!(output, result.as_str());
    output
}

#[test]
fn verified_edits_into_edits_and_error_display_are_stable() {
    let doc = parse("A=1\n").0;
    let result = doc.set("A", "2").unwrap();
    let expected = result.edits().to_vec();
    assert_eq!(result.into_edits(), expected);
    assert!(format!("{}", EditError::Missing).contains("requested key"));
    assert!(format!("{}", EditError::Duplicate(2)).contains("2 declarations"));
    assert!(format!("{}", EditError::InvalidTarget).contains("out of range"));
    assert!(format!("{}", EditError::InvalidKey).contains("cannot be represented"));
    assert!(format!("{}", EditError::InvalidReferenceKey).contains("variable reference"));
    assert!(format!("{}", EditError::Unencodable).contains("selected dialect"));
    assert!(format!("{}", EditError::ConflictingEdits).contains("conflicting"));
    assert!(format!("{}", EditError::WouldChangeMeaning).contains("change other parts"));
    let limit_err = doc
        .set_with_options(
            "A",
            "12345",
            crate::EditOptions::new().with_max_output_bytes(3),
        )
        .unwrap_err();
    assert!(format!("{limit_err}").contains("edit exceeded limit:"));
}

#[test]
fn native_value_edits_retain_tokens_and_quote_style() {
    let source = "\u{feff}  export A \t= 'old'  # note\r\nB=stay\r\n";
    let (doc, _) = parse(source);
    let result = doc.set("A", "new\\literal\nλ").unwrap();
    assert_eq!(
        result.as_str(),
        "\u{feff}  export A \t= 'new\\literal\nλ'  # note\r\nB=stay\r\n"
    );
    assert_eq!(result.edits().len(), 1);
    assert_eq!(
        result.edits()[0].range,
        doc.unique("A").unwrap().value_span().unwrap()
    );
    replay(source, &result);
    let result = doc.set("A", "can't\"\r\t").unwrap();
    assert_eq!(parse(result.as_str()).0.get("A"), Some("can't\"\r\t"));
    assert_eq!(
        parse(result.as_str()).0.unique("A").unwrap().quote(),
        Quote::Double
    );
    let (double, _) = parse("A=\"old\"\n");
    assert_eq!(
        double.set("A", "simple").unwrap().as_str(),
        "A=\"simple\"\n"
    );
}

#[test]
fn empty_and_bare_assignments_preserve_comment_separation() {
    for source in ["A= # note\n", "A \t# note\n", "A=\t# note\n"] {
        let (doc, _) = parse(source);
        let result = doc.set("A", "literal").unwrap();
        let (after, _) = parse(result.as_str());
        assert_eq!(after.get("A"), Some("literal"));
        assert!(result.as_str().ends_with("# note\n"));
        assert!(after.unique("A").unwrap().tokens().inline_comment.is_some());
        replay(source, &result);
    }
    assert_eq!(set("A", "A", "").unwrap(), "A=");
    assert_eq!(set("A=", "A", "x").unwrap(), "A=x");
    assert_eq!(set("A # retained", "A", "").unwrap(), "A= # retained");
}

#[test]
fn setting_empty_does_not_grow_a_directly_attached_comment() {
    for (dialect, source) in [
        (crate::Dialect::PORTABLE, "PORT=#note\n"),
        (crate::Dialect::PYTHON_DOTENV, "PORT=#note\n"),
        (crate::Dialect::NODE_DOTENV, "PORT=#note\n"),
    ] {
        let (doc, _) = parse_with(source, dialect);
        let once = doc.set("PORT", "").unwrap();
        let (again, _) = parse_with(once.as_str(), dialect);
        let twice = again.set("PORT", "").unwrap();
        assert_eq!(once.as_str(), twice.as_str(), "{dialect:?}");
    }
}

#[test]
fn batch_combines_independent_properties_on_original_occurrences() {
    let source = "# heading\r\nA=old\r\nB=two\r\nC=three";
    let (doc, _) = parse(source);
    let result = doc
        .edit(&[
            Edit::Set {
                entry: 0,
                value: "new value",
            },
            Edit::Rename {
                entry: 0,
                key: "RENAMED",
            },
            Edit::Export {
                entry: 0,
                enabled: true,
            },
            Edit::Remove {
                item: doc.unique("B").unwrap().item_id(),
            },
            Edit::Set {
                entry: 2,
                value: "last",
            },
            Edit::Insert {
                key: "D",
                value: None,
                position: InsertPosition::AfterEntry(2),
            },
        ])
        .unwrap();
    let (after, _) = parse(result.as_str());
    let meanings: Vec<_> = after
        .entries()
        .map(|entry| (entry.key(), entry.value(), entry.exported()))
        .collect();
    assert_eq!(
        meanings,
        [
            ("RENAMED", Some("new value"), true),
            ("C", Some("last"), false),
            ("D", None, false)
        ]
    );
    assert!(result.as_str().starts_with("# heading\r\n"));
    assert!(!result.as_str().ends_with('\n'));
    replay(source, &result);
}

#[test]
fn eof_insertions_do_not_merge_with_simultaneously_assigned_bare_keys() {
    for source in ["A", "A="] {
        let (doc, _) = parse(source);
        let result = doc
            .edit(&[
                Edit::Set {
                    entry: 0,
                    value: "x",
                },
                Edit::Rename {
                    entry: 0,
                    key: "RENAMED",
                },
                Edit::Export {
                    entry: 0,
                    enabled: true,
                },
                Edit::Insert {
                    key: "B",
                    value: Some("y"),
                    position: InsertPosition::End,
                },
            ])
            .unwrap();
        assert_eq!(result.as_str(), "export RENAMED=x\nB=y");
        replay(source, &result);
    }
}

#[test]
fn insertion_before_an_exported_and_renamed_target_is_ordered() {
    let (doc, _) = parse("B=old\n");
    let result = doc
        .edit(&[
            Edit::Export {
                entry: 0,
                enabled: true,
            },
            Edit::Rename { entry: 0, key: "C" },
            Edit::Insert {
                key: "A",
                value: Some("first"),
                position: InsertPosition::Sorted,
            },
        ])
        .unwrap();
    assert_eq!(result.as_str(), "A=first\nexport C=old\n");
    replay(doc.source(), &result);
}

#[test]
fn commenting_and_batch_uncommenting_restore_multiline_unicode_and_endings() {
    for source in [
        "A='α\nβ\nγ' # note\nB=ok\n",
        "A=\"α\r\nβ\"\r\nB=ok\r\n",
        "A='α\rβ'",
    ] {
        let (doc, _) = parse(source);
        let commented = doc.comment("A").unwrap();
        replay(source, &commented);
        let (comments, _) = parse(commented.as_str());
        assert!(comments.get_entry("A").is_none());
        assert_eq!(comments.get("B"), doc.get("B"));
        let selected: Vec<_> = comments
            .items()
            .take_while(|item| item.kind() == ItemKind::Comment)
            .map(|item| Edit::Uncomment { item: item.id() })
            .collect();
        assert!(selected.len() >= 2);
        let restored = comments.edit(&selected).unwrap();
        assert_eq!(restored.as_str(), source);
        replay(commented.as_str(), &restored);
    }
}

#[test]
fn uncommenting_must_not_capture_unselected_comments_or_entries() {
    for source in [
        "# A='open\n# close'\nB=ok\n",
        "# A='open\nB=close'\n",
        "# A=\"open\n# close\"\n",
    ] {
        let (doc, _) = parse(source);
        assert_eq!(doc.uncomment(0), Err(EditError::WouldChangeMeaning));
    }
    let (doc, _) = parse("# A='open\n# close'\nB=ok\n");
    let result = doc
        .edit(&[Edit::Uncomment { item: 0 }, Edit::Uncomment { item: 1 }])
        .unwrap();
    assert_eq!(result.as_str(), "A='open\nclose'\nB=ok\n");
    assert_eq!(parse(result.as_str()).0.get("A"), Some("open\nclose"));
}

#[test]
fn batch_uncomment_verifies_the_complete_result_not_each_action() {
    let source = "# A='line one\n# line two'\n# B=old\nC=stay\n";
    let (doc, _) = parse(source);
    let result = doc
        .edit(&[
            Edit::Uncomment { item: 0 },
            Edit::Uncomment { item: 1 },
            Edit::Uncomment { item: 2 },
            Edit::Set {
                entry: 0,
                value: "changed",
            },
        ])
        .unwrap();
    assert_eq!(
        result.as_str(),
        "A='line one\nline two'\nB=old\nC=changed\n"
    );
    replay(source, &result);
}

#[test]
fn recovering_edits_preserve_invalid_bytes_and_primary_diagnostic_identity() {
    let source = "=bad\nA=old\nBROKEN=\"x\" tail\n# untouched\nB\n";
    let (doc, before) = parse(source);
    let result = doc
        .edit(&[
            Edit::Set {
                entry: doc.unique("A").unwrap().id(),
                value: "much longer",
            },
            Edit::Export {
                entry: doc.unique("B").unwrap().id(),
                enabled: true,
            },
        ])
        .unwrap();
    let (after, diagnostics) = parse(result.as_str());
    assert_eq!(after.get("A"), Some("much longer"));
    assert!(after.unique("B").unwrap().exported());
    let invalid_before: Vec<_> = doc
        .items()
        .filter(|item| item.kind() == ItemKind::Invalid)
        .map(|item| (item.raw(), item.invalid_code()))
        .collect();
    let invalid_after: Vec<_> = after
        .items()
        .filter(|item| item.kind() == ItemKind::Invalid)
        .map(|item| (item.raw(), item.invalid_code()))
        .collect();
    assert_eq!(invalid_after, invalid_before);
    assert_eq!(
        before
            .iter()
            .filter(|d| matches!(
                d.code(),
                DiagnosticCode::InvalidKey | DiagnosticCode::TrailingText
            ))
            .count(),
        2
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|d| matches!(
                d.code(),
                DiagnosticCode::InvalidKey | DiagnosticCode::TrailingText
            ))
            .count(),
        2
    );
    assert_eq!(set(source, "BROKEN", "fixed"), Err(EditError::Missing));
    replay(source, &result);
}

#[test]
fn invalid_items_can_only_be_removed_or_commented_explicitly() {
    let source = "A=ok\nBROKEN='first\nsecond' trailing\n";
    let (doc, _) = parse(source);
    let invalid = doc
        .items()
        .find(|item| item.kind() == ItemKind::Invalid)
        .unwrap();
    assert_eq!(doc.remove("BROKEN"), Err(EditError::Missing));
    let commented = doc.edit(&[Edit::Comment { item: invalid.id() }]).unwrap();
    assert_eq!(
        commented.as_str(),
        "A=ok\n# BROKEN='first\n# second' trailing\n"
    );
    assert!(parse(commented.as_str())
        .0
        .items()
        .all(|item| item.kind() != ItemKind::Invalid));
    let removed = doc.edit(&[Edit::Remove { item: invalid.id() }]).unwrap();
    assert_eq!(removed.as_str(), "A=ok\n");
    let (doc, _) = parse("# belongs to nobody\nA=x\n# after\n");
    assert_eq!(
        doc.remove("A").unwrap().as_str(),
        "# belongs to nobody\n# after\n"
    );
    assert_eq!(
        doc.edit(&[Edit::Remove { item: 0 }]).unwrap().as_str(),
        "A=x\n# after\n"
    );
}

#[test]
fn duplicate_keys_require_occurrences_but_occurrence_batches_are_supported() {
    let source = "A=one\nA=two\nA\nB=stay\n";
    let (doc, _) = parse(source);
    assert_eq!(doc.set("A", "x"), Err(EditError::Duplicate(3)));
    assert_eq!(doc.remove("A"), Err(EditError::Duplicate(3)));
    assert_eq!(doc.comment("A"), Err(EditError::Duplicate(3)));
    assert_eq!(doc.rename("A", "X"), Err(EditError::Duplicate(3)));
    assert_eq!(doc.set_export("A", true), Err(EditError::Duplicate(3)));
    assert_eq!(
        doc.insert("C", None, InsertPosition::AfterKey("A")),
        Err(EditError::Duplicate(3))
    );
    let result = doc
        .edit(&[
            Edit::Set {
                entry: 0,
                value: "first",
            },
            Edit::Rename {
                entry: 1,
                key: "RENAMED",
            },
            Edit::Export {
                entry: 1,
                enabled: true,
            },
            Edit::Remove {
                item: doc.entry(2).unwrap().item_id(),
            },
            Edit::Insert {
                key: "C",
                value: None,
                position: InsertPosition::AfterEntry(1),
            },
        ])
        .unwrap();
    assert_eq!(result.as_str(), "A=first\nexport RENAMED=two\nC\nB=stay\n");
    replay(source, &result);
}

#[test]
fn rename_with_references_updates_declaration_and_matching_names() {
    let source = "HOST=localhost\nURL=${HOST}/api\nPORT=$HOST\n# keep HOST\n";
    let (doc, _) = parse(source);
    let result = doc.rename_with_references("HOST", "SERVER").unwrap();
    assert_eq!(
        result.as_str(),
        "SERVER=localhost\nURL=${SERVER}/api\nPORT=$SERVER\n# keep HOST\n"
    );
    assert_eq!(result.edits().len(), 3);
    replay(source, &result);
    let (after, _) = parse(result.as_str());
    assert_eq!(after.get_entry("SERVER").unwrap().line(), 1);
    let reference = &after.unique("URL").unwrap().references()[0];
    assert_eq!(
        reference.name.clone().map(|name| &after.source()[name]),
        Some("SERVER")
    );
}

#[test]
fn rename_with_references_braces_a_dotted_unbraced_name() {
    let source = "HOST=localhost\nURL=$HOST/api\n";
    let (doc, _) = parse(source);
    let result = doc.rename_with_references("HOST", "APP.HOST").unwrap();
    assert_eq!(result.as_str(), "APP.HOST=localhost\nURL=${APP.HOST}/api\n");
    assert_eq!(result.edits().len(), 2);
    replay(source, &result);
    let (after, _) = parse(result.as_str());
    let reference = &after.unique("URL").unwrap().references()[0];
    assert_eq!(
        reference.name.clone().map(|name| &after.source()[name]),
        Some("APP.HOST")
    );
}

#[test]
fn rename_with_references_handles_nested_operators_and_existing_targets() {
    let source = "HOST=localhost\nURL=${HOST:-$HOST}/api\nOTHER=$HOST\n";
    let (doc, _) = parse(source);
    let result = doc.rename_with_references("HOST", "SERVER").unwrap();
    assert_eq!(
        result.as_str(),
        "SERVER=localhost\nURL=${SERVER:-$SERVER}/api\nOTHER=$SERVER\n"
    );
    assert_eq!(result.edits().len(), 4);
    replay(source, &result);

    let source = "A=1\nB=$NEW\nC=$A\n";
    let (doc, _) = parse(source);
    let result = doc.rename_with_references("A", "NEW").unwrap();
    assert_eq!(result.as_str(), "NEW=1\nB=$NEW\nC=$NEW\n");
    replay(source, &result);

    let source = "A=1\nURL=${A:-$A}\n";
    let (doc, _) = parse(source);
    let result = doc.rename_with_references("A", "APP.A").unwrap();
    assert_eq!(result.as_str(), "APP.A=1\nURL=${APP.A:-${APP.A}}\n");
    replay(source, &result);
}

#[test]
fn rename_with_references_keeps_escaped_and_single_quoted_text() {
    let source = "A=1\nB=\"\\$A\"\nC='$A'\n";
    let (doc, _) = parse(source);
    let result = doc.rename_with_references("A", "SERVER").unwrap();
    assert_eq!(result.as_str(), "SERVER=1\nB=\"\\$SERVER\"\nC='$SERVER'\n");
    replay(source, &result);
}

#[test]
fn rename_with_references_handles_quoted_keys() {
    let dialect = crate::Dialect::PYTHON_DOTENV;
    let (doc, _) = parse_with("'A B'=1\nX=${A B}\n", dialect);
    let result = doc.rename_with_references("A B", "C").unwrap();
    assert_eq!(result.as_str(), "'C'=1\nX=${C}\n");
    let (doc, _) = parse_with("'A B'=1\nX=${A B}\n", dialect);
    let result = doc.rename_with_references("A B", "D E").unwrap();
    assert_eq!(result.as_str(), "'D E'=1\nX=${D E}\n");
}

#[test]
fn rename_with_references_preserves_bom_crlf_and_multiline_items() {
    let source = "\u{feff}A=1\r\nB='line one\r\n$A'\r\nC=$A\r\n";
    let (doc, _) = parse(source);
    let result = doc.rename_with_references("A", "NEW").unwrap();
    assert_eq!(
        result.as_str(),
        "\u{feff}NEW=1\r\nB='line one\r\n$NEW'\r\nC=$NEW\r\n"
    );
    replay(source, &result);
}

#[test]
fn rename_with_references_preserves_unrelated_source_and_rejects_unspellable_names() {
    let source = "# A=old\nBAD KEY=value\nA=1\nB=$A\n";
    let (doc, _) = parse(source);
    let result = doc.rename_with_references("A", "RENAMED").unwrap();
    assert_eq!(
        result.as_str(),
        "# A=old\nBAD KEY=value\nRENAMED=1\nB=$RENAMED\n"
    );
    replay(source, &result);

    let (doc, _) = parse("A=1\nB=$A\n");
    assert_eq!(
        doc.rename_with_references("A", "A-B"),
        Err(EditError::InvalidReferenceKey)
    );
    let (doc, _) = parse("A=1\nB=2\n");
    assert_eq!(
        doc.rename_with_references("A", "A-B").unwrap().as_str(),
        "A-B=1\nB=2\n"
    );
}

#[test]
fn rename_with_references_rejects_a_new_line_limit_error() {
    let dialect = crate::Dialect::PORTABLE.with_line_byte_limit(Some(8));
    let (doc, _) = parse_with("A=1\nC=xxxx$A\n", dialect);
    assert_eq!(
        doc.rename_with_references("A", "LONG"),
        Err(EditError::WouldChangeMeaning)
    );
}

#[test]
fn rename_with_references_uses_regular_rename_errors() {
    let (doc, _) = parse("A=1\nA=2\n");
    assert_eq!(
        doc.rename_with_references("A", "B"),
        Err(EditError::Duplicate(2))
    );
    let (doc, _) = parse("A=1\nB=2\n");
    assert_eq!(
        doc.rename_with_references("A", "B"),
        Err(EditError::Duplicate(2))
    );
    assert_eq!(
        doc.rename_with_references("MISSING", "B"),
        Err(EditError::Missing)
    );
}

#[test]
fn insertion_preserves_endings_and_final_newline_conventions() {
    for (source, expected) in [
        ("", "B=x"),
        ("\u{feff}", "\u{feff}B=x"),
        ("A=one", "A=one\nB=x"),
        ("A=one\n", "A=one\nB=x\n"),
        ("A=one\r\n", "A=one\r\nB=x\r\n"),
        (
            "A='one\r\ntwo\r\nthree\r\nfour'\n",
            "A='one\r\ntwo\r\nthree\r\nfour'\nB=x\r\n",
        ),
        ("# final comment", "# final comment\nB=x"),
    ] {
        let (doc, _) = parse(source);
        let result = doc.insert("B", Some("x"), InsertPosition::End).unwrap();
        assert_eq!(result.as_str(), expected);
        replay(source, &result);
    }
    let (doc, _) = parse("A=one\r\nC=three");
    assert_eq!(
        doc.insert("B", Some(""), InsertPosition::AfterKey("A"))
            .unwrap()
            .as_str(),
        "A=one\r\nB=\r\nC=three"
    );
    let result = doc
        .edit(&[
            Edit::Insert {
                key: "BB",
                value: None,
                position: InsertPosition::Sorted,
            },
            Edit::Insert {
                key: "B",
                value: Some("two"),
                position: InsertPosition::Sorted,
            },
        ])
        .unwrap();
    assert_eq!(result.as_str(), "A=one\r\nB=two\r\nBB\r\nC=three");
    assert_eq!(
        parse(result.as_str()).0.unique("BB").unwrap().state(),
        AssignmentState::Bare
    );
}

#[test]
fn export_toggle_preserves_indentation_and_unrelated_assignment_spacing() {
    let (doc, _) = parse(" \texport\t A \t=\t\"x\" # note\nB=two\n");
    let result = doc.set_export("A", false).unwrap();
    assert_eq!(result.as_str(), " \tA \t=\t\"x\" # note\nB=two\n");
    let (doc, _) = parse(result.as_str());
    assert_eq!(
        doc.set_export("A", true).unwrap().as_str(),
        " \texport A \t=\t\"x\" # note\nB=two\n"
    );
}

#[test]
fn invalid_selectors_keys_controls_and_conflicts_return_errors() {
    let (doc, _) = parse("A=one\nB=two\n# comment\n");
    for operation in [
        Edit::Set {
            entry: usize::MAX,
            value: "x",
        },
        Edit::Rename {
            entry: usize::MAX,
            key: "C",
        },
        Edit::Export {
            entry: usize::MAX,
            enabled: true,
        },
        Edit::Remove { item: usize::MAX },
        Edit::Comment { item: usize::MAX },
        Edit::Uncomment { item: usize::MAX },
        Edit::Uncomment { item: 0 },
        Edit::Insert {
            key: "C",
            value: None,
            position: InsertPosition::AfterEntry(usize::MAX),
        },
    ] {
        assert_eq!(doc.edit(&[operation]), Err(EditError::InvalidTarget));
    }
    for key in ["", "bad key", "A\nB", "A=B", "λ"] {
        assert_eq!(doc.rename("A", key), Err(EditError::InvalidKey));
        assert_eq!(
            doc.insert(key, None, InsertPosition::End),
            Err(EditError::InvalidKey)
        );
    }
    assert!(doc.rename("A", "B").is_err());
    for operations in [
        vec![
            Edit::Set {
                entry: 0,
                value: "x",
            },
            Edit::Set {
                entry: 0,
                value: "y",
            },
        ],
        vec![
            Edit::Remove { item: 0 },
            Edit::Set {
                entry: 0,
                value: "x",
            },
        ],
        vec![
            Edit::Comment { item: 0 },
            Edit::Export {
                entry: 0,
                enabled: true,
            },
        ],
        vec![Edit::Uncomment { item: 2 }, Edit::Remove { item: 2 }],
    ] {
        assert_eq!(doc.edit(&operations), Err(EditError::ConflictingEdits));
    }
}

#[test]
fn empty_batch_is_lossless_even_with_recovery_duplicates_and_bom() {
    let source = "\u{feff}# comment\r\nA=one\r\nA\n=x\nB='unterminated";
    let (doc, _) = parse(source);
    let result = doc.edit(&[]).unwrap();
    assert_eq!(result.as_str(), source);
    assert_eq!(result.edits(), []);
    assert_eq!(
        remove("A=x\n\u{feff}B=y\n", "A"),
        Err(EditError::WouldChangeMeaning)
    );
}

#[test]
fn insertion_cannot_be_absorbed_by_an_unterminated_unselected_statement() {
    let (doc, _) = parse("A='unterminated");
    assert_eq!(
        doc.insert("B", Some("has'quote"), InsertPosition::End),
        Err(EditError::WouldChangeMeaning)
    );
    assert_eq!(
        doc.insert("B", Some("safe"), InsertPosition::End)
            .unwrap()
            .as_str(),
        "A='unterminated\nB=safe"
    );
    let (doc, _) = parse("# A='one\n# two'\n");
    assert_eq!(
        doc.edit(&[
            Edit::Uncomment { item: 0 },
            Edit::Uncomment { item: 1 },
            Edit::Insert {
                key: "B",
                value: Some("x"),
                position: InsertPosition::End
            },
        ])
        .unwrap()
        .as_str(),
        "A='one\ntwo'\nB=x\n"
    );
}

#[test]
fn uncommenting_an_empty_eof_comment_preserves_the_remaining_source() {
    for (source, item, expected) in [
        ("#", 0, ""),
        ("# ", 0, ""),
        ("\u{feff}#", 0, "\u{feff}"),
        ("A=1\n#", 1, "A=1\n"),
    ] {
        let (doc, _) = parse(source);
        let result = doc.uncomment(item).unwrap();
        assert_eq!(result.as_str(), expected);
        replay(source, &result);
    }
}

#[test]
fn uncomment_batches_can_restore_a_statement_across_explicit_removals() {
    let source = "# A='one\n# discard\n# two'\n";
    let (doc, _) = parse(source);
    let result = doc
        .edit(&[
            Edit::Uncomment { item: 0 },
            Edit::Remove { item: 1 },
            Edit::Uncomment { item: 2 },
        ])
        .unwrap();
    assert_eq!(result.as_str(), "A='one\ntwo'\n");
    replay(source, &result);
}

#[test]
fn final_insertions_use_the_planned_output_for_newline_separation() {
    let source = "A=1\nZ=2";
    let (doc, _) = parse(source);
    let result = doc
        .edit(&[
            Edit::Remove { item: 1 },
            Edit::Insert {
                key: "M",
                value: None,
                position: InsertPosition::AfterEntry(0),
            },
        ])
        .unwrap();
    assert_eq!(result.as_str(), "A=1\nM");
    replay(source, &result);
    for (source, expected) in [("#", "M=x"), ("# \u{feff}", "\u{feff}M=x")] {
        let (doc, _) = parse(source);
        let result = doc
            .edit(&[
                Edit::Uncomment { item: 0 },
                Edit::Insert {
                    key: "M",
                    value: Some("x"),
                    position: InsertPosition::End,
                },
            ])
            .unwrap();
        assert_eq!(result.as_str(), expected);
        replay(source, &result);
    }
}

#[test]
fn uncomment_can_expose_a_selected_bom_but_never_an_unselected_one() {
    for (source, operations) in [
        ("# \u{feff}A=1", vec![Edit::Uncomment { item: 0 }]),
        (
            "Z=2\n# \u{feff}A=1",
            vec![Edit::Remove { item: 0 }, Edit::Uncomment { item: 1 }],
        ),
    ] {
        let (doc, _) = parse(source);
        let result = doc.edit(&operations).unwrap();
        assert_eq!(result.as_str(), "\u{feff}A=1");
        let (after, _) = parse(result.as_str());
        assert!(after.has_bom());
        assert_eq!(after.get("A"), Some("1"));
        replay(source, &result);
    }
    let (doc, _) = parse("# discard\n\u{feff}A=1");
    assert_eq!(
        doc.edit(&[Edit::Remove { item: 0 }]),
        Err(EditError::WouldChangeMeaning)
    );
}

#[test]
fn strip_all_bom_edits_preserve_alignment_and_reject_bom_run_changes() {
    let strip_all = crate::Dialect::PORTABLE.with_bom(crate::BomPolicy::StripAll);
    let source = "\u{feff}\u{feff}A=1\nB=2\n";
    let doc = crate::parse_strict_with(source, strip_all).unwrap();
    let edited = doc.set("A", "9").unwrap();
    assert_eq!(edited.as_str(), "\u{feff}\u{feff}A=9\nB=2\n");
    assert_eq!(edited.edits()[0].range, 8..9);
    replay(source, &edited);

    let renamed = doc.rename_with_references("A", "ALPHA").unwrap();
    assert_eq!(renamed.as_str(), "\u{feff}\u{feff}ALPHA=1\nB=2\n");
    replay(source, &renamed);

    // Removing the first line must not silently extend a StripAll BOM run.
    let (exposed, _) = parse_with("\u{feff}A=1\n\u{feff}B=2\n", strip_all);
    assert_eq!(exposed.bom_span(), Some(0..3));
    assert_eq!(
        exposed.edit(&[Edit::Remove { item: 0 }]),
        Err(EditError::WouldChangeMeaning)
    );

    // Explicitly uncommenting a leading multi-BOM comment is allowed.
    let (commented, _) = parse_with("# \u{feff}\u{feff}A=1\n", strip_all);
    let uncommented = commented.edit(&[Edit::Uncomment { item: 0 }]).unwrap();
    assert_eq!(uncommented.as_str(), "\u{feff}\u{feff}A=1\n");
    let (after, _) = parse_with(uncommented.as_str(), strip_all);
    assert_eq!(after.bom_span(), Some(0..6));
    assert_eq!(after.get("A"), Some("1"));
    replay("# \u{feff}\u{feff}A=1\n", &uncommented);

    // Uncommenting a BOM-only comment after an existing BOM under StripAll + insert.
    let (extend, _) = parse_with("\u{feff}# \u{feff}", strip_all);
    let extended = extend
        .edit(&[
            Edit::Uncomment { item: 0 },
            Edit::Insert {
                key: "A",
                value: Some("1"),
                position: InsertPosition::End,
            },
        ])
        .unwrap();
    assert_eq!(extended.as_str(), "\u{feff}\u{feff}A=1");
    let (after, _) = parse_with(extended.as_str(), strip_all);
    assert_eq!(after.bom_span(), Some(0..6));
    assert_eq!(after.get("A"), Some("1"));
    replay("\u{feff}# \u{feff}", &extended);
}

#[test]
fn edit_options_bound_output_bytes_lines_and_verification_parsing() {
    use crate::{EditOptions, LimitKind};

    let doc = crate::parse_strict("A=1\nB='x'\n").unwrap();

    // Exact byte boundary succeeds; 1 byte tighter fails cleanly with OutputBytes.
    let exact = doc
        .set_with_options("A", "123", EditOptions::new().with_max_output_bytes(12))
        .unwrap();
    assert_eq!(exact.as_str(), "A=123\nB='x'\n");
    let err = doc
        .set_with_options("A", "123", EditOptions::new().with_max_output_bytes(11))
        .unwrap_err();
    assert!(
        matches!(err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes && limit.limit() == 11 && limit.observed() == 12),
        "{err:?}"
    );
    assert!(err.to_string().contains("output-bytes limit 11 exceeded"));

    // Oversized single Set / Insert / Rename fails before unbounded allocation.
    let huge = "x".repeat(4096);
    let err = doc
        .set_with_options("A", &huge, EditOptions::new().with_max_output_bytes(64))
        .unwrap_err();
    assert!(
        matches!(err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes && limit.observed() > 64)
    );

    let err = doc
        .insert_with_options(
            "BIG",
            Some(&huge),
            InsertPosition::End,
            EditOptions::new().with_max_output_bytes(64),
        )
        .unwrap_err();
    assert!(matches!(err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes));

    let err = doc
        .rename_with_options("A", &huge, EditOptions::new().with_max_output_bytes(64))
        .unwrap_err();
    assert!(matches!(err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes));

    // Many small inserts in one batch trigger cumulative byte cap.
    let batch: Vec<_> = (0..20)
        .map(|i| Edit::Insert {
            key: if i % 2 == 0 { "K_EVEN" } else { "K_ODD" },
            value: Some("value"),
            position: InsertPosition::End,
        })
        .collect();
    let err = doc
        .edit_with_options(&batch, EditOptions::new().with_max_output_bytes(64))
        .unwrap_err();
    assert!(matches!(err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes));

    // Output line budget catches multiline single-quoted Set and Insert.
    let ok_lines = doc
        .set_with_options("B", "l1\nl2", EditOptions::new().with_max_output_lines(3))
        .unwrap();
    assert_eq!(ok_lines.as_str(), "A=1\nB='l1\nl2'\n");
    let err = doc
        .set_with_options("B", "l1\nl2", EditOptions::new().with_max_output_lines(2))
        .unwrap_err();
    assert!(
        matches!(err, EditError::Limit(limit) if limit.kind() == LimitKind::Lines && limit.limit() == 2 && limit.observed() == 3),
        "{err:?}"
    );

    // Double-quoted value escapes newlines onto a single physical line, so max_output_lines(2) allows it.
    let double_doc = crate::parse_strict("A=\"1\"\nB=2\n").unwrap();
    let escaped = double_doc
        .set_with_options(
            "A",
            "l1\nl2\nl3",
            EditOptions::new().with_max_output_lines(2),
        )
        .unwrap();
    assert_eq!(escaped.as_str(), "A=\"l1\\nl2\\nl3\"\nB=2\n");

    // remove_with_options works and shrinks output back within budget.
    let removed = doc
        .remove_with_options(
            "B",
            EditOptions::new()
                .with_max_output_bytes(4)
                .with_max_output_lines(1),
        )
        .unwrap();
    assert_eq!(removed.as_str(), "A=1\n");

    // Empty batch still honors configured limits.
    let empty_err = doc
        .edit_with_options(&[], EditOptions::new().with_max_output_bytes(2))
        .unwrap_err();
    assert!(matches!(empty_err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes));
}

#[test]
fn edit_options_forward_parse_limits_and_match_chunk_scanner() {
    use crate::{EditOptions, LimitKind, LineBreaks, ParseOptions};

    let doc = crate::parse_strict("A=1\nB='x'\n").unwrap();
    let huge = "x".repeat(4096);

    // parse_limits forwards InputBytes, Lines, Items, and Diagnostics to verification.
    let err = doc
        .set_with_options(
            "A",
            "123",
            EditOptions::new().with_parse_limits(ParseOptions::new().with_max_input_bytes(11)),
        )
        .unwrap_err();
    assert!(matches!(err, EditError::Limit(limit) if limit.kind() == LimitKind::InputBytes));

    let err = doc
        .insert_with_options(
            "C",
            Some("3"),
            InsertPosition::End,
            EditOptions::new().with_parse_limits(ParseOptions::new().with_max_items(2)),
        )
        .unwrap_err();
    assert!(matches!(err, EditError::Limit(limit) if limit.kind() == LimitKind::Items));

    let err = doc
        .insert_with_options(
            "C",
            Some("3"),
            InsertPosition::End,
            EditOptions::new().with_parse_limits(ParseOptions::new().with_max_lines(2)),
        )
        .unwrap_err();
    assert!(matches!(err, EditError::Limit(limit) if limit.kind() == LimitKind::Lines));

    let warn_doc = crate::parse("A=1\nA=2\n").0;
    let err = warn_doc
        .edit_with_options(
            &[Edit::Set {
                entry: 0,
                value: "9",
            }],
            EditOptions::new().with_parse_limits(ParseOptions::new().with_max_diagnostics(0)),
        )
        .unwrap_err();
    assert!(matches!(err, EditError::Limit(limit) if limit.kind() == LimitKind::Diagnostics));

    // InvalidTarget / ConflictingEdits precede byte-budget rejection.
    assert_eq!(
        doc.edit_with_options(
            &[Edit::Set {
                entry: 99,
                value: &huge
            }],
            EditOptions::new().with_max_output_bytes(4),
        ),
        Err(EditError::InvalidTarget)
    );

    // Cross-chunk CRLF / CR / LF streaming check matches whole-string exceeds_line_limit.
    let chunks_source = "A=1\rX\rB=2\rC=3\n";
    let chunk_edits = [
        crate::TextEdit {
            range: 4..5,
            replacement: String::from("\n"),
        },
        crate::TextEdit {
            range: 6..6,
            replacement: String::from("\n"),
        },
    ];
    let mut applied = String::from(chunks_source);
    for edit in chunk_edits.iter().rev() {
        applied.replace_range(edit.range.clone(), &edit.replacement);
    }
    for breaks in [LineBreaks::Universal, LineBreaks::Lf] {
        for limit in 0..=6 {
            let planned =
                crate::edit::planned_exceeds_line_limit(chunks_source, &chunk_edits, breaks, limit);
            let direct = crate::parser::exceeds_line_limit(&applied, breaks, limit)
                .map(|(_, observed)| observed);
            assert_eq!(planned, direct, "breaks={breaks:?} limit={limit}");
        }
    }
}

#[test]
fn edit_options_combine_byte_and_line_caps_and_pass_when_within_parse_limits() {
    use crate::{EditOptions, LimitKind, ParseOptions};

    assert_eq!(EditOptions::default(), EditOptions::new());
    let doc = crate::parse_strict("A=1\nB=2\n").unwrap();

    // Passing edit with non-empty parse_limits exercises verification Ok path.
    let ok = doc
        .edit_with_options(
            &[Edit::Set {
                entry: 0,
                value: "10",
            }],
            EditOptions::new()
                .with_max_output_bytes(32)
                .with_max_output_lines(4)
                .with_parse_limits(
                    ParseOptions::new()
                        .with_max_input_bytes(32)
                        .with_max_lines(4)
                        .with_max_items(4)
                        .with_max_diagnostics(4),
                ),
        )
        .unwrap();
    assert_eq!(ok.as_str(), "A=10\nB=2\n");

    // Tighter cap wins consistently between max_output_bytes and parse_limits.max_input_bytes.
    let out_tighter = doc
        .set_with_options(
            "A",
            "1234",
            EditOptions::new()
                .with_max_output_bytes(10)
                .with_parse_limits(ParseOptions::new().with_max_input_bytes(20)),
        )
        .unwrap_err();
    assert!(matches!(
        out_tighter,
        EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes && limit.limit() == 10
    ));
    let in_tighter = doc
        .set_with_options(
            "A",
            "1234",
            EditOptions::new()
                .with_max_output_bytes(20)
                .with_parse_limits(ParseOptions::new().with_max_input_bytes(10)),
        )
        .unwrap_err();
    assert!(matches!(
        in_tighter,
        EditError::Limit(limit) if limit.kind() == LimitKind::InputBytes && limit.limit() == 10
    ));

    // Tighter line cap wins between max_output_lines and parse_limits.max_lines.
    let line_tighter = doc
        .insert_with_options(
            "C",
            Some("3"),
            InsertPosition::End,
            EditOptions::new()
                .with_max_output_lines(5)
                .with_parse_limits(ParseOptions::new().with_max_lines(2)),
        )
        .unwrap_err();
    assert!(matches!(
        line_tighter,
        EditError::Limit(limit) if limit.kind() == LimitKind::Lines && limit.limit() == 2
    ));
}

#[test]
fn edit_options_bound_multi_set_comment_and_strip_all_bom() {
    use crate::{BomPolicy, Dialect, EditOptions, LimitKind};

    let doc = crate::parse_strict("A=1\nB=2\n").unwrap();

    // Cumulative multi-Set batch triggers early byte cap; Comment triggers planned byte cap.
    let multi_set = doc
        .edit_with_options(
            &[
                Edit::Set {
                    entry: 0,
                    value: "12345",
                },
                Edit::Set {
                    entry: 1,
                    value: "67890",
                },
            ],
            EditOptions::new().with_max_output_bytes(8),
        )
        .unwrap_err();
    assert!(matches!(
        multi_set,
        EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes
    ));
    let commented = doc
        .edit_with_options(
            &[Edit::Comment { item: 0 }],
            EditOptions::new().with_max_output_bytes(9),
        )
        .unwrap_err();
    assert!(matches!(
        commented,
        EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes && limit.observed() == 10
    ));

    // StripAll multi-BOM prefix bytes count toward output_bytes.
    let bom_doc = crate::parse_strict_with(
        "\u{feff}\u{feff}A=1\n",
        Dialect::PORTABLE.with_bom(BomPolicy::StripAll),
    )
    .unwrap();
    assert!(bom_doc
        .set_with_options("A", "2", EditOptions::new().with_max_output_bytes(10))
        .is_ok());
    assert!(matches!(
        bom_doc.set_with_options("A", "2", EditOptions::new().with_max_output_bytes(9)),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::OutputBytes && limit.observed() == 10
    ));
}

#[test]
fn edit_options_early_budget_boundaries_lone_cr_and_bom_prelude() {
    use crate::{BomPolicy, Dialect, EditOptions, LimitKind};

    // Exact operation minimum byte budget succeeds (min_required == limit).
    let empty = crate::parse_strict("").unwrap();
    let exact = empty
        .insert_with_options(
            "A",
            Some("123"),
            InsertPosition::End,
            EditOptions::new().with_max_output_bytes(5),
        )
        .unwrap();
    assert_eq!(exact.as_str(), "A=123");

    // Pre-encoding byte budget rejects oversized values before encoding fails.
    let docker = crate::parse_strict_with("A=1\n", Dialect::DOCKER_RAW).unwrap();
    let unencodable = "\n".repeat(32);
    let early = docker
        .set_with_options(
            "A",
            &unencodable,
            EditOptions::new().with_max_output_bytes(16),
        )
        .unwrap_err();
    assert!(matches!(
        early,
        EditError::Limit(limit)
            if limit.kind() == LimitKind::OutputBytes
                && limit.limit() == 16
                && limit.observed() == 32
    ));

    // Lone CR counts as a physical line break only under LineBreaks::Universal.
    let lf_doc = crate::parse_strict_with("# a\rb\nA=1", Dialect::COMPOSE).unwrap();
    assert!(lf_doc
        .set_with_options("A", "2", EditOptions::new().with_max_output_lines(2))
        .is_ok());
    let universal_doc = crate::parse_strict("# a\rb\nA=1").unwrap();
    assert!(matches!(
        universal_doc.set_with_options("A", "2", EditOptions::new().with_max_output_lines(2)),
        Err(EditError::Limit(limit))
            if limit.kind() == LimitKind::Lines && limit.limit() == 2 && limit.observed() == 3
    ));

    // BOM-prelude omission applies only when uncommenting a BOM-only comment.
    for bom in [
        BomPolicy::Strip,
        BomPolicy::StripAll,
        BomPolicy::Keep,
        BomPolicy::Error,
    ] {
        let dialect = Dialect::PORTABLE.with_bom(bom);
        let kept = crate::parse_strict_with("# \u{feff}", dialect)
            .unwrap()
            .insert("A", Some("1"), InsertPosition::End)
            .unwrap();
        assert_eq!(kept.as_str(), "# \u{feff}\nA=1");

        let uncommented = crate::parse_strict_with("# A=1", dialect)
            .unwrap()
            .edit(&[
                Edit::Uncomment { item: 0 },
                Edit::Insert {
                    key: "B",
                    value: Some("2"),
                    position: InsertPosition::End,
                },
            ])
            .unwrap();
        assert_eq!(uncommented.as_str(), "A=1\nB=2");
    }
}

#[test]
fn indexed_insertions_keep_source_order_and_snapshot_duplicate_rules() {
    let (doc, _) = parse("Z=first\nB=second\nA=third\n");
    let result = doc
        .edit(&[
            Edit::Insert {
                key: "C",
                value: None,
                position: InsertPosition::Sorted,
            },
            Edit::Insert {
                key: "D",
                value: None,
                position: InsertPosition::Sorted,
            },
        ])
        .unwrap();
    assert_eq!(result.as_str(), "C\nD\nZ=first\nB=second\nA=third\n");
    let (doc, _) = parse("A=1\nA=2\n");
    assert_eq!(
        doc.edit(&[
            Edit::Insert {
                key: "B",
                value: None,
                position: InsertPosition::AfterKey("A")
            },
            Edit::Insert {
                key: "C",
                value: None,
                position: InsertPosition::Sorted
            },
        ]),
        Err(EditError::Duplicate(2))
    );
}

#[test]
fn batch_rename_collisions_do_not_reject_unmodified_duplicate_names() {
    let (doc, _) = parse("A=1\nA=2\nB=3\nC=4\n");
    assert_eq!(
        doc.edit(&[
            Edit::Rename {
                entry: 2,
                key: "FRESH"
            },
            Edit::Rename {
                entry: 3,
                key: "FRESH"
            },
        ]),
        Err(EditError::Duplicate(2))
    );
    let result = doc
        .edit(&[
            Edit::Rename { entry: 0, key: "A" },
            Edit::Rename {
                entry: 2,
                key: "NEW_B",
            },
            Edit::Rename {
                entry: 3,
                key: "NEW_C",
            },
        ])
        .unwrap();
    assert_eq!(result.as_str(), "A=1\nA=2\nNEW_B=3\nNEW_C=4\n");
}

#[test]
fn renames_free_and_claim_keys_like_insertions() {
    let (doc, _) = parse("A=1\nB=2\n# B=old\n");
    let rename = |entry, key| Edit::Rename { entry, key };
    let ok = |operations: &[Edit<'_>]| doc.edit(operations).map(|v| v.as_str().to_string());
    // Removing, commenting out or renaming away the old key frees it.
    assert_eq!(
        ok(&[Edit::Remove { item: 1 }, rename(0, "B")]),
        Ok("B=1\n# B=old\n".to_string())
    );
    assert_eq!(
        ok(&[Edit::Comment { item: 1 }, rename(0, "B")]),
        Ok("B=1\n# B=2\n# B=old\n".to_string())
    );
    assert_eq!(
        ok(&[rename(0, "B"), rename(1, "A")]),
        Ok("B=1\nA=2\n# B=old\n".to_string())
    );
    assert_eq!(
        ok(&[rename(1, "C"), rename(0, "B")]),
        Ok("B=1\nC=2\n# B=old\n".to_string())
    );
    // Any second declaration of the new key in the output collides.
    assert_eq!(ok(&[rename(0, "B")]), Err(EditError::Duplicate(2)));
    assert_eq!(doc.rename("A", "B"), Err(EditError::Duplicate(2)));
    assert_eq!(
        ok(&[rename(0, "C"), rename(1, "C")]),
        Err(EditError::Duplicate(2))
    );
    assert_eq!(
        ok(&[
            Edit::Insert {
                key: "C",
                value: Some("3"),
                position: InsertPosition::End,
            },
            rename(0, "C"),
        ]),
        Err(EditError::Duplicate(2))
    );
    assert_eq!(
        ok(&[Edit::Uncomment { item: 2 }, rename(0, "C"), rename(1, "D")]),
        Ok("C=1\nD=2\nB=old\n".to_string())
    );
    assert_eq!(
        ok(&[
            Edit::Uncomment { item: 2 },
            Edit::Remove { item: 1 },
            rename(0, "B")
        ]),
        Err(EditError::Duplicate(2))
    );
}

#[test]
fn uncommenting_claims_keys_like_insertions() {
    let (doc, _) = parse("A=1\n# A=old\n# B=2\n");
    let ok = |operations: &[Edit<'_>]| doc.edit(operations).map(|v| v.as_str().to_string());
    // Restoring a second declaration of a key that the output keeps collides.
    assert_eq!(doc.uncomment(1), Err(EditError::Duplicate(2)));
    assert_eq!(
        ok(&[
            Edit::Uncomment { item: 2 },
            Edit::Insert {
                key: "B",
                value: Some("3"),
                position: InsertPosition::End,
            },
        ]),
        Err(EditError::Duplicate(2))
    );
    // Removing, commenting out or renaming away the old declaration frees it.
    assert_eq!(
        ok(&[Edit::Remove { item: 0 }, Edit::Uncomment { item: 1 }]),
        Ok("A=old\n# B=2\n".to_string())
    );
    assert_eq!(
        ok(&[Edit::Comment { item: 0 }, Edit::Uncomment { item: 1 }]),
        Ok("# A=1\nA=old\n# B=2\n".to_string())
    );
    assert_eq!(
        ok(&[
            Edit::Rename { entry: 0, key: "C" },
            Edit::Uncomment { item: 1 }
        ]),
        Ok("C=1\nA=old\n# B=2\n".to_string())
    );
    assert_eq!(doc.uncomment(2).unwrap().as_str(), "A=1\n# A=old\nB=2\n");
    // Existing duplicates that the batch does not touch stay allowed.
    let (doc, _) = parse("A=1\nA=2\n# B=3\n");
    assert_eq!(doc.uncomment(2).unwrap().as_str(), "A=1\nA=2\nB=3\n");
}

#[test]
fn large_interleaved_set_rename_batch_keeps_sorted_native_edits() {
    let count = 256;
    let mut source = String::new();
    for index in 0..count {
        writeln!(source, "K{index:04}=old").expect("writing to String cannot fail");
    }
    let (document, diagnostics) = parse(&source);
    assert_eq!(diagnostics, []);
    let keys: Vec<_> = (0..count).map(|index| format!("R{index:04}")).collect();
    let values: Vec<_> = (0..count).map(|index| format!("value-{index}")).collect();
    let mut operations = Vec::with_capacity(count * 2);
    for entry in 0..count {
        operations.push(Edit::Set {
            entry,
            value: &values[entry],
        });
        operations.push(Edit::Rename {
            entry,
            key: &keys[entry],
        });
    }
    let result = document.edit(&operations).unwrap();
    assert!(result
        .edits()
        .windows(2)
        .all(|pair| pair[0].range.end <= pair[1].range.start));
    let (after, diagnostics) = parse(result.as_str());
    assert_eq!(diagnostics, []);
    assert_eq!(after.entries().len(), count);
    for (index, entry) in after.entries().enumerate() {
        assert_eq!(entry.key(), keys[index]);
        assert_eq!(entry.value(), Some(values[index].as_str()));
    }
}

#[test]
fn insertion_rejects_keys_that_exist_or_that_the_batch_creates() {
    let (doc, _) = parse("A=1\nB=2\n");
    assert_eq!(
        doc.insert("A", Some("evil"), InsertPosition::End),
        Err(EditError::Duplicate(2))
    );
    assert_eq!(
        doc.edit(&[
            Edit::Rename { entry: 1, key: "C" },
            Edit::Insert {
                key: "C",
                value: None,
                position: InsertPosition::End,
            },
        ]),
        Err(EditError::Duplicate(2))
    );
    assert_eq!(
        doc.edit(&[
            Edit::Insert {
                key: "C",
                value: Some("1"),
                position: InsertPosition::End,
            },
            Edit::Insert {
                key: "C",
                value: Some("2"),
                position: InsertPosition::Sorted,
            },
        ]),
        Err(EditError::Duplicate(2))
    );

    // Duplicates and bare declarations count; a commented-out one does not,
    // unless the same batch uncomments it.
    let (doc, _) = parse("A=1\nA=2\nBARE\n# C=old\n");
    assert_eq!(
        doc.insert("A", None, InsertPosition::End),
        Err(EditError::Duplicate(3))
    );
    assert_eq!(
        doc.insert("BARE", Some("x"), InsertPosition::Sorted),
        Err(EditError::Duplicate(2))
    );
    let result = doc.insert("C", Some("new"), InsertPosition::End).unwrap();
    assert_eq!(result.as_str(), "A=1\nA=2\nBARE\n# C=old\nC=new\n");
    replay(doc.source(), &result);
    assert_eq!(
        doc.edit(&[
            Edit::Uncomment { item: 3 },
            Edit::Insert {
                key: "C",
                value: Some("new"),
                position: InsertPosition::End,
            },
        ]),
        Err(EditError::Duplicate(2))
    );
}

#[test]
fn removing_commenting_or_renaming_a_declaration_frees_its_key_for_insertion() {
    let (doc, _) = parse("A=1\nA=2\nBARE\n# C=old\n");
    // Every existing declaration must go, not just one of them.
    assert_eq!(
        doc.edit(&[
            Edit::Remove { item: 0 },
            Edit::Insert {
                key: "A",
                value: Some("3"),
                position: InsertPosition::End,
            },
        ]),
        Err(EditError::Duplicate(2))
    );
    let result = doc
        .edit(&[
            Edit::Remove { item: 0 },
            Edit::Remove { item: 1 },
            Edit::Insert {
                key: "A",
                value: Some("3"),
                position: InsertPosition::End,
            },
        ])
        .unwrap();
    assert_eq!(result.as_str(), "BARE\n# C=old\nA=3\n");
    replay(doc.source(), &result);
    let (after, _) = parse(result.as_str());
    assert_eq!(after.get_all("A").count(), 1);
    assert_eq!(after.get("A"), Some("3"));

    // Commenting out or renaming the declaration also leaves the key free.
    let (doc, _) = parse("A=1\nB=2\n");
    let result = doc
        .edit(&[
            Edit::Comment { item: 0 },
            Edit::Insert {
                key: "A",
                value: Some("3"),
                position: InsertPosition::AfterEntry(1),
            },
        ])
        .unwrap();
    assert_eq!(result.as_str(), "# A=1\nB=2\nA=3\n");
    let result = doc
        .edit(&[
            Edit::Rename { entry: 0, key: "Z" },
            Edit::Insert {
                key: "A",
                value: Some("3"),
                position: InsertPosition::End,
            },
        ])
        .unwrap();
    assert_eq!(result.as_str(), "Z=1\nB=2\nA=3\n");
}

#[test]
fn edits_at_the_same_offset_apply_in_reverse_slice_order() {
    let (doc, _) = parse("A=1\n");
    let result = doc
        .edit(&[
            Edit::Export {
                entry: 0,
                enabled: true,
            },
            Edit::Rename { entry: 0, key: "B" },
        ])
        .unwrap();
    assert_eq!(
        result.edits(),
        [
            TextEdit {
                range: 0..0,
                replacement: "export ".into(),
            },
            TextEdit {
                range: 0..1,
                replacement: "B".into(),
            },
        ]
    );
    assert_eq!(
        apply_in_reverse(doc.source(), result.edits()),
        "export B=1\n"
    );
    assert_eq!(result.as_str(), "export B=1\n");

    // Applying "from the highest offset to the lowest" after a stable sort by
    // start offset applies the insertion first and corrupts the result.
    let mut by_offset = result.edits().to_vec();
    by_offset.sort_by_key(|edit| core::cmp::Reverse(edit.range.start));
    let mut text = doc.source().to_string();
    for edit in &by_offset {
        text.replace_range(edit.range.clone(), &edit.replacement);
    }
    assert_eq!(text, "Bxport A=1\n");

    // An insertion after one entry starts where the next entry's edits start.
    let (doc, _) = parse("A=1\nB=2\n");
    let result = doc
        .edit(&[
            Edit::Insert {
                key: "C",
                value: Some("3"),
                position: InsertPosition::AfterEntry(0),
            },
            Edit::Export {
                entry: 1,
                enabled: true,
            },
            Edit::Rename { entry: 1, key: "D" },
        ])
        .unwrap();
    assert_eq!(result.as_str(), "A=1\nC=3\nexport D=2\n");
    assert_eq!(
        apply_in_reverse(doc.source(), result.edits()),
        result.as_str()
    );
}

fn batch_source_line() -> impl Strategy<Value = &'static str> {
    prop::sample::select(
        [
            "A=1",
            "export B=two",
            "C",
            "D=",
            "# E=5",
            "#",
            "",
            "  F = 'six' # note",
            "G=\"x\ny\"",
            "H=\"quoted\"",
            "A=again",
        ]
        .as_slice(),
    )
}

fn batch_operation(
    (kind, target, name, value): (u8, usize, usize, usize),
    entries: usize,
    items: usize,
) -> Edit<'static> {
    const KEYS: [&str; 5] = ["N", "A", "M", "B2", "Z"];
    const VALUES: [&str; 5] = ["v", "", "two words", "#hash", "line\nbreak"];
    let entry = target % entries.max(1);
    let item = target % items.max(1);
    let key = KEYS[name % KEYS.len()];
    let value = VALUES[value % VALUES.len()];
    match kind % 10 {
        0 => Edit::Set { entry, value },
        1 => Edit::Remove { item },
        2 => Edit::Rename { entry, key },
        3 => Edit::Comment { item },
        4 => Edit::Uncomment { item },
        5 => Edit::Export {
            entry,
            enabled: target % 2 == 0,
        },
        6 => Edit::Insert {
            key,
            value: Some(value),
            position: InsertPosition::AfterEntry(entry),
        },
        7 => Edit::Insert {
            key,
            value: None,
            position: InsertPosition::Sorted,
        },
        8 => Edit::Insert {
            key,
            value: Some(value),
            position: InsertPosition::End,
        },
        _ => Edit::Insert {
            key,
            value: Some(value),
            position: InsertPosition::AfterKey("C"),
        },
    }
}

proptest! {
    #[test]
    fn verified_edits_apply_in_reverse_slice_order(
        lines in prop::collection::vec(batch_source_line(), 0..8),
        crlf in any::<bool>(),
        final_newline in any::<bool>(),
        operations in prop::collection::vec(
            (any::<u8>(), any::<usize>(), any::<usize>(), any::<usize>()),
            0..6,
        ),
    ) {
        let ending = if crlf { "\r\n" } else { "\n" };
        let mut source = lines.join(ending);
        if final_newline {
            source.push_str(ending);
        }
        let (doc, _) = parse(&source);
        let operations: Vec<_> = operations
            .into_iter()
            .map(|operation| batch_operation(operation, doc.entries().len(), doc.items().len()))
            .collect();
        if let Ok(result) = doc.edit(&operations) {
            prop_assert!(result
                .edits()
                .windows(2)
                .all(|pair| pair[0].range.end <= pair[1].range.start));
            prop_assert_eq!(apply_in_reverse(&source, result.edits()), result.as_str());
            prop_assert_eq!(replay(&source, &result), result.as_str());
        }
    }
}

#[test]
fn set_keeps_a_key_that_is_only_a_bom() {
    let source = "# c\n\u{feff}=old\n";
    let dialect = crate::Dialect::PYTHON_DOTENV;
    let (doc, _) = crate::parse_with(source, dialect);
    let result = doc.set("\u{feff}", "new").unwrap();
    assert_eq!(result.as_str(), "# c\n\u{feff}=new\n");
    let (after, _) = crate::parse_with(result.as_str(), dialect);
    assert_eq!(after.get("\u{feff}"), Some("new"));
}

#[test]
fn uncommenting_an_ignored_statement_is_rejected() {
    let (doc, _) = parse_with("# BAD KEY=secret\n", crate::Dialect::NODE_DOTENV);
    assert_eq!(doc.uncomment(0), Err(EditError::WouldChangeMeaning));
    let (doc, _) = parse_with("# A=1\n", crate::Dialect::NODE_DOTENV);
    assert_eq!(doc.uncomment(0).unwrap().as_str(), "A=1\n");
}

#[test]
fn compose_value_edits_preserve_a_separate_comment_suffix() {
    for source in [
        "A='one' # note\n",
        "A = 'one'\t# note\n",
        "A='one'   # note\n",
    ] {
        let (doc, diagnostics) = parse_with(source, crate::Dialect::COMPOSE);
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity() != Severity::Error),
            "{source:?}: {diagnostics:?}"
        );
        let result = doc.set("A", "two").unwrap();
        assert!(result.as_str().contains("# note"), "{:?}", result.as_str());
        let (after, _) = parse_with(result.as_str(), crate::Dialect::COMPOSE);
        assert_eq!(after.get("A"), Some("two"));
        replay(source, &result);
    }
}

#[test]
fn lf_only_crlf_file_with_trailing_lone_cr_inserts_without_corrupting_items() {
    let source = "A=1\r\nB=2\r";
    for dialect in [
        crate::Dialect::DOCKER_RAW,
        crate::Dialect::COMPOSE,
        crate::Dialect::PORTABLE.with_line_breaks(crate::LineBreaks::Lf),
    ] {
        let (doc, _) = parse_with(source, dialect);
        let inserted = doc.insert("C", Some("3"), InsertPosition::End).unwrap();
        assert_eq!(inserted.as_str(), "A=1\r\nB=2\r\nC=3\r\n");
        let (after, _) = parse_with(inserted.as_str(), dialect);
        assert_eq!(after.get("B"), Some("2"));
        assert_eq!(after.get("C"), Some("3"));
        replay(source, &inserted);

        let batch = doc
            .edit(&[
                Edit::Set {
                    entry: 1,
                    value: "updated",
                },
                Edit::Insert {
                    key: "C",
                    value: Some("3"),
                    position: InsertPosition::End,
                },
            ])
            .unwrap();
        let (after, _) = parse_with(batch.as_str(), dialect);
        assert_eq!(after.get("B"), Some("updated"));
        assert_eq!(after.get("C"), Some("3"));
        replay(source, &batch);
    }

    let comment_source = "A=1\r\n# note\r";
    let (doc, _) = parse_with(comment_source, crate::Dialect::DOCKER_RAW);
    let inserted = doc.insert("C", Some("3"), InsertPosition::End).unwrap();
    assert_eq!(inserted.as_str(), "A=1\r\n# note\r\nC=3\r\n");
    let (after, _) = parse_with(inserted.as_str(), crate::Dialect::DOCKER_RAW);
    assert_eq!(after.item(1).unwrap().comment_text(), Some(" note"));

    let inline_source = "A=1\r\nB=2 # note\r";
    let (doc, _) = parse_with(inline_source, crate::Dialect::COMPOSE);
    let inserted = doc.insert("C", Some("3"), InsertPosition::End).unwrap();
    assert_eq!(inserted.as_str(), "A=1\r\nB=2 # note\r\nC=3\r\n");
    let (after, _) = parse_with(inserted.as_str(), crate::Dialect::COMPOSE);
    assert_eq!(after.unique("B").unwrap().inline_comment(), Some("# note"));
}

#[test]
fn rename_with_references_braces_digit_leading_keys_and_rejects_escape_or_quote_keys() {
    let source = "HOST=localhost\nURL=$HOST\n";
    let doc = crate::parse_strict(source).unwrap();
    let renamed = doc.rename_with_references("HOST", "1HOST").unwrap();
    assert_eq!(renamed.as_str(), "1HOST=localhost\nURL=${1HOST}\n");
    replay(source, &renamed);

    let (py_doc, _) = parse_with(
        "HOST=localhost\nURL=\"${HOST}\"\n",
        crate::Dialect::PYTHON_DOTENV,
    );
    assert_eq!(
        py_doc.rename_with_references("HOST", r"A\nB"),
        Err(EditError::InvalidReferenceKey)
    );
    assert_eq!(
        py_doc.rename_with_references("HOST", "A\"B"),
        Err(EditError::InvalidReferenceKey)
    );

    // Single-quoted and backtick-quoted enclosing delimiters are rejected too.
    let (single_doc, _) = parse_with(
        "HOST=localhost\nURL='${HOST}'\n",
        crate::Dialect::PYTHON_DOTENV,
    );
    assert_eq!(
        single_doc.rename_with_references("HOST", "A'B"),
        Err(EditError::InvalidReferenceKey)
    );
    let (tick_doc, _) = parse_with(
        "HOST=localhost\nURL=`${HOST}`\n",
        crate::Dialect::PYTHON_DOTENV.with_backticks(true),
    );
    assert_eq!(
        tick_doc.rename_with_references("HOST", "A`B"),
        Err(EditError::InvalidReferenceKey)
    );

    // Renaming AWAY from an escaped key inside double quotes succeeds cleanly.
    let escaped_old = "'A\\nB'=1\nX=\"${A\\nB}\"\n";
    let (esc_doc, _) = parse_with(escaped_old, crate::Dialect::PYTHON_DOTENV);
    let renamed_esc = esc_doc.rename_with_references(r"A\nB", "HOST").unwrap();
    assert_eq!(renamed_esc.as_str(), "'HOST'=1\nX=\"${HOST}\"\n");
    replay(escaped_old, &renamed_esc);
}

#[test]
fn rename_with_references_with_options_enforces_output_and_parse_limits() {
    use crate::{EditOptions, LimitKind};

    // Growing rename: exact output byte cap succeeds, one byte smaller fails.
    let source = "HOST=localhost\nURL=$HOST/api\nPORT=${HOST}\n";
    let doc = crate::parse_strict(source).unwrap();
    let expected = "SERVER_HOST=localhost\nURL=$SERVER_HOST/api\nPORT=${SERVER_HOST}\n";
    let exact = doc
        .rename_with_references_with_options(
            "HOST",
            "SERVER_HOST",
            EditOptions::new()
                .with_max_output_bytes(expected.len())
                .with_max_output_lines(3),
        )
        .unwrap();
    assert_eq!(exact.as_str(), expected);
    replay(source, &exact);

    let err = doc
        .rename_with_references_with_options(
            "HOST",
            "SERVER_HOST",
            EditOptions::new().with_max_output_bytes(expected.len() - 1),
        )
        .unwrap_err();
    assert!(
        matches!(err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes && limit.limit() == expected.len() - 1 && limit.observed() == expected.len()),
        "{err:?}"
    );

    // Line cap enforced before applying edits.
    let line_err = doc
        .rename_with_references_with_options(
            "HOST",
            "SERVER_HOST",
            EditOptions::new().with_max_output_lines(2),
        )
        .unwrap_err();
    assert!(
        matches!(line_err, EditError::Limit(limit) if limit.kind() == LimitKind::Lines && limit.limit() == 2),
        "{line_err:?}"
    );

    // Shrinking rename: intermediate declaration-only output (27 bytes) exceeds
    // the 15-byte cap, but the combined output ("A=1\nURL=$A\n" = 11 bytes) fits.
    let long_source = "VERY_LONG_OLD_KEY=1\nURL=$VERY_LONG_OLD_KEY\n";
    let long_doc = crate::parse_strict(long_source).unwrap();
    let shrunk = long_doc
        .rename_with_references_with_options(
            "VERY_LONG_OLD_KEY",
            "A",
            EditOptions::new().with_max_output_bytes(15),
        )
        .unwrap();
    assert_eq!(shrunk.as_str(), "A=1\nURL=$A\n");
    replay(long_source, &shrunk);
}

#[test]
fn rename_with_references_with_options_enforces_parse_limits_and_early_caps() {
    use crate::{EditOptions, LimitKind, ParseOptions};

    let source = "HOST=localhost\nURL=$HOST/api\nPORT=${HOST}\n";
    let doc = crate::parse_strict(source).unwrap();
    let expected = "SERVER_HOST=localhost\nURL=$SERVER_HOST/api\nPORT=${SERVER_HOST}\n";

    // Tighter parse_limits.max_input_bytes reports InputBytes; tighter max_output_bytes
    // reports OutputBytes.
    let input_err = doc
        .rename_with_references_with_options(
            "HOST",
            "SERVER_HOST",
            EditOptions::new()
                .with_max_output_bytes(expected.len() + 10)
                .with_parse_limits(ParseOptions::new().with_max_input_bytes(expected.len() - 1)),
        )
        .unwrap_err();
    assert!(
        matches!(input_err, EditError::Limit(limit) if limit.kind() == LimitKind::InputBytes),
        "{input_err:?}"
    );

    // Verification re-parse enforces parse_limits.max_items and max_lines.
    let item_err = doc
        .rename_with_references_with_options(
            "HOST",
            "SERVER_HOST",
            EditOptions::new().with_parse_limits(ParseOptions::new().with_max_items(2)),
        )
        .unwrap_err();
    assert!(
        matches!(item_err, EditError::Limit(limit) if limit.kind() == LimitKind::Items),
        "{item_err:?}"
    );

    let parse_line_err = doc
        .rename_with_references_with_options(
            "HOST",
            "SERVER_HOST",
            EditOptions::new().with_parse_limits(ParseOptions::new().with_max_lines(2)),
        )
        .unwrap_err();
    assert!(
        matches!(parse_line_err, EditError::Limit(limit) if limit.kind() == LimitKind::Lines),
        "{parse_line_err:?}"
    );

    // Early cap rejection during declaration or reference collection avoids
    // unbounded intermediate replacement allocations.
    let decl_cap_err = doc
        .rename_with_references_with_options(
            "HOST",
            "SERVER_HOST",
            EditOptions::new().with_max_output_bytes(5),
        )
        .unwrap_err();
    assert!(
        matches!(decl_cap_err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes && limit.observed() == "SERVER_HOST".len()),
        "{decl_cap_err:?}"
    );

    let ref_cap_err = doc
        .rename_with_references_with_options(
            "HOST",
            "SERVER_HOST",
            EditOptions::new().with_max_output_bytes(15),
        )
        .unwrap_err();
    assert!(
        matches!(ref_cap_err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes && limit.observed() == 22),
        "{ref_cap_err:?}"
    );
}

#[test]
fn rename_with_references_with_options_handles_quoting_self_ref_and_duplicates() {
    use crate::{Dialect, Duplicates, EditOptions, LimitKind};

    let source = "HOST=localhost\nURL=$HOST/api\nPORT=${HOST}\n";
    let doc = crate::parse_strict(source).unwrap();

    // Unquoted -> newly-quoted key + unbraced -> braced reference honors exact byte cap.
    let (py_doc, _) = parse_with("A=1\nX=$A\n", Dialect::PYTHON_DOTENV);
    let py_expected = "'D E'=1\nX=${D E}\n";
    let py_ok = py_doc
        .rename_with_references_with_options(
            "A",
            "D E",
            EditOptions::new().with_max_output_bytes(py_expected.len()),
        )
        .unwrap();
    assert_eq!(py_ok.as_str(), py_expected);
    replay("A=1\nX=$A\n", &py_ok);
    let py_err = py_doc
        .rename_with_references_with_options(
            "A",
            "D E",
            EditOptions::new().with_max_output_bytes(py_expected.len() - 1),
        )
        .unwrap_err();
    assert!(
        matches!(py_err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes),
        "{py_err:?}"
    );

    // Self-rename and zero-reference rename also honor EditOptions.
    let self_err = doc
        .rename_with_references_with_options(
            "HOST",
            "HOST",
            EditOptions::new().with_max_output_bytes(4),
        )
        .unwrap_err();
    assert!(
        matches!(self_err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes),
        "{self_err:?}"
    );
    let noref_doc = crate::parse_strict("A=1\nB=2\n").unwrap();
    let noref_err = noref_doc
        .rename_with_references_with_options(
            "A",
            "LONG_NAME",
            EditOptions::new().with_max_output_bytes(10),
        )
        .unwrap_err();
    assert!(
        matches!(noref_err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes),
        "{noref_err:?}"
    );

    // Self-referencing declaration and duplicate target with references (Last and First policies).
    let self_ref = crate::parse_strict("HOST=$HOST\n").unwrap();
    let self_renamed = self_ref
        .rename_with_references_with_options(
            "HOST",
            "SERVER",
            EditOptions::new().with_max_output_bytes(32),
        )
        .unwrap();
    assert_eq!(self_renamed.as_str(), "SERVER=$SERVER\n");
    replay("HOST=$HOST\n", &self_renamed);

    let dup_source = "A=1\nB=2\nC=$A\n";
    for dialect in [
        Dialect::PORTABLE,
        Dialect::PORTABLE.with_duplicates(Duplicates::First),
    ] {
        let (dup_doc, _) = parse_with(dup_source, dialect);
        assert_eq!(
            dup_doc.rename_with_references_with_options("A", "B", EditOptions::new()),
            Err(EditError::Duplicate(2))
        );
    }
}

#[test]
fn rename_with_references_and_insert_separator_edge_cases() {
    use crate::{Dialect, EditOptions, LimitKind};

    // Unspellable keys (empty, containing quote, or space without quoted_keys) fail with InvalidKey.
    let (py_doc, _) = parse_with("A=1\nX=$A\n", Dialect::PYTHON_DOTENV);
    assert_eq!(
        py_doc.rename_with_references("A", ""),
        Err(EditError::InvalidKey)
    );
    assert_eq!(
        py_doc.rename_with_references("A", "A 'B"),
        Err(EditError::InvalidKey)
    );
    let (portable_doc, _) = parse("A=1\nX=$A\n");
    assert_eq!(
        portable_doc.rename_with_references("A", "A B"),
        Err(EditError::InvalidKey)
    );

    // Exact decl_len and total_added caps still report observed > limit.
    let small = crate::parse_strict("A=1\nB=$A\n").unwrap();
    let decl_exact = small
        .rename_with_references_with_options(
            "A",
            "LONG",
            EditOptions::new().with_max_output_bytes(4),
        )
        .unwrap_err();
    assert!(
        matches!(decl_exact, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes && limit.observed() > 4),
        "{decl_exact:?}"
    );
    let ref_exact = small
        .rename_with_references_with_options(
            "A",
            "LONG",
            EditOptions::new().with_max_output_bytes(8),
        )
        .unwrap_err();
    assert!(
        matches!(ref_exact, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes && limit.observed() > 8),
        "{ref_exact:?}"
    );

    // Compose NextStatement where declaration edit starts exactly at previous entry's value_span.end.
    let (compose_adj, _) = parse_with("A=\"$B\"B=1\n", Dialect::COMPOSE);
    let adj_renamed = compose_adj.rename_with_references("B", "HOST").unwrap();
    assert_eq!(adj_renamed.as_str(), "A=\"$HOST\"HOST=1\n");
    replay("A=\"$B\"B=1\n", &adj_renamed);

    // Compose EOF insertion adds newline after unquoted value, keeps same line after quoted value.
    let (compose_unquoted, _) = parse_with("A=1", Dialect::COMPOSE);
    assert_eq!(
        compose_unquoted
            .insert("B", Some("2"), InsertPosition::End)
            .unwrap()
            .as_str(),
        "A=1\nB=2"
    );
    let (compose_quoted, _) = parse_with("A=\"1\"", Dialect::COMPOSE);
    assert_eq!(
        compose_quoted
            .insert("B", Some("2"), InsertPosition::End)
            .unwrap()
            .as_str(),
        "A=\"1\"B=2"
    );

    // LF-only CRLF file without trailing CR keeps CRLF separator on EOF insert.
    let (docker_crlf, _) = parse_with("A=1\r\nB=2", Dialect::DOCKER_RAW);
    let inserted = docker_crlf
        .insert("C", Some("3"), InsertPosition::End)
        .unwrap();
    assert_eq!(inserted.as_str(), "A=1\r\nB=2\r\nC=3");
    replay("A=1\r\nB=2", &inserted);
}

#[test]
fn rename_with_references_rejects_command_quote_break_and_handles_compose_empty_keys() {
    use crate::Dialect;

    // P1 #1: Injecting shell quotes inside $(...) must not corrupt command structure,
    // including command extension (pulling trailing text in) and truncation (closing early).
    let (py_cmd, _) = parse_with("A=1\nX=$(echo \"${A}\")\n", Dialect::PYTHON_DOTENV);
    assert_eq!(
        py_cmd.rename_with_references("A", "B\"C"),
        Err(EditError::WouldChangeMeaning)
    );
    let (py_two, _) = parse_with(
        "A=1\nX=$(echo \"${A}\")$(echo ok)\n",
        Dialect::PYTHON_DOTENV,
    );
    assert_eq!(
        py_two.rename_with_references("A", "B\"C"),
        Err(EditError::WouldChangeMeaning)
    );
    let (py_extend, _) = parse_with(
        "A=1\nX=$(echo \"${A}\" ) ; rm -rf /\")\n",
        Dialect::PYTHON_DOTENV,
    );
    assert_eq!(
        py_extend.rename_with_references("A", "B\"C"),
        Err(EditError::WouldChangeMeaning)
    );
    let (py_truncate, _) = parse_with("A=1\nX=$(echo \"${A}\" \")\" ok)\n", Dialect::PYTHON_DOTENV);
    assert_eq!(
        py_truncate.rename_with_references("A", "B\"C"),
        Err(EditError::WouldChangeMeaning)
    );
    let (py_truncate_unbraced, _) =
        parse_with("A=1\nX=$(echo \"$A\" \")\" ok)\n", Dialect::PYTHON_DOTENV);
    assert_eq!(
        py_truncate_unbraced.rename_with_references("A", "B\"C"),
        Err(EditError::WouldChangeMeaning)
    );
    let (docker_cmd, _) = parse_with("A=1\nX=$(echo '${A}')\n", Dialect::DOCKER_RAW);
    assert_eq!(
        docker_cmd.rename_with_references("A", "B'C"),
        Err(EditError::WouldChangeMeaning)
    );
    let (docker_extend, _) =
        parse_with("A=1\nX=$(echo '${A}' ) ; rm -rf /')\n", Dialect::DOCKER_RAW);
    assert_eq!(
        docker_extend.rename_with_references("A", "B'C"),
        Err(EditError::WouldChangeMeaning)
    );
    let (docker_truncate, _) = parse_with("A=1\nX=$(echo '${A}' ')' ok)\n", Dialect::DOCKER_RAW);
    assert_eq!(
        docker_truncate.rename_with_references("A", "B'C"),
        Err(EditError::WouldChangeMeaning)
    );
    // Balanced quotes, single-quote-with-escape, and nested command ancestors must also fail.
    let (py_balanced, _) = parse_with("A=1\nX=$(echo \"${A}\")\n", Dialect::PYTHON_DOTENV);
    assert_eq!(
        py_balanced.rename_with_references("A", "B\" ; id ; \"C"),
        Err(EditError::WouldChangeMeaning)
    );
    let (docker_balanced, _) = parse_with("A=1\nX=$(echo '${A}')\n", Dialect::DOCKER_RAW);
    assert_eq!(
        docker_balanced.rename_with_references("A", "B';id;'C"),
        Err(EditError::WouldChangeMeaning)
    );
    let (py_mixed_quote, _) = parse_with("A=1\nX=$(echo \"${A}'\" \\')\n", Dialect::PYTHON_DOTENV);
    assert_eq!(
        py_mixed_quote.rename_with_references("A", "B\"C"),
        Err(EditError::WouldChangeMeaning)
    );
    let (py_nested_cmd, _) = parse_with(
        "A=1\nX=$(echo \"${OUTER:-${A}}\")\n",
        Dialect::PYTHON_DOTENV,
    );
    assert_eq!(
        py_nested_cmd.rename_with_references("A", "B\" ; id ; \"C"),
        Err(EditError::WouldChangeMeaning)
    );

    // Outside $(...), unquoted references still permit dialect-valid quote keys.
    let (py_non_cmd, _) = parse_with("A=1\nX=${A}\n", Dialect::PYTHON_DOTENV);
    let py_ok = py_non_cmd.rename_with_references("A", "B\"C").unwrap();
    assert_eq!(py_ok.as_str(), "B\"C=1\nX=${B\"C}\n");
    replay("A=1\nX=${A}\n", &py_ok);

    let (docker_non_cmd, _) = parse_with("A=1\nX=${A}\n", Dialect::DOCKER_RAW);
    let docker_ok = docker_non_cmd.rename_with_references("A", "B'C").unwrap();
    assert_eq!(docker_ok.as_str(), "B'C=1\nX=${B'C}\n");
    replay("A=1\nX=${A}\n", &docker_ok);
}

#[test]
fn rename_with_references_rejects_command_shell_metacharacters() {
    use crate::Dialect;

    // Shell metacharacters (backticks, command termination via escaped \${...},
    // list/pipe/redirection/comment/whitespace operators, and reverse-direction `\`)
    // must be rejected whenever a renamed reference has a $(...) ancestor.
    for dialect in [Dialect::PYTHON_DOTENV, Dialect::DOCKER_RAW] {
        let (esc_cmd, _) = parse_with("A=1\nX=$(echo \\${A})\n", dialect);
        assert_eq!(
            esc_cmd.rename_with_references("A", "B);id;(C"),
            Err(EditError::WouldChangeMeaning)
        );
        let (tick_cmd, _) = parse_with("A=1\nX=$(echo \"$A\")\n", dialect);
        assert_eq!(
            tick_cmd.rename_with_references("A", "B`id`C"),
            Err(EditError::WouldChangeMeaning)
        );
        let (tick_esc_cmd, _) = parse_with("A=1\nX=$(echo \"\\${A}\")\n", dialect);
        assert_eq!(
            tick_esc_cmd.rename_with_references("A", "B`id`C"),
            Err(EditError::WouldChangeMeaning)
        );
        for meta_key in ["B)C", "B;id;C", "B|id", "B&id", "B>out", "B<in"] {
            let (meta_doc, _) = parse_with("A=1\nX=$(echo $A)\n", dialect);
            assert_eq!(
                meta_doc.rename_with_references("A", meta_key),
                Err(EditError::WouldChangeMeaning),
                "{dialect:?} key={meta_key}"
            );
        }
        let (rev_bs, _) = parse_with("A\\=1\nX=$(echo \\${A\\} ' ; id ; ' })\n", dialect);
        assert_eq!(
            rev_bs.rename_with_references("A\\", "B"),
            Err(EditError::WouldChangeMeaning)
        );
    }
    let (py_space_cmd, _) = parse_with("A=1\nX=$(echo $A)\n", Dialect::PYTHON_DOTENV);
    assert_eq!(
        py_space_cmd.rename_with_references("A", "B C"),
        Err(EditError::WouldChangeMeaning)
    );
    let (docker_hash_cmd, _) = parse_with("A=1\nX=$(echo $A)\n", Dialect::DOCKER_RAW);
    assert_eq!(
        docker_hash_cmd.rename_with_references("A", "B#C"),
        Err(EditError::WouldChangeMeaning)
    );
    // Shell parameter-expansion assignments (=), indirect expansion (!), array
    // subscripts ([...]), globs/suffix-strips (%, *, @, /, ^, ,), reverse-direction
    // colon (:), and non-ASCII whitespace/control characters must also be rejected
    // inside $(...) by the strict allowlist.
    for key in [
        "ENV=/tmp/pwn",
        "A[B=1]",
        "!AWS_*",
        "B%pat",
        "*",
        "@",
        "B/a/b",
        "B^",
        "B,",
        "B\u{00a0}C",
    ] {
        let (py_doc, _) = parse_with("A=1\nX=$(echo $A)\n", Dialect::PYTHON_DOTENV);
        assert_eq!(
            py_doc.rename_with_references("A", key),
            Err(EditError::WouldChangeMeaning),
            "PYTHON_DOTENV key={key:?}"
        );
    }
    let (compose_arr, _) = parse_with("A=1\nX=$(echo $A)\n", Dialect::COMPOSE);
    assert_eq!(
        compose_arr.rename_with_references("A", "A[0]"),
        Err(EditError::WouldChangeMeaning)
    );
    let (docker_colon, _) = parse_with("A:0=1\nX=$(echo ${A:0})\n", Dialect::DOCKER_RAW);
    assert_eq!(
        docker_colon.rename_with_references("A:0", "B"),
        Err(EditError::WouldChangeMeaning)
    );
    let (docker_nbsp, _) = parse_with("A=1\nX=$(echo $A)\n", Dialect::DOCKER_RAW);
    assert_eq!(
        docker_nbsp.rename_with_references("A", "B\u{00a0}C"),
        Err(EditError::WouldChangeMeaning)
    );

    // Normal dotted keys inside $(...) remain allowed.
    for new_key in ["APP.HOST", "DB.1.HOST", "_INTERNAL"] {
        let (cmd_dotted, _) = parse_with("A=1\nX=$(echo \"${A}\")\n", Dialect::PORTABLE);
        let dotted_ok = cmd_dotted.rename_with_references("A", new_key).unwrap();
        assert_eq!(
            dotted_ok.as_str(),
            format!("{new_key}=1\nX=$(echo \"${{{new_key}}}\")\n")
        );
        replay("A=1\nX=$(echo \"${A}\")\n", &dotted_ok);
    }
}

#[test]
fn rename_with_references_rejects_positional_dot_and_early_closed_command_keys() {
    use crate::Dialect;

    // Positional / special shell parameters and leading/trailing/double-dot keys
    // are rejected inside $(...) in both directions.
    for bad_key in ["0", "1", "12", "1HOST", ".", "..", ".FOO", "A.", "A..B"] {
        let (doc, _) = parse_with("A=1\nX=$(echo $A)\n", Dialect::PORTABLE);
        assert_eq!(
            doc.rename_with_references("A", bad_key),
            Err(EditError::WouldChangeMeaning),
            "PORTABLE A -> {bad_key:?}"
        );
        let reverse_src = format!("{bad_key}=1\nX=$(echo ${{{bad_key}}})\n");
        let (rev_doc, _) = parse_with(&reverse_src, Dialect::PORTABLE);
        if rev_doc.unique(bad_key).is_ok() {
            assert_eq!(
                rev_doc.rename_with_references(bad_key, "A"),
                Err(EditError::WouldChangeMeaning),
                "PORTABLE {bad_key:?} -> A"
            );
        }
    }

    // Even when raw-source scanning closes `$(...)` early (escaped `\"` inside a
    // double-quoted value or a shell `case ... in pat)` arm), renaming a variable
    // after a preceding `$(` enforces the strict command-key allowlist.
    let (escaped_quote_doc, _) =
        parse_with("A=1\nX=\"$(echo \\\"a)\\\" $A)\"\n", Dialect::PYTHON_DOTENV);
    assert_eq!(
        escaped_quote_doc.rename_with_references("A", "B);id;(C"),
        Err(EditError::WouldChangeMeaning)
    );
    let (case_doc, _) = parse_with(
        "A=1\nX=$(case x in x) echo $A;; esac)\n",
        Dialect::PYTHON_DOTENV,
    );
    assert_eq!(
        case_doc.rename_with_references("A", "B;id;C"),
        Err(EditError::WouldChangeMeaning)
    );
    // Normal identifier/dotted renames after a preceding `$(...)` still succeed.
    let safe_after_cmd = case_doc.rename_with_references("A", "APP.HOST").unwrap();
    assert_eq!(
        safe_after_cmd.as_str(),
        "APP.HOST=1\nX=$(case x in x) echo ${APP.HOST};; esac)\n"
    );
    replay("A=1\nX=$(case x in x) echo $A;; esac)\n", &safe_after_cmd);
}

#[test]
fn rename_with_references_handles_compose_empty_keys_and_duplicate_precedence() {
    use crate::Dialect;

    // Compose empty keys at item start and unclosed `${` at value end / EOF / append tail.
    for (src, expected, dialect) in [
        ("=1\nX=${}\n", "NEW=1\nX=${NEW}\n", Dialect::COMPOSE),
        ("A=\"${}\"=1\n", "A=\"${NEW}\"NEW=1\n", Dialect::COMPOSE),
        ("A=\"${\"=1\n", "A=\"${NEW\"NEW=1\n", Dialect::COMPOSE),
        ("=1\nX=${\n", "NEW=1\nX=${NEW\n", Dialect::COMPOSE),
        ("=1\nX=${", "NEW=1\nX=${NEW", Dialect::COMPOSE),
        (
            "=1\nX=${ # note\n",
            "NEW=1\nX=${NEW # note\n",
            Dialect::COMPOSE,
        ),
        (
            "=1\nX=${:-${}}\n",
            "NEW=1\nX=${NEW:-${NEW}}\n",
            Dialect::COMPOSE,
        ),
        ("=1\nX=${:-${", "NEW=1\nX=${NEW:-${NEW", Dialect::COMPOSE),
        (
            "=1\nX=\"a\"${\n",
            "NEW=1\nX=\"a\"${NEW\n",
            Dialect::COMPOSE.with_quoted_tail(crate::QuotedTail::Append),
        ),
        (
            "=1\nX=\"a\"${",
            "NEW=1\nX=\"a\"${NEW",
            Dialect::COMPOSE.with_quoted_tail(crate::QuotedTail::Append),
        ),
        (
            "=1\nX=\"${\"b",
            "NEW=1\nX=\"${NEW\"b",
            Dialect::COMPOSE.with_quoted_tail(crate::QuotedTail::Append),
        ),
    ] {
        let (doc, _) = parse_with(src, dialect);
        let renamed = doc.rename_with_references("", "NEW").unwrap();
        assert_eq!(renamed.as_str(), expected, "source: {src:?}");
        replay(src, &renamed);
    }

    // P2 #3: Duplicate target precedence matches 1.6.1 even when new_key has an operator.
    let (dup_op, _) = parse("A=1\nB-2=2\nX=$A\n");
    assert_eq!(
        dup_op.rename_with_references("A", "B-2"),
        Err(EditError::Duplicate(2))
    );
}

#[test]
fn rename_with_references_with_options_covers_bom_line_breaks_tails_and_ties() {
    use crate::{BomPolicy, Dialect, EditOptions, LimitKind, LineBreaks, QuotedTail};

    // BOM policies count BOM bytes in max_output_bytes and preserve BOM runs.
    for (policy, prefix) in [
        (BomPolicy::Strip, "\u{feff}"),
        (BomPolicy::StripAll, "\u{feff}\u{feff}"),
        (BomPolicy::Keep, "\u{feff}"),
        (BomPolicy::Error, "\u{feff}"),
    ] {
        let dialect = Dialect::PORTABLE.with_bom(policy);
        let source = format!("{prefix}HOST=1\nX=$HOST\n");
        let (doc, _) = parse_with(&source, dialect);
        if policy == BomPolicy::Keep {
            // Keep makes BOM part of the key name; verify renaming the second key works.
            let keep_src = format!("{prefix}A=1\nHOST=2\nX=$HOST\n");
            let (keep_doc, _) = parse_with(&keep_src, dialect);
            let expected = format!("{prefix}A=1\nSRV=2\nX=$SRV\n");
            let ok = keep_doc
                .rename_with_references_with_options(
                    "HOST",
                    "SRV",
                    EditOptions::new().with_max_output_bytes(expected.len()),
                )
                .unwrap();
            assert_eq!(ok.as_str(), expected);
            replay(&keep_src, &ok);
            continue;
        }
        let expected = format!("{prefix}SRV=1\nX=$SRV\n");
        let ok = doc
            .rename_with_references_with_options(
                "HOST",
                "SRV",
                EditOptions::new().with_max_output_bytes(expected.len()),
            )
            .unwrap();
        assert_eq!(ok.as_str(), expected);
        replay(&source, &ok);
        let too_small = doc
            .rename_with_references_with_options(
                "HOST",
                "SRV",
                EditOptions::new().with_max_output_bytes(expected.len() - 1),
            )
            .unwrap_err();
        assert!(matches!(
            too_small,
            EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes
        ));
    }

    // LineBreaks::Lf vs Universal with lone CR and max_output_lines.
    let cr_src = "A=1\rB=$A\r";
    let (uni_doc, _) = parse_with(cr_src, Dialect::PORTABLE);
    assert_eq!(
        uni_doc
            .rename_with_references_with_options(
                "A",
                "Z",
                EditOptions::new().with_max_output_lines(2)
            )
            .unwrap()
            .as_str(),
        "Z=1\rB=$Z\r"
    );
    assert!(matches!(
        uni_doc.rename_with_references_with_options(
            "A",
            "Z",
            EditOptions::new().with_max_output_lines(1)
        ),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::Lines
    ));
    let lf_dialect = Dialect::PORTABLE.with_line_breaks(LineBreaks::Lf);
    let (lf_doc, _) = parse_with("A=1\r\nB=$A\r\n", lf_dialect);
    assert_eq!(
        lf_doc
            .rename_with_references_with_options(
                "A",
                "Z",
                EditOptions::new().with_max_output_lines(2)
            )
            .unwrap()
            .as_str(),
        "Z=1\r\nB=$Z\r\n"
    );

    // QuotedTail::Append renames reference inside appended tail under EditOptions.
    let append = Dialect::PORTABLE.with_quoted_tail(QuotedTail::Append);
    let (tail_doc, _) = parse_with("A=1\nX=\"hi\"$A\n", append);
    let tail_ok = tail_doc
        .rename_with_references_with_options(
            "A",
            "HOST",
            EditOptions::new().with_max_output_bytes(32),
        )
        .unwrap();
    assert_eq!(tail_ok.as_str(), "HOST=1\nX=\"hi\"$HOST\n");
    replay("A=1\nX=\"hi\"$A\n", &tail_ok);
}

#[test]
fn rename_with_references_with_options_covers_diagnostics_ties_and_rich_forms() {
    use crate::{Dialect, EditOptions, LimitKind, ParseOptions};

    // parse_limits.max_diagnostics and tie-breaking (max_output_bytes == max_input_bytes -> OutputBytes).
    let (warn_doc, _) = parse("A=1\nBAD KEY=x\nX=$A\n");
    let diag_err = warn_doc
        .rename_with_references_with_options(
            "A",
            "B",
            EditOptions::new().with_parse_limits(ParseOptions::new().with_max_diagnostics(0)),
        )
        .unwrap_err();
    assert!(
        matches!(diag_err, EditError::Limit(limit) if limit.kind() == LimitKind::Diagnostics),
        "{diag_err:?}"
    );
    let (tie_doc, _) = parse("A=1\nX=$A\n");
    let tie_err = tie_doc
        .rename_with_references_with_options(
            "A",
            "SERVER",
            EditOptions::new()
                .with_max_output_bytes(10)
                .with_parse_limits(ParseOptions::new().with_max_input_bytes(10)),
        )
        .unwrap_err();
    assert!(
        matches!(tie_err, EditError::Limit(limit) if limit.kind() == LimitKind::OutputBytes),
        "{tie_err:?}"
    );

    // Nested operators, commands, escaped, single-quoted and backtick-quoted values.
    let rich_dialect = Dialect::PORTABLE.with_backticks(true);
    let rich = "A=1\nN=${A:-$A}\nC=$(echo $A)\nE=\"\\$A\"\nS='$A'\nT=`$A`\n";
    let (rich_doc, _) = parse_with(rich, rich_dialect);
    let rich_ok = rich_doc
        .rename_with_references_with_options(
            "A",
            "SRV",
            EditOptions::new().with_max_output_bytes(128),
        )
        .unwrap();
    assert_eq!(
        rich_ok.as_str(),
        "SRV=1\nN=${SRV:-$SRV}\nC=$(echo $SRV)\nE=\"\\$SRV\"\nS='$SRV'\nT=`$SRV`\n"
    );
    replay(rich, &rich_ok);
}

#[test]
fn comment_uncomment_set_export_with_options_enforce_limits_and_precedence() {
    use crate::{EditOptions, LimitKind, ParseOptions};

    let doc = crate::parse_strict("A=1\n# B=2\nexport C=3\n").unwrap();

    // comment_with_options: exact byte cap passes, 1 byte tighter fails.
    let expected_comment = "# A=1\n# B=2\nexport C=3\n";
    let commented = doc
        .comment_with_options(
            "A",
            EditOptions::new().with_max_output_bytes(expected_comment.len()),
        )
        .unwrap();
    assert_eq!(commented.as_str(), expected_comment);
    replay(doc.source(), &commented);
    assert!(matches!(
        doc.comment_with_options(
            "A",
            EditOptions::new().with_max_output_bytes(expected_comment.len() - 1),
        ),
        Err(EditError::Limit(limit))
            if limit.kind() == LimitKind::OutputBytes
                && limit.limit() == expected_comment.len() - 1
                && limit.observed() == expected_comment.len()
    ));

    // Multiline entry comment expands every line by "# " and respects line/item parse limits.
    let multi = crate::parse_strict("M='l1\nl2'\n").unwrap();
    let multi_commented = multi
        .comment_with_options(
            "M",
            EditOptions::new()
                .with_max_output_bytes("# M='l1\n# l2'\n".len())
                .with_max_output_lines(2)
                .with_parse_limits(ParseOptions::new().with_max_items(2)),
        )
        .unwrap();
    assert_eq!(multi_commented.as_str(), "# M='l1\n# l2'\n");
    assert!(matches!(
        multi.comment_with_options(
            "M",
            EditOptions::new().with_parse_limits(ParseOptions::new().with_max_items(1)),
        ),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::Items
    ));

    // InvalidTarget still precedes Limit when a batch contains Edit::Comment.
    assert_eq!(
        doc.edit_with_options(
            &[
                Edit::Comment { item: 0 },
                Edit::Set {
                    entry: 99,
                    value: "x",
                },
            ],
            EditOptions::new().with_max_output_bytes(1),
        ),
        Err(EditError::InvalidTarget)
    );
    assert_eq!(
        doc.comment_with_options("MISSING", EditOptions::new().with_max_output_bytes(1)),
        Err(EditError::Missing)
    );
}

#[test]
fn uncomment_and_set_export_with_options_enforce_limits() {
    use crate::{EditOptions, LimitKind};

    let doc = crate::parse_strict("A=1\n# B=2\nexport C=3\n").unwrap();

    // uncomment_with_options: shrinking output fits tighter byte cap; too tight fails.
    let expected_uncomment = "A=1\nB=2\nexport C=3\n";
    let uncommented = doc
        .uncomment_with_options(
            1,
            EditOptions::new().with_max_output_bytes(expected_uncomment.len()),
        )
        .unwrap();
    assert_eq!(uncommented.as_str(), expected_uncomment);
    replay(doc.source(), &uncommented);
    assert!(matches!(
        doc.uncomment_with_options(
            1,
            EditOptions::new().with_max_output_bytes(expected_uncomment.len() - 1),
        ),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::OutputBytes
    ));
    assert_eq!(
        doc.uncomment_with_options(0, EditOptions::new().with_max_output_bytes(1)),
        Err(EditError::InvalidTarget)
    );

    // set_export_with_options: adding (+7 bytes), removing (-7 bytes), and no-op.
    let expected_export = "export A=1\n# B=2\nexport C=3\n";
    let exported = doc
        .set_export_with_options(
            "A",
            true,
            EditOptions::new().with_max_output_bytes(expected_export.len()),
        )
        .unwrap();
    assert_eq!(exported.as_str(), expected_export);
    replay(doc.source(), &exported);
    assert!(matches!(
        doc.set_export_with_options(
            "A",
            true,
            EditOptions::new().with_max_output_bytes(expected_export.len() - 1),
        ),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::OutputBytes
    ));

    let expected_unexport = "A=1\n# B=2\nC=3\n";
    let unexported = doc
        .set_export_with_options(
            "C",
            false,
            EditOptions::new().with_max_output_bytes(expected_unexport.len()),
        )
        .unwrap();
    assert_eq!(unexported.as_str(), expected_unexport);
    assert!(matches!(
        doc.set_export_with_options(
            "C",
            true,
            EditOptions::new().with_max_output_bytes(doc.source().len() - 1),
        ),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::OutputBytes
    ));
}

#[test]
fn rename_with_references_with_options_enforces_max_references() {
    use crate::{EditOptions, LimitKind};

    let doc = crate::parse_strict("A=1\nX=$A\nY=${A:-$OTHER}\n").unwrap();
    // Total reference candidates across all entries: $A (in X), ${A:-$OTHER} (in Y), $OTHER (nested in Y) = 3.
    let ok = doc
        .rename_with_references_with_options("A", "B", EditOptions::new().with_max_references(3))
        .unwrap();
    assert_eq!(ok.as_str(), "B=1\nX=$B\nY=${B:-$OTHER}\n");
    replay(doc.source(), &ok);

    let err = doc
        .rename_with_references_with_options("A", "B", EditOptions::new().with_max_references(2))
        .unwrap_err();
    assert!(
        matches!(
            err,
            EditError::Limit(limit)
                if limit.kind() == LimitKind::References
                    && limit.limit() == 2
                    && limit.observed() == 3
        ),
        "{err:?}"
    );
    assert_eq!(
        err.to_string(),
        "edit exceeded limit: references limit 2 exceeded with observed 3"
    );

    // Zero reference budget succeeds when there are no references, fails on even unrelated references.
    let no_refs = crate::parse_strict("A=1\nB=2\n").unwrap();
    assert_eq!(
        no_refs
            .rename_with_references_with_options(
                "A",
                "Z",
                EditOptions::new().with_max_references(0),
            )
            .unwrap()
            .as_str(),
        "Z=1\nB=2\n"
    );
    let unrelated = crate::parse_strict("A=1\nX=$OTHER\n").unwrap();
    assert!(matches!(
        unrelated.rename_with_references_with_options(
            "A",
            "Z",
            EditOptions::new().with_max_references(0),
        ),
        Err(EditError::Limit(limit))
            if limit.kind() == LimitKind::References
                && limit.limit() == 0
                && limit.observed() == 1
    ));

    // Self-rename (key == new_key) does not scan references.
    let same = doc
        .rename_with_references_with_options("A", "A", EditOptions::new().with_max_references(0))
        .unwrap();
    assert_eq!(same.as_str(), doc.source());

    // Deeply nested or wide delimiter forms are bounded by max_references too.
    for value in [
        "$(".repeat(2_000),
        "${".repeat(2_000),
        format!("$({})", "${A}".repeat(2_000)),
    ] {
        let source = format!("A=1\nX={value}\n");
        let bomb = crate::parse_strict(&source).unwrap();
        for cap in [0, 4] {
            let err = bomb
                .rename_with_references_with_options(
                    "A",
                    "B",
                    EditOptions::new().with_max_references(cap),
                )
                .unwrap_err();
            assert!(
                matches!(
                    err,
                    EditError::Limit(limit)
                        if limit.kind() == LimitKind::References
                            && limit.limit() == cap
                            && limit.observed() == cap + 1
                ),
                "cap={cap}, err={err:?}"
            );
        }
    }
}

#[test]
fn edit_options_defer_key_comment_allocs_and_prevent_cache_starvation() {
    use crate::{EditOptions, LimitKind};

    // Variable-name `${}` prefix and LIFO nested forms stay linear under max_references.
    let m = 400usize;
    let payload = "a".repeat(2_000);
    let starvation = format!(
        "A=1\nX=${{{}:-{}{}{}}}\n",
        "${}".repeat(m),
        "${A:-".repeat(m - 1),
        payload,
        "}".repeat(m - 1),
    );
    let doc = crate::parse_strict(&starvation).unwrap();
    let renamed = doc
        .rename_with_references_with_options("A", "B", EditOptions::new().with_max_references(m))
        .unwrap();
    assert!(renamed.as_str().starts_with("B=1\n"));
    assert!(matches!(
        doc.rename_with_references_with_options(
            "A",
            "B",
            EditOptions::new().with_max_references(m - 1)
        ),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::References
    ));

    // Deferred quoted-insert, rename, and exact multiline comment byte caps.
    let py_doc = crate::parse_strict_with("A=1\n", crate::Dialect::PYTHON_DOTENV).unwrap();
    let big_quoted_key = format!("A {}", "x".repeat(256));
    assert!(matches!(
        py_doc.insert_with_options(
            &big_quoted_key,
            Some("1"),
            InsertPosition::End,
            EditOptions::new().with_max_output_bytes(32),
        ),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::OutputBytes
    ));
    assert!(matches!(
        py_doc.rename_with_options(
            "A",
            &big_quoted_key,
            EditOptions::new().with_max_output_bytes(32),
        ),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::OutputBytes
    ));
    let many_lines = format!("M='{}'\n", "\n".repeat(30));
    let multi_doc = crate::parse_strict(&many_lines).unwrap();
    let expected_comment_len = many_lines.len() + 2 * 31;
    assert_eq!(
        multi_doc
            .comment_with_options(
                "M",
                EditOptions::new().with_max_output_bytes(expected_comment_len)
            )
            .unwrap()
            .as_str()
            .len(),
        expected_comment_len
    );
    assert!(matches!(
        multi_doc.comment_with_options(
            "M",
            EditOptions::new().with_max_output_bytes(expected_comment_len - 1)
        ),
        Err(EditError::Limit(limit))
            if limit.kind() == LimitKind::OutputBytes
                && limit.observed() == expected_comment_len
    ));
}
