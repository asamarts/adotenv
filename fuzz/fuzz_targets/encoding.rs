#![no_main]
#![forbid(unsafe_code)]

use adotenv::{
    encode_value, encode_value_with, parse_with, Dialect, DoubleEscapes, InlineComments, ItemKind,
    LineBreaks, Multiline, QuotedTail, Severity, SingleEscapes, UnknownEscapes,
};
use libfuzzer_sys::fuzz_target;

const PRESETS: [Dialect; 5] = [
    Dialect::PORTABLE,
    Dialect::NODE_DOTENV,
    Dialect::PYTHON_DOTENV,
    Dialect::COMPOSE,
    Dialect::DOCKER_RAW,
];

fn check_token(token: &str, value: &str, dialect: Dialect) {
    for ending in ["\n", ""] {
        let source = format!("A={token}{ending}");
        let (document, diagnostics) = parse_with(&source, dialect);
        assert!(!diagnostics.iter().any(|d| d.severity() == Severity::Error));
        assert_eq!(document.entries().len(), 1);
        let entry = document.entries().next().unwrap();
        assert_eq!(entry.key(), "A");
        assert_eq!(entry.value(), Some(value));
        assert_eq!(document.get("A"), Some(value));
    }
}

fn require_encoding(value: &str, dialect: Dialect) {
    let token = encode_value_with(value, dialect).unwrap_or_else(|error| {
        panic!("representable value={value:?}, dialect={dialect:?}: {error:?}")
    });
    check_token(&token, value, dialect);
}

fn check_inverse(token: &str, dialect: Dialect) {
    let source = format!("A={token}\n");
    let (document, diagnostics) = parse_with(&source, dialect);
    if diagnostics.iter().any(|d| d.severity() == Severity::Error)
        || document.entries().len() != 1
        || document
            .items()
            .any(|item| matches!(item.kind(), ItemKind::Invalid | ItemKind::Ignored))
    {
        return;
    }
    let entry = document.entries().next().unwrap();
    if entry.key() == "A" {
        if let Some(value) = entry.value() {
            // Unlike conditional round trips this rejects false negatives: a
            // legal public-parser token is a witness that the value exists.
            require_encoding(value, dialect);
        }
    }
}

fn dialect_for(header: &[u8]) -> Dialect {
    let mut dialect = PRESETS[usize::from(header[0]) % PRESETS.len()];
    if header[0] & 0x40 == 0 {
        return dialect;
    }
    let flags = [
        DoubleEscapes::LF,
        DoubleEscapes::CR,
        DoubleEscapes::BACKSLASH,
        DoubleEscapes::DOUBLE_QUOTE,
        DoubleEscapes::OCTAL,
        DoubleEscapes::TAB,
        DoubleEscapes::DOLLAR,
        DoubleEscapes::SINGLE_QUOTE,
    ];
    dialect.double_escapes = DoubleEscapes::NONE;
    for (bit, flag) in flags.into_iter().enumerate() {
        if header[1] & (1 << bit) != 0 {
            dialect.double_escapes = dialect.double_escapes.union(flag);
        }
    }
    dialect.unknown_escapes = match header[2] & 3 {
        0 => UnknownEscapes::Preserve,
        1 => UnknownEscapes::Warn,
        2 => UnknownEscapes::DropBackslash,
        _ => UnknownEscapes::Error,
    };
    dialect.single_escapes = match ((header[2] >> 2) & 3) % 3 {
        0 => SingleEscapes::None,
        1 => SingleEscapes::Quote,
        _ => SingleEscapes::QuoteAndBackslash,
    };
    dialect.quoted_tail = match (header[2] >> 4) % 6 {
        0 => QuotedTail::Error,
        1 => QuotedTail::Ignore,
        2 => QuotedTail::Append,
        3 => QuotedTail::LiteralFallback,
        4 => QuotedTail::Skip,
        _ => QuotedTail::NextStatement,
    };
    dialect.multiline = Multiline {
        single: header[3] & 1 != 0,
        double: header[3] & 2 != 0,
        backtick: header[3] & 4 != 0,
    };
    dialect.backticks = header[3] & 8 != 0;
    dialect.normalize_newlines = header[3] & 16 != 0;
    dialect.line_breaks = if header[3] & 32 == 0 {
        LineBreaks::Universal
    } else {
        LineBreaks::Lf
    };
    if header[3] & 64 != 0 {
        dialect.line_byte_limit = Some(2 + usize::from(header[1] & 0x7f));
    }
    if header[3] & 128 != 0 {
        dialect.inline_comments = InlineComments::AnyHash;
    }
    dialect
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    // Bound work independently of the engine's max_len. Lossy conversion still
    // explores arbitrary valid UTF-8 scalars and every ASCII control byte.
    let payload = &data[4..data.len().min(516)];
    let value = String::from_utf8_lossy(payload);
    for dialect in [Dialect::PORTABLE, Dialect::PYTHON_DOTENV, Dialect::COMPOSE] {
        require_encoding(&value, dialect);
    }
    let legacy = encode_value(&value);
    if value
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\r' | '\n' | '\t'))
    {
        assert!(legacy.is_none());
    } else {
        check_token(
            &legacy.expect("legacy-compatible Portable value"),
            &value,
            Dialect::PORTABLE,
        );
    }
    let dialect = dialect_for(&data[..4]);
    if let Ok(token) = encode_value_with(&value, dialect) {
        check_token(&token, &value, dialect);
    }
    for token in [
        value.to_string(),
        format!("'{value}'"),
        format!("\"{value}\""),
        format!("`{value}`"),
        format!("'#'{value}"),
        format!("\"#\"{value}"),
    ] {
        check_inverse(&token, dialect);
    }
});
