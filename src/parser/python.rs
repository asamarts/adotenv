//! Python dotenv's literal stream binding grammar.

use super::{is_space, line_breaks, skip_whitespace, valid_key_in, Parser, PhysicalLine};
use crate::{
    ColonSeparator, DiagnosticCode, Dialect, EntryTokens, ItemKind, KeySyntax, LineEnding, Quote,
};

/// Scan a binding without translating offsets away from the original UTF-8 input.
pub(super) fn statement<const D: bool>(
    parser: &mut Parser<'_, D>,
    start: usize,
    _first: PhysicalLine,
) -> usize {
    let source = parser.source;
    let dialect = parser.dialect();
    // Unlike the horizontal whitespace around `=`, the reader's binding prefix
    // consumes newlines too. Keep that entire prefix in the following item.
    let mut at = skip_whitespace(source, start, source.len(), dialect.whitespace);
    let indent = start..at;
    if at == source.len() {
        parser.plain_item(ItemKind::Blank, start..at, final_ending(source, at), None);
        return at;
    }
    let physical = parser.physical(at);
    if at == physical.content_end {
        parser.plain_item(ItemKind::Blank, start..physical.end, physical.ending, None);
        return physical.end;
    }
    if source.as_bytes()[at] == b'#' {
        parser.plain_item(
            ItemKind::Comment,
            start..physical.end,
            physical.ending,
            Some(at + 1..physical.content_end),
        );
        return physical.end;
    }
    let export = if dialect.export
        && source[at..physical.content_end].starts_with("export")
        && source[at + 6..physical.content_end]
            .chars()
            .next()
            .is_some_and(|ch| is_space(ch, dialect.whitespace))
    {
        let range = at..at + 6;
        at += 6;
        Some(range)
    } else {
        None
    };
    let export_end = at;
    if export.is_some() {
        at = skip_whitespace(source, at, physical.content_end, dialect.whitespace);
    }
    // Python accepts `export # comment` as a comment-only binding.
    if source.as_bytes().get(at) == Some(&b'#') && at < physical.content_end {
        parser.plain_item(
            ItemKind::Comment,
            start..physical.end,
            physical.ending,
            Some(at + 1..physical.content_end),
        );
        return physical.end;
    }
    let key_start = at;
    let quoted = dialect.quoted_keys && source.as_bytes().get(at) == Some(&b'\'');
    let (key_end, physical) = match scan_key(parser, start, physical, key_start, quoted) {
        Ok(key) => key,
        Err(end) => return end,
    };
    let key = key_start..key_end;
    at = skip_whitespace(source, key_end, physical.content_end, dialect.whitespace);
    let mut tokens = EntryTokens {
        indent,
        export,
        space_after_export: export_end..key_start,
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
    if at == physical.content_end || parser.bare_comment_at(at, key_end) {
        tokens.inline_comment = (at < physical.content_end).then_some(at..physical.content_end);
        return parser.entry(start, physical, tokens, Quote::None);
    }
    if source.as_bytes()[at] != b'=' && !colon_at(source, at, dialect) {
        return parser.invalid(
            start,
            physical,
            if quoted {
                DiagnosticCode::TrailingText
            } else {
                DiagnosticCode::InvalidKey
            },
            at..physical.content_end,
            parser.line + line_breaks(&source[start..at]),
        );
    }
    tokens.eq = Some(at..at + 1);
    at += 1;
    let after_eq = at;
    at = skip_whitespace(source, at, physical.content_end, dialect.whitespace);
    tokens.space_after_eq = after_eq..at;
    parser.parse_value(start, physical, tokens)
}

fn scan_key<const D: bool>(
    parser: &mut Parser<'_, D>,
    start: usize,
    mut physical: PhysicalLine,
    key_start: usize,
    quoted: bool,
) -> Result<(usize, PhysicalLine), usize> {
    let source = parser.source;
    let dialect = parser.dialect();
    let mut at = key_start;
    if quoted {
        // The key regex is deliberately not the value quote scanner: there is
        // no escape processing, and apostrophe keys may span physical lines.
        let close = source[at + 1..].find('\'').map(|offset| at + 1 + offset);
        let Some(close) = close.filter(|&close| close > at + 1) else {
            return Err(parser.invalid(
                start,
                physical,
                DiagnosticCode::InvalidKey,
                at..at + 1,
                parser.line + line_breaks(&source[start..at]),
            ));
        };
        at = close + 1;
        physical = parser.physical(at);
    } else {
        at = key_end(source, at, physical.content_end, dialect);
        if !valid_key_in(&source[key_start..at], dialect) {
            return Err(parser.invalid(
                start,
                physical,
                DiagnosticCode::InvalidKey,
                key_start..at,
                parser.line + line_breaks(&source[start..key_start]),
            ));
        }
    }
    Ok((at, physical))
}

/// Key recognition follows the selected key policy, not the statement family.
/// Delimiter whitespace is scanned separately after this maximal key token.
pub(super) fn key_end(source: &str, start: usize, end: usize, dialect: Dialect) -> usize {
    let mut at = start;
    for ch in source[start..end].chars() {
        if ch == '=' || colon_at(source, at, dialect) {
            break;
        }
        let allowed = if dialect.keys == KeySyntax::Posix {
            ch.is_ascii_alphanumeric() || ch == '_'
        } else {
            let mut encoded = [0; 4];
            valid_key_in(ch.encode_utf8(&mut encoded), dialect)
        };
        if !allowed {
            break;
        }
        at += ch.len_utf8();
    }
    at
}

pub(super) fn colon_at(source: &str, at: usize, dialect: Dialect) -> bool {
    source.as_bytes().get(at) == Some(&b':')
        && match dialect.colon {
            ColonSeparator::None => false,
            ColonSeparator::Any => true,
            ColonSeparator::Whitespace => source[at + 1..]
                .chars()
                .next()
                .is_some_and(|ch| is_space(ch, dialect.whitespace)),
        }
}

pub(super) fn final_ending(source: &str, end: usize) -> LineEnding {
    if source[..end].ends_with("\r\n") {
        LineEnding::CrLf
    } else if source[..end].ends_with('\n') {
        LineEnding::Lf
    } else if source[..end].ends_with('\r') {
        LineEnding::Cr
    } else {
        LineEnding::None
    }
}
