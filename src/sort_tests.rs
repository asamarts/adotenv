use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};

use proptest::prelude::*;

use crate::{parse, parse_strict, Document, ExpandOptions, ExpandScope};

type GroupValues = BTreeMap<(usize, String), Vec<(Option<String>, Option<String>)>>;

// Resolved values keyed by blank-line group and key. Sorting moves entries
// only within their group, so this identifies each declaration across a sort.
fn resolved_by_group(document: &Document<'_>, options: &ExpandOptions) -> Option<GroupValues> {
    let resolved = document.resolve(|_| None, options).ok()?;
    let mut values = GroupValues::new();
    for (group, items) in document.groups().enumerate() {
        for entry in items.entries() {
            let resolved = &resolved.entries()[entry.id()];
            values
                .entry((group, entry.key().to_string()))
                .or_default()
                .push((resolved.value.clone(), resolved.effective_value.clone()));
        }
    }
    Some(values)
}

fn resolution_options() -> [ExpandOptions; 4] {
    [
        ExpandOptions::portable(),
        ExpandOptions::portable().with_scope(ExpandScope::WholeFile),
        ExpandOptions::compose(),
        ExpandOptions::compose().with_scope(ExpandScope::WholeFile),
    ]
}

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
fn reference_chains_keep_their_source_order() {
    let doc = parse_strict("D=4\nC=${B}\nB=${A}\nA=1\n").unwrap();
    let sorted = doc.sort().unwrap();
    assert_eq!(sorted.as_str(), "C=${B}\nB=${A}\nA=1\nD=4\n");
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
    let source = "# header\nC=${A} # C note\n# B note\nB=2\n# A note\nA=1 # A note\n";
    let doc = parse_strict(source).unwrap();
    let sorted = doc.sort().unwrap();
    assert_eq!(
        sorted.as_str(),
        "# header\n# B note\nB=2\nC=${A} # C note\n# A note\nA=1 # A note\n"
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

#[test]
fn referencing_pairs_keep_their_relative_order() {
    for source in ["B=${A}\nA=1\n", "A=1\nB=${A}\n", "C=${B}\nB=${A}\nA=1\n"] {
        let doc = parse_strict(source).unwrap();
        let sorted = doc.sort().unwrap();
        assert_eq!(sorted.as_str(), source);
        assert!(sorted.is_empty());
    }
    let doc = parse_strict("D=4\nB=${A}\nC=3\nA=1\n").unwrap();
    assert_eq!(doc.sort().unwrap().as_str(), "B=${A}\nA=1\nC=3\nD=4\n");
}

#[test]
fn sorting_keeps_sequential_and_whole_file_resolution() {
    for source in [
        "B=${A}\nA=1\n",
        "A=old\n\nB=${A}\nA=new\n",
        "A=old\n\nC=$A\nB=${A:-unset}\nA=new\n",
        "Z=${A}\nY=${B}\nB=2\nA=1\n",
    ] {
        let doc = parse_strict(source).unwrap();
        let sorted = doc.sort().unwrap();
        let after = parse_strict(sorted.as_str()).unwrap();
        for options in resolution_options() {
            assert_eq!(
                resolved_by_group(&doc, &options),
                resolved_by_group(&after, &options),
                "{source:?} sorted to {:?}",
                sorted.as_str()
            );
        }
    }
}

#[test]
fn a_last_group_without_a_final_line_ending_sorts() {
    for (source, expected) in [
        ("B=2\nA=1", "A=1\nB=2"),
        ("B=2\r\nA=1", "A=1\r\nB=2"),
        (
            "C=3\nB=2 # note\n# A note\nA=1",
            "# A note\nA=1\nB=2 # note\nC=3",
        ),
        ("X=1\n\nB=2\nA='multi\nline'", "X=1\n\nA='multi\nline'\nB=2"),
        ("B=2\nA=1\n# bottom", "A=1\nB=2\n# bottom"),
        ("A=1\nC=${A}\nB=2", "A=1\nB=2\nC=${A}"),
    ] {
        let doc = parse_strict(source).unwrap();
        let sorted = doc.sort().unwrap();
        assert_eq!(sorted.as_str(), expected, "{source:?}");
        replay(doc.source(), sorted.as_str(), sorted.edits());
        let after = parse_strict(sorted.as_str()).unwrap();
        assert!(!after.has_final_newline());
        for entry in doc.entries() {
            assert_eq!(after.get(entry.key()), doc.get(entry.key()));
        }
    }
}

#[test]
fn a_declaration_that_shares_its_line_with_the_next_statement_keeps_its_group() {
    let source = "B=\"2\" A=1\nC=3\n";
    let (doc, _) = crate::parse_with(source, crate::Dialect::COMPOSE);
    assert_eq!(doc.entries().len(), 3);
    let sorted = doc.sort().unwrap();
    assert_eq!(sorted.as_str(), source);
    assert!(sorted.is_empty());
}

fn simple_line() -> impl Strategy<Value = String> {
    let key = prop::sample::select(["A", "B", "C", "D"].as_slice());
    let value = prop::sample::select(["1", "", "two words", "'q'", "\"d\"", "${A}"].as_slice());
    prop_oneof![
        1 => Just(String::new()),
        1 => Just("# note".to_string()),
        6 => (key, value).prop_map(|(key, value)| format!("{key}={value}")),
    ]
}

proptest! {
    #[test]
    fn sorting_keeps_a_missing_final_newline_missing(
        lines in prop::collection::vec(simple_line(), 1..10),
        crlf in any::<bool>(),
    ) {
        let source = lines.join(if crlf { "\r\n" } else { "\n" });
        let (doc, _) = parse(&source);
        let sorted = doc.sort().unwrap();
        prop_assert_eq!(
            sorted.as_str().ends_with(['\n', '\r']),
            source.ends_with(['\n', '\r'])
        );
        let (after, _) = parse(sorted.as_str());
        prop_assert_eq!(after.entries().len(), doc.entries().len());
        let again = after.sort().unwrap();
        prop_assert!(again.is_empty());
    }
}

fn reference_line() -> impl Strategy<Value = String> {
    let key = || prop::sample::select(["A", "B", "C", "D", "E"].as_slice());
    let value = prop_oneof![
        Just("1".to_string()),
        Just(String::new()),
        key().prop_map(|key| format!("${{{key}}}")),
        key().prop_map(|key| format!("x${key}y")),
        (key(), key()).prop_map(|(left, right)| format!("${{{left}:-${right}}}")),
        (key(), key()).prop_map(|(left, right)| format!("${{{left}:+${{{right}}}}}")),
        key().prop_map(|key| format!("\"${{{key}}}\"")),
        key().prop_map(|key| format!("'${{{key}}}'")),
    ];
    prop_oneof![
        1 => Just(String::new()),
        1 => Just("# note".to_string()),
        8 => (key(), value).prop_map(|(key, value)| format!("{key}={value}")),
    ]
}

proptest! {
    #[test]
    fn sorting_never_changes_resolved_values(
        lines in prop::collection::vec(reference_line(), 0..16),
        final_newline in any::<bool>(),
    ) {
        let mut source = lines.join("\n");
        if final_newline {
            source.push('\n');
        }
        let (doc, _) = parse(&source);
        let sorted = doc.sort().unwrap();
        let (after, _) = parse(sorted.as_str());
        for options in resolution_options() {
            prop_assert_eq!(
                resolved_by_group(&doc, &options),
                resolved_by_group(&after, &options),
                "{:?} sorted to {:?} under {:?}",
                source,
                sorted.as_str(),
                options.scope
            );
        }
    }
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
