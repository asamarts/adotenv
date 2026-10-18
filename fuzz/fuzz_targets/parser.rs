#![no_main]
#![forbid(unsafe_code)]

use std::{ops::Range, vec::Vec};

use adotenv::{
    parse, parse_owned, parse_owned_with, parse_strict, parse_strict_with, parse_with,
    AssignmentState, BareKeys, BomPolicy, ColumnEncoding, Dialect, Document, Entry, EntryTokens,
    Item, PositionError, Quote, QuotedTail, ReferenceKind, ReferenceRules, Severity,
};
use libfuzzer_sys::fuzz_target;

fn check_span(source: &str, span: Range<usize>) {
    assert!(span.start <= span.end);
    assert!(source.get(span).is_some());
}

fn take<'s>(source: &'s str, span: &Range<usize>, cursor: &mut usize) -> &'s str {
    assert_eq!(span.start, *cursor);
    *cursor = span.end;
    &source[span.clone()]
}

fn horizontal(text: &str) {
    assert!(text.bytes().all(|byte| matches!(byte, b' ' | b'\t')));
}

fn check_tokens(source: &str, entry: Entry<'_>, item: Item<'_>, tokens: &EntryTokens) {
    assert_eq!(entry.line_span(), item.span());
    let raw = item.raw();
    let ending = if raw.ends_with("\r\n") {
        "\r\n"
    } else if raw.ends_with('\n') {
        "\n"
    } else if raw.ends_with('\r') {
        "\r"
    } else {
        ""
    };
    assert_eq!(item.line_ending().as_str(), ending);
    let content_end = item.span().end - ending.len();
    assert_eq!(item.content_span(), item.span().start..content_end);
    let mut at = item.span().start;
    horizontal(take(source, &tokens.indent, &mut at));
    assert_eq!(entry.exported(), tokens.export.is_some());
    if let Some(export) = &tokens.export {
        assert_eq!(take(source, export, &mut at), "export");
        assert!(!tokens.space_after_export.is_empty());
    } else {
        assert!(tokens.space_after_export.is_empty());
    }
    horizontal(take(source, &tokens.space_after_export, &mut at));
    assert_eq!(tokens.key, entry.key_span());
    let key = take(source, &tokens.key, &mut at);
    assert_eq!(key, entry.key());
    assert!(
        !key.is_empty()
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    );
    horizontal(take(source, &tokens.space_before_eq, &mut at));
    if let Some(eq) = &tokens.eq {
        assert_eq!(take(source, eq, &mut at), "=");
        assert!(tokens.value.is_some() && tokens.body.is_some());
    } else {
        assert!(
            tokens.space_after_eq.is_empty() && tokens.value.is_none() && tokens.body.is_none()
        );
        assert!(tokens.trailing_space.is_empty());
        assert_eq!(entry.quote(), Quote::None);
        assert_eq!(entry.value(), None);
    }
    horizontal(take(source, &tokens.space_after_eq, &mut at));
    assert_eq!(tokens.value, entry.value_span());
    if let Some(value) = &tokens.value {
        let text = take(source, value, &mut at);
        assert_eq!(entry.raw_value(), Some(text));
        let body = tokens.body.as_ref().unwrap();
        match entry.quote() {
            Quote::None => {
                assert_eq!(body, value);
                assert!(!text.starts_with(['\'', '"']));
                assert!(!text.starts_with([' ', '\t']) && !text.ends_with([' ', '\t']));
            }
            Quote::Single | Quote::Double => {
                let delimiter = if entry.quote() == Quote::Single {
                    b'\''
                } else {
                    b'"'
                };
                assert!(text.len() >= 2);
                assert_eq!(text.as_bytes().first(), Some(&delimiter));
                assert_eq!(text.as_bytes().last(), Some(&delimiter));
                assert_eq!(*body, value.start + 1..value.end - 1);
            }
            _ => unreachable!("update the oracle for new quote styles"),
        }
    } else {
        assert_eq!(entry.raw_value(), None);
    }
    assert_eq!(
        entry.state(),
        match entry.value() {
            None => AssignmentState::Bare,
            Some("") => AssignmentState::Empty,
            Some(_) => AssignmentState::Value,
        }
    );
    assert!(tokens.quoted_tail.is_none());
    horizontal(take(source, &tokens.trailing_space, &mut at));
    if let Some(comment) = &tokens.inline_comment {
        assert!(comment.start > 0 && matches!(source.as_bytes()[comment.start - 1], b' ' | b'\t'));
        assert!(take(source, comment, &mut at).starts_with('#'));
    }
    assert_eq!(at, content_end);
}

fn check_mapping(source: &str, entry: Entry<'_>) {
    let Some(value) = entry.value() else {
        assert_eq!(
            entry.decoded_offset_to_raw(0),
            Err(PositionError::OutOfBounds)
        );
        return;
    };
    let body = entry.tokens().body.as_ref().unwrap();
    let samples = [0, value.len() / 2, value.len()];
    let mut expected = [None; 3];
    let mut decoded = value.char_indices();
    let mut raw = source[body.clone()].char_indices().peekable();
    while let Some((raw_offset, mut ch)) = raw.next() {
        if entry.quote() == Quote::Double && ch == '\\' {
            let known = raw.peek().and_then(|(_, next)| match next {
                'n' => Some('\n'),
                'r' => Some('\r'),
                't' => Some('\t'),
                '\\' => Some('\\'),
                '"' => Some('"'),
                '$' => Some('$'),
                _ => None,
            });
            if let Some(unescaped) = known {
                raw.next();
                ch = unescaped;
            }
        }
        let (offset, actual) = decoded.next().expect("decoded value lost a raw character");
        assert_eq!(actual, ch);
        for (sample, mapped) in samples.iter().zip(&mut expected) {
            if *sample == offset {
                *mapped = Some(body.start + raw_offset);
            }
        }
    }
    assert!(decoded.next().is_none(), "decoded value gained a character");
    for (index, offset) in samples.into_iter().enumerate() {
        if index != 0 && samples[index - 1] == offset {
            continue;
        }
        if !value.is_char_boundary(offset) {
            assert_eq!(
                entry.decoded_offset_to_raw(offset),
                Err(PositionError::InvalidUtf8Boundary)
            );
        } else {
            let raw = if offset == value.len() {
                body.end
            } else {
                expected[index].unwrap()
            };
            assert_eq!(entry.decoded_offset_to_raw(offset), Ok(raw));
        }
    }
}

fn check_references(source: &str, entry: Entry<'_>) {
    let references = entry.references();
    let value_span = entry.value_span();
    let mut previous_start = 0;
    for reference in &references {
        assert!(reference.span.start >= previous_start);
        previous_start = reference.span.start;
        assert!(reference.span.start < reference.span.end);
        check_span(source, reference.span.clone());
        assert!(source.is_char_boundary(reference.span.start));
        assert!(source.is_char_boundary(reference.span.end));
        if let Some(value_span) = &value_span {
            assert!(reference.span.start >= value_span.start);
            assert!(reference.span.end <= value_span.end);
        }
        for span in [
            reference.name.as_ref(),
            reference.operator_span.as_ref(),
            reference.argument.as_ref(),
            reference.command.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            check_span(source, span.clone());
            assert!(span.start >= reference.span.start);
            assert!(span.end <= reference.span.end);
            assert!(source.is_char_boundary(span.start));
            assert!(source.is_char_boundary(span.end));
        }
        assert_eq!(
            reference.operator.is_some(),
            reference.operator_span.is_some()
        );
        assert_eq!(reference.operator.is_some(), reference.argument.is_some());
        assert_eq!(
            reference.kind == ReferenceKind::Command,
            reference.command.is_some()
        );
        assert_eq!(
            reference.kind == ReferenceKind::Variable,
            reference.name.is_some()
        );
        assert_eq!(reference.containers.len(), reference.depth);
        if reference.kind == ReferenceKind::Variable && !reference.braced {
            assert!(!reference.name.as_ref().unwrap().is_empty());
        }
        for rules in [
            ReferenceRules::NONE,
            ReferenceRules::PORTABLE,
            ReferenceRules::COMPOSE,
            ReferenceRules::DOTENV_EXPAND,
        ] {
            let _ = reference.is_eligible(rules);
        }
    }
    if value_span.is_none() {
        assert!(references.is_empty());
    }
}

fn check_selected(source: &str, dialect: Dialect) {
    let (doc, diagnostics) = parse_with(source, dialect);
    let mut end = doc.bom_span().map_or(0, |span| span.end);
    for item in doc.items() {
        let span = item.span();
        check_span(source, span.clone());
        assert_eq!(span.start, end);
        assert!(span.end > span.start);
        end = span.end;
        if let Some(entry) = item.entry() {
            let t = entry.tokens();
            let mut cursor = span.start;
            for token in [
                Some(&t.indent),
                t.export.as_ref(),
                Some(&t.space_after_export),
                Some(&t.key),
                Some(&t.space_before_eq),
                t.eq.as_ref(),
                Some(&t.space_after_eq),
                t.value.as_ref(),
                t.quoted_tail.as_ref(),
                Some(&t.trailing_space),
                t.inline_comment.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                check_span(source, token.clone());
                assert_eq!(token.start, cursor);
                cursor = token.end;
            }
            assert_eq!(cursor, item.content_span().end);
            assert_eq!(&source[entry.key_span()], entry.key());
            if let Some(value) = entry.value() {
                let token = entry.value_span().unwrap();
                let mut previous = token.start;
                for offset in [0, value.len() / 2, value.len()] {
                    if !value.is_char_boundary(offset) {
                        assert_eq!(
                            entry.decoded_offset_to_raw(offset),
                            Err(PositionError::InvalidUtf8Boundary)
                        );
                        continue;
                    }
                    let raw = entry.decoded_offset_to_raw(offset).unwrap();
                    assert!(source.is_char_boundary(raw));
                    assert!(previous <= raw && raw <= token.end);
                    previous = raw;
                }
            }
            check_references(source, entry);
        }
    }
    assert_eq!(end, source.len());
    for diagnostic in &diagnostics {
        check_span(source, diagnostic.span());
        if let Some(span) = diagnostic.secondary_span() {
            check_span(source, span);
        }
    }
    assert!(diagnostics
        .windows(2)
        .all(|pair| pair[0].span().start <= pair[1].span().start));
    let first = diagnostics
        .iter()
        .find(|d| d.severity() == Severity::Error)
        .cloned();
    assert_eq!(
        parse_strict_with(source, dialect),
        first.map_or(Ok(doc.clone()), Err)
    );
    let (owned, owned_diagnostics) = parse_owned_with(source.to_owned(), dialect);
    assert_eq!(owned_diagnostics, diagnostics);
    assert_eq!(owned, doc.into_owned());
}

fn check_portability(source: &str, document: &Document<'_>, dialects: &[Dialect]) {
    let report = document.portability(dialects);
    let line_count = document.line_index().line_count();
    let mut unique = Vec::new();
    unique.push(document.dialect());
    for dialect in dialects {
        if !unique.contains(dialect) {
            unique.push(*dialect);
        }
    }
    let mut previous_line = 0;
    for difference in report.differences() {
        assert!(difference.first_line >= 1);
        assert_eq!(difference.last_line, difference.first_line);
        assert!(difference.last_line <= line_count);
        assert!(difference.first_line > previous_line);
        previous_line = difference.first_line;
        check_span(source, difference.span.clone());
        assert!(source.is_char_boundary(difference.span.start));
        assert!(source.is_char_boundary(difference.span.end));
        assert_eq!(difference.readings.len(), unique.len());
        for (reading, expected) in difference.readings.iter().zip(&unique) {
            assert_eq!(reading.dialect, *expected);
        }
    }
    let reversed: Vec<_> = dialects.iter().rev().copied().collect();
    let reversed_report = document.portability(&reversed);
    let lines: Vec<_> = report
        .differences()
        .iter()
        .map(|difference| (difference.first_line, difference.span.clone()))
        .collect();
    let reversed_lines: Vec<_> = reversed_report
        .differences()
        .iter()
        .map(|difference| (difference.first_line, difference.span.clone()))
        .collect();
    assert_eq!(lines, reversed_lines);
}

fuzz_target!(|bytes: &[u8]| {
    let Ok(source) = std::str::from_utf8(bytes) else {
        return;
    };
    let presets = [
        Dialect::PORTABLE,
        Dialect::NODE_DOTENV,
        Dialect::PYTHON_DOTENV,
        Dialect::COMPOSE,
        Dialect::DOCKER_RAW,
    ];
    for dialect in presets {
        check_selected(source, dialect);
    }
    let selector = bytes.first().copied().unwrap_or(0);
    let mut custom = presets[usize::from(selector) % presets.len()];
    custom.bom = if selector & 8 != 0 {
        BomPolicy::Error
    } else {
        BomPolicy::Keep
    };
    custom.bare_keys = if selector & 16 != 0 {
        BareKeys::Empty
    } else {
        BareKeys::Error
    };
    custom.quoted_tail = if selector & 32 != 0 {
        QuotedTail::Append
    } else {
        QuotedTail::Ignore
    };
    custom.raw_values = selector & 64 != 0;
    custom.normalize_newlines = selector & 128 != 0;
    custom.line_byte_limit = if selector & 4 != 0 {
        Some(bytes.len() % 32)
    } else {
        None
    };
    check_selected(source, custom);
    let (document, diagnostics) = parse(source);
    assert_eq!(document.source(), source);
    assert_eq!(document.to_string(), source);

    let mut end = document.bom_span().map_or(0, |span| span.end);
    for item in document.items() {
        let span = item.span();
        check_span(source, span.clone());
        assert_eq!(span.start, end);
        assert_eq!(item.raw(), &source[span.clone()]);
        end = span.end;
    }
    assert_eq!(end, source.len());

    for entry in document.entries() {
        let item = document.item(entry.item_id()).unwrap();
        let item_span = item.span();
        assert_eq!(item.entry().unwrap().id(), entry.id());
        assert_eq!(&source[entry.key_span()], entry.key());
        let tokens = entry.tokens();
        let spans = [
            Some(tokens.indent.clone()),
            tokens.export.clone(),
            Some(tokens.space_after_export.clone()),
            Some(tokens.key.clone()),
            Some(tokens.space_before_eq.clone()),
            tokens.eq.clone(),
            Some(tokens.space_after_eq.clone()),
            tokens.value.clone(),
            tokens.body.clone(),
            Some(tokens.trailing_space.clone()),
            tokens.inline_comment.clone(),
        ];
        for span in spans.into_iter().flatten() {
            check_span(source, span.clone());
            assert!(span.start >= item_span.start && span.end <= item_span.end);
        }
        check_tokens(source, entry, item, tokens);
        check_mapping(source, entry);
        check_references(source, entry);
    }

    for diagnostic in &diagnostics {
        check_span(source, diagnostic.span());
        if let Some(span) = diagnostic.secondary_span() {
            check_span(source, span);
        }
    }
    match parse_strict(source) {
        Ok(strict) => {
            assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
            assert_eq!(strict.source(), source);
        }
        Err(error) => {
            let first = diagnostics
                .iter()
                .find(|d| d.severity() == Severity::Error)
                .unwrap();
            assert_eq!(error.code(), first.code());
            assert_eq!(error.span(), first.span());
        }
    }

    check_portability(source, &document, &presets);
    check_portability(source, &document, &[custom]);
    assert!(document.portability(&[]).is_empty());

    let index = document.line_index();
    for offset in [0, source.len() / 2, source.len()] {
        for encoding in [
            ColumnEncoding::Utf8,
            ColumnEncoding::Scalar,
            ColumnEncoding::Utf16,
        ] {
            let error = if offset > 0
                && source.as_bytes()[offset - 1] == b'\r'
                && source.as_bytes().get(offset) == Some(&b'\n')
            {
                Some(PositionError::InsideCrLf)
            } else if encoding != ColumnEncoding::Utf8 && !source.is_char_boundary(offset) {
                Some(PositionError::InvalidUtf8Boundary)
            } else {
                None
            };
            match error {
                Some(error) => assert_eq!(index.to_position(offset, encoding), Err(error)),
                None => {
                    let position = index.to_position(offset, encoding).unwrap();
                    assert_eq!(index.to_offset(position, encoding), Ok(offset));
                }
            }
        }
    }
    let (owned, owned_diagnostics) = parse_owned(source.to_owned());
    assert_eq!(owned, document.into_owned());
    assert_eq!(owned_diagnostics, diagnostics);
});
