#![no_main]
#![forbid(unsafe_code)]

use std::ops::Range;

use adotenv::{
    parse, parse_owned, parse_strict, AssignmentState, ColumnEncoding, Entry, EntryTokens, Item,
    PositionError, Quote, Severity,
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

fuzz_target!(|bytes: &[u8]| {
    let Ok(source) = std::str::from_utf8(bytes) else {
        return;
    };
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
