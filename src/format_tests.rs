use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use proptest::prelude::*;

use crate::{
    parse, parse_strict, parse_with, BlankLines, Dialect, EqualsSpacing, FinalNewline, FormatError,
    FormatOptions, FormatResult, QuoteStyle,
};

fn options(
    equals_spacing: EqualsSpacing,
    trim: bool,
    final_newline: FinalNewline,
) -> FormatOptions {
    FormatOptions {
        equals_spacing,
        trim_trailing_whitespace: trim,
        blank_lines: BlankLines::Preserve,
        quote_style: QuoteStyle::Preserve,
        final_newline,
    }
}

#[test]
fn spaces_around_equals_and_tabs_are_normalized() {
    let doc = parse_strict("A=1\nB =  2\nexport K\t=\t'v'\n").unwrap();
    let formatted = doc
        .format(&options(
            EqualsSpacing::Spaces,
            false,
            FinalNewline::Preserve,
        ))
        .unwrap();
    assert_eq!(formatted.as_str(), "A = 1\nB = 2\nexport K = 'v'\n");
}

#[test]
fn spacing_does_not_turn_hash_values_into_comments() {
    let doc = parse_strict("A=#value\nB= #comment\n").unwrap();
    let spaces = doc
        .format(&options(
            EqualsSpacing::Spaces,
            false,
            FinalNewline::Preserve,
        ))
        .unwrap();
    assert_eq!(spaces.as_str(), "A =#value\nB = #comment\n");
    let none = doc
        .format(&options(EqualsSpacing::None, false, FinalNewline::Preserve))
        .unwrap();
    assert_eq!(none.as_str(), "A=#value\nB= #comment\n");
    let (after, _) = parse(none.as_str());
    assert_eq!(after.get("A"), Some("#value"));
    assert_eq!(after.get("B"), Some(""));
    assert_eq!(
        after.unique("B").unwrap().inline_comment(),
        Some("#comment")
    );
}

#[test]
fn quote_style_can_normalize_without_changing_values() {
    let source = "A='value'\nB=\"two words\"\nC=plain\nD=value # note\n";
    let doc = parse_strict(source).unwrap();
    let formatted = doc
        .format(&FormatOptions {
            quote_style: QuoteStyle::Double,
            ..FormatOptions::default()
        })
        .unwrap();
    assert_eq!(
        formatted.as_str(),
        "A=\"value\"\nB=\"two words\"\nC=\"plain\"\nD=\"value\" # note\n"
    );
    let after = parse_strict(formatted.as_str()).unwrap();
    for (left, right) in doc.entries().zip(after.entries()) {
        assert_eq!(left.key(), right.key());
        assert_eq!(left.value(), right.value());
        assert_eq!(right.quote(), crate::Quote::Double);
    }
    let again = parse_strict(formatted.as_str())
        .unwrap()
        .format(&FormatOptions {
            quote_style: QuoteStyle::Double,
            ..FormatOptions::default()
        })
        .unwrap();
    assert!(again.is_empty());
}

#[test]
fn quote_style_handles_a_key_that_is_only_a_bom() {
    let source = "# c\n\u{feff}=old\n";
    let (doc, _) = parse_with(source, Dialect::PYTHON_DOTENV);
    let formatted = doc
        .format(&FormatOptions {
            quote_style: QuoteStyle::Double,
            ..FormatOptions::default()
        })
        .unwrap();
    assert_eq!(formatted.as_str(), "# c\n\u{feff}=\"old\"\n");
    let (after, _) = parse_with(formatted.as_str(), Dialect::PYTHON_DOTENV);
    assert_eq!(after.get("\u{feff}"), Some("old"));
}

#[test]
fn quote_style_can_use_single_quotes_for_multiline_values() {
    let source = "A=\"first\nsecond\"\nB='two'\nC=''\n";
    let doc = parse_strict(source).unwrap();
    let formatted = doc
        .format(&FormatOptions {
            quote_style: QuoteStyle::Single,
            ..FormatOptions::default()
        })
        .unwrap();
    assert_eq!(formatted.as_str(), "A='first\nsecond'\nB='two'\nC=''\n");
    let after = parse_strict(formatted.as_str()).unwrap();
    assert_eq!(after.get("A"), Some("first\nsecond"));
    assert_eq!(after.get("B"), Some("two"));
}

#[test]
fn quote_style_can_prefer_unquoted_values_without_guessing() {
    let source = "A=\"simple\"\nB=\"two words\"\nC=''\n";
    let doc = parse_strict(source).unwrap();
    let formatted = doc
        .format(&FormatOptions {
            quote_style: QuoteStyle::Unquoted,
            ..FormatOptions::default()
        })
        .unwrap();
    assert_eq!(formatted.as_str(), "A=simple\nB=two words\nC=''\n");
    let after = parse_strict(formatted.as_str()).unwrap();
    assert_eq!(after.get("A"), Some("simple"));
    assert_eq!(after.get("B"), Some("two words"));
    assert_eq!(after.get("C"), Some(""));
}

#[test]
fn quote_normalization_skips_raw_values_and_bare_keys() {
    let (doc, _) = parse_with("A='value'\nBARE\n", Dialect::DOCKER_RAW);
    let formatted = doc
        .format(&FormatOptions {
            quote_style: QuoteStyle::Double,
            ..FormatOptions::default()
        })
        .unwrap();
    assert!(formatted.is_empty());
}

#[test]
fn trailing_whitespace_is_trimmed_outside_values_and_invalid_lines() {
    let source = "# c   \n\n   \nA=\"x  \ny\"   \nB=x # c   \nBAD KEY=value   \n";
    let (doc, _) = parse(source);
    let formatted = doc
        .format(&options(
            EqualsSpacing::Preserve,
            true,
            FinalNewline::Preserve,
        ))
        .unwrap();
    assert_eq!(
        formatted.as_str(),
        "# c\n\n\nA=\"x  \ny\"\nB=x # c\nBAD KEY=value   \n"
    );
}

#[test]
fn trimming_does_not_expose_a_mixed_line_ending_cr() {
    let source = "A=x \r \n";
    let doc = parse_strict(source).unwrap();
    let formatted = doc
        .format(&options(
            EqualsSpacing::Preserve,
            true,
            FinalNewline::Preserve,
        ))
        .unwrap();
    assert_eq!(formatted.as_str(), source);
    assert!(formatted.is_empty());
}

#[test]
fn raw_values_keep_trailing_spaces() {
    let (doc, _) = parse_with("A= x  \n", Dialect::DOCKER_RAW);
    let formatted = doc
        .format(&options(
            EqualsSpacing::Spaces,
            true,
            FinalNewline::Preserve,
        ))
        .unwrap();
    assert_eq!(formatted.as_str(), "A= x  \n");
    assert!(formatted.is_empty());
    let (after, _) = parse_with(formatted.as_str(), Dialect::DOCKER_RAW);
    assert_eq!(after.get("A"), Some(" x  "));
}

#[test]
fn empty_compose_keys_are_not_indented_or_reformatted() {
    let source = "= value\nC=1\n";
    let (doc, _) = parse_with(source, Dialect::COMPOSE);
    let formatted = doc.format(&FormatOptions::normalize()).unwrap();
    assert_eq!(formatted.as_str(), "= value\nC = 1\n");
    let again = parse_with(formatted.as_str(), Dialect::COMPOSE)
        .0
        .format(&FormatOptions::normalize())
        .unwrap();
    assert!(again.is_empty());
}

#[test]
fn blank_line_runs_collapse_to_one_group_separator() {
    let source = "# header\n\n\nA=1\n\n\n\nB=2\n\n";
    let doc = parse_strict(source).unwrap();
    let formatted = doc.format(&FormatOptions::normalize_groups()).unwrap();
    assert_eq!(formatted.as_str(), "# header\n\nA = 1\n\nB = 2\n\n");
    assert_eq!(formatted.as_str().matches("\n\n").count(), 3);
    let again = parse_strict(formatted.as_str())
        .unwrap()
        .format(&FormatOptions::normalize_groups())
        .unwrap();
    assert!(again.is_empty());
}

#[test]
fn blank_line_collapse_handles_leading_and_trailing_runs() {
    let source = "\n\n\nA=1\n\n\n";
    let doc = parse_strict(source).unwrap();
    let formatted = doc
        .format(&FormatOptions {
            blank_lines: BlankLines::Collapse,
            ..FormatOptions::normalize()
        })
        .unwrap();
    assert_eq!(formatted.as_str(), "\nA = 1\n\n");
}

#[test]
fn collapse_is_idempotent_across_removed_cr_blank_runs() {
    // Regression from the blank-collapse fuzz oracle: removing CR-only blank
    // items changes which kept item owns trailing-whitespace planning.
    let source = "\n]\nA]\n\n\n#A\n]A\u{1}\0\0\0\0\0\0\0e6666\n\n\n\n#\nA]A\n\n\r\r\r#1heling\0\0 \n\n#\nAo\n\n\n";
    let (doc, _) = parse(source);
    let first = doc.format(&FormatOptions::normalize_groups()).unwrap();
    let (again, _) = parse(first.as_str());
    let second = again.format(&FormatOptions::normalize_groups()).unwrap();
    assert!(
        second.is_empty(),
        "second pass changed: {:?}",
        second.edits()
    );
    assert_eq!(second.as_str(), first.as_str());
}

#[test]
fn blank_line_preserve_is_exact() {
    let source = "A=1\n\n\nB=2\n";
    let doc = parse_strict(source).unwrap();
    let formatted = doc
        .format(&FormatOptions {
            equals_spacing: EqualsSpacing::Preserve,
            trim_trailing_whitespace: false,
            blank_lines: BlankLines::Preserve,
            quote_style: QuoteStyle::Preserve,
            final_newline: FinalNewline::Preserve,
        })
        .unwrap();
    assert!(formatted.is_empty());
}

#[test]
fn final_newline_policies_are_exact() {
    let doc = parse_strict("A=1").unwrap();
    let ensured = doc
        .format(&options(
            EqualsSpacing::Preserve,
            false,
            FinalNewline::Ensure,
        ))
        .unwrap();
    assert_eq!(ensured.as_str(), "A=1\n");

    let doc = parse_strict("A=1\r\n").unwrap();
    let removed = doc
        .format(&options(
            EqualsSpacing::Preserve,
            false,
            FinalNewline::Remove,
        ))
        .unwrap();
    assert_eq!(removed.as_str(), "A=1");

    let doc = parse_strict("A=1\n\n").unwrap();
    let once = doc
        .format(&options(
            EqualsSpacing::Preserve,
            false,
            FinalNewline::Remove,
        ))
        .unwrap();
    assert_eq!(once.as_str(), "A=1\n\n");
    assert!(once.is_empty());

    let (empty, _) = parse("");
    let formatted = empty
        .format(&options(
            EqualsSpacing::Preserve,
            false,
            FinalNewline::Ensure,
        ))
        .unwrap();
    assert_eq!(formatted.as_str(), "");
}

#[test]
fn remove_final_cr_trims_the_final_line_once() {
    let options = FormatOptions::default()
        .with_trim_trailing_whitespace(true)
        .with_final_newline(FinalNewline::Remove);
    for (source, expected) in [
        ("U=\"multi\nline\" \r", "U=\"multi\nline\""),
        ("# a comment  \r", "# a comment"),
        ("A=1  \r", "A=1"),
        ("A=1\r", "A=1"),
        ("A=\"x  \" \r", "A=\"x  \""),
        ("A=1  \r\n", "A=1"),
        ("  \r", "  \r"),
    ] {
        let doc = parse_strict(source).unwrap();
        let formatted = doc.format(&options).unwrap();
        assert_eq!(formatted.as_str(), expected, "{source:?}");
        let (after, _) = parse_with(formatted.as_str(), Dialect::PORTABLE);
        assert!(
            after.format(&options).unwrap().is_empty(),
            "{source:?} formatted to {:?} and then changed",
            formatted.as_str()
        );
    }

    let preserve = FormatOptions::default().with_trim_trailing_whitespace(true);
    let doc = parse_strict("A=1  \r").unwrap();
    assert_eq!(doc.format(&preserve).unwrap().as_str(), "A=1  \r");

    // In an LF-only dialect, removing one final CR can expose another CR as a
    // line ending. Leave the file unchanged instead of removing one per pass.
    for source in ["A=1\r\r", "A=1\r\r\n", "A=1  \r\r"] {
        let (doc, _) = parse_with(source, Dialect::COMPOSE);
        let formatted = doc.format(&options).unwrap();
        assert_eq!(formatted.as_str(), source, "{source:?}");
        assert!(formatted.is_empty(), "{source:?}");
    }

    // Raw-value dialects keep trailing spaces, but the final CR still goes.
    let (docker, _) = parse_with("A=x  \r", Dialect::DOCKER_RAW);
    let formatted = docker.format(&options).unwrap();
    assert_eq!(formatted.as_str(), "A=x  ");
    let (after, _) = parse_with(formatted.as_str(), Dialect::DOCKER_RAW);
    assert!(after.format(&options).unwrap().is_empty());
}

#[test]
fn normalize_is_idempotent_and_replays_to_verified_text() {
    let source = "# header   \n\nA=1   \nB =  2\n";
    let doc = parse_strict(source).unwrap();
    let first = doc.format(&FormatOptions::normalize()).unwrap();
    assert_eq!(first.as_str(), "# header\n\nA = 1\nB = 2\n");
    assert!(!first.is_empty());

    let mut replayed = String::new();
    let mut cursor = 0;
    for edit in first.edits() {
        assert!(edit.range.start >= cursor);
        replayed.push_str(&source[cursor..edit.range.start]);
        replayed.push_str(&edit.replacement);
        cursor = edit.range.end;
    }
    replayed.push_str(&source[cursor..]);
    assert_eq!(replayed, first.as_str());

    let second = parse_strict(first.as_str())
        .unwrap()
        .format(&FormatOptions::normalize())
        .unwrap();
    assert!(second.is_empty());
    assert_eq!(second.as_str(), first.as_str());
}

#[test]
fn formatting_rejects_unverifiable_changes() {
    // The formatter's risky spacing cases are skipped rather than producing an
    // invalid result, so a normal document never needs this path; the error
    // remains available for future rules and defensive verification.
    let error: FormatError = FormatError::WouldChangeMeaning;
    assert_eq!(
        error.to_string(),
        "formatting would change document meaning"
    );
}

proptest! {
    #[test]
    fn formatting_preserves_declarations_and_is_idempotent(
        chars in prop::collection::vec(
            prop_oneof![
                Just('A'), Just('B'), Just('='), Just('1'), Just('#'), Just(' '),
                Just('\t'), Just('\n'), Just('\r'), Just('"'), Just('\''),
                Just('\\'), any::<char>(),
            ],
            0..240,
        )
    ) {
        let source: String = chars.into_iter().collect();
        let (doc, _) = parse(&source);
        if let Ok(formatted) = doc.format(&FormatOptions::normalize()) {
            let (after, _) = parse(formatted.as_str());
            prop_assert_eq!(doc.entries().len(), after.entries().len());
            for (left, right) in doc.entries().zip(after.entries()) {
                prop_assert_eq!(left.key(), right.key());
                prop_assert_eq!(left.value(), right.value());
                prop_assert_eq!(left.state(), right.state());
                prop_assert_eq!(left.quote(), right.quote());
                prop_assert_eq!(left.exported(), right.exported());
                prop_assert_eq!(left.participates_in_lookup(), right.participates_in_lookup());
            }
            let mut cursor = 0;
            for edit in formatted.edits() {
                prop_assert!(edit.range.start >= cursor);
                cursor = edit.range.end;
            }
            let again = parse(formatted.as_str())
                .0
                .format(&FormatOptions::normalize())
                .map_or(true, |again| again.is_empty());
            prop_assert!(again);
        }
    }
}

#[test]
fn trimming_keeps_a_whitespace_only_last_line_without_a_line_ending() {
    let trim = FormatOptions::default().with_trim_trailing_whitespace(true);
    for dialect in [Dialect::PORTABLE, Dialect::COMPOSE, Dialect::DOCKER_RAW] {
        for (source, expected) in [
            (" ", " "),
            ("A=1\n   ", "A=1\n   "),
            ("# c   \n\t", "# c\n\t"),
            (" \n ", "\n "),
        ] {
            let (doc, _) = parse_with(source, dialect);
            for options in [trim, trim.with_final_newline(FinalNewline::Remove)] {
                let formatted = doc.format(&options).unwrap();
                assert_eq!(formatted.as_str(), expected, "{source:?} {dialect:?}");
                let (after, _) = parse_with(formatted.as_str(), dialect);
                assert!(after.format(&trim).unwrap().is_empty());
            }
        }
        // A final newline turns that line into an empty blank line instead.
        let (doc, _) = parse_with("A=1\n   ", dialect);
        let ensured = doc
            .format(&trim.with_final_newline(FinalNewline::Ensure))
            .unwrap();
        assert_eq!(ensured.as_str(), "A=1\n\n");
    }
}

#[test]
fn collapsing_trailing_blank_lines_does_not_add_another_line_ending() {
    let options = FormatOptions::normalize_groups();
    for (source, expected) in [
        ("A=1\n\n\n   ", "A = 1\n\n"),
        (" \n ", "\n"),
        (" \r ", " \r"),
        ("A=1\n\n ", "A = 1\n\n"),
    ] {
        let (doc, _) = parse(source);
        let formatted = doc.format(&options).unwrap();
        assert_eq!(formatted.as_str(), expected, "{source:?}");
        let (after, _) = parse(formatted.as_str());
        assert!(after.format(&options).unwrap().is_empty(), "{source:?}");
    }
}

#[test]
fn a_change_that_would_read_differently_is_skipped() {
    let double = FormatOptions::default().with_quote_style(QuoteStyle::Double);
    let remove = FormatOptions::default().with_final_newline(FinalNewline::Remove);
    for (dialect, options, source, expected) in [
        // A final line ending after a Compose bare key turns the value of the
        // empty key into a bare key, and removing one does the reverse.
        (
            Dialect::COMPOSE,
            FormatOptions::normalize(),
            "A =1\nG",
            "A = 1\nG",
        ),
        (Dialect::COMPOSE, remove, "A=1\nG\n", "A=1\nG\n"),
        // A double quote added after an unterminated quote would close it.
        (
            Dialect::PORTABLE,
            double,
            "A='1'\nBROKEN=\"\nPORT='8080'\n",
            "A=\"1\"\nBROKEN=\"\nPORT='8080'\n",
        ),
    ] {
        let (doc, _) = parse_with(source, dialect);
        let formatted = doc.format(&options).unwrap();
        assert_eq!(formatted.as_str(), expected, "{source:?}");
        let (after, _) = parse_with(formatted.as_str(), dialect);
        assert!(after.format(&options).unwrap().is_empty(), "{source:?}");
    }
}

#[test]
fn spacing_keeps_a_leading_hash_and_a_node_colon_separator() {
    let options = FormatOptions::normalize_groups();
    for (dialect, source, expected) in [
        (Dialect::PORTABLE, "A=#x # c\nB =2\n", "A =#x # c\nB = 2\n"),
        (Dialect::NODE_DOTENV, "A: 1\nB =2\n", "A: 1\nB = 2\n"),
    ] {
        let (doc, _) = parse_with(source, dialect);
        let formatted = doc.format(&options).unwrap();
        assert_eq!(formatted.as_str(), expected, "{source:?}");
    }
}

#[test]
fn quote_styles_keep_values_with_a_dollar_sign() {
    let source = "H=h\nA=${H}\nB='$H'\nC=\"a$b\"\n";
    let doc = parse_strict(source).unwrap();
    for (style, expected) in [
        (QuoteStyle::Single, "H='h'\nA=${H}\nB='$H'\nC=\"a$b\"\n"),
        (QuoteStyle::Double, "H=\"h\"\nA=${H}\nB='$H'\nC=\"a$b\"\n"),
        (QuoteStyle::Unquoted, "H=h\nA=${H}\nB='$H'\nC=\"a$b\"\n"),
    ] {
        let options = FormatOptions::default().with_quote_style(style);
        assert_eq!(
            doc.format(&options).unwrap().as_str(),
            expected,
            "{style:?}"
        );
    }
}

fn any_format_options() -> impl Strategy<Value = FormatOptions> {
    (
        0..3usize,
        any::<bool>(),
        any::<bool>(),
        0..4usize,
        0..3usize,
    )
        .prop_map(|(equals, trim, collapse, quote, final_newline)| {
            FormatOptions::default()
                .with_equals_spacing(
                    [
                        EqualsSpacing::Preserve,
                        EqualsSpacing::Spaces,
                        EqualsSpacing::None,
                    ][equals],
                )
                .with_trim_trailing_whitespace(trim)
                .with_blank_lines(if collapse {
                    BlankLines::Collapse
                } else {
                    BlankLines::Preserve
                })
                .with_quote_style(
                    [
                        QuoteStyle::Preserve,
                        QuoteStyle::Double,
                        QuoteStyle::Single,
                        QuoteStyle::Unquoted,
                    ][quote],
                )
                .with_final_newline(
                    [
                        FinalNewline::Preserve,
                        FinalNewline::Ensure,
                        FinalNewline::Remove,
                    ][final_newline],
                )
        })
}

proptest! {
    #[test]
    fn every_option_set_preserves_declarations_and_is_idempotent(
        chars in prop::collection::vec(
            prop_oneof![
                Just('A'), Just('B'), Just('='), Just('1'), Just('#'), Just(' '),
                Just('\t'), Just('\n'), Just('\r'), Just('"'), Just('\''),
                Just('\\'), Just('$'), any::<char>(),
            ],
            0..160,
        ),
        options in any_format_options(),
        dialect in prop::sample::select(vec![
            Dialect::PORTABLE,
            Dialect::NODE_DOTENV,
            Dialect::PYTHON_DOTENV,
            Dialect::COMPOSE,
            Dialect::DOCKER_RAW,
        ]),
    ) {
        let source: String = chars.into_iter().collect();
        let (doc, _) = parse_with(&source, dialect);
        if let Ok(formatted) = doc.format(&options) {
            let (after, _) = parse_with(formatted.as_str(), dialect);
            if options.blank_lines == BlankLines::Collapse {
                let adjacent_blanks = after
                    .items()
                    .zip(after.items().skip(1))
                    .any(|(left, right)| {
                        left.kind() == crate::ItemKind::Blank
                            && right.kind() == crate::ItemKind::Blank
                    });
                prop_assert!(!adjacent_blanks);
            }
            prop_assert_eq!(doc.entries().len(), after.entries().len());
            for (left, right) in doc.entries().zip(after.entries()) {
                prop_assert_eq!(left.key(), right.key());
                prop_assert_eq!(left.value(), right.value());
                prop_assert_eq!(left.state(), right.state());
                if options.quote_style == QuoteStyle::Preserve {
                    prop_assert_eq!(left.quote(), right.quote());
                }
                prop_assert_eq!(left.exported(), right.exported());
                prop_assert_eq!(left.participates_in_lookup(), right.participates_in_lookup());
            }
            let mut reversed = source.clone();
            for edit in formatted.edits().iter().rev() {
                reversed.replace_range(edit.range.clone(), &edit.replacement);
            }
            prop_assert_eq!(reversed.as_str(), formatted.as_str());
            let again = after.format(&options);
            prop_assert!(
                again.as_ref().is_ok_and(FormatResult::is_empty),
                "{:?} formatted to {:?}, then {:?}",
                source,
                formatted.as_str(),
                again.map(FormatResult::into_string)
            );
        }
    }
}

#[test]
fn format_options_builders_and_result_views_are_complete() {
    let defaults = FormatOptions::default();
    assert_eq!(defaults.equals_spacing, EqualsSpacing::Preserve);
    assert!(!defaults.trim_trailing_whitespace);
    assert_eq!(defaults.blank_lines, BlankLines::Preserve);
    assert_eq!(defaults.quote_style, QuoteStyle::Preserve);
    assert_eq!(defaults.final_newline, FinalNewline::Preserve);

    let options = defaults
        .with_equals_spacing(EqualsSpacing::None)
        .with_trim_trailing_whitespace(true)
        .with_blank_lines(BlankLines::Collapse)
        .with_quote_style(QuoteStyle::Single)
        .with_final_newline(FinalNewline::Ensure);
    assert_eq!(options.equals_spacing, EqualsSpacing::None);
    assert!(options.trim_trailing_whitespace);
    assert_eq!(options.blank_lines, BlankLines::Collapse);
    assert_eq!(options.quote_style, QuoteStyle::Single);
    assert_eq!(options.final_newline, FinalNewline::Ensure);

    let doc = parse_strict("A=1\n").unwrap();
    let unchanged = doc.format(&defaults).unwrap();
    assert_eq!(unchanged.edits(), &[]);
    assert_eq!(unchanged.clone().into_edits(), Vec::new());
    assert_eq!(unchanged.into_string(), "A=1\n");

    let formatted = doc.format(&FormatOptions::normalize()).unwrap();
    assert_eq!(formatted.as_str(), "A = 1\n");
    assert_eq!(formatted.clone().into_edits(), formatted.edits().to_vec());
    assert_eq!(formatted.into_string(), "A = 1\n");
}

#[test]
fn unquoted_quote_style_does_not_mix_with_other_rewrites() {
    let source = "A=\"simple\"\n";
    let doc = parse_strict(source).unwrap();
    let formatted = doc
        .format(&FormatOptions {
            equals_spacing: EqualsSpacing::Spaces,
            quote_style: QuoteStyle::Unquoted,
            ..FormatOptions::default()
        })
        .unwrap();
    assert_eq!(formatted.as_str(), "A = \"simple\"\n");
    assert!(formatted.as_str().contains("\"simple\""));
}

#[test]
fn quote_planning_is_idempotent_when_other_rules_shorten_the_line() {
    let dialect = Dialect {
        line_byte_limit: Some(11),
        ..Dialect::PORTABLE
    };
    let options = FormatOptions {
        quote_style: QuoteStyle::Double,
        ..options(EqualsSpacing::None, false, FinalNewline::Preserve)
    };
    let doc = parse_with("PORT =8080\n", dialect).0;
    let first = doc.format(&options).unwrap();
    assert_eq!(first.as_str(), "PORT=\"8080\"\n");
    let (again, _) = parse_with(first.as_str(), dialect);
    assert_eq!(again.format(&options).unwrap().as_str(), first.as_str());
}

#[test]
fn quote_planning_is_idempotent_after_trailing_whitespace_trimming() {
    let dialect = Dialect {
        line_byte_limit: Some(5),
        ..Dialect::PORTABLE
    };
    let options = FormatOptions {
        quote_style: QuoteStyle::Double,
        ..options(EqualsSpacing::Preserve, true, FinalNewline::Preserve)
    };
    let doc = parse_with("A=1  \n", dialect).0;
    let first = doc.format(&options).unwrap();
    assert_eq!(first.as_str(), "A=\"1\"\n");
    let (again, _) = parse_with(first.as_str(), dialect);
    assert_eq!(again.format(&options).unwrap().as_str(), first.as_str());
}

#[test]
fn format_with_options_enforces_output_parse_and_pre_encode_limits() {
    use crate::{EditError, EditOptions, LimitKind};

    // Expanding format: exact byte cap passes, 1 byte tighter fails.
    let doc = parse_strict("A=1\n").unwrap();
    let ok = doc
        .format_with_options(
            &FormatOptions::normalize(),
            EditOptions::new().with_max_output_bytes(6),
        )
        .unwrap();
    assert_eq!(ok.as_str(), "A = 1\n");
    let err = doc
        .format_with_options(
            &FormatOptions::normalize(),
            EditOptions::new().with_max_output_bytes(5),
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            EditError::Limit(limit)
                if limit.kind() == LimitKind::OutputBytes
                    && limit.limit() == 5
                    && limit.observed() == 6
        ),
        "{err:?}"
    );
    assert_eq!(
        err.to_string(),
        "edit exceeded limit: output-bytes limit 5 exceeded with observed 6"
    );

    // Shrinking format fits tighter output cap even though original source is larger.
    let roomy = parse_strict("A =   1   \n\n\n").unwrap();
    assert!(roomy.source().len() > 6);
    let shrunk = roomy
        .format_with_options(
            &FormatOptions::normalize_groups(),
            EditOptions::new().with_max_output_bytes(7),
        )
        .unwrap();
    assert_eq!(shrunk.as_str(), "A = 1\n\n");
    assert!(matches!(
        roomy.format_with_options(
            &FormatOptions::normalize_groups(),
            EditOptions::new().with_max_output_bytes(6),
        ),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::OutputBytes
    ));

    // Pre-encode byte cap in plan_quotes rejects oversized decoded value before re-encoding.
    let big_val = "x".repeat(64);
    let big_source = format!("A={big_val}\n");
    let big_doc = parse_strict(&big_source).unwrap();
    let pre_encode = big_doc
        .format_with_options(
            &FormatOptions::default().with_quote_style(QuoteStyle::Double),
            EditOptions::new().with_max_output_bytes(32),
        )
        .unwrap_err();
    assert!(matches!(
        pre_encode,
        EditError::Limit(limit)
            if limit.kind() == LimitKind::OutputBytes
                && limit.limit() == 32
                && limit.observed() == 64
    ));

    // Cumulative pre-encode byte cap rejects many small entries before encoding all of them.
    let multi_doc = parse_strict("A=123456\nB=123456\nC=123456\n").unwrap();
    let cumulative = multi_doc
        .format_with_options(
            &FormatOptions::default().with_quote_style(QuoteStyle::Double),
            EditOptions::new().with_max_output_bytes(15),
        )
        .unwrap_err();
    assert!(matches!(
        cumulative,
        EditError::Limit(limit)
            if limit.kind() == LimitKind::OutputBytes
                && limit.limit() == 15
                && limit.observed() == 18
    ));
}

#[test]
fn format_with_options_enforces_line_and_parse_limits() {
    use crate::{EditError, EditOptions, LimitKind, ParseOptions};

    let doc = parse_strict("A=1\n").unwrap();

    // Output line budget: Single quote expands escaped newline to 2 lines; Double collapses to 1.
    let escaped = parse_strict("A=\"a\\nb\"\n").unwrap();
    assert!(matches!(
        escaped.format_with_options(
            &FormatOptions::default().with_quote_style(QuoteStyle::Single),
            EditOptions::new().with_max_output_lines(1),
        ),
        Err(EditError::Limit(limit))
            if limit.kind() == LimitKind::Lines && limit.limit() == 1 && limit.observed() == 2
    ));
    let expanded = escaped
        .format_with_options(
            &FormatOptions::default().with_quote_style(QuoteStyle::Single),
            EditOptions::new().with_max_output_lines(2),
        )
        .unwrap();
    assert_eq!(expanded.as_str(), "A='a\nb'\n");
    let collapsed = parse_strict(expanded.as_str())
        .unwrap()
        .format_with_options(
            &FormatOptions::default().with_quote_style(QuoteStyle::Double),
            EditOptions::new().with_max_output_lines(1),
        )
        .unwrap();
    assert_eq!(collapsed.as_str(), "A=\"a\\nb\"\n");

    // parse_limits forwards InputBytes, Items, Lines, Diagnostics and tightest cap wins.
    assert!(matches!(
        doc.format_with_options(
            &FormatOptions::normalize(),
            EditOptions::new()
                .with_max_output_bytes(20)
                .with_parse_limits(ParseOptions::new().with_max_input_bytes(5)),
        ),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::InputBytes
    ));
    assert!(matches!(
        doc.format_with_options(
            &FormatOptions::default(),
            EditOptions::new().with_parse_limits(ParseOptions::new().with_max_items(0)),
        ),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::Items
    ));
    let dup = parse("A=1\nA=2\n").0;
    assert!(matches!(
        dup.format_with_options(
            &FormatOptions::default(),
            EditOptions::new().with_parse_limits(ParseOptions::new().with_max_diagnostics(0)),
        ),
        Err(EditError::Limit(limit)) if limit.kind() == LimitKind::Diagnostics
    ));
}

#[test]
fn lf_only_quote_style_is_idempotent_with_trim_and_final_newline_remove() {
    use crate::LineBreaks;

    let lf = Dialect::PORTABLE.with_line_breaks(LineBreaks::Lf);
    let lf_capped_8 = lf.with_line_byte_limit(Some(8));
    let lf_capped_7 = lf.with_line_byte_limit(Some(7));
    let dq_trim = FormatOptions::default()
        .with_quote_style(QuoteStyle::Double)
        .with_trim_trailing_whitespace(true);
    let dq_remove = FormatOptions::default()
        .with_quote_style(QuoteStyle::Double)
        .with_final_newline(FinalNewline::Remove);
    let sq = FormatOptions::default().with_quote_style(QuoteStyle::Single);

    let cases = [
        (lf, "A=1\r \n", dq_trim, "A=\"1\\r\"\n"),
        (lf_capped_7, "A=1\r \n", dq_trim, "A=\"1\\r\"\n"),
        (lf, "A=1\r ", dq_trim, "A=\"1\\r\""),
        (
            lf,
            "A=1\r ",
            dq_trim.with_final_newline(FinalNewline::Ensure),
            "A=\"1\\r\"\n",
        ),
        (
            lf,
            "A=1\r \r",
            dq_trim.with_final_newline(FinalNewline::Remove),
            "A=\"1\\r\"",
        ),
        (lf, "A=1\r\r\n", dq_remove, "A=\"1\\r\""),
        (lf_capped_7, "A=1\r\r\n", dq_remove, "A=\"1\\r\""),
        (lf_capped_8, "A=1\r\r", dq_remove, "A=\"1\\r\""),
        (
            lf,
            "A=1\r \n",
            sq.with_trim_trailing_whitespace(true),
            "A='1\r'\n",
        ),
        (
            lf,
            "A=1\r\r\n",
            sq.with_final_newline(FinalNewline::Remove),
            "A='1\r'",
        ),
        (
            lf,
            "A=1\r \r",
            FormatOptions::default()
                .with_trim_trailing_whitespace(true)
                .with_final_newline(FinalNewline::Remove),
            "A=1\r ",
        ),
        (
            lf.with_quoted_tail(crate::QuotedTail::Append),
            "A=\"a'b\\r\"",
            sq.with_final_newline(FinalNewline::Ensure),
            "A=''a'b\r\r",
        ),
    ];

    for (dialect, source, opts, expected) in cases {
        let (doc, _) = parse_with(source, dialect);
        let first = doc.format(&opts).unwrap();
        assert_eq!(first.as_str(), expected, "{source:?} {opts:?}");
        let (after, _) = parse_with(first.as_str(), dialect);
        assert_eq!(after.get("A"), doc.get("A"), "{source:?} {opts:?}");
        let second = after.format(&opts).unwrap();
        assert!(
            second.is_empty(),
            "{source:?} formatted to {:?}, then {:?}",
            first.as_str(),
            second.as_str()
        );
    }
}

#[test]
fn python_quote_style_is_idempotent_for_bom_export_keys() {
    use crate::{BomPolicy, QuotedTail};

    let python = Dialect::PYTHON_DOTENV;
    let python_keep = python.with_bom(BomPolicy::Keep);
    let python_append = python.with_quoted_tail(QuotedTail::Append);

    for (dialect, source, expected_none_double) in [
        (
            python,
            "# c\n\u{feff}export = 1\n",
            "# c\n\u{feff}export=\"1\"\n",
        ),
        (
            python,
            "\u{feff}\u{feff}export = 1\n",
            "\u{feff}\u{feff}export=\"1\"\n",
        ),
        (
            python_keep,
            "\u{feff}export = 1\n",
            "\u{feff}export=\"1\"\n",
        ),
        (
            python_append,
            "# c\n\u{feff}export = 1\n",
            "# c\n\u{feff}export=\"1\"\n",
        ),
    ] {
        let none_double = FormatOptions::default()
            .with_equals_spacing(EqualsSpacing::None)
            .with_quote_style(QuoteStyle::Double);
        let (doc, _) = parse_with(source, dialect);
        let first = doc.format(&none_double).unwrap();
        assert_eq!(first.as_str(), expected_none_double, "{source:?}");
        let (after, _) = parse_with(first.as_str(), dialect);
        assert_eq!(after.get("\u{feff}export"), Some("1"), "{source:?}");
        assert!(after.format(&none_double).unwrap().is_empty(), "{source:?}");

        for spacing in [
            EqualsSpacing::Preserve,
            EqualsSpacing::Spaces,
            EqualsSpacing::None,
        ] {
            for quote in [QuoteStyle::Double, QuoteStyle::Single, QuoteStyle::Unquoted] {
                let opts = FormatOptions::default()
                    .with_equals_spacing(spacing)
                    .with_quote_style(quote);
                let first = doc.format(&opts).unwrap();
                let (after, _) = parse_with(first.as_str(), dialect);
                assert_eq!(after.get("\u{feff}export"), Some("1"));
                let second = after.format(&opts).unwrap();
                assert!(
                    second.is_empty(),
                    "{source:?} {opts:?}: {:?} -> {:?}",
                    first.as_str(),
                    second.as_str()
                );
            }
        }
    }
}

proptest! {
    #[test]
    fn lf_only_capped_and_python_bom_dialects_stay_idempotent(
        tokens in prop::collection::vec(
            prop_oneof![
                Just("A"), Just("B"), Just("="), Just("1"), Just("#"), Just(" "),
                Just("\t"), Just("\n"), Just("\r"), Just("\""), Just("'"),
                Just("\\"), Just("\u{feff}"), Just("export"),
            ],
            0..48,
        ),
        options in any_format_options(),
        dialect in prop::sample::select(vec![
            Dialect::PORTABLE.with_line_breaks(crate::LineBreaks::Lf),
            Dialect::PORTABLE
                .with_line_breaks(crate::LineBreaks::Lf)
                .with_line_byte_limit(Some(8)),
            Dialect::PORTABLE
                .with_line_breaks(crate::LineBreaks::Lf)
                .with_line_byte_limit(Some(16)),
            Dialect::COMPOSE.with_line_byte_limit(Some(12)),
            Dialect::PYTHON_DOTENV,
            Dialect::PYTHON_DOTENV.with_quoted_tail(crate::QuotedTail::Append),
        ]),
    ) {
        let source: String = tokens.into_iter().collect();
        let (doc, _) = parse_with(&source, dialect);
        if let Ok(formatted) = doc.format(&options) {
            let (after, _) = parse_with(formatted.as_str(), dialect);
            if options.blank_lines == BlankLines::Collapse {
                let adjacent_blanks = after
                    .items()
                    .zip(after.items().skip(1))
                    .any(|(left, right)| {
                        left.kind() == crate::ItemKind::Blank
                            && right.kind() == crate::ItemKind::Blank
                    });
                prop_assert!(!adjacent_blanks);
            }
            prop_assert_eq!(doc.entries().len(), after.entries().len());
            for (left, right) in doc.entries().zip(after.entries()) {
                prop_assert_eq!(left.key(), right.key());
                prop_assert_eq!(left.value(), right.value());
                prop_assert_eq!(left.state(), right.state());
            }
            let again = after.format(&options);
            prop_assert!(
                again.as_ref().is_ok_and(FormatResult::is_empty),
                "{:?} ({:?}) formatted to {:?}, then {:?}",
                source,
                options,
                formatted.as_str(),
                again.map(FormatResult::into_string)
            );
        }
    }
}

#[test]
fn blank_collapse_does_not_revert_under_split_line_limit_desync() {
    // Regression from `fuzz (format)`: under Compose with `line_byte_limit(Some(16))`,
    // spacing/requoting item 0 splits it across lines and swallows items 1..=4 across a
    // collapsed blank run. Verification must blame only item 0 and preserve blank collapse.
    let fuzz_source = "B=\" # note for B\nA='1'  \n  \t\nC=\"3\"D=\"4\" # d\n\n\nEA=\"EY=  BA=\"\" #commA = '9\0liind\"4edvalue ? =\"\" #commA = '9\0liinr B\nA='1'  \n  \t\nC=\"d\"   \x0e";
    let opt = FormatOptions::default()
        .with_equals_spacing(EqualsSpacing::Spaces)
        .with_trim_trailing_whitespace(false)
        .with_blank_lines(BlankLines::Collapse)
        .with_quote_style(QuoteStyle::Single)
        .with_final_newline(FinalNewline::Remove);
    for dialect in [
        Dialect::PORTABLE,
        Dialect::NODE_DOTENV,
        Dialect::PYTHON_DOTENV,
        Dialect::COMPOSE,
        Dialect::DOCKER_RAW,
        Dialect::PORTABLE.with_line_byte_limit(Some(16)),
        Dialect::PORTABLE
            .with_line_breaks(crate::LineBreaks::Lf)
            .with_line_byte_limit(Some(16)),
        Dialect::COMPOSE.with_line_byte_limit(Some(16)),
    ] {
        let (doc, _) = parse_with(fuzz_source, dialect);
        for options in [
            opt,
            FormatOptions::normalize_groups(),
            FormatOptions::normalize_groups().with_quote_style(QuoteStyle::Double),
            FormatOptions::normalize_groups().with_quote_style(QuoteStyle::Single),
        ] {
            let formatted = doc.format(&options).unwrap();
            let (after, _) = parse_with(formatted.as_str(), dialect);
            assert!(
                !after
                    .items()
                    .zip(after.items().skip(1))
                    .any(|(left, right)| {
                        left.kind() == crate::ItemKind::Blank
                            && right.kind() == crate::ItemKind::Blank
                    }),
                "adjacent blank lines remained for {dialect:?} {options:?}: {:?}",
                formatted.as_str()
            );
            assert!(
                after.format(&options).unwrap().is_empty(),
                "not idempotent for {dialect:?} {options:?}: {:?}",
                formatted.as_str()
            );
        }
    }

    // Exact minimized check under Compose `line_byte_limit(Some(16))`:
    // item 0 keeps its original text, the blank run collapses to one `\n`,
    // and later entries format cleanly.
    let (compose_doc, _) = parse_with(fuzz_source, Dialect::COMPOSE.with_line_byte_limit(Some(16)));
    let compose_fmt = compose_doc.format(&opt).unwrap();
    assert_eq!(
        compose_fmt.as_str(),
        "B=\" # note for B\nA='1'  \n  \t\nC=\"3\"D=\"4\" # d\n\nEA=\"EY=  BA=\"\" #commA = '9\0liind\"4edvalue ? =\"\" #commA = '9\0liinr B\nA = '1'  \n  \t\nC = 'd'   \x0e"
    );

    // Portable unterminated quote + blank-line run + double-quote normalization:
    // PORT's requote is skipped, while blank-line collapse is preserved.
    let portable_source = "BROKEN=\"\n\n\nPORT='8080'\n";
    let (portable_doc, _) = parse(portable_source);
    let portable_opt = FormatOptions::normalize_groups().with_quote_style(QuoteStyle::Double);
    let portable_fmt = portable_doc.format(&portable_opt).unwrap();
    assert_eq!(portable_fmt.as_str(), "BROKEN=\"\n\nPORT='8080'\n");
    let (portable_after, _) = parse(portable_fmt.as_str());
    assert!(portable_after.format(&portable_opt).unwrap().is_empty());
}
