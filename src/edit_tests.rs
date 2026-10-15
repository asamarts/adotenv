use crate::{
    parse, remove, set, AssignmentState, DiagnosticCode, Edit, EditError, InsertPosition, ItemKind,
    Quote,
};
use alloc::{string::String, vec, vec::Vec};

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
    assert_eq!(
        result.as_str(),
        "# heading\r\nexport RENAMED=\"new value\"\r\nC=last\r\nD"
    );
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
    assert_eq!(
        result.as_str(),
        "=bad\nA=\"much longer\"\nBROKEN=\"x\" tail\n# untouched\nexport B\n"
    );
    let (after, diagnostics) = parse(result.as_str());
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
    assert_eq!(doc.set("A", "bad\0"), Err(EditError::Unencodable));
    assert_eq!(
        doc.insert("C", Some("bad\u{7f}"), InsertPosition::End),
        Err(EditError::Unencodable)
    );
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
        Err(EditError::WouldChangeMeaning)
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
