//! Node dotenv's default regular-expression grammar (not its opt-in scanner).

use super::python::{colon_at, key_end};
use super::{is_space, line_breaks, skip_whitespace, valid_key_in, Parser, PhysicalLine};
use crate::{ColonSeparator, DiagnosticCode, EntryTokens, ItemKind, LineEnding, Quote};
use core::ops::Range;

pub(super) fn statement<const D: bool>(
    parser: &mut Parser<'_, D>,
    start: usize,
    physical: PhysicalLine,
) -> usize {
    let source = parser.source;
    let dialect = parser.dialect();
    // The regex's leading \s* is greedy and may span physical lines, but the
    // blank lines it skips carry no meaning. Keep each one as a blank item so
    // that blank lines delimit groups as in the other presets. LS/PS are
    // regex anchors, so they count as blank even under a custom whitespace
    // policy that does not classify them as whitespace.
    let blank = source[start..physical.content_end]
        .chars()
        .all(|ch| is_space(ch, dialect.whitespace) || matches!(ch, '\u{2028}' | '\u{2029}'));
    if blank {
        return parser.plain_item(ItemKind::Blank, start..physical.end, physical.ending, None);
    }
    // The statement keeps the indentation on its own line.
    let at = skip_whitespace(source, start, physical.content_end, dialect.whitespace);
    let indent = start..at;
    let first = anchored_line(parser, at);
    if at == first.content_end {
        // An anchor that the whitespace policy does not skip ends this item.
        // Retain the separator and make progress.
        let end = at + source[at..].chars().next().map_or(0, char::len_utf8);
        return parser.plain_item(ItemKind::Blank, start..end, LineEnding::None, None);
    }
    if source.as_bytes()[at] == b'#' {
        return parser.plain_item(
            ItemKind::Comment,
            start..first.end,
            first.ending,
            Some(at + 1..first.content_end),
        );
    }
    // `export` is an optional regex group. If consuming it prevents the rest
    // from matching (e.g. `export =x`), the regex retries it as an ordinary key.
    let header = header(parser, at, true).or_else(|| header(parser, at, false));
    let Some(header) = header else {
        return parser.invalid(
            start,
            first,
            DiagnosticCode::InvalidKey,
            at..first.content_end,
            parser.line + line_breaks(&source[start..at]),
        );
    };
    let mut tokens = EntryTokens {
        indent,
        export: header.export,
        space_after_export: header.export_end..header.key.start,
        key: header.key,
        space_before_eq: header.key_end..header.delimiter,
        eq: header.eq.clone(),
        space_after_eq: header.delimiter..header.delimiter,
        value: None,
        body: None,
        quoted_tail: None,
        trailing_space: header.delimiter..header.delimiter,
        inline_comment: None,
    };
    let Some(eq) = header.eq else {
        let last = anchored_line(parser, header.key_end);
        tokens.inline_comment =
            (header.delimiter < last.content_end).then_some(header.delimiter..last.content_end);
        return parser.entry(start, last, tokens, Quote::None);
    };
    let after_eq = eq.end;
    let mut value_start = after_eq;
    // The colon arm is :\s+?, unlike the equals arm's \s*?. One whitespace
    // character is mandatory and may itself be a normalized CRLF/newline.
    if !dialect.raw_values
        && source.as_bytes()[eq.start] == b':'
        && dialect.colon == ColonSeparator::Whitespace
    {
        value_start += source[value_start..]
            .chars()
            .next()
            .map_or(0, char::len_utf8);
        if source.as_bytes().get(value_start.wrapping_sub(1)) == Some(&b'\r')
            && source.as_bytes().get(value_start) == Some(&b'\n')
        {
            value_start += 1;
        }
    }
    let mut value_line = parser.physical(value_start);
    if !dialect.raw_values {
        // Each quoted arm has its own greedy \s* prefix, which can reach a
        // quote on a subsequent physical line. The unquoted arm cannot start
        // at a newline, so do not consume it when no quoted arm can match.
        let quoted_start = skip_whitespace(source, value_start, source.len(), dialect.whitespace);
        let quote = source.as_bytes().get(quoted_start).copied();
        if matches!(quote, Some(b'\'' | b'"')) || (dialect.backticks && quote == Some(b'`')) {
            value_start = quoted_start;
            value_line = parser.physical(value_start);
        } else {
            value_start = skip_whitespace(
                source,
                value_start,
                value_line.content_end,
                dialect.whitespace,
            );
        }
    }
    tokens.space_after_eq = after_eq..value_start;
    tokens.trailing_space = value_start..value_start;
    parser.parse_value(start, value_line, tokens)
}

struct Header {
    export: Option<Range<usize>>,
    export_end: usize,
    key: Range<usize>,
    key_end: usize,
    delimiter: usize,
    eq: Option<Range<usize>>,
}

fn header<const D: bool>(
    parser: &Parser<'_, D>,
    start: usize,
    allow_export: bool,
) -> Option<Header> {
    let source = parser.source;
    let dialect = parser.dialect();
    let mut at = start;
    let export = if allow_export
        && dialect.export
        && source[at..].starts_with("export")
        && source[at + 6..]
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
        at = skip_whitespace(source, at, source.len(), dialect.whitespace);
    }
    let key_start = at;
    let key_line_end = anchored_line(parser, at).content_end;
    if dialect.quoted_keys && source.as_bytes().get(at) == Some(&b'\'') {
        let close = source[at + 1..].find('\'').map(|offset| at + 1 + offset)?;
        if close == at + 1 {
            return None;
        }
        at = close + 1;
    } else {
        at = key_end(source, at, key_line_end, dialect);
        if !valid_key_in(&source[key_start..at], dialect) {
            return None;
        }
    }
    let key_end = at;
    // Before `=` whitespace is greedy and can cross physical lines. A colon
    // must be adjacent to the key, just as in the regex's separate colon arm.
    let spaced = skip_whitespace(source, at, source.len(), dialect.whitespace);
    let delimiter = if source.as_bytes().get(spaced) == Some(&b'=') {
        spaced
    } else if colon_at(source, at, dialect) {
        at
    } else if dialect.colon == ColonSeparator::Any && colon_at(source, spaced, dialect) {
        spaced
    } else {
        // Retain bare declarations without letting a failed assignment swallow
        // whitespace or an unrelated statement on a subsequent physical line.
        let end = anchored_line(parser, at).content_end;
        let tail = skip_whitespace(source, at, end, dialect.whitespace);
        if tail != end && !parser.bare_comment_at(tail, key_end) {
            return None;
        }
        return Some(Header {
            export,
            export_end,
            key: key_start..key_end,
            key_end,
            delimiter: tail,
            eq: None,
        });
    };
    Some(Header {
        export,
        export_end,
        key: key_start..key_end,
        key_end,
        delimiter,
        eq: Some(delimiter..delimiter + 1),
    })
}

/// ECMAScript anchors include LS/PS; its unquoted value class does not exclude
/// them. Use this only for headers/recovery/comments, never unquoted values.
pub(super) fn anchored_line<const D: bool>(parser: &Parser<'_, D>, at: usize) -> PhysicalLine {
    let physical = parser.physical(at);
    match parser.source[at..physical.content_end].find(['\u{2028}', '\u{2029}']) {
        Some(relative) => PhysicalLine {
            content_end: at + relative,
            end: at + relative,
            ending: LineEnding::None,
        },
        None => physical,
    }
}
