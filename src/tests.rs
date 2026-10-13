use super::*;

fn doc(text: &str) -> Document {
    parse(text).expect("parse")
}

fn val<'a>(d: &'a Document, k: &str) -> &'a str {
    d.get(k).unwrap_or_else(|| panic!("no {k}"))
}

// ---- values ----

#[test]
fn basic_pairs_export_and_comments() {
    let d = doc("# a comment\n\
         NODE_ENV=production\n\
         export PORT=8080\n\
         \n\
         EMPTY=\n\
         INLINE=value # trailing comment\n\
         HASH=a#b\n");
    assert_eq!(val(&d, "NODE_ENV"), "production");
    assert_eq!(val(&d, "PORT"), "8080");
    assert!(d.entries()[1].exported());
    assert_eq!(val(&d, "EMPTY"), "");
    assert_eq!(val(&d, "INLINE"), "value");
    assert_eq!(
        val(&d, "HASH"),
        "a#b",
        "`#` with no leading space is literal"
    );
    assert_eq!(d.entries().len(), 5);
}

#[test]
fn no_variable_expansion() {
    let d = doc("HOME_REF=${HOME}/bin\nDOLLAR=$USER\nDQ=\"${HOME}/x\"\nSQ='${HOME}/y'\n");
    assert_eq!(val(&d, "HOME_REF"), "${HOME}/bin");
    assert_eq!(val(&d, "DOLLAR"), "$USER");
    assert_eq!(val(&d, "DQ"), "${HOME}/x");
    assert_eq!(val(&d, "SQ"), "${HOME}/y");
}

#[test]
fn quotes_and_escapes() {
    let d = doc("SINGLE='literal # not a comment'\n\
         DOUBLE=\"a\\nb\\t\\\"c\\\"\"\n\
         SPACES=\"  padded  \"\n");
    assert_eq!(val(&d, "SINGLE"), "literal # not a comment");
    assert_eq!(d.entries()[0].quote(), Quote::Single);
    assert_eq!(val(&d, "DOUBLE"), "a\nb\t\"c\"");
    assert_eq!(d.entries()[1].quote(), Quote::Double);
    assert_eq!(val(&d, "SPACES"), "  padded  ");
}

#[test]
fn double_quote_cr_backslash_and_unknown_escapes() {
    let d = doc("CR=\"a\\rb\"\nBS=\"a\\\\b\"\nUNK=\"a\\qb\"\n");
    assert_eq!(val(&d, "CR"), "a\rb");
    assert_eq!(val(&d, "BS"), "a\\b");
    assert_eq!(val(&d, "UNK"), "a\\qb", "unknown escape kept as written");
}

#[test]
fn duplicate_keys_are_kept_and_last_wins() {
    let d = doc("K=first\nK=second\n");
    assert_eq!(val(&d, "K"), "second");
    assert_eq!(d.entries_for("K").count(), 2);
    assert_eq!(d.unique("K"), Err(EditError::Duplicate(2)));
    assert_eq!(d.unique("NOPE"), Err(EditError::Missing));
}

#[test]
fn crlf_line_endings() {
    assert_eq!(val(&doc("A=1\r\nB=2\r\n"), "B"), "2");
}

#[test]
fn a_lone_cr_does_not_end_a_line() {
    assert_eq!(val(&doc("A=1\rB=2\n"), "A"), "1\rB=2");
}

#[test]
fn bom_is_skipped() {
    let d = doc("\u{feff}NODE_ENV=production\nPORT=8080\n");
    assert!(d.has_bom());
    assert_eq!(val(&d, "NODE_ENV"), "production");
    assert_eq!(d.entries().len(), 2);
}

#[test]
fn equals_inside_a_value_is_kept() {
    let d = doc("DATABASE_URL=postgres://u:p@h/db?sslmode=require\n");
    assert_eq!(
        val(&d, "DATABASE_URL"),
        "postgres://u:p@h/db?sslmode=require"
    );
}

#[test]
fn spaces_around_equals() {
    let d = doc("KEY = value\nPADDED =  x \n");
    assert_eq!(val(&d, "KEY"), "value");
    assert_eq!(val(&d, "PADDED"), "x");
}

#[test]
fn export_is_a_prefix_only_with_a_following_space() {
    let d = doc("export=x\n");
    assert_eq!(val(&d, "export"), "x");
    assert!(!d.entries()[0].exported());
}

#[test]
fn empty_value_with_inline_comment() {
    let d = doc("SECRET= # TODO fill before prod\nOK=x # keep\n");
    assert_eq!(val(&d, "SECRET"), "");
    assert_eq!(val(&d, "OK"), "x");
}

#[test]
fn text_after_a_closing_quote_is_ignored() {
    assert_eq!(val(&doc("A='x' # c\nB=\"y\"tail\n"), "B"), "y");
}

// ---- errors ----

#[test]
fn malformed_lines_are_errors_with_line_numbers() {
    let e = parse("A=1\nNOEQUALS\n").unwrap_err();
    assert_eq!(e.line(), 2);
    assert_eq!(
        e.to_string(),
        "line 2: expected `KEY=VALUE`, found `NOEQUALS`"
    );
    for (text, msg) in [
        ("=novalue\n", "line 1: empty key before `=`"),
        ("A B=1\n", "line 1: key `A B` contains whitespace"),
        (
            "Q=\"unterminated\n",
            "line 1: unterminated double-quoted value",
        ),
        (
            "Q='unterminated\n",
            "line 1: unterminated single-quoted value",
        ),
        ("B=\"trailing\\", "line 1: trailing backslash in value"),
        (
            "export NOEQ  \n",
            "line 1: expected `KEY=VALUE`, found `NOEQ`",
        ),
    ] {
        assert_eq!(parse(text).unwrap_err().to_string(), msg, "{text:?}");
    }
}

#[test]
fn empty_and_comment_only_input() {
    assert_eq!(doc("").entries(), []);
    assert_eq!(doc("\n  \n# x\n\t# y").entries(), []);
}

// ---- spans ----

#[test]
fn value_span_covers_unquoted_single_and_double_quoted() {
    let src = "A=plain\nB='lit'\nC=\"esc\\n\"\n";
    let d = doc(src);
    let span = |i: usize| &src[d.entries()[i].value_span()];
    assert_eq!(span(0), "plain");
    assert_eq!(span(1), "'lit'");
    assert_eq!(span(2), "\"esc\\n\"");
}

#[test]
fn spans_skip_export_and_exclude_the_inline_comment() {
    let src = "export PORT = 8080 # the port\n";
    let d = doc(src);
    let e = &d.entries()[0];
    assert_eq!(&src[e.value_span()], "8080");
    assert_eq!(&src[e.key_span()], "PORT");
    assert_eq!(&src[e.line_span()], src);
}

#[test]
fn empty_value_span_is_zero_width_after_equals() {
    let src = "EMPTY=\n";
    let s = doc(src).entries()[0].value_span();
    assert!(s.is_empty());
    assert_eq!(&src[..s.start], "EMPTY=");
}

#[test]
fn spans_are_offset_past_a_leading_bom() {
    let src = "\u{feff}A=hi\n";
    let d = doc(src);
    let e = &d.entries()[0];
    assert_eq!(&src[e.value_span()], "hi");
    assert_eq!(&src[e.key_span()], "A");
    assert_eq!(&src[e.line_span()], "A=hi\n");
}

#[test]
fn line_span_includes_crlf_and_the_last_line_has_none() {
    let src = "A=1\r\nB=2";
    let d = doc(src);
    assert_eq!(&src[d.entries()[0].line_span()], "A=1\r\n");
    assert_eq!(&src[d.entries()[1].line_span()], "B=2");
}

#[test]
fn spans_are_byte_accurate_with_multibyte_text() {
    let src = "café=☕\nPORT=1\n";
    let d = doc(src);
    let e = &d.entries()[0];
    assert_eq!(&src[e.key_span()], "café");
    assert_eq!(&src[e.value_span()], "☕");
}

// ---- encoding ----

#[test]
fn encode_quotes_only_when_needed() {
    let e = |s: &str| encode_value(s).unwrap();
    assert_eq!(e("plain"), "plain");
    assert_eq!(e("a b"), "a b");
    assert_eq!(e("q\"x"), "q\"x");
    assert_eq!(e(" pad "), "\" pad \"");
    assert_eq!(e("a #c"), "\"a #c\"");
    assert_eq!(e("'x"), "\"'x\"");
    assert_eq!(e("#x"), "\"#x\"");
    assert_eq!(e("a#x"), "a#x");
    assert_eq!(e("l1\nl2"), "\"l1\\nl2\"");
    assert_eq!(e("a\tb"), "\"a\\tb\"");
}

#[test]
fn encode_declines_unwritable_control_characters() {
    assert_eq!(encode_value("a\u{0}b"), None);
    assert_eq!(encode_value("a\u{1b}b"), None);
}

#[test]
fn encoded_values_round_trip() {
    for v in [
        "plain",
        "a b",
        "q\"x",
        " pad ",
        "a #c",
        "'quoted'",
        "\"dq\"",
        "back\\slash",
        "l1\nl2\r\n",
        "tab\tsep",
        "",
        "#leading-hash-no-space",
        "${HOME}",
        "x=y",
    ] {
        let line = format!("K={}\n", encode_value(v).unwrap());
        assert_eq!(doc(&line).get("K"), Some(v), "{line:?}");
    }
}

// ---- editing ----

#[test]
fn set_changes_only_the_value() {
    assert_eq!(
        set("A='old' # keep\nB=1\n", "A", "new").unwrap(),
        "A=new # keep\nB=1\n"
    );
    assert_eq!(set("A=1\n", "A", " pad ").unwrap(), "A=\" pad \"\n");
    assert_eq!(set("EMPTY=\n", "EMPTY", "v").unwrap(), "EMPTY=v\n");
    assert_eq!(set("\u{feff}A=1\r\n", "A", "2").unwrap(), "\u{feff}A=2\r\n");
}

#[test]
fn set_quotes_where_a_bare_value_would_read_differently() {
    // ` #` after `=` and whitespace would start a comment.
    assert_eq!(
        set("export A = a # c\n", "A", "#").unwrap(),
        "export A = \"#\" # c\n"
    );
    // Text after a closing quote is ignored, but would join a bare value.
    assert_eq!(set("B=\"y\"tail\n", "B", "z").unwrap(), "B=\"z\"tail\n");
}

#[test]
fn remove_takes_the_whole_line() {
    assert_eq!(remove("A=1\nDROP=2\nB=3\n", "DROP").unwrap(), "A=1\nB=3\n");
    assert_eq!(remove("A=1\nDROP=2", "DROP").unwrap(), "A=1\n");
    assert_eq!(
        remove("\u{feff}A=1\r\nDROP=2\r\n", "DROP").unwrap(),
        "\u{feff}A=1\r\n"
    );
}

#[test]
fn edits_refuse_what_they_cannot_do_exactly() {
    assert_eq!(set("K=1\nK=2\n", "K", "x"), Err(EditError::Duplicate(2)));
    assert_eq!(remove("K=1\nK=2\n", "K"), Err(EditError::Duplicate(2)));
    assert_eq!(set("A=1\n", "B", "x"), Err(EditError::Missing));
    assert_eq!(set("A=1\n", "A", "\u{7}"), Err(EditError::Unencodable));
    assert!(matches!(set("oops\n", "A", "x"), Err(EditError::Parse(_))));
}

#[test]
fn a_commented_out_assignment_is_not_a_duplicate() {
    let src = "# DBURL=commented-out\nDBURL=real\n";
    assert_eq!(remove(src, "DBURL").unwrap(), "# DBURL=commented-out\n");
}

// ---- properties ----

mod props {
    use super::*;
    use proptest::prelude::*;

    /// Values from an alphabet rich in the characters `.env` treats specially.
    fn value() -> impl Strategy<Value = String> {
        proptest::collection::vec(
            prop_oneof![
                Just(' '),
                Just('#'),
                Just('\''),
                Just('"'),
                Just('\\'),
                Just('='),
                Just('\n'),
                Just('\r'),
                Just('\t'),
                Just('$'),
                Just('{'),
                Just('}'),
                Just('a'),
                Just('Z'),
                Just('é'),
                Just('☕'),
                Just('\u{feff}'),
            ],
            0..12,
        )
        .prop_map(|v| v.into_iter().collect())
    }

    fn key() -> impl Strategy<Value = String> {
        "[A-Z_][A-Z0-9_]{0,6}"
    }

    /// Lines that parse: assignments in every quoting style, comments, blanks.
    fn line() -> impl Strategy<Value = String> {
        prop_oneof![
            (key(), value()).prop_map(|(k, v)| format!("{k}={}", encode_value(&v).unwrap())),
            (key(), "[a-z ]{0,6}").prop_map(|(k, v)| format!("export {k} = {v} # c")),
            (key(), "[a-z#\" ]{0,6}").prop_map(|(k, v)| format!("{k}='{v}'")),
            (key(), "[a-z]{0,4}", "[a-z#= ]{0,4}")
                .prop_map(|(k, v, tail)| format!("{k}=\"{v}\"{tail}")),
            Just("# comment".to_string()),
            Just(String::new()),
        ]
    }

    fn file() -> impl Strategy<Value = String> {
        (
            any::<bool>(),
            proptest::collection::vec(line(), 0..8),
            prop_oneof![Just("\n"), Just("\r\n")],
            any::<bool>(),
        )
            .prop_map(|(bom, lines, eol, trailing)| {
                let mut s = if bom {
                    "\u{feff}".to_string()
                } else {
                    String::new()
                };
                s.push_str(&lines.join(eol));
                if trailing && !lines.is_empty() {
                    s.push_str(eol);
                }
                s
            })
    }

    proptest! {
        #[test]
        fn encode_then_parse_is_identity(v in value()) {
            let line = format!("K={}", encode_value(&v).unwrap());
            let d = doc(&line);
            prop_assert_eq!(d.get("K"), Some(v.as_str()));
        }

        #[test]
        fn spans_slice_what_they_claim(text in file()) {
            let d = doc(&text);
            for e in d.entries() {
                prop_assert_eq!(&text[e.key_span()], e.key());
                prop_assert!(text[e.line_span()].contains(&text[e.value_span()]));
                let line = &text[e.line_span()];
                prop_assert!(line.ends_with('\n') || e.line_span().end == text.len());
            }
        }

        #[test]
        fn set_changes_exactly_one_value(text in file(), v in value()) {
            let d = doc(&text);
            for e in d.entries() {
                if d.entries_for(e.key()).count() != 1 {
                    continue;
                }
                let out = set(&text, e.key(), &v).unwrap();
                let after = doc(&out);
                prop_assert_eq!(after.get(e.key()), Some(v.as_str()));
                // Every other assignment reads back unchanged.
                for other in d.entries().iter().filter(|o| o.key() != e.key()) {
                    prop_assert_eq!(after.get(other.key()), d.get(other.key()));
                }
                // Bytes outside the value span are untouched.
                let s = e.value_span();
                prop_assert!(out.starts_with(&text[..s.start]));
                prop_assert!(out.ends_with(&text[s.end..]));
            }
        }

        #[test]
        fn remove_drops_exactly_one_assignment(text in file()) {
            let d = doc(&text);
            for e in d.entries() {
                if d.entries_for(e.key()).count() != 1 {
                    continue;
                }
                let after = doc(&remove(&text, e.key()).unwrap());
                prop_assert_eq!(after.get(e.key()), None);
                prop_assert_eq!(after.entries().len(), d.entries().len() - 1);
            }
        }
    }
}
