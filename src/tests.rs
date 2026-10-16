use alloc::{format, string::String, vec::Vec};
use proptest::prelude::*;

use crate::{
    encode_value, parse, parse_owned, parse_strict, AssignmentState, DiagnosticCode as Code,
    ItemKind, PositionError, Quote, Severity,
};

#[test]
fn portable_literals_exports_comments_and_distinct_assignment_states() {
    let text = "\u{feff}\t export  KEY \t= \t\"a\\n\\t\\r\\\\\\\"\\$\\q\" \t# note\r\nBARE # declaration\nEMPTY=\nQUOTED=''\nTIGHT=a#b\nHASH=#literal\nCOMMENT= # ignored\nPATH.KEY-1 = /tmp/$USER\n";
    let (doc, diagnostics) = parse(text);
    assert!(doc.has_bom());
    assert_eq!(doc.get("KEY"), Some("a\n\t\r\\\"$\\q"));
    let entry = doc.unique("KEY").unwrap();
    assert!(entry.exported());
    assert_eq!(entry.quote(), Quote::Double);
    assert_eq!(entry.inline_comment(), Some("# note"));
    assert_eq!(&text[entry.tokens().indent.clone()], "\t ");
    assert_eq!(&text[entry.tokens().space_after_export.clone()], "  ");
    assert_eq!(&text[entry.tokens().space_before_eq.clone()], " \t");
    assert_eq!(&text[entry.tokens().space_after_eq.clone()], " \t");
    assert_eq!(&text[entry.tokens().trailing_space.clone()], " \t");
    assert_eq!(doc.unique("BARE").unwrap().state(), AssignmentState::Bare);
    assert_eq!(doc.get("BARE"), None);
    assert_eq!(doc.unique("EMPTY").unwrap().state(), AssignmentState::Empty);
    assert_eq!(doc.get("EMPTY"), Some(""));
    assert_eq!(doc.unique("QUOTED").unwrap().quote(), Quote::Single);
    assert_eq!(doc.get("QUOTED"), Some(""));
    assert_eq!(doc.get("TIGHT"), Some("a#b"));
    assert_eq!(doc.get("HASH"), Some("#literal"));
    assert_eq!(doc.get("COMMENT"), Some(""));
    assert_eq!(doc.get("PATH.KEY-1"), Some("/tmp/$USER"));
    let codes: Vec<_> = diagnostics.iter().map(crate::Diagnostic::code).collect();
    assert_eq!(
        codes,
        [
            Code::ByteOrderMark,
            Code::UnknownEscape,
            Code::BareKey,
            Code::TightHash,
            Code::TightHash,
            Code::NonPosixKey
        ]
    );
    assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
}

#[test]
fn last_occurrence_keeps_bare_empty_and_value_distinct() {
    let (doc, diagnostics) = parse("A=first\nA=\nA\n");
    let entries: Vec<_> = doc.get_all("A").map(|e| (e.value(), e.state())).collect();
    assert_eq!(
        entries,
        [
            (Some("first"), AssignmentState::Value),
            (Some(""), AssignmentState::Empty),
            (None, AssignmentState::Bare)
        ]
    );
    assert_eq!(doc.get("A"), None);
    assert_eq!(doc.get_entry("A").unwrap().line(), 3);
    assert_eq!(doc.unique("A").unwrap_err(), crate::EditError::Duplicate(3));
    let duplicates: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code() == Code::DuplicateKey)
        .map(|d| (d.span(), d.secondary_span()))
        .collect();
    assert_eq!(duplicates, [(8..9, Some(0..1)), (11..12, Some(0..1))]);
}

#[test]
fn matching_multiline_quotes_take_precedence_over_assignment_looking_body() {
    let text = "A=\"first\r\nB=inside\rend\" # tail\nC='literal\\n\nD=also inside'\nE=outside\n";
    let (doc, diagnostics) = parse(text);
    assert_eq!(doc.get("A"), Some("first\r\nB=inside\rend"));
    assert_eq!(doc.get("B"), None);
    assert_eq!(doc.get("C"), Some("literal\\n\nD=also inside"));
    assert_eq!(doc.get("D"), None);
    assert_eq!(doc.unique("E").unwrap().line(), 6);
    assert_eq!(
        doc.items().map(crate::Item::kind).collect::<Vec<_>>(),
        [ItemKind::Entry, ItemKind::Entry, ItemKind::Entry]
    );
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| (d.code(), d.line()))
            .collect::<Vec<_>>(),
        [(Code::LoneCarriageReturn, 2)]
    );
}

#[test]
fn unmatched_quote_recovers_only_the_opening_physical_line() {
    let text = "BAD='lost\r\nGOOD=ok\nOTHER=2\n";
    let (doc, diagnostics) = parse(text);
    assert_eq!(doc.item(0).unwrap().raw(), "BAD='lost\r\n");
    assert_eq!(
        doc.item(0).unwrap().invalid_code(),
        Some(Code::UnterminatedQuote)
    );
    assert_eq!(doc.get("GOOD"), Some("ok"));
    assert_eq!(doc.unique("OTHER").unwrap().line(), 3);
    assert_eq!(diagnostics[0].span(), 4..5);
    assert_eq!(
        parse_strict(text).unwrap_err().code(),
        Code::UnterminatedQuote
    );
    let (doc, _) = parse("A='lost\nB=\"ok\"\nC=3\n");
    assert_eq!(doc.get("B"), Some("ok"));
    assert_eq!(doc.get("C"), Some("3"));
}

#[test]
fn matched_quoted_tail_errors_keep_the_entire_statement_invalid() {
    let text = "BAD=\"first\nSHADOW=inside\nend\"oops\nSAFE=ok\n";
    let (doc, diagnostics) = parse(text);
    assert_eq!(
        doc.item(0).unwrap().raw(),
        "BAD=\"first\nSHADOW=inside\nend\"oops\n"
    );
    assert_eq!(doc.get("SHADOW"), None);
    assert_eq!(doc.get("SAFE"), Some("ok"));
    assert_eq!(diagnostics[0].code(), Code::TrailingText);
    assert_eq!(&text[diagnostics[0].span()], "oops");
    assert_eq!(diagnostics[0].line(), 3);
    assert_eq!(
        parse_strict("A=\"x\"#tight\n").unwrap_err().code(),
        Code::TrailingText
    );
    assert_eq!(
        parse_strict("A=\"x\" # spaced\n").unwrap().get("A"),
        Some("x")
    );
}

#[test]
fn unicode_keys_invalid_export_and_embedded_bom_are_retained_not_guessed() {
    let text = "\u{feff}é=value\nexport \t=secret\nA B=bad\n\u{feff}B=bad\nGOOD=ok\n";
    let (doc, diagnostics) = parse(text);
    assert_eq!(
        doc.entries().map(crate::Entry::key).collect::<Vec<_>>(),
        ["GOOD"]
    );
    assert_eq!(
        doc.items().map(crate::Item::kind).collect::<Vec<_>>(),
        [
            ItemKind::Invalid,
            ItemKind::Invalid,
            ItemKind::Invalid,
            ItemKind::Invalid,
            ItemKind::Entry
        ]
    );
    assert_eq!(parse_strict(text).unwrap_err().span(), 3..5);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|d| d.severity() == Severity::Error)
            .map(crate::Diagnostic::line)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
}

#[test]
fn backticks_and_unknown_escapes_are_literal_without_shell_evaluation() {
    let (doc, diagnostics) = parse("A=`echo $HOME`\nB=\"\\q\\é\"\nC='\\n\\$'\n");
    assert_eq!(doc.get("A"), Some("`echo $HOME`"));
    assert_eq!(doc.get("B"), Some("\\q\\é"));
    assert_eq!(doc.get("C"), Some("\\n\\$"));
    assert_eq!(
        diagnostics
            .iter()
            .map(crate::Diagnostic::code)
            .collect::<Vec<_>>(),
        [
            Code::Backtick,
            Code::Backtick,
            Code::UnknownEscape,
            Code::UnknownEscape
        ]
    );
}

#[test]
fn decoded_spans_map_to_complete_original_escape_bytes() {
    let text = "K=\"α\\n😀\\$\\q\"\n";
    let (doc, _) = parse(text);
    let entry = doc.unique("K").unwrap();
    assert_eq!(entry.value(), Some("α\n😀$\\q"));
    for (cooked, raw) in [(0, 3), (2, 5), (3, 7), (7, 11), (8, 13), (10, 15)] {
        assert_eq!(entry.decoded_offset_to_raw(cooked), Ok(raw));
    }
    assert_eq!(&text[entry.decoded_span_to_raw(2..8).unwrap()], "\\n😀\\$");
    assert_eq!(
        entry.decoded_offset_to_raw(4),
        Err(PositionError::InvalidUtf8Boundary)
    );
    assert_eq!(
        entry.decoded_offset_to_raw(11),
        Err(PositionError::OutOfBounds)
    );
    assert_eq!(
        entry.decoded_span_to_raw(core::ops::Range { start: 8, end: 2 }),
        Err(PositionError::OutOfBounds)
    );
    assert_eq!(
        parse_strict("BARE\n")
            .unwrap()
            .unique("BARE")
            .unwrap()
            .decoded_offset_to_raw(0),
        Err(PositionError::OutOfBounds)
    );
}

#[test]
fn decoded_mapping_keeps_adjacent_escapes_and_unicode_exact_across_many_segments() {
    let raw = "α\\n\\t😀\\\\\\$x\\q";
    let cooked = "α\n\t😀\\$x\\q";
    let text = format!("K=\"{}\"\n", raw.repeat(128));
    let doc = parse_strict(&text).unwrap();
    let entry = doc.entry(0).unwrap();
    assert_eq!(entry.value(), Some(cooked.repeat(128).as_str()));
    for chunk in 0..128 {
        for (decoded, original) in [
            (0, 0),
            (2, 2),
            (3, 4),
            (4, 6),
            (8, 10),
            (9, 12),
            (10, 14),
            (11, 15),
            (12, 16),
            (13, 17),
        ] {
            assert_eq!(
                entry.decoded_offset_to_raw(chunk * cooked.len() + decoded),
                Ok(3 + chunk * raw.len() + original),
            );
        }
    }
}

#[test]
fn ownership_conversion_outlives_and_detaches_from_original_source() {
    let mut original = String::from("A=\"x\\ny\"\n");
    let retained = parse(&original).0.into_owned();
    original.clear();
    assert_eq!(retained.get("A"), Some("x\ny"));
    let (consumed, _) = parse_owned(String::from("A='owned'\nBROKEN=\"open\n"));
    assert_eq!(consumed.get("A"), Some("owned"));
    assert_eq!(
        consumed.item(1).unwrap().invalid_code(),
        Some(Code::UnterminatedQuote)
    );
}

#[test]
fn selected_entries_outlive_a_temporary_lookup_key() {
    let doc = parse_strict("A=first\nA=last\n").unwrap();
    let selected: Vec<_> = {
        let query = String::from("A");
        doc.get_all(&query).collect()
    };
    assert_eq!(
        selected.iter().map(|e| e.value()).collect::<Vec<_>>(),
        [Some("first"), Some("last")]
    );
}

#[test]
fn long_shared_prefixes_and_duplicates_keep_precise_original_locations() {
    let prefix = "A".repeat(4096);
    let text = format!("{prefix}A=1\n{prefix}B=2\n{prefix}A=3\n");
    let (doc, diagnostics) = parse(&text);
    assert_eq!(doc.get(&format!("{prefix}A")), Some("3"));
    let duplicate = diagnostics
        .iter()
        .find(|d| d.code() == Code::DuplicateKey)
        .unwrap();
    assert_eq!(duplicate.secondary_span(), Some(0..4097));
    assert_eq!(duplicate.line(), 3);
}

#[test]
fn diagnostics_remain_source_ordered_across_multiline_escapes_and_lone_crs() {
    let source = "\u{feff}1A=\"\\q\rnext\\é\"\r1A\r";
    let (_, diagnostics) = parse(source);
    assert!(diagnostics
        .windows(2)
        .all(|pair| pair[0].span().start <= pair[1].span().start));
    assert_eq!(
        diagnostics
            .iter()
            .map(crate::Diagnostic::code)
            .collect::<Vec<_>>(),
        [
            Code::ByteOrderMark,
            Code::NonPosixKey,
            Code::UnknownEscape,
            Code::LoneCarriageReturn,
            Code::UnknownEscape,
            Code::LoneCarriageReturn,
            Code::NonPosixKey,
            Code::DuplicateKey,
            Code::BareKey,
            Code::LoneCarriageReturn
        ]
    );
    assert_eq!(
        parse_strict(source)
            .unwrap()
            .get_entry("1A")
            .unwrap()
            .state(),
        AssignmentState::Bare
    );
}

#[test]
fn strict_parse_preserves_warning_heavy_documents_and_first_errors() {
    for ending in ["\n", "\r\n", "\r"] {
        let mut source = format!(
            "\u{feff}# header{ending}export PATH.KEY=x#`{ending}PATH.KEY=\"a\\n\\q\\é${ending}next\" # tail{ending}BARE{ending}EMPTY={ending}"
        );
        let (document, diagnostics) = parse(&source);
        assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
        assert_eq!(parse_strict(&source), Ok(document));

        source.push_str("BAD=\"x\"#tail");
        source.push_str(ending);
        source.push_str("LAST=\"unterminated");
        let (_, diagnostics) = parse(&source);
        let first_error = diagnostics
            .into_iter()
            .find(|d| d.severity() == Severity::Error)
            .unwrap();
        assert_eq!(first_error.code(), Code::TrailingText);
        assert_eq!(parse_strict(&source), Err(first_error));
    }
}

proptest! {
    #[test]
    fn recovering_parse_partitions_utf8_and_reports_addressable_diagnostics(
        chars in prop::collection::vec(prop_oneof![Just('A'), Just('='), Just('"'), Just('\''), Just('\\'), Just('\n'), Just('\r'), Just(' '), Just('#'), any::<char>()], 0..400)
    ) {
        let text: String = chars.into_iter().collect();
        let (doc, diagnostics) = parse(&text);
        let mut at = if doc.has_bom() { 3 } else { 0 };
        for item in doc.items() {
            let range = item.span();
            prop_assert_eq!(range.start, at);
            prop_assert!(range.end > range.start && range.end <= text.len());
            prop_assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
            match item.kind() {
                ItemKind::Entry => {
                    let entry = item.entry().unwrap();
                    prop_assert_eq!(entry.line_span(), range.clone());
                    prop_assert!(crate::parser::valid_key(entry.key()));
                }
                ItemKind::Invalid => prop_assert_eq!(item.diagnostic().unwrap().severity(), Severity::Error),
                _ => {}
            }
            at = range.end;
        }
        prop_assert_eq!(at, text.len());
        for diagnostic in &diagnostics {
            let range = diagnostic.span();
            prop_assert!(range.start <= range.end && range.end <= text.len());
            prop_assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
            if let Some(secondary) = diagnostic.secondary_span() {
                prop_assert!(secondary.start <= secondary.end && secondary.end <= text.len());
                prop_assert!(text.is_char_boundary(secondary.start) && text.is_char_boundary(secondary.end));
            }
        }
        prop_assert!(diagnostics.windows(2).all(|pair| pair[0].span().start <= pair[1].span().start));
        let first_error = diagnostics.into_iter().find(|d| d.severity() == Severity::Error);
        prop_assert_eq!(parse_strict(&text), first_error.map_or(Ok(doc), Err));
    }

    #[test]
    fn portable_encoder_roundtrips_exact_unicode_literals(value in any::<String>()) {
        if let Some(encoded) = encode_value(&value) {
            let text = format!("VALUE={encoded}\n");
            let (doc, diagnostics) = parse(&text);
            prop_assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
            prop_assert_eq!(doc.get("VALUE"), Some(value.as_str()));
        }
    }
}
