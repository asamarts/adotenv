use alloc::{string::String, vec, vec::Vec};
use proptest::prelude::*;

use crate::{
    parse, parse_with, AssignmentState, BareKeys, BomPolicy, Dialect, Duplicates, Multiline,
    PortabilityDeclaration, PortabilityPart, PortabilityReading,
};

fn declaration<'a>(
    reading: &'a PortabilityReading,
    key: &str,
) -> Option<&'a PortabilityDeclaration> {
    reading.parts.iter().find_map(|part| match part {
        PortabilityPart::Declaration(declaration) if declaration.key == key => Some(declaration),
        _ => None,
    })
}

#[test]
fn empty_and_repeated_configurations_have_no_differences() {
    let (doc, _) = parse_with("A=a#b\n", Dialect::PORTABLE);
    assert!(doc.portability(&[]).is_empty());
    assert!(doc
        .portability(&[Dialect::PORTABLE, Dialect::PORTABLE])
        .is_empty());
    assert!(doc.portability(&[Dialect::PORTABLE]).is_empty());
    let (empty, _) = parse_with("", Dialect::PORTABLE);
    assert!(empty.portability(&[Dialect::NODE_DOTENV]).is_empty());
}

#[test]
fn crlf_line_spans_exclude_the_ending() {
    let source = "A=a#b\r\nB=ok\r\n";
    let (doc, _) = parse_with(source, Dialect::PORTABLE);
    let report = doc.portability(&[Dialect::NODE_DOTENV]);
    assert_eq!(report.len(), 1);
    assert_eq!(&source[report.differences()[0].span.clone()], "A=a#b");
}

#[test]
fn value_and_comment_differences_are_reported_per_line() {
    let source = "A=a#b\nB=ok\n";
    let (doc, _) = parse_with(source, Dialect::PORTABLE);
    let report = doc.portability(&[Dialect::NODE_DOTENV, Dialect::DOCKER_RAW]);
    assert_eq!(report.len(), 1);
    let difference = &report.differences()[0];
    assert_eq!(difference.first_line, 1);
    assert_eq!(difference.last_line, 1);
    assert_eq!(&source[difference.span.clone()], "A=a#b");
    assert_eq!(difference.readings.len(), 3);
    assert_eq!(difference.readings[0].dialect, Dialect::PORTABLE);
    assert_eq!(difference.readings[1].dialect, Dialect::NODE_DOTENV);
    assert_eq!(difference.readings[2].dialect, Dialect::DOCKER_RAW);
    assert_eq!(
        declaration(&difference.readings[0], "A")
            .unwrap()
            .value
            .as_deref(),
        Some("a#b")
    );
    assert_eq!(
        declaration(&difference.readings[1], "A")
            .unwrap()
            .value
            .as_deref(),
        Some("a")
    );
    assert_eq!(
        declaration(&difference.readings[2], "A")
            .unwrap()
            .value
            .as_deref(),
        Some("a#b")
    );
}

#[test]
fn statement_grouping_differences_are_reported() {
    let (doc, _) = parse_with("A=\"x\"B=y\n", Dialect::PORTABLE);
    let report = doc.portability(&[Dialect::COMPOSE]);
    assert_eq!(report.len(), 1);
    let difference = &report.differences()[0];
    assert!(difference.readings[0]
        .parts
        .iter()
        .any(|part| matches!(part, PortabilityPart::Invalid)));
    assert_eq!(
        difference.readings[1]
            .parts
            .iter()
            .filter(|part| matches!(part, PortabilityPart::Declaration(_)))
            .count(),
        2
    );
}

#[test]
fn bare_lookup_participation_differs() {
    let (doc, _) = parse_with("A\n", Dialect::PORTABLE);
    let report = doc.portability(&[Dialect::NODE_DOTENV]);
    assert_eq!(report.len(), 1);
    let difference = &report.differences()[0];
    let portable = declaration(&difference.readings[0], "A").unwrap();
    let node = declaration(&difference.readings[1], "A").unwrap();
    assert_eq!(portable.state, AssignmentState::Bare);
    assert!(portable.participates);
    assert_eq!(node.state, AssignmentState::Bare);
    assert!(!node.participates);
}

#[test]
fn multiline_grouping_differences_cover_every_line() {
    let mut single_line = Dialect::PORTABLE;
    single_line.multiline = Multiline::NONE;
    let (doc, _) = parse_with("A=\"one\nB=two\"\nC=x\n", Dialect::PORTABLE);
    let report = doc.portability(&[single_line]);
    assert_eq!(report.len(), 2);
    assert_eq!(report.differences()[0].first_line, 1);
    assert_eq!(report.differences()[1].first_line, 2);
    assert_eq!(report.differences()[0].readings[0].parts[0], {
        let entry = doc.unique("A").unwrap();
        PortabilityPart::Declaration(PortabilityDeclaration {
            key: String::from(entry.key()),
            value: entry.value().map(String::from),
            state: entry.state(),
            exported: entry.exported(),
            participates: entry.participates_in_lookup(),
            effective_value: entry.value().map(String::from),
            wins_lookup: true,
        })
    });
}

#[test]
fn bare_empty_policy_difference_is_reported() {
    let mut empty = Dialect::PORTABLE;
    empty.bare_keys = BareKeys::Empty;
    let (doc, _) = parse_with("A\n", Dialect::PORTABLE);
    let report = doc.portability(&[empty]);
    assert_eq!(report.len(), 1);
    let difference = &report.differences()[0];
    let portable = declaration(&difference.readings[0], "A").unwrap();
    let empty = declaration(&difference.readings[1], "A").unwrap();
    assert_eq!(portable.effective_value, None);
    assert_eq!(empty.effective_value.as_deref(), Some(""));
    assert!(portable.wins_lookup);
    assert!(empty.wins_lookup);
}

#[test]
fn duplicate_lookup_winner_difference_is_reported() {
    let mut first = Dialect::PORTABLE;
    first.duplicates = Duplicates::First;
    let (doc, _) = parse_with("A=1\nA=2\n", Dialect::PORTABLE);
    let report = doc.portability(&[first]);
    assert_eq!(report.len(), 2);
    assert_eq!(report.differences()[0].first_line, 1);
    assert_eq!(report.differences()[1].first_line, 2);
    assert!(
        !declaration(&report.differences()[0].readings[0], "A")
            .unwrap()
            .wins_lookup
    );
    assert!(
        declaration(&report.differences()[0].readings[1], "A")
            .unwrap()
            .wins_lookup
    );
    assert!(
        declaration(&report.differences()[1].readings[0], "A")
            .unwrap()
            .wins_lookup
    );
    assert!(
        !declaration(&report.differences()[1].readings[1], "A")
            .unwrap()
            .wins_lookup
    );
}

#[test]
fn bom_policy_differences_are_visible() {
    let mut keep_bom = Dialect::PORTABLE;
    keep_bom.bom = BomPolicy::Keep;
    let (doc, _) = parse_with("\u{feff}A=x\n", Dialect::PORTABLE);
    let report = doc.portability(&[keep_bom]);
    assert_eq!(report.len(), 1);
    assert!(report.differences()[0].readings[1]
        .parts
        .iter()
        .any(|part| matches!(part, PortabilityPart::Invalid)));
}

#[test]
fn only_differing_lines_are_reported_in_order() {
    let source = "A=x\nB=a#b\nC=y\n";
    let (doc, _) = parse_with(source, Dialect::PORTABLE);
    let report = doc.portability(&[Dialect::NODE_DOTENV]);
    assert_eq!(report.len(), 1);
    let difference = &report.differences()[0];
    assert_eq!(difference.first_line, 2);
    assert_eq!(&source[difference.span.clone()], "B=a#b");
}

proptest! {
    #[test]
    fn portability_spans_are_valid_and_lines_are_ordered(
        chars in prop::collection::vec(
            prop_oneof![
                Just('A'),
                Just('='),
                Just('"'),
                Just('\''),
                Just('`'),
                Just('\\'),
                Just('\n'),
                Just('\r'),
                Just(' '),
                Just('#'),
                Just('$'),
                any::<char>(),
            ],
            0..400,
        )
    ) {
        let source: String = chars.into_iter().collect();
        let (doc, _) = parse(&source);
        let requested = [
            Dialect::NODE_DOTENV,
            Dialect::PYTHON_DOTENV,
            Dialect::COMPOSE,
            Dialect::DOCKER_RAW,
        ];
        let report = doc.portability(&requested);
        let line_count = doc.line_index().line_count();
        let mut previous = 0;
        for difference in report.differences() {
            prop_assert!(difference.first_line >= 1);
            prop_assert_eq!(difference.last_line, difference.first_line);
            prop_assert!(difference.last_line <= line_count);
            prop_assert!(difference.first_line > previous);
            previous = difference.last_line;
            prop_assert!(source.get(difference.span.clone()).is_some());
            prop_assert!(source.is_char_boundary(difference.span.start));
            prop_assert!(source.is_char_boundary(difference.span.end));
            prop_assert_eq!(difference.readings.len(), 5);
            prop_assert_eq!(difference.readings[0].dialect, Dialect::PORTABLE);
        }
        let reversed = doc.portability(&[
            Dialect::DOCKER_RAW,
            Dialect::COMPOSE,
            Dialect::PYTHON_DOTENV,
            Dialect::NODE_DOTENV,
        ]);
        let lines: Vec<_> = report
            .differences()
            .iter()
            .map(|difference| (difference.first_line, difference.span.clone()))
            .collect();
        let reversed_lines: Vec<_> = reversed
            .differences()
            .iter()
            .map(|difference| (difference.first_line, difference.span.clone()))
            .collect();
        prop_assert_eq!(lines, reversed_lines);
    }
}
