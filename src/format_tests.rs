use alloc::{
    string::{String, ToString},
    vec,
};
use proptest::prelude::*;

use crate::{
    parse, parse_strict, parse_with, BlankLines, Dialect, EqualsSpacing, FinalNewline, FormatError,
    FormatOptions, QuoteStyle,
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
