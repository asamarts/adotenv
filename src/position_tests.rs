use alloc::string::{String, ToString};

use crate::{ColumnEncoding, LineIndex, Position, PositionError};

const ENCODINGS: [ColumnEncoding; 3] = [
    ColumnEncoding::Utf8,
    ColumnEncoding::Scalar,
    ColumnEncoding::Utf16,
];

fn position(line: usize, column: usize) -> Position {
    Position { line, column }
}

#[test]
fn empty_source_has_one_representable_position() {
    let index = LineIndex::new("");
    assert_eq!(index.line_count(), 1);
    for encoding in ENCODINGS {
        assert_eq!(index.to_position(0, encoding), Ok(position(0, 0)));
        assert_eq!(index.to_offset(position(0, 0), encoding), Ok(0));
        assert_eq!(
            index.to_position(1, encoding),
            Err(PositionError::OutOfBounds)
        );
        assert_eq!(
            index.to_offset(position(0, 1), encoding),
            Err(PositionError::OutOfBounds)
        );
        assert_eq!(
            index.to_offset(position(1, 0), encoding),
            Err(PositionError::OutOfBounds)
        );
    }
}

#[test]
fn mixed_line_endings_preserve_exact_boundaries() {
    let source = String::from("ab\r\nc\rd\nef");
    let index = LineIndex::new(&source);
    assert_eq!(index.line_count(), 4);
    let boundaries = [
        (0, position(0, 0)),
        (1, position(0, 1)),
        (2, position(0, 2)), // CRLF starts at content end.
        (4, position(1, 0)),
        (5, position(1, 1)), // Lone CR.
        (6, position(2, 0)),
        (7, position(2, 1)), // LF.
        (8, position(3, 0)),
        (9, position(3, 1)),
        (10, position(3, 2)), // EOF without an ending.
    ];
    for encoding in ENCODINGS {
        for (offset, expected) in boundaries {
            assert_eq!(index.to_position(offset, encoding), Ok(expected));
            assert_eq!(index.to_offset(expected, encoding), Ok(offset));
        }
        assert_eq!(
            index.to_position(3, encoding),
            Err(PositionError::InsideCrLf)
        );
        assert_eq!(
            index.to_offset(position(0, 3), encoding),
            Err(PositionError::OutOfBounds)
        );
        assert_eq!(
            index.to_offset(position(1, 2), encoding),
            Err(PositionError::OutOfBounds)
        );
    }
}

#[test]
fn each_terminal_ending_creates_an_empty_line() {
    for source in ["x\n", "x\r", "x\r\n"] {
        let index = LineIndex::new(source);
        assert_eq!(index.line_count(), 2);
        for encoding in ENCODINGS {
            assert_eq!(index.to_position(1, encoding), Ok(position(0, 1)));
            assert_eq!(index.to_offset(position(0, 1), encoding), Ok(1));
            assert_eq!(
                index.to_position(source.len(), encoding),
                Ok(position(1, 0))
            );
            assert_eq!(index.to_offset(position(1, 0), encoding), Ok(source.len()));
            assert_eq!(
                index.to_offset(position(1, 1), encoding),
                Err(PositionError::OutOfBounds)
            );
        }
    }
}

#[test]
fn adjacent_endings_include_empty_physical_lines() {
    let index = LineIndex::new("\r\n\r\n\n\r");
    assert_eq!(index.line_count(), 5);
    for encoding in ENCODINGS {
        for (line, offset) in [0, 2, 4, 5, 6].into_iter().enumerate() {
            assert_eq!(index.to_position(offset, encoding), Ok(position(line, 0)));
            assert_eq!(index.to_offset(position(line, 0), encoding), Ok(offset));
            assert_eq!(
                index.to_offset(position(line, 1), encoding),
                Err(PositionError::OutOfBounds)
            );
        }
        for offset in [1, 3] {
            assert_eq!(
                index.to_position(offset, encoding),
                Err(PositionError::InsideCrLf)
            );
        }
    }
}

#[test]
fn utf8_columns_preserve_interior_byte_offsets() {
    let source = "é😀\r\n界";
    let index = LineIndex::new(source);
    for offset in 0..=source.len() {
        if offset == 7 {
            assert_eq!(
                index.to_position(offset, ColumnEncoding::Utf8),
                Err(PositionError::InsideCrLf)
            );
        } else {
            let expected = if offset <= 6 {
                position(0, offset)
            } else {
                position(1, offset - 8)
            };
            assert_eq!(
                index.to_position(offset, ColumnEncoding::Utf8),
                Ok(expected)
            );
            assert_eq!(index.to_offset(expected, ColumnEncoding::Utf8), Ok(offset));
        }
    }
}

#[test]
fn unicode_columns_are_exact_and_not_display_cell_widths() {
    // A tab, a two-byte scalar, a supplementary scalar, a combining mark,
    // and a wide scalar each contribute one scalar column.
    let index = LineIndex::new("\té😀\u{301}界");
    let boundaries = [
        (0, 0, 0),
        (1, 1, 1),
        (3, 2, 2),
        (7, 3, 4),
        (9, 4, 5),
        (12, 5, 6),
    ];
    for (offset, scalar, utf16) in boundaries {
        assert_eq!(
            index.to_position(offset, ColumnEncoding::Scalar),
            Ok(position(0, scalar))
        );
        assert_eq!(
            index.to_offset(position(0, scalar), ColumnEncoding::Scalar),
            Ok(offset)
        );
        assert_eq!(
            index.to_position(offset, ColumnEncoding::Utf16),
            Ok(position(0, utf16))
        );
        assert_eq!(
            index.to_offset(position(0, utf16), ColumnEncoding::Utf16),
            Ok(offset)
        );
    }
    for offset in [2, 4, 5, 6, 8, 10, 11] {
        for encoding in [ColumnEncoding::Scalar, ColumnEncoding::Utf16] {
            assert_eq!(
                index.to_position(offset, encoding),
                Err(PositionError::InvalidUtf8Boundary)
            );
        }
    }
    assert_eq!(
        index.to_offset(position(0, 6), ColumnEncoding::Scalar),
        Err(PositionError::OutOfBounds)
    );
    assert_eq!(
        index.to_offset(position(0, 7), ColumnEncoding::Utf16),
        Err(PositionError::OutOfBounds)
    );
}

#[test]
fn utf16_rejects_each_split_surrogate_without_clamping() {
    let index = LineIndex::new("😀𐐀z\n😀");
    for pos in [position(0, 1), position(0, 3), position(1, 1)] {
        assert_eq!(
            index.to_offset(pos, ColumnEncoding::Utf16),
            Err(PositionError::SplitSurrogate)
        );
    }
    for (pos, offset) in [
        (position(0, 0), 0),
        (position(0, 2), 4),
        (position(0, 4), 8),
        (position(0, 5), 9),
        (position(1, 0), 10),
        (position(1, 2), 14),
    ] {
        assert_eq!(index.to_offset(pos, ColumnEncoding::Utf16), Ok(offset));
        assert_eq!(index.to_position(offset, ColumnEncoding::Utf16), Ok(pos));
    }
    assert_eq!(
        index.to_offset(position(1, 3), ColumnEncoding::Utf16),
        Err(PositionError::OutOfBounds)
    );
}

#[test]
fn non_ascii_line_starts_and_bom_use_source_columns() {
    let index = LineIndex::new("\u{feff}é\r\n😀\r界\n");
    assert_eq!(index.line_count(), 4);
    for encoding in ENCODINGS {
        for (offset, line) in [(0, 0), (7, 1), (12, 2), (16, 3)] {
            assert_eq!(index.to_position(offset, encoding), Ok(position(line, 0)));
            assert_eq!(index.to_offset(position(line, 0), encoding), Ok(offset));
        }
        assert_eq!(
            index.to_position(6, encoding),
            Err(PositionError::InsideCrLf)
        );
    }
    assert_eq!(
        index.to_position(3, ColumnEncoding::Utf8),
        Ok(position(0, 3))
    );
    assert_eq!(
        index.to_position(3, ColumnEncoding::Scalar),
        Ok(position(0, 1))
    );
    assert_eq!(
        index.to_position(3, ColumnEncoding::Utf16),
        Ok(position(0, 1))
    );
}

#[test]
fn without_bom_omits_the_mark_from_line_zero_columns() {
    let source = "\u{feff}A=1\nB=2\n";
    let default = LineIndex::new(source);
    let visible = default.clone().without_bom();

    assert_eq!(
        default.to_position(3, ColumnEncoding::Utf8),
        Ok(position(0, 3))
    );
    assert_eq!(
        default.to_position(3, ColumnEncoding::Scalar),
        Ok(position(0, 1))
    );
    assert_eq!(
        default.to_position(3, ColumnEncoding::Utf16),
        Ok(position(0, 1))
    );

    for encoding in ENCODINGS {
        for offset in 0..3 {
            assert_eq!(visible.to_position(offset, encoding), Ok(position(0, 0)));
        }
        assert_eq!(visible.to_position(3, encoding), Ok(position(0, 0)));
        assert_eq!(visible.to_position(6, encoding), Ok(position(0, 3)));
        assert_eq!(visible.to_offset(position(0, 0), encoding), Ok(3));
        assert_eq!(visible.to_offset(position(0, 3), encoding), Ok(6));
        assert_eq!(visible.to_position(7, encoding), Ok(position(1, 0)));
        assert_eq!(visible.to_offset(position(1, 0), encoding), Ok(7));
    }
}

#[test]
fn without_bom_is_a_no_op_without_a_mark_and_keeps_boundaries() {
    let plain = LineIndex::new("A=1\n").without_bom();
    for encoding in ENCODINGS {
        assert_eq!(plain.to_position(0, encoding), Ok(position(0, 0)));
        assert_eq!(plain.to_position(3, encoding), Ok(position(0, 3)));
        assert_eq!(plain.to_offset(position(0, 0), encoding), Ok(0));
        assert_eq!(plain.to_offset(position(0, 3), encoding), Ok(3));
    }

    let only = LineIndex::new("\u{feff}").without_bom();
    for encoding in ENCODINGS {
        assert_eq!(only.to_position(0, encoding), Ok(position(0, 0)));
        assert_eq!(only.to_position(3, encoding), Ok(position(0, 0)));
        assert_eq!(only.to_offset(position(0, 0), encoding), Ok(3));
        assert_eq!(
            only.to_offset(position(0, 1), encoding),
            Err(PositionError::OutOfBounds)
        );
    }

    let non_ascii = LineIndex::new("\u{feff}é\n").without_bom();
    assert_eq!(
        non_ascii.to_position(4, ColumnEncoding::Scalar),
        Err(PositionError::InvalidUtf8Boundary)
    );
    assert_eq!(
        non_ascii.to_position(5, ColumnEncoding::Scalar),
        Ok(position(0, 1))
    );
}

#[test]
fn conversions_reject_out_of_range_coordinates_without_overflow() {
    let index = LineIndex::new("é\n😀");
    for encoding in ENCODINGS {
        for offset in [8, usize::MAX] {
            assert_eq!(
                index.to_position(offset, encoding),
                Err(PositionError::OutOfBounds)
            );
        }
        for pos in [
            position(2, 0),
            position(usize::MAX, 0),
            position(0, usize::MAX),
            position(1, usize::MAX),
        ] {
            assert_eq!(
                index.to_offset(pos, encoding),
                Err(PositionError::OutOfBounds)
            );
        }
    }
}

#[test]
fn display_coordinates_require_explicit_checked_conversion() {
    assert_eq!(position(0, 0).one_based(), Ok(position(1, 1)));
    assert_eq!(position(3, 8).one_based(), Ok(position(4, 9)));
    assert_eq!(
        position(usize::MAX - 1, usize::MAX - 1).one_based(),
        Ok(position(usize::MAX, usize::MAX))
    );
    for pos in [
        position(usize::MAX, 0),
        position(0, usize::MAX),
        position(usize::MAX, usize::MAX),
    ] {
        assert_eq!(pos.one_based(), Err(PositionError::OutOfBounds));
    }
    let index = LineIndex::new("a\nb");
    let raw = index.to_position(2, ColumnEncoding::Scalar).unwrap();
    assert_eq!(raw, position(1, 0));
    assert_eq!(raw.one_based(), Ok(position(2, 1)));
    assert_eq!(index.to_offset(raw, ColumnEncoding::Scalar), Ok(2));
}

#[test]
fn position_errors_have_stable_display_text() {
    let cases = [
        (
            PositionError::OutOfBounds,
            "source position is out of bounds",
        ),
        (
            PositionError::InvalidUtf8Boundary,
            "source offset is not a UTF-8 character boundary",
        ),
        (
            PositionError::SplitSurrogate,
            "UTF-16 column splits a surrogate pair",
        ),
        (
            PositionError::InsideCrLf,
            "source offset lies inside a CRLF line ending",
        ),
    ];
    for (error, message) in cases {
        assert_eq!(error.to_string(), message);
    }
}

#[test]
fn with_bom_policy_matches_all_bom_policies_and_encodings() {
    use crate::BomPolicy;

    let source = "\u{feff}\u{feff}A=1\nB=2\n";
    let base = LineIndex::new(source);

    // Keep and Error both keep every leading BOM in line 0 columns.
    for policy in [BomPolicy::Keep, BomPolicy::Error] {
        let idx = base.clone().with_bom_policy(policy);
        assert_eq!(idx.to_position(6, ColumnEncoding::Utf8), Ok(position(0, 6)));
        assert_eq!(
            idx.to_position(6, ColumnEncoding::Scalar),
            Ok(position(0, 2))
        );
        assert_eq!(
            idx.to_position(6, ColumnEncoding::Utf16),
            Ok(position(0, 2))
        );
        for encoding in ENCODINGS {
            assert_eq!(idx.to_offset(position(0, 0), encoding), Ok(0));
            assert_eq!(idx.to_position(10, encoding), Ok(position(1, 0)));
        }
    }

    // Strip hides only the first BOM (3 bytes); the second BOM remains visible.
    let strip = base.clone().with_bom_policy(BomPolicy::Strip);
    assert_eq!(
        strip.to_position(6, ColumnEncoding::Utf8),
        Ok(position(0, 3))
    );
    assert_eq!(
        strip.to_position(6, ColumnEncoding::Scalar),
        Ok(position(0, 1))
    );
    assert_eq!(
        strip.to_position(6, ColumnEncoding::Utf16),
        Ok(position(0, 1))
    );
    for encoding in ENCODINGS {
        for offset in 0..=3 {
            assert_eq!(strip.to_position(offset, encoding), Ok(position(0, 0)));
        }
        assert_eq!(strip.to_offset(position(0, 0), encoding), Ok(3));
        assert_eq!(strip.to_position(10, encoding), Ok(position(1, 0)));
        assert_eq!(strip.to_offset(position(1, 0), encoding), Ok(10));
    }

    // StripAll hides every consecutive leading BOM (6 bytes).
    let strip_all = base.clone().with_bom_policy(BomPolicy::StripAll);
    for encoding in ENCODINGS {
        for offset in 0..=6 {
            assert_eq!(strip_all.to_position(offset, encoding), Ok(position(0, 0)));
        }
        assert_eq!(strip_all.to_position(9, encoding), Ok(position(0, 3)));
        assert_eq!(strip_all.to_offset(position(0, 0), encoding), Ok(6));
        assert_eq!(strip_all.to_offset(position(0, 3), encoding), Ok(9));
        assert_eq!(strip_all.to_position(10, encoding), Ok(position(1, 0)));
        assert_eq!(strip_all.to_offset(position(1, 0), encoding), Ok(10));
    }

    // Re-applying with_bom_policy replaces the previous hidden prefix cleanly.
    let restored = strip_all.clone().with_bom_policy(BomPolicy::Keep);
    assert_eq!(
        restored.to_position(6, ColumnEncoding::Scalar),
        Ok(position(0, 2))
    );
    assert_eq!(
        restored.to_offset(position(0, 0), ColumnEncoding::Utf8),
        Ok(0)
    );
    let downgraded = strip_all.with_bom_policy(BomPolicy::Strip);
    assert_eq!(
        downgraded.to_position(6, ColumnEncoding::Scalar),
        Ok(position(0, 1))
    );
    assert_eq!(
        downgraded.to_offset(position(0, 0), ColumnEncoding::Utf8),
        Ok(3)
    );
}

#[test]
fn with_bom_policy_handles_bom_only_newlines_and_astral_characters() {
    use crate::BomPolicy;

    // Multi-BOM-only and BOM + newline + astral characters.
    let only = LineIndex::new("\u{feff}\u{feff}").with_bom_policy(BomPolicy::StripAll);
    for encoding in ENCODINGS {
        assert_eq!(only.to_position(0, encoding), Ok(position(0, 0)));
        assert_eq!(only.to_position(6, encoding), Ok(position(0, 0)));
        assert_eq!(only.to_offset(position(0, 0), encoding), Ok(6));
        assert_eq!(
            only.to_offset(position(0, 1), encoding),
            Err(PositionError::OutOfBounds)
        );
    }

    let bom_nl = LineIndex::new("\u{feff}\u{feff}\nA=1\n").with_bom_policy(BomPolicy::StripAll);
    for encoding in ENCODINGS {
        assert_eq!(bom_nl.to_position(6, encoding), Ok(position(0, 0)));
        assert_eq!(bom_nl.to_offset(position(0, 0), encoding), Ok(6));
        assert_eq!(bom_nl.to_position(7, encoding), Ok(position(1, 0)));
        assert_eq!(bom_nl.to_offset(position(1, 0), encoding), Ok(7));
    }

    let bom_crlf =
        LineIndex::new("\u{feff}\u{feff}\r\nA=1\r\n").with_bom_policy(BomPolicy::StripAll);
    for encoding in ENCODINGS {
        assert_eq!(bom_crlf.to_position(6, encoding), Ok(position(0, 0)));
        assert_eq!(bom_crlf.to_offset(position(0, 0), encoding), Ok(6));
        assert_eq!(
            bom_crlf.to_position(7, encoding),
            Err(PositionError::InsideCrLf)
        );
        assert_eq!(bom_crlf.to_position(8, encoding), Ok(position(1, 0)));
        assert_eq!(bom_crlf.to_offset(position(1, 0), encoding), Ok(8));
    }

    let astral = LineIndex::new("\u{feff}\u{feff}é😀!\n").with_bom_policy(BomPolicy::StripAll);
    assert_eq!(
        astral.to_position(12, ColumnEncoding::Utf8),
        Ok(position(0, 6))
    );
    assert_eq!(
        astral.to_position(12, ColumnEncoding::Scalar),
        Ok(position(0, 2))
    );
    assert_eq!(
        astral.to_position(12, ColumnEncoding::Utf16),
        Ok(position(0, 3))
    );
    assert_eq!(
        astral.to_offset(position(0, 2), ColumnEncoding::Utf16),
        Err(PositionError::SplitSurrogate)
    );
    assert_eq!(
        astral.to_offset(position(0, 3), ColumnEncoding::Utf16),
        Ok(12)
    );
    assert_eq!(
        astral.to_position(7, ColumnEncoding::Scalar),
        Err(PositionError::InvalidUtf8Boundary)
    );
}
