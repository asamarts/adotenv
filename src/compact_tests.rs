use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use crate::{
    parse_entries_compact, parse_entries_compact_with, parse_entries_compact_with_options,
    parse_values, parse_values_compact, parse_values_compact_with,
    parse_values_compact_with_options, parse_values_with, parse_with, AssignmentState, BareKeys,
    BomPolicy, CompactEntries, DiagnosticCode, Dialect, Duplicates, LimitKind, ParseOptions,
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

#[cfg(feature = "load")]
#[test]
fn compact_debug_keeps_keys_and_merge_prefers_later_values() {
    let values = parse_values_compact("A=1\nC=3\n").unwrap();
    assert!(format!("{values:?}").contains('A'));

    let later = parse_values_compact("B=2\nC=4\n").unwrap();
    let merged = values.clone().merge(&later);
    assert_eq!(pairs(&merged), [("A", "1"), ("B", "2"), ("C", "4")]);

    let empty = parse_values_compact("").unwrap();
    assert_eq!(
        pairs(&empty.clone().merge(&later)),
        [("B", "2"), ("C", "4")]
    );
    assert_eq!(
        pairs(&values.clone().merge(&empty)),
        [("A", "1"), ("C", "3")]
    );
    assert_eq!(
        pairs(
            &values
                .clone()
                .merge(&parse_values_compact("C=9\n").unwrap())
        ),
        [("A", "1"), ("C", "9")]
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

#[test]
fn compact_entries_preserve_bare_declarations_and_dialect_semantics() {
    let entries = parse_entries_compact("PORT=8080\nPORT=9090\nDEBUG\nEMPTY=\n").unwrap();
    assert_eq!(entries.len(), 3);
    assert!(!entries.is_empty());
    assert_eq!(entries.get("PORT"), Some(Some("9090")));
    assert_eq!(entries.get("EMPTY"), Some(Some("")));
    assert_eq!(entries.get("DEBUG"), Some(None));
    assert_eq!(entries.get("MISSING"), None);
    assert_eq!(entries.get_value("PORT"), Some("9090"));
    assert_eq!(entries.get_value("EMPTY"), Some(""));
    assert_eq!(entries.get_value("DEBUG"), None);
    assert_eq!(entries.get_value("MISSING"), None);
    assert_eq!(entries.get_state("PORT"), Some(AssignmentState::Value));
    assert_eq!(entries.get_state("EMPTY"), Some(AssignmentState::Empty));
    assert_eq!(entries.get_state("DEBUG"), Some(AssignmentState::Bare));
    assert_eq!(entries.get_state("MISSING"), None);
    assert!(entries.contains_key("DEBUG"));
    assert!(!entries.contains_key("MISSING"));
    assert_eq!(
        entries.keys().collect::<Vec<_>>(),
        ["DEBUG", "EMPTY", "PORT"]
    );
    assert_eq!(
        entries.values().collect::<Vec<_>>(),
        [None, Some(""), Some("9090")]
    );
    assert_eq!(
        entries.iter().collect::<Vec<_>>(),
        [("DEBUG", None), ("EMPTY", Some("")), ("PORT", Some("9090"))]
    );
    assert_eq!(entries.clone(), entries);
    assert!(format!("{entries:?}").contains("DEBUG"));
    let map = entries.into_btreemap();
    assert_eq!(map.get("DEBUG"), Some(&None));
    assert_eq!(map.get("EMPTY"), Some(&Some(String::new())));
    assert_eq!(map.get("PORT"), Some(&Some(String::from("9090"))));

    // Duplicate resolution between assigned and bare declarations.
    let last_bare = parse_entries_compact("KEY=1\nKEY\n").unwrap();
    assert_eq!(last_bare.get("KEY"), Some(None));
    let first_assigned = parse_entries_compact_with(
        "KEY=1\nKEY\n",
        Dialect::PORTABLE.with_duplicates(Duplicates::First),
    )
    .unwrap();
    assert_eq!(first_assigned.get("KEY"), Some(Some("1")));
    let first_bare = parse_entries_compact_with(
        "KEY\nKEY=1\n",
        Dialect::PORTABLE.with_duplicates(Duplicates::First),
    )
    .unwrap();
    assert_eq!(first_bare.get("KEY"), Some(None));
    let last_assigned = parse_entries_compact("KEY\nKEY=1\n").unwrap();
    assert_eq!(last_assigned.get("KEY"), Some(Some("1")));

    // BareKeys::Empty -> Some(Some("")), BareKeys::Ignore -> omitted, BareKeys::Error -> Err.
    let empty_bare = parse_entries_compact_with(
        "DEBUG\nPORT=1\n",
        Dialect::PORTABLE.with_bare_keys(BareKeys::Empty),
    )
    .unwrap();
    assert_eq!(empty_bare.get("DEBUG"), Some(Some("")));
    assert_eq!(empty_bare.get_state("DEBUG"), Some(AssignmentState::Empty));

    let ignored_bare =
        parse_entries_compact_with("DEBUG\nKEY=1\nKEY\n", Dialect::NODE_DOTENV).unwrap();
    assert_eq!(ignored_bare.get("DEBUG"), None);
    assert_eq!(ignored_bare.get("KEY"), Some(Some("1")));

    let err_bare =
        parse_entries_compact_with("DEBUG\n", Dialect::PORTABLE.with_bare_keys(BareKeys::Error))
            .unwrap_err();
    assert_eq!(err_bare.code(), DiagnosticCode::ForbiddenBareKey);

    // Empty Compose key omitted; StripAll multi-BOM works; empty source returns default.
    let compose = parse_entries_compact_with("=x\nA=1\nBARE\n", Dialect::COMPOSE).unwrap();
    assert_eq!(compose.iter().collect::<Vec<_>>(), [("A", Some("1"))]);
    let multi_bom = parse_entries_compact_with(
        "\u{feff}\u{feff}A=1\nBARE\n",
        Dialect::PORTABLE.with_bom(BomPolicy::StripAll),
    )
    .unwrap();
    assert_eq!(
        multi_bom.iter().collect::<Vec<_>>(),
        [("A", Some("1")), ("BARE", None)]
    );
    assert_eq!(
        parse_entries_compact("").unwrap(),
        CompactEntries::default()
    );
}

#[test]
fn compact_entries_match_document_and_enforce_limits_and_fast_path() {
    let source = "A=1\nA=2\nBARE\nC=\nD=3\nD\nE\nE=4\n";
    for dialect in [
        Dialect::PORTABLE,
        Dialect::PORTABLE.with_duplicates(Duplicates::First),
        Dialect::PORTABLE.with_bare_keys(BareKeys::Empty),
        Dialect::PORTABLE.with_bare_keys(BareKeys::Ignore),
        Dialect::NODE_DOTENV,
        Dialect::PYTHON_DOTENV,
        Dialect::COMPOSE,
        Dialect::DOCKER_RAW,
    ] {
        let (doc, _) = parse_with(source, dialect);
        let compact = parse_entries_compact_with(source, dialect).unwrap();
        let mut expected = BTreeMap::new();
        for entry in doc.entries() {
            if !entry.participates_in_lookup() || entry.key().is_empty() {
                continue;
            }
            let effective = entry.value().or_else(|| {
                (entry.state() == AssignmentState::Bare && dialect.bare_keys == BareKeys::Empty)
                    .then_some("")
            });
            match dialect.duplicates {
                Duplicates::First => {
                    expected.entry(entry.key()).or_insert(effective);
                }
                Duplicates::Last => {
                    expected.insert(entry.key(), effective);
                }
            }
        }
        let actual: BTreeMap<&str, Option<&str>> = compact.iter().collect();
        assert_eq!(actual, expected, "{dialect:?}");
    }

    // >512-byte sorted Portable fast path and unsorted/duplicate/bare fallback.
    let mut big = String::new();
    for index in 0..80 {
        writeln!(big, "KEY_{index:03}=VAL_{index:03}").unwrap();
    }
    assert!(big.len() > 512);
    let fast = parse_entries_compact(&big).unwrap();
    assert_eq!(fast.len(), 80);
    assert_eq!(fast.get("KEY_079"), Some(Some("VAL_079")));

    let mut unsorted_big = String::new();
    for index in (0..80).rev() {
        writeln!(unsorted_big, "KEY_{index:03}=VAL_{index:03}").unwrap();
    }
    assert!(unsorted_big.len() > 512);
    assert_eq!(parse_entries_compact(&unsorted_big).unwrap(), fast);
    assert_eq!(
        parse_values_compact(&unsorted_big).unwrap().len(),
        parse_values_compact(&big).unwrap().len()
    );

    // Duplicate bare keys, equality across ordering, multiline escapes, and BomPolicy::Error.
    let dup_bare = parse_entries_compact("A\nA\n").unwrap();
    assert_eq!(dup_bare.len(), 1);
    assert_eq!(dup_bare.get("A"), Some(None));
    assert_eq!(
        parse_entries_compact("A=1\nB=2\n").unwrap(),
        parse_entries_compact("B=2\nA=1\n").unwrap()
    );
    assert_ne!(
        parse_entries_compact("A\n").unwrap(),
        parse_entries_compact("A=\n").unwrap()
    );
    let escaped = parse_entries_compact("A=\"a\\nb\"\nB='c\nd'\n").unwrap();
    assert_eq!(escaped.get("A"), Some(Some("a\nb")));
    assert_eq!(escaped.get("B"), Some(Some("c\nd")));
    assert_eq!(
        parse_entries_compact_with(
            "\u{feff}A=1\n",
            Dialect::PORTABLE.with_bom(BomPolicy::Error)
        )
        .unwrap_err()
        .code(),
        DiagnosticCode::ForbiddenByteOrderMark
    );

    // ParseOptions limits (bytes, lines, items).
    let byte_err = parse_entries_compact_with_options(
        "A=1\n",
        Dialect::PORTABLE,
        ParseOptions::new().with_max_input_bytes(2),
    )
    .unwrap_err();
    assert_eq!(byte_err.limit().unwrap().kind(), LimitKind::InputBytes);

    let line_err = parse_entries_compact_with_options(
        &big,
        Dialect::PORTABLE,
        ParseOptions::new().with_max_lines(5),
    )
    .unwrap_err();
    assert_eq!(line_err.limit().unwrap().kind(), LimitKind::Lines);

    let item_err = parse_entries_compact_with_options(
        "A=1\nB=2\n",
        Dialect::PORTABLE,
        ParseOptions::new().with_max_items(1),
    )
    .unwrap_err();
    assert_eq!(item_err.limit().unwrap().kind(), LimitKind::Items);
}

#[test]
fn compact_fast_path_guards_and_empty_entries() {
    assert!(parse_entries_compact("").unwrap().is_empty());
    assert!(parse_entries_compact("# comment\n").unwrap().is_empty());

    // Simple KEY=value inputs with non-Portable duplicate policy must bypass Portable fast path.
    let first_values = parse_values_compact_with(
        "A=1\nA=2\n",
        Dialect::PORTABLE.with_duplicates(Duplicates::First),
    )
    .unwrap();
    assert_eq!(first_values.get("A"), Some("1"));

    let first_entries = parse_entries_compact_with(
        "A=1\nA=2\n",
        Dialect::PORTABLE.with_duplicates(Duplicates::First),
    )
    .unwrap();
    assert_eq!(first_entries.get("A"), Some(Some("1")));

    // Simple KEY=value Portable inputs must bypass fast path when max_items is configured.
    let item_err = parse_values_compact_with_options(
        "A=1\nB=2\n",
        Dialect::PORTABLE,
        ParseOptions::new().with_max_items(1),
    )
    .unwrap_err();
    assert_eq!(item_err.limit().unwrap().kind(), LimitKind::Items);

    // CompactValues supports PartialEq / Eq.
    let left = parse_values_compact("B=2\nA=1\n").unwrap();
    let right = parse_values_compact("A=1\nB=2\n").unwrap();
    let other = parse_values_compact("A=1\nB=3\n").unwrap();
    assert_eq!(left, right);
    assert_ne!(left, other);

    // BOM-prefixed limited documents keep diagnostic span inside item span,
    // and max_items(0) catches BOM-only errors.
    let (limited, diags) = crate::parse_with_options(
        "\u{feff}A=1\n",
        Dialect::PORTABLE,
        ParseOptions::new().with_max_input_bytes(2),
    );
    let item = limited.items().next().unwrap();
    assert!(diags[0].span().start >= item.span().start);
    let (_, bom_diags) = crate::parse_with_options(
        "\u{feff}",
        Dialect::PORTABLE.with_bom(crate::BomPolicy::Error),
        ParseOptions::new().with_max_items(0),
    );
    assert!(bom_diags
        .iter()
        .any(|d| d.limit().is_some_and(|l| l.kind() == LimitKind::Items)));
}
