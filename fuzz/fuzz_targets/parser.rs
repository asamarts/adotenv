#![no_main]
#![forbid(unsafe_code)]

use std::ops::Range;

use adotenv::{parse, parse_owned, parse_strict, ColumnEncoding, Severity};
use libfuzzer_sys::fuzz_target;

fn check_span(source: &str, span: Range<usize>) {
    assert!(span.start <= span.end);
    assert!(source.get(span).is_some());
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
        if let Some(value) = entry.value() {
            for offset in [0, value.len() / 2, value.len()] {
                if value.is_char_boundary(offset) {
                    let raw = entry.decoded_offset_to_raw(offset).unwrap();
                    assert!(source.is_char_boundary(raw));
                    let body = tokens.body.as_ref().unwrap();
                    assert!(raw >= body.start && raw <= body.end);
                }
            }
        }
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
            if let Ok(position) = index.to_position(offset, encoding) {
                assert_eq!(index.to_offset(position, encoding).unwrap(), offset);
            }
        }
    }
    let (owned, owned_diagnostics) = parse_owned(source.to_owned());
    assert_eq!(owned, document.into_owned());
    assert_eq!(owned_diagnostics, diagnostics);
});
