use alloc::{format, string::String, vec, vec::Vec};

use crate::{
    encode_value, encode_value_with, parse_with, Dialect, DoubleEscapes, EditError, EncodeError,
    InlineComments, InsertPosition, ItemKind, LineBreaks, Multiline, QuotedTail, Severity,
    SingleEscapes, UnknownEscapes,
};

const PRESETS: [Dialect; 5] = [
    Dialect::PORTABLE,
    Dialect::NODE_DOTENV,
    Dialect::PYTHON_DOTENV,
    Dialect::COMPOSE,
    Dialect::DOCKER_RAW,
];

fn assert_decodes(token: &str, value: &str, dialect: Dialect) {
    // Both contexts are part of the standalone contract. In particular, an LF
    // round trip alone misses raw terminal-CR loss at EOF.
    for ending in ["\n", ""] {
        let source = format!("A={token}{ending}");
        let (document, diagnostics) = parse_with(&source, dialect);
        assert!(
            !diagnostics.iter().any(|d| d.severity() == Severity::Error),
            "value={value:?}, token={token:?}, dialect={dialect:?}, diagnostics={diagnostics:?}"
        );
        assert_eq!(
            document.entries().len(),
            1,
            "source={source:?}, dialect={dialect:?}"
        );
        let entry = document.entries().next().unwrap();
        assert_eq!(entry.key(), "A");
        assert_eq!(
            entry.value(),
            Some(value),
            "source={source:?}, dialect={dialect:?}"
        );
        assert_eq!(document.get("A"), Some(value));
    }
}

fn assert_encodes(value: &str, dialect: Dialect) {
    let token = encode_value_with(value, dialect).unwrap_or_else(|error| {
        panic!("representable value={value:?}, dialect={dialect:?}: {error:?}")
    });
    assert_decodes(&token, value, dialect);
}

fn assert_inverse(token: &str, dialect: Dialect) -> bool {
    let source = format!("A={token}\n");
    let (document, diagnostics) = parse_with(&source, dialect);
    if diagnostics.iter().any(|d| d.severity() == Severity::Error)
        || document.entries().len() != 1
        || document
            .items()
            .any(|item| matches!(item.kind(), ItemKind::Invalid | ItemKind::Ignored))
    {
        return false;
    }
    let entry = document.entries().next().unwrap();
    if entry.key() != "A" {
        return false;
    }
    let Some(value) = entry.value() else {
        return false;
    };
    // The parser is the representability witness, not a conditional assertion
    // that would let an encoder refusing every input pass.
    assert_encodes(value, dialect);
    true
}

fn custom_dialects() -> Vec<Dialect> {
    let mut dialects = Vec::new();
    for escapes in [
        DoubleEscapes::NONE,
        DoubleEscapes::LF,
        DoubleEscapes::CR,
        DoubleEscapes::BACKSLASH,
        DoubleEscapes::DOUBLE_QUOTE,
        DoubleEscapes::LF.union(DoubleEscapes::CR),
        DoubleEscapes::DOUBLE_QUOTE.union(DoubleEscapes::BACKSLASH),
        DoubleEscapes::OCTAL.union(DoubleEscapes::DOUBLE_QUOTE),
    ] {
        for unknown in [
            UnknownEscapes::Preserve,
            UnknownEscapes::Warn,
            UnknownEscapes::DropBackslash,
            UnknownEscapes::Error,
        ] {
            dialects.push(Dialect {
                double_escapes: escapes,
                unknown_escapes: unknown,
                multiline: Multiline::NONE,
                backticks: false,
                ..Dialect::PORTABLE
            });
        }
    }
    for single in [
        SingleEscapes::None,
        SingleEscapes::Quote,
        SingleEscapes::QuoteAndBackslash,
    ] {
        for tail in [QuotedTail::Append, QuotedTail::LiteralFallback] {
            dialects.push(Dialect {
                single_escapes: single,
                double_escapes: DoubleEscapes::NONE,
                unknown_escapes: UnknownEscapes::Error,
                quoted_tail: tail,
                multiline: Multiline::NONE,
                backticks: false,
                inline_comments: InlineComments::AnyHash,
                ..Dialect::PORTABLE
            });
        }
    }
    dialects.extend([
        Dialect {
            backticks: true,
            multiline: Multiline::NONE,
            ..Dialect::PORTABLE
        },
        Dialect {
            normalize_newlines: true,
            ..Dialect::PORTABLE
        },
        Dialect {
            double_escapes: DoubleEscapes::NONE,
            ..Dialect::NODE_DOTENV
        },
        Dialect {
            backticks: false,
            multiline: Multiline::NONE,
            ..Dialect::NODE_DOTENV
        },
        Dialect {
            raw_values: true,
            line_breaks: LineBreaks::Lf,
            ..Dialect::PORTABLE
        },
        Dialect {
            normalize_newlines: true,
            ..Dialect::DOCKER_RAW
        },
        Dialect {
            line_byte_limit: Some(8),
            ..Dialect::DOCKER_RAW
        },
        Dialect {
            line_byte_limit: Some(8),
            ..Dialect::PORTABLE
        },
    ]);
    dialects
}

#[test]
fn new_encoder_supports_controls_without_changing_legacy_policy() {
    for scalar in (0..=0x9f).filter_map(char::from_u32) {
        let value = format!("left{scalar}right'\"\\#");
        for dialect in [Dialect::PORTABLE, Dialect::PYTHON_DOTENV, Dialect::COMPOSE] {
            assert_encodes(&value, dialect);
        }
        if scalar.is_control() && !matches!(scalar, '\n' | '\r' | '\t') {
            assert_eq!(encode_value(&value), None);
        } else {
            let token = encode_value(&value).expect("legacy Portable supports this scalar");
            assert_decodes(&token, &value, Dialect::PORTABLE);
        }
    }
    for value in [
        "",
        "simple",
        " a#b ",
        "${NAME}",
        "\r\n\t",
        "é😀\u{2028}\u{2029}",
    ] {
        let token = encode_value(value).unwrap();
        assert_decodes(&token, value, Dialect::PORTABLE);
    }
}

#[test]
fn arbitrary_utf8_property_uses_fixed_bounded_generator() {
    let special = [
        '\0', '\u{7}', '\u{8}', '\t', '\n', '\u{b}', '\u{c}', '\r', '\u{1b}', '\u{7f}', '\u{85}',
        '\u{a0}', '\u{2028}', '\u{2029}', '\u{feff}', '\'', '"', '`', '\\', '#', '$', ' ', 'n',
        'r', '0', 'é', '😀',
    ];
    let mut state = 0x6a09_e667_f3bc_c909_u64;
    for sample in 0..256 {
        let mut value = String::new();
        for position in 0..sample % 65 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let scalar = if position % 3 == 0 {
                char::from_u32((state % 0x11_0000) as u32).unwrap_or('\0')
            } else {
                special[usize::try_from(state % u64::try_from(special.len()).unwrap()).unwrap()]
            };
            value.push(scalar);
        }
        for dialect in [Dialect::PORTABLE, Dialect::PYTHON_DOTENV, Dialect::COMPOSE] {
            assert_encodes(&value, dialect);
        }
    }
}

#[test]
fn legal_short_tokens_are_positive_inverse_witnesses() {
    // Exhaustive short products exercise slash/quote overlaps rather than only
    // values conveniently produced by the encoder under test.
    let alphabet = ['a', 'n', 'r', '\\', '\'', '"', '`', '#', ' ', '\r', '\n'];
    let mut bodies = vec![String::new()];
    for first in alphabet {
        bodies.push(String::from(first));
        for second in alphabet {
            let mut body = String::new();
            body.push(first);
            body.push(second);
            bodies.push(body);
        }
    }
    let mut dialects = PRESETS.to_vec();
    dialects.extend(custom_dialects());
    for dialect in dialects {
        let mut witnesses = 0;
        for body in &bodies {
            for token in [
                body.clone(),
                format!("'{body}'"),
                format!("\"{body}\""),
                format!("`{body}`"),
            ] {
                witnesses += usize::from(assert_inverse(&token, dialect));
            }
        }
        assert!(
            witnesses > 10,
            "inverse matrix lacks witnesses for {dialect:?}"
        );
    }
}

#[test]
fn literal_fallback_comment_padding_is_a_positive_inverse() {
    let dialect = Dialect {
        inline_comments: InlineComments::AnyHash,
        double_escapes: DoubleEscapes::NONE,
        unknown_escapes: UnknownEscapes::Warn,
        single_escapes: SingleEscapes::None,
        backticks: false,
        quoted_tail: QuotedTail::LiteralFallback,
        multiline: Multiline {
            single: true,
            double: false,
            backtick: false,
        },
        normalize_newlines: true,
        ..Dialect::PORTABLE
    };
    let value = "\".'\t\u{7}\0\"";
    assert!(assert_inverse(&format!("{value}#ignored"), dialect));
}

#[test]
fn escape_overlap_octal_and_quote_closers_have_positive_witnesses() {
    let tokens = [
        r"\n",
        r"\r",
        r"\\n",
        r"\\r",
        r"\\\n",
        r"\\\r",
        r"\nr\rn",
        r"'trailing\'",
        r"'two\\'",
        r"'a\'b'",
        r"'a\\\'b'",
        r#""a\"b""#,
        r#""a\\""#,
        r#""\n\r\t\\\"\$\q""#,
        r#""\0000x""#,
        r#""\0377x""#,
        r#""\0400x""#,
        r#""\0789x""#,
        r#""\0x""#,
        r#""\0123""#,
        r#""\01234""#,
        "'first\r\nsecond'",
        "\"first\rsecond\"",
        "`first\nsecond`",
        "'a'\"`tail",
        "\"a\"'`tail",
        "'a'\\n",
        "'a'#comment",
        "'a'\u{2028}'b'",
        "\"a\"\u{2029}\"b\"",
    ];
    let mut dialects = PRESETS.to_vec();
    dialects.extend(custom_dialects());
    for dialect in dialects {
        for token in tokens {
            assert_inverse(token, dialect);
        }
    }
}

#[test]
fn node_uses_real_literal_fallback_but_reports_true_limitations() {
    for value in [
        "a'\"`b",
        "'a\"`b",
        "a'\"`",
        "\\n\\r",
        "\\\\n\\\\r",
        "#\\",
        "#\n\\n",
        "#\rtext",
        "a\u{2028}'b'\u{2029}c",
        "'\"`\\nr",
    ] {
        assert_encodes(value, Dialect::NODE_DOTENV);
    }
    // LF excludes the raw/literal fallback. Each of Node's three quote forms
    // then collides with an unescapable delimiter in this value.
    assert_eq!(
        encode_value_with("\n'\"`#", Dialect::NODE_DOTENV),
        Err(EncodeError::Unrepresentable)
    );
    for token in [
        "'a\"`b",
        "a'\"`b",
        "\"\\\\n\\r\"",
        "\"a\\\"b\"",
        "x\u{2028}'b'\u{2029}y",
        "\"a\u{2028}b\"tail",
    ] {
        assert!(
            assert_inverse(token, Dialect::NODE_DOTENV),
            "token={token:?}"
        );
    }
}

#[test]
fn raw_lf_only_preserves_internal_and_terminal_cr_without_losing_eof() {
    for value in ["", "\0", " a # b ", "a\rb", "\r", "a\r", "a\r\r", "é\r"] {
        assert_encodes(value, Dialect::DOCKER_RAW);
    }
    for value in ["\n", "a\nb", "\r\n"] {
        assert_eq!(
            encode_value_with(value, Dialect::DOCKER_RAW),
            Err(EncodeError::Unrepresentable)
        );
    }
    let normalized = Dialect {
        normalize_newlines: true,
        ..Dialect::DOCKER_RAW
    };
    for value in ["\n", "a\nb", "\n\n", "a\n", "\0\n#"] {
        assert_encodes(value, normalized);
    }
    for value in ["\r", "a\rb", "\r\n"] {
        assert_eq!(
            encode_value_with(value, normalized),
            Err(EncodeError::Unrepresentable)
        );
    }
    let universal = Dialect {
        line_breaks: LineBreaks::Universal,
        ..Dialect::DOCKER_RAW
    };
    assert_eq!(
        encode_value_with("a\rb", universal),
        Err(EncodeError::Unrepresentable)
    );
}

#[test]
fn canonical_header_and_raw_cr_padding_count_toward_byte_caps() {
    let raw = Dialect {
        line_byte_limit: Some(8),
        ..Dialect::DOCKER_RAW
    };
    for value in ["123456", "ééé", "1234\r"] {
        assert_encodes(value, raw);
    }
    for value in ["1234567", "éééx", "12345\r"] {
        assert!(encode_value_with(value, raw).is_err(), "value={value:?}");
    }
    for limit in [0, 1] {
        let dialect = Dialect {
            line_byte_limit: Some(limit),
            ..Dialect::DOCKER_RAW
        };
        assert!(encode_value_with("", dialect).is_err());
    }
    assert_encodes(
        "",
        Dialect {
            line_byte_limit: Some(2),
            ..Dialect::DOCKER_RAW
        },
    );
    let exact = "x".repeat(65_533);
    assert_encodes(&exact, Dialect::DOCKER_RAW);
    assert!(encode_value_with(&format!("{exact}x"), Dialect::DOCKER_RAW).is_err());
    let cr_exact = format!("{}\r", "x".repeat(65_531));
    assert_encodes(&cr_exact, Dialect::DOCKER_RAW);
    assert!(encode_value_with(&format!("x{cr_exact}"), Dialect::DOCKER_RAW).is_err());
}

#[test]
fn append_tail_can_represent_a_delimiter_outside_the_quoted_body() {
    let append = Dialect {
        double_escapes: DoubleEscapes::NONE,
        unknown_escapes: UnknownEscapes::Error,
        single_escapes: SingleEscapes::None,
        quoted_tail: QuotedTail::Append,
        multiline: Multiline::NONE,
        backticks: false,
        inline_comments: InlineComments::AnyHash,
        ..Dialect::PORTABLE
    };
    // The # forces a quoted prefix, while both quote kinds must survive.
    let token = "'#'\"'tail";
    let source = format!("A={token}\n");
    let (document, diagnostics) = parse_with(&source, append);
    assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
    assert_eq!(document.get("A"), Some("#\"'tail"));
    assert_encodes("#\"'tail", append);
    assert_eq!(
        encode_value_with("#\n'\"", append),
        Err(EncodeError::Unrepresentable)
    );
}

#[test]
fn disabled_multiline_and_normalization_still_use_available_escapes() {
    let no_multiline = Dialect {
        multiline: Multiline::NONE,
        backticks: false,
        ..Dialect::PORTABLE
    };
    for value in ["a\nb", "a\rb", "\n\r", "'\"\\\n#"] {
        assert_encodes(value, no_multiline);
    }
    let no_lf = Dialect {
        multiline: Multiline::NONE,
        backticks: false,
        double_escapes: DoubleEscapes::CR,
        ..Dialect::PORTABLE
    };
    assert_eq!(
        encode_value_with("\n", no_lf),
        Err(EncodeError::Unrepresentable)
    );
    assert_encodes("\r", no_lf);
    let normalized = Dialect {
        normalize_newlines: true,
        ..Dialect::PORTABLE
    };
    for value in ["\r", "\r\n", "\n\r", "'\"\r\\n"] {
        assert_encodes(value, normalized);
    }
    let no_cr = Dialect {
        normalize_newlines: true,
        double_escapes: DoubleEscapes::NONE,
        ..Dialect::PORTABLE
    };
    assert_eq!(
        encode_value_with("\r", no_cr),
        Err(EncodeError::Unrepresentable)
    );
}

#[test]
fn native_control_edits_preserve_neighbors_comments_and_order() {
    let value = "\0\u{7}\u{7f}'\"\\\n\r\t#literal";
    for dialect in [Dialect::PORTABLE, Dialect::PYTHON_DOTENV, Dialect::COMPOSE] {
        for ending in ["\n", "\r\n"] {
            let prefix = format!("# synthetic{ending}KEEP='left'{ending}");
            let suffix = format!(" # retained{ending}LAST=right{ending}");
            let source = format!("{prefix}A='old'{suffix}");
            let (document, diagnostics) = parse_with(&source, dialect);
            assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
            let edited = document.set("A", value).unwrap();
            assert!(edited.as_str().starts_with(&prefix));
            assert!(edited.as_str().ends_with(&suffix));
            let (after, diagnostics) = parse_with(edited.as_str(), dialect);
            assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
            let meanings: Vec<_> = after
                .entries()
                .map(|entry| (entry.key(), entry.value()))
                .collect();
            assert_eq!(
                meanings,
                [
                    ("KEEP", Some("left")),
                    ("A", Some(value)),
                    ("LAST", Some("right"))
                ]
            );
            assert_eq!(
                after.unique("A").unwrap().inline_comment(),
                document.unique("A").unwrap().inline_comment(),
            );

            let inserted = after
                .insert("NEW", Some(value), InsertPosition::End)
                .unwrap();
            let (result, diagnostics) = parse_with(inserted.as_str(), dialect);
            assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
            let meanings: Vec<_> = result
                .entries()
                .map(|entry| (entry.key(), entry.value()))
                .collect();
            assert_eq!(
                meanings,
                [
                    ("KEEP", Some("left")),
                    ("A", Some(value)),
                    ("LAST", Some("right")),
                    ("NEW", Some(value))
                ]
            );
        }
        let (document, _) = parse_with("KEEP=left\nA='old'", dialect);
        let edited = document.set("A", value).unwrap();
        let (after, diagnostics) = parse_with(edited.as_str(), dialect);
        assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
        assert_eq!(after.get("A"), Some(value));
        assert_eq!(after.get("KEEP"), Some("left"));
    }
}

#[test]
fn native_raw_controls_and_terminal_cr_respect_actual_line_endings() {
    for ending in ["\n", "\r\n", ""] {
        let source = format!("KEEP=left\nA=old{ending}");
        let (document, diagnostics) = parse_with(&source, Dialect::DOCKER_RAW);
        assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
        for value in ["\0", "a\0b", "a\rb", "\r", "a\r", "a\r\r"] {
            let edited = document.set("A", value).unwrap();
            assert!(edited.as_str().starts_with("KEEP=left\n"));
            let (after, diagnostics) = parse_with(edited.as_str(), Dialect::DOCKER_RAW);
            assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
            let meanings: Vec<_> = after
                .entries()
                .map(|entry| (entry.key(), entry.value()))
                .collect();
            assert_eq!(meanings, [("KEEP", Some("left")), ("A", Some(value))]);

            let inserted = document
                .insert("NEW", Some(value), InsertPosition::End)
                .unwrap();
            let (after, diagnostics) = parse_with(inserted.as_str(), Dialect::DOCKER_RAW);
            assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
            let meanings: Vec<_> = after
                .entries()
                .map(|entry| (entry.key(), entry.value()))
                .collect();
            assert_eq!(
                meanings,
                [
                    ("KEEP", Some("left")),
                    ("A", Some("old")),
                    ("NEW", Some(value))
                ]
            );
        }
        assert_eq!(
            document.set("A", "x\nINJECTED=y"),
            Err(EditError::Unencodable)
        );
        assert_eq!(
            document.insert("NEW", Some("\n"), InsertPosition::End),
            Err(EditError::Unencodable)
        );
    }
}

#[test]
fn native_encoding_uses_the_final_batch_header_budget() {
    let dialect = Dialect {
        line_byte_limit: Some(10),
        ..Dialect::PORTABLE
    };
    for (source, header_change) in [
        (
            "LONGNAME=x\nKEEP=y\n",
            crate::Edit::Rename { entry: 0, key: "A" },
        ),
        (
            "export A=x\nKEEP=y\n",
            crate::Edit::Export {
                entry: 0,
                enabled: false,
            },
        ),
    ] {
        let (document, diagnostics) = parse_with(source, dialect);
        assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
        let set = crate::Edit::Set {
            entry: 0,
            value: "12345678",
        };
        for operations in [[header_change, set], [set, header_change]] {
            let edited = document.edit(&operations).unwrap();
            let (after, diagnostics) = parse_with(edited.as_str(), dialect);
            assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
            assert_eq!(after.get("A"), Some("12345678"));
            assert_eq!(after.get("KEEP"), Some("y"));
            assert!(!after.entry(0).unwrap().exported());
        }
    }
}

#[test]
fn ending_field_encoding_accounts_for_the_generated_crlf_separator() {
    let (document, _) = parse_with("KEEP=x\r\nA", Dialect::DOCKER_RAW);
    let edited = document
        .edit(&[
            crate::Edit::Set {
                entry: 1,
                value: "x\r",
            },
            crate::Edit::Insert {
                key: "B",
                value: Some("y"),
                position: InsertPosition::End,
            },
        ])
        .unwrap();
    let (after, diagnostics) = parse_with(edited.as_str(), Dialect::DOCKER_RAW);
    assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
    let meanings: Vec<_> = after
        .entries()
        .map(|entry| (entry.key(), entry.value()))
        .collect();
    assert_eq!(
        meanings,
        [("KEEP", Some("x")), ("A", Some("x\r")), ("B", Some("y"))]
    );
}

#[test]
fn compose_quote_changes_keep_following_blank_items_unselected() {
    let source = "A='x'\r\n";
    let value = "#\r\r\\";
    let document = parse_with(source, Dialect::COMPOSE).0;
    let result = document.set("A", value).unwrap();
    let (after, diagnostics) = parse_with(result.as_str(), Dialect::COMPOSE);
    assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
    assert_eq!(after.get("A"), Some(value));
    assert!(result.as_str().ends_with("\r\n"));
}

#[test]
fn native_byte_caps_include_neighbor_statements_and_external_endings() {
    for (source, limit) in [("A='x'\r\n", 6), ("A='x'B=y\n", 8), ("P='x'A='y'\n", 10)] {
        let dialect = Dialect {
            line_byte_limit: Some(limit),
            ..Dialect::COMPOSE
        };
        let (document, diagnostics) = parse_with(source, dialect);
        assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
        let result = document.set("A", "'").unwrap();
        let (after, diagnostics) = parse_with(result.as_str(), dialect);
        assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
        assert_eq!(after.get("A"), Some("'"));
        for key in ["B", "P"] {
            assert_eq!(after.get(key), document.get(key));
        }
        assert_eq!(after.entries().len(), document.entries().len());
    }
}
