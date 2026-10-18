use alloc::{collections::BTreeMap, format, string::String, vec::Vec};

use proptest::prelude::*;

use crate::{parse, parse_strict};

#[test]
fn sorts_keys_without_comments() {
    let doc = parse_strict("B=2\nA=1\nC=3\n").unwrap();
    let sorted = doc.sort().unwrap();
    assert_eq!(sorted.as_str(), "A=1\nB=2\nC=3\n");
    assert_eq!(
        sorted
            .as_str()
            .split('\n')
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>(),
        ["A=1", "B=2", "C=3"]
    );
    let replay = replay(doc.source(), sorted.as_str(), sorted.edits());
    assert_eq!(replay, sorted.as_str());
}

#[test]
fn comments_before_first_entry_stay_as_group_header() {
    let source = "# header\nB=2\n# A note\nA=1\n# trailing\n";
    let doc = parse_strict(source).unwrap();
    let sorted = doc.sort().unwrap();
    assert_eq!(
        sorted.as_str(),
        "# header\n# A note\nA=1\nB=2\n# trailing\n"
    );
}

#[test]
fn inline_and_leading_comments_move_with_entries() {
    let source = "B=2 # inline B\n# A note\nA=1 # inline A\n";
    let doc = parse_strict(source).unwrap();
    let sorted = doc.sort().unwrap();
    assert_eq!(
        sorted.as_str(),
        "# A note\nA=1 # inline A\nB=2 # inline B\n"
    );
    let (after, _) = parse(sorted.as_str());
    assert_eq!(after.get("A"), Some("1"));
    assert_eq!(after.get("B"), Some("2"));
    assert_eq!(
        after.unique("A").unwrap().inline_comment(),
        Some("# inline A")
    );
}

#[test]
fn dependencies_order_before_dependents() {
    let doc = parse_strict("C=${B}\nB=${A}\nA=1\n").unwrap();
    let sorted = doc.sort().unwrap();
    assert_eq!(sorted.as_str(), "A=1\nB=${A}\nC=${B}\n");
}

#[test]
fn independent_ready_keys_are_chosen_in_key_order() {
    let doc = parse_strict("C=3\nB=${C}\nA=1\n").unwrap();
    let sorted = doc.sort().unwrap();
    assert_eq!(sorted.as_str(), "A=1\nC=3\nB=${C}\n");
}

#[test]
fn dependency_cycles_and_duplicate_keys_are_left_unchanged() {
    let cycle = parse_strict("B=${A}\nA=${B}\n").unwrap();
    let sorted = cycle.sort().unwrap();
    assert_eq!(sorted.as_str(), cycle.source());
    assert!(sorted.is_empty());

    let duplicate = parse_strict("B=2\nA=1\nA=3\n").unwrap();
    let sorted = duplicate.sort().unwrap();
    assert_eq!(sorted.as_str(), duplicate.source());
}

#[test]
fn invalid_groups_are_left_unchanged() {
    let source = "B=2\nA=1\nBAD KEY=value\n";
    let (doc, _) = parse(source);
    let sorted = doc.sort().unwrap();
    assert_eq!(sorted.as_str(), source);
}

#[test]
fn groups_sort_independently_and_preserve_separators() {
    let doc = parse_strict("B=2\nA=1\n\nD=4\nC=3\n").unwrap();
    let sorted = doc.sort().unwrap();
    assert_eq!(sorted.as_str(), "A=1\nB=2\n\nC=3\nD=4\n");
}

#[test]
fn unsortable_groups_do_not_block_later_groups() {
    let source = "B=2\nA=1\n\nX=${Y}\nY=${X}\n\nD=4\nC=3\n";
    let doc = parse_strict(source).unwrap();
    let sorted = doc.sort().unwrap();
    assert_eq!(sorted.as_str(), "A=1\nB=2\n\nX=${Y}\nY=${X}\n\nC=3\nD=4\n");
}

#[test]
fn dependencies_keep_comments_attached() {
    let source = "# header\nB=${A} # B note\n# A note\nA=1 # A note\n";
    let doc = parse_strict(source).unwrap();
    let sorted = doc.sort().unwrap();
    assert_eq!(
        sorted.as_str(),
        "# header\n# A note\nA=1 # A note\nB=${A} # B note\n"
    );
}

#[test]
fn comments_after_the_last_entry_stay_at_the_bottom() {
    let source = "B=2\nA=1\n# bottom\n";
    let doc = parse_strict(source).unwrap();
    let sorted = doc.sort().unwrap();
    assert_eq!(sorted.as_str(), "A=1\nB=2\n# bottom\n");
}

#[test]
fn no_dependency_on_a_key_outside_the_group() {
    let source = "B=${A}\n\nA=1\n";
    let doc = parse_strict(source).unwrap();
    let sorted = doc.sort().unwrap();
    assert_eq!(sorted.as_str(), source);
}

proptest! {
    #[test]
    fn sorting_preserves_group_entries_and_is_idempotent(
        source in "\\PC{0,240}",
    ) {
        let (doc, _) = parse(&source);
        let Ok(sorted) = doc.sort() else {
            return Ok(());
        };
        let (after, _) = parse(sorted.as_str());
        prop_assert_eq!(doc.groups().count(), after.groups().count());
        for (before, after) in doc.groups().zip(after.groups()) {
            let mut left: BTreeMap<&str, Vec<String>> = BTreeMap::new();
            for entry in before.entries() {
                left.entry(entry.key()).or_default().push(format!(
                    "{:?}|{:?}|{:?}|{:?}|{:?}",
                    entry.value(),
                    entry.state(),
                    entry.quote(),
                    entry.exported(),
                    entry.participates_in_lookup()
                ));
            }
            let mut right: BTreeMap<&str, Vec<String>> = BTreeMap::new();
            for entry in after.entries() {
                right.entry(entry.key()).or_default().push(format!(
                    "{:?}|{:?}|{:?}|{:?}|{:?}",
                    entry.value(),
                    entry.state(),
                    entry.quote(),
                    entry.exported(),
                    entry.participates_in_lookup()
                ));
            }
            prop_assert_eq!(left, right);
        }
        let again = parse(sorted.as_str()).0.sort().unwrap();
        prop_assert!(again.is_empty());
        prop_assert_eq!(again.as_str(), sorted.as_str());
    }
}

fn replay(source: &str, expected: &str, edits: &[crate::TextEdit]) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    for edit in edits {
        output.push_str(&source[cursor..edit.range.start]);
        output.push_str(&edit.replacement);
        cursor = edit.range.end;
    }
    output.push_str(&source[cursor..]);
    assert_eq!(output, expected);
    output
}
