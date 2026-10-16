//! Compose-go 2.16.1's literal statement stage, without interpolation or lookup.

use super::{
    is_space, line_breaks, skip_whitespace, trim_whitespace, valid_key_in, Parser, PhysicalLine,
};
use crate::dialect::{ColonSeparator, KeySyntax, LineBreaks, Whitespace};
use crate::{DiagnosticCode, EntryTokens, ItemKind, LineEnding, Quote};

pub(super) fn statement<const D: bool>(
    parser: &mut Parser<'_, D>,
    start: usize,
    first: PhysicalLine,
) -> usize {
    let source = parser.source;
    let dialect = parser.dialect();
    let bytes = source.as_bytes();
    // Compose's statement trim differs deliberately from its horizontal scanner.
    let trim_mode = if dialect.whitespace == Whitespace::Compose {
        Whitespace::Unicode
    } else {
        dialect.whitespace
    };
    let mut at = skip_whitespace(source, start, first.content_end, trim_mode);
    let indent = start..at;
    if at == first.content_end {
        parser.plain_item(ItemKind::Blank, start..first.end, first.ending, None);
        return first.end;
    }
    if bytes[at] == b'#' {
        parser.plain_item(
            ItemKind::Comment,
            start..first.end,
            first.ending,
            Some(at + 1..first.content_end),
        );
        return first.end;
    }
    let export = if dialect.export
        && source[at..first.content_end].starts_with("export")
        && source[at + 6..].chars().next().is_some_and(|ch| {
            if dialect.whitespace == Whitespace::Compose {
                matches!(ch, '\t' | '\n' | '\u{c}' | '\r' | ' ')
            } else {
                is_space(ch, dialect.whitespace)
            }
        }) {
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
        eof_quirk,
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
    // Upstream's EOF-only bare-key bug leaves key empty and reuses the source
    // as a value. Keep it in the Compose key grammar, not custom key policies.
    if eof_quirk {
        tokens.key = key_start..key_start;
        tokens.space_before_eq = key_start..key_start;
        tokens.space_after_eq = key_start..key_start;
        return parser.parse_value(start, last, tokens);
    }
    parser.entry(start, last, tokens, Quote::None)
}

struct ScannedKey {
    key: core::ops::Range<usize>,
    at: usize,
    last: PhysicalLine,
    separator: bool,
    eof_quirk: bool,
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
    let key_end = if quoted {
        at
    } else {
        trim_whitespace(source, key_start, at, dialect.whitespace)
    };
    let key = key_start..key_end;
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
    // With LF-only scanning, a final CR is trimmed data rather than an EOF
    // terminator. Universal scanning still treats CR as a real line break.
    let eof = last.ending == LineEnding::None
        || (dialect.line_breaks == LineBreaks::Lf && last.ending == LineEnding::Cr);
    let eof_quirk = !quoted
        && !separator
        && dialect.keys == KeySyntax::Compose
        && key_start < last.content_end
        && eof;
    let valid = if quoted {
        key_end > key_start + 2
    } else if eof_quirk {
        source[key.clone()]
            .split(' ')
            .all(|part| valid_key_in(part, dialect))
    } else {
        valid_key_in(&source[key.clone()], dialect)
    };
    if !valid
        || (eof && key_start == last.content_end)
        || (quoted && at < last.content_end && !separator)
    {
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
        eof_quirk,
    })
}
