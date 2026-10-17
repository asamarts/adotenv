//! Docker CLI 29.8.2 kvfile syntax, without bare-key environment lookup.

use super::{is_space, line_breaks, skip_whitespace, valid_key_in, Parser, PhysicalLine};
use crate::dialect::ColonSeparator;
use crate::{DiagnosticCode, EntryTokens, ItemKind, Quote};

pub(super) fn statement<const D: bool>(
    parser: &mut Parser<'_, D>,
    start: usize,
    first: PhysicalLine,
) -> usize {
    let source = parser.source;
    let dialect = parser.dialect();
    let bytes = source.as_bytes();
    let mut at = skip_whitespace(source, start, first.content_end, dialect.whitespace);
    let indent = start..at;
    if at == first.content_end {
        return parser.plain_item(ItemKind::Blank, start..first.end, first.ending, None);
    }
    if bytes[at] == b'#' {
        return parser.plain_item(
            ItemKind::Comment,
            start..first.end,
            first.ending,
            Some(at + 1..first.content_end),
        );
    }
    let export = if dialect.export
        && source[at..first.content_end].starts_with("export")
        && source[at + 6..first.content_end]
            .chars()
            .next()
            .is_some_and(|ch| is_space(ch, dialect.whitespace))
    {
        let span = at..at + 6;
        at += 6;
        Some(span)
    } else {
        None
    };
    let after_export = at;
    if export.is_some() {
        at = skip_whitespace(source, at, first.content_end, dialect.whitespace);
    }
    let space_after_export = after_export..at;
    let key_start = at;
    let ScannedKey {
        key,
        mut at,
        last,
        separator,
    } = match scan_key(parser, start, first, key_start) {
        Ok(key) => key,
        Err(end) => return end,
    };
    let key_end = key.end;
    let mut tokens = EntryTokens {
        indent,
        export,
        space_after_export,
        key,
        space_before_eq: key_end..at,
        eq: None,
        space_after_eq: at..at,
        value: None,
        body: None,
        quoted_tail: None,
        trailing_space: at..at,
        inline_comment: None,
    };
    if separator {
        tokens.eq = Some(at..at + 1);
        let after_eq = at + 1;
        at = if dialect.raw_values {
            after_eq
        } else {
            skip_whitespace(source, after_eq, last.content_end, dialect.whitespace)
        };
        tokens.space_after_eq = after_eq..at;
        return parser.parse_value(start, last, tokens);
    }
    parser.entry(start, last, tokens, Quote::None)
}

struct ScannedKey {
    key: core::ops::Range<usize>,
    at: usize,
    last: PhysicalLine,
    separator: bool,
}

fn scan_key<const D: bool>(
    parser: &mut Parser<'_, D>,
    start: usize,
    first: PhysicalLine,
    key_start: usize,
) -> Result<ScannedKey, usize> {
    let source = parser.source;
    let dialect = parser.dialect();
    let bytes = source.as_bytes();
    let mut at = key_start;
    let mut last = first;
    let quoted = dialect.quoted_keys && bytes.get(at) == Some(&b'\'');
    if quoted {
        let Some(relative) = source[at + 1..].find('\'') else {
            return Err(parser.invalid(
                start,
                first,
                DiagnosticCode::InvalidKey,
                at..first.content_end,
                parser.line,
            ));
        };
        at += relative + 2;
        last = parser.physical(at);
    } else {
        // No key trimming: only leading line whitespace is removed. In the
        // preset, literal space/tab are forbidden but NUL and internal Unicode
        // whitespace are accepted, despite the upstream package documentation.
        while at < last.content_end {
            let ch = source[at..].chars().next().expect("character boundary");
            let colon = ch == ':'
                && match dialect.colon {
                    ColonSeparator::None => false,
                    ColonSeparator::Any => true,
                    ColonSeparator::Whitespace => source[at + 1..last.content_end]
                        .chars()
                        .next()
                        .is_some_and(|next| is_space(next, dialect.whitespace)),
                };
            if ch == '=' || colon {
                break;
            }
            at += ch.len_utf8();
        }
    }
    let key = key_start..at;
    let key_end = at;
    if quoted {
        at = skip_whitespace(source, at, last.content_end, dialect.whitespace);
    }
    let separator = at < last.content_end
        && (bytes[at] == b'='
            || (bytes[at] == b':'
                && match dialect.colon {
                    ColonSeparator::None => false,
                    ColonSeparator::Any => true,
                    ColonSeparator::Whitespace => source[at + 1..last.content_end]
                        .chars()
                        .next()
                        .is_some_and(|next| is_space(next, dialect.whitespace)),
                }));
    let valid = if quoted {
        key_end > key_start + 2
    } else {
        valid_key_in(&source[key.clone()], dialect)
    };
    if !valid || (quoted && at < last.content_end && !separator) {
        return Err(parser.invalid(
            start,
            last,
            DiagnosticCode::InvalidKey,
            key,
            parser.line + line_breaks(&source[start..key_start]),
        ));
    }
    Ok(ScannedKey {
        key,
        at,
        last,
        separator,
    })
}
