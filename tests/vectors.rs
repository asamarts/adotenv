//! Shared format-contract vectors.

use std::ops::Range;

use adotenv::{
    ColumnEncoding, Document, Edit, LineIndex, Position, PositionError, TextEdit, VerifiedEdits,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

const POSITIONS: &str = include_str!("vectors/positions.json");
const EDIT_ORDER: &str = include_str!("vectors/edit-order.json");
const GITHUB_ESCAPING: &str = include_str!("vectors/github-escaping.json");
const CHECK_REPORT_SCHEMA: &str = include_str!("vectors/schema/check-report.v1.json");
const MANIFEST: &str = include_str!("vectors/manifest.json");

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("vector is valid JSON")
}

fn expect_number(value: &Value, name: &str) -> usize {
    value
        .as_u64()
        .unwrap_or_else(|| panic!("{name} must be an unsigned integer"))
        .try_into()
        .expect("vector coordinate fits usize")
}

fn expected_position(value: &Value) -> Result<Position, PositionError> {
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(match error {
            "inside-crlf" => PositionError::InsideCrLf,
            "invalid-utf8-boundary" => PositionError::InvalidUtf8Boundary,
            "out-of-bounds" => PositionError::OutOfBounds,
            "split-surrogate" => PositionError::SplitSurrogate,
            other => panic!("unknown position error {other}"),
        });
    }
    Ok(Position {
        line: expect_number(&value["line"], "line"),
        column: expect_number(&value["column"], "column"),
    })
}

fn range(value: &Value) -> Range<usize> {
    expect_number(&value["start"], "start")..expect_number(&value["end"], "end")
}

fn text_edits(case: &Value) -> Vec<TextEdit> {
    case["edits"]
        .as_array()
        .expect("edits array")
        .iter()
        .map(|edit| TextEdit {
            range: range(&edit["range"]),
            replacement: edit["replacement"]
                .as_str()
                .expect("replacement string")
                .to_owned(),
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(output, "{byte:02x}").unwrap();
    }
    output
}

#[test]
fn vector_manifest_matches_the_files() {
    let manifest = parse(MANIFEST);
    assert_eq!(manifest["version"].as_u64(), Some(1), "manifest version");
    let files = manifest["files"].as_object().expect("files object");
    for (name, text) in [
        ("positions.json", POSITIONS),
        ("edit-order.json", EDIT_ORDER),
        ("github-escaping.json", GITHUB_ESCAPING),
        ("schema/check-report.v1.json", CHECK_REPORT_SCHEMA),
    ] {
        let expected = files
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("manifest has no hash for {name}"));
        let actual = hex(Sha256::digest(text.as_bytes()).as_slice());
        assert_eq!(actual, expected, "manifest hash for {name}");
    }
    assert_eq!(files.len(), 4, "manifest covers every vector");
}

#[test]
fn position_vectors_match() {
    let vectors = parse(POSITIONS);
    assert_eq!(vectors["version"].as_u64(), Some(1), "positions version");
    for case in vectors["cases"].as_array().expect("cases array") {
        let name = case["name"].as_str().expect("case name");
        let source = case["source"].as_str().expect("source string");
        let offset = expect_number(&case["offset"], "offset");
        let index = LineIndex::new(source);
        for (field, encoding) in [
            ("utf8", ColumnEncoding::Utf8),
            ("scalar", ColumnEncoding::Scalar),
            ("utf16", ColumnEncoding::Utf16),
        ] {
            let expected = expected_position(&case["expected"][field]);
            assert_eq!(
                index.to_position(offset, encoding),
                expected,
                "{name} / {field}"
            );
            if let Ok(position) = expected {
                assert_eq!(
                    index.to_offset(position, encoding),
                    Ok(offset),
                    "{name} / {field} / round trip"
                );
            }
        }
        if let Some(without) = case.get("without_bom") {
            let visible = index.clone().without_bom();
            for (field, encoding) in [
                ("utf8", ColumnEncoding::Utf8),
                ("scalar", ColumnEncoding::Scalar),
                ("utf16", ColumnEncoding::Utf16),
            ] {
                assert_eq!(
                    visible.to_position(offset, encoding),
                    expected_position(&without[field]),
                    "{name} / without_bom / {field}"
                );
            }
        }
    }
}

#[test]
fn edit_order_vectors_match() {
    let vectors = parse(EDIT_ORDER);
    assert_eq!(vectors["version"].as_u64(), Some(1), "edit-order version");
    for case in vectors["cases"].as_array().expect("cases array") {
        let name = case["name"].as_str().expect("case name");
        let source = case["source"].as_str().expect("source string");
        let expected = case["expected"].as_str().expect("expected string");
        let edits = text_edits(case);
        for edit in &edits {
            assert!(edit.range.start <= edit.range.end, "{name}: range order");
            assert!(edit.range.end <= source.len(), "{name}: range bounds");
        }
        for pair in edits.windows(2) {
            assert!(pair[0].range.start <= pair[1].range.start, "{name}: sorted");
            assert!(pair[0].range.end <= pair[1].range.start, "{name}: disjoint");
            if pair[0].range.start == pair[1].range.start {
                assert_eq!(
                    pair[0].range.end, pair[0].range.start,
                    "{name}: insertion first"
                );
            }
        }
        let mut output = source.to_owned();
        for edit in edits.iter().rev() {
            output.replace_range(edit.range.clone(), &edit.replacement);
        }
        assert_eq!(output, expected, "{name}");
    }
}

#[test]
fn document_edit_uses_the_vector_order_for_equal_starts() {
    let document: Document<'_> = adotenv::parse_strict("A=1\n").unwrap();
    let verified: VerifiedEdits = document
        .edit(&[
            Edit::Export {
                entry: 0,
                enabled: true,
            },
            Edit::Rename { entry: 0, key: "B" },
        ])
        .unwrap();
    let expected = [
        TextEdit {
            range: 0..0,
            replacement: "export ".to_owned(),
        },
        TextEdit {
            range: 0..1,
            replacement: "B".to_owned(),
        },
    ];
    assert_eq!(verified.edits(), expected.as_slice());
    assert_eq!(verified.as_str(), "export B=1\n");
}
