use alloc::{collections::BTreeMap, vec::Vec};

use proptest::prelude::*;

use crate::{
    parse_strict, parse_with, AssignmentState, BareKeys, Dialect, DifferenceKind, Duplicates,
};

#[test]
fn comparison_reports_value_state_missing_and_export_changes() {
    let left = parse_strict("A=1\nB=\nC=3\nexport D=4\nBARE\n").unwrap();
    let right = parse_strict("A=2\nB\nD=4\nBARE=value\n").unwrap();
    let comparison = left.compare(&right);

    let found: Vec<_> = comparison
        .differences()
        .iter()
        .map(|entry| (entry.key.as_str(), entry.kind))
        .collect();
    assert_eq!(
        found,
        [
            ("A", DifferenceKind::Value),
            ("B", DifferenceKind::State),
            ("C", DifferenceKind::OnlyLeft),
            ("D", DifferenceKind::Export),
            ("BARE", DifferenceKind::State),
        ]
    );
    let a = &comparison.differences()[0];
    assert_eq!(a.left.as_ref().unwrap().value.as_deref(), Some("1"));
    assert_eq!(a.right.as_ref().unwrap().value.as_deref(), Some("2"));
    let b = &comparison.differences()[1];
    assert_eq!(b.left.as_ref().unwrap().state, AssignmentState::Empty);
    assert_eq!(b.right.as_ref().unwrap().state, AssignmentState::Bare);
    assert!(comparison.differences()[2].right.is_none());
    assert!(comparison.differences()[3].left.as_ref().unwrap().exported);
    assert!(!comparison.differences()[3].right.as_ref().unwrap().exported);
}

#[test]
fn empty_and_absent_are_distinct() {
    let left = parse_strict("A=\n").unwrap();
    let right = parse_strict("").unwrap();
    let comparison = left.compare(&right);
    assert_eq!(comparison.len(), 1);
    let entry = &comparison.differences()[0];
    assert_eq!(entry.key, "A");
    assert_eq!(entry.kind, DifferenceKind::OnlyLeft);
    assert_eq!(entry.left.as_ref().unwrap().state, AssignmentState::Empty);

    let reverse = right.compare(&left);
    assert_eq!(reverse.differences()[0].kind, DifferenceKind::OnlyRight);
    assert_eq!(
        reverse.differences()[0].right.as_ref().unwrap().state,
        AssignmentState::Empty
    );
}

#[test]
fn duplicate_policy_selects_the_effective_declaration() {
    let first = Dialect {
        duplicates: Duplicates::First,
        ..Dialect::PORTABLE
    };
    let last = Dialect {
        duplicates: Duplicates::Last,
        ..Dialect::PORTABLE
    };
    let (left_first, _) = parse_with("A=1\nA=2\n", first);
    let (left_last, _) = parse_with("A=1\nA=2\n", last);
    let right = parse_strict("A=2\n").unwrap();
    assert_eq!(left_first.compare(&right).len(), 1);
    assert!(left_last.compare(&right).is_empty());
}

#[test]
fn nonparticipating_bare_keys_are_not_effective_keys() {
    let dialect = Dialect {
        bare_keys: BareKeys::Ignore,
        ..Dialect::PORTABLE
    };
    let (left, _) = parse_with("A\n", dialect);
    let right = parse_strict("").unwrap();
    assert!(left.compare(&right).is_empty());
}

#[test]
fn identical_documents_compare_empty_and_order_is_stable() {
    let left = parse_strict("Z=1\nA=2\n").unwrap();
    assert!(left.compare(&left).is_empty());

    let right = parse_strict("B=1\n").unwrap();
    let comparison = left.compare(&right);
    let keys: Vec<_> = comparison
        .differences()
        .iter()
        .map(|entry| entry.key.as_str())
        .collect();
    assert_eq!(keys, ["Z", "A", "B"]);
}

proptest! {
    #[test]
    fn comparison_is_symmetric_for_arbitrary_utf8(
        left in "\\PC{0,120}",
        right in "\\PC{0,120}",
    ) {
        let (left, _) = crate::parse(&left);
        let (right, _) = crate::parse(&right);
        let forward = left.compare(&right);
        let reverse = right.compare(&left);
        prop_assert_eq!(forward.len(), reverse.len());
        let by_key: BTreeMap<_, _> = reverse
            .differences()
            .iter()
            .map(|entry| (entry.key.as_str(), entry))
            .collect();
        for a in forward.differences() {
            let b = by_key.get(a.key.as_str()).expect("same key set");
            prop_assert_eq!(&a.left, &b.right);
            prop_assert_eq!(&a.right, &b.left);
            let expected = match b.kind {
                DifferenceKind::OnlyLeft => DifferenceKind::OnlyRight,
                DifferenceKind::OnlyRight => DifferenceKind::OnlyLeft,
                kind => kind,
            };
            prop_assert_eq!(a.kind, expected);
        }
    }
}
