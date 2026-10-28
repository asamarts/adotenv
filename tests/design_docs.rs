//! Compile-checked examples mirrored from the design guide.

use adotenv::{
    parse, parse_strict, BareKeys, Dialect, DifferenceKind, FormatOptions, InlineComments,
};

#[test]
fn comparison_guide_example() {
    let left = parse_strict("A=1\nB=\nC=3\n").unwrap();
    let right = parse_strict("A=2\nB\nD=4\n").unwrap();
    let comparison = left.compare(&right);
    let changes: Vec<_> = comparison
        .differences()
        .iter()
        .map(|entry| (entry.key.as_str(), entry.kind))
        .collect();

    assert_eq!(
        changes,
        [
            ("A", DifferenceKind::Value),
            ("B", DifferenceKind::State),
            ("C", DifferenceKind::OnlyLeft),
            ("D", DifferenceKind::OnlyRight),
        ]
    );
}

#[test]
fn formatting_guide_example() {
    let source = "# app   \n\nA=1   \nB =  2 # note   \n";
    let doc = parse_strict(source).unwrap();
    let formatted = doc.format(&FormatOptions::normalize()).unwrap();
    assert_eq!(formatted.as_str(), "# app\n\nA = 1\nB = 2 # note\n");
}

#[test]
fn sorting_guide_example() {
    let source = "# header\nB=${A}\n# A note\nA=1\n# bottom\n";
    let doc = parse_strict(source).unwrap();
    assert_eq!(
        doc.sort().unwrap().as_str(),
        "# header\n# A note\nA=1\nB=${A}\n# bottom\n"
    );
}

#[test]
fn custom_dialect_guide_example() {
    let dialect = Dialect::NODE_DOTENV
        .with_inline_comments(InlineComments::None)
        .with_bare_keys(BareKeys::Ignore);
    assert_eq!(dialect.inline_comments, InlineComments::None);
    assert_eq!(dialect.bare_keys, BareKeys::Ignore);
    assert_eq!(
        parse("A=one#two\n")
            .0
            .effective_entries()
            .next()
            .unwrap()
            .value(),
        "one#two"
    );
}
