use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::{
    parse_values, parse_values_compact, parse_values_compact_with,
    parse_values_compact_with_options, parse_values_with, BareKeys, Dialect, Duplicates, LimitKind,
    ParseOptions,
};

fn pairs(values: &crate::CompactValues) -> Vec<(&str, &str)> {
    values.iter().collect()
}

fn map_pairs(values: &BTreeMap<String, String>) -> Vec<(&str, &str)> {
    values
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect()
}

#[test]
fn compact_values_match_values_only_parsing() {
    let source = "A=1\nA=2\nBARE\nC=3\nEMPTY=\n";
    for dialect in [
        Dialect::PORTABLE,
        Dialect::PORTABLE.with_duplicates(Duplicates::First),
        Dialect::PORTABLE.with_bare_keys(BareKeys::Empty),
        Dialect::NODE_DOTENV,
    ] {
        let map = parse_values_with(source, dialect).unwrap();
        let compact = parse_values_compact_with(source, dialect).unwrap();
        assert_eq!(pairs(&compact), map_pairs(&map));
    }

    let compact = parse_values_compact("PORT=8080\nHOST=localhost\n").unwrap();
    assert_eq!(compact.len(), 2);
    assert!(!compact.is_empty());
    assert_eq!(compact.get("PORT"), Some("8080"));
    assert_eq!(compact.get("HOST"), Some("localhost"));
    assert_eq!(compact.get("MISSING"), None);
    assert!(compact.contains_key("PORT"));
    assert!(!compact.contains_key("MISSING"));
    assert_eq!(compact.keys().collect::<Vec<_>>(), ["HOST", "PORT"]);
    assert_eq!(compact.values().collect::<Vec<_>>(), ["localhost", "8080"]);
    assert_eq!(
        compact.clone().into_btreemap(),
        parse_values("PORT=8080\nHOST=localhost\n").unwrap()
    );
}

#[test]
fn compact_values_keep_first_duplicate_and_bare_policy() {
    let first = Dialect::PORTABLE.with_duplicates(Duplicates::First);
    let values = parse_values_compact_with("A=first\nA=second\nBARE\n", first).unwrap();
    assert_eq!(pairs(&values), [("A", "first")]);

    let empty = Dialect::PORTABLE.with_bare_keys(BareKeys::Empty);
    let values = parse_values_compact_with("DEBUG\nPORT=1\n", empty).unwrap();
    assert_eq!(pairs(&values), [("DEBUG", ""), ("PORT", "1")]);

    let compose = parse_values_compact_with("=x\nA=1\n", Dialect::COMPOSE).unwrap();
    assert_eq!(pairs(&compose), [("A", "1")]);
}

#[test]
fn compact_values_decode_quoted_and_escaped_values() {
    let values = parse_values_compact("A=\"a\\nb\"\nB='c d'\n").unwrap();
    assert_eq!(values.get("A"), Some("a\nb"));
    assert_eq!(values.get("B"), Some("c d"));
}

#[test]
fn compact_values_honor_input_limits() {
    let error = parse_values_compact_with_options(
        "A=1\n",
        Dialect::PORTABLE,
        ParseOptions::default().with_max_input_bytes(3),
    )
    .unwrap_err();
    assert_eq!(error.limit().unwrap().kind(), LimitKind::InputBytes);
}

#[test]
fn compact_values_omit_empty_sources_and_non_values() {
    assert!(parse_values_compact("").unwrap().is_empty());
    assert!(parse_values_compact("\n\n").unwrap().is_empty());
    assert!(parse_values_compact("# comment\n").unwrap().is_empty());
    assert!(parse_values_compact("DEBUG\n").unwrap().is_empty());
}
