use alloc::{string::String, vec::Vec};
use core::ops::Range;

use super::{line_breaks, Parser};
use crate::dialect::{Dialect, DoubleEscapes, QuotedTail, SingleEscapes, Syntax, UnknownEscapes};
use crate::model::DecodeSegment;
use crate::{Diagnostic, DiagnosticCode, Quote};

/// Lazily materialize only changed values. Every replacement is recorded in the
/// original UTF-8 coordinate space; unchanged runs remain implicit in the map.
struct Output<'a> {
    source: &'a str,
    start: usize,
    copied: usize,
    value: Option<String>,
    map: Vec<DecodeSegment>,
}
impl<'a> Output<'a> {
    fn new(source: &'a str, body: &Range<usize>) -> Self {
        Self {
            source,
            start: body.start,
            copied: body.start,
            value: None,
            map: Vec::new(),
        }
    }
    fn replace(&mut self, raw: Range<usize>, cooked: &str) {
        let output = self
            .value
            .get_or_insert_with(|| String::with_capacity(raw.end - self.start));
        output.push_str(&self.source[self.copied..raw.start]);
        let begin = output.len();
        output.push_str(cooked);
        // A removed delimiter anchors the following decoded byte after it.
        let mapped = if cooked.is_empty() {
            raw.end..raw.end
        } else {
            raw.clone()
        };
        self.map.push(DecodeSegment {
            decoded: begin..output.len(),
            raw: mapped,
        });
        self.copied = raw.end;
    }
    fn finish(
        mut self,
        body: &Range<usize>,
        tail: Option<&Range<usize>>,
    ) -> (Option<String>, Vec<DecodeSegment>) {
        if let Some(value) = &mut self.value {
            value.push_str(&self.source[self.copied..body.end]);
        }
        if let Some(tail) = tail.filter(|tail| !tail.is_empty()) {
            let value = self
                .value
                .get_or_insert_with(|| String::from(&self.source[body.clone()]));
            let first = self.source[tail.clone()]
                .chars()
                .next()
                .expect("nonempty tail");
            let begin = value.len();
            value.push_str(&self.source[tail.clone()]);
            // Anchor the first tail character across the omitted closing quote.
            self.map.push(DecodeSegment {
                decoded: begin..begin + first.len_utf8(),
                raw: tail.start..tail.start + first.len_utf8(),
            });
        }
        (self.value, self.map)
    }

    fn replace_octal(&mut self, at: usize, body_end: usize) -> usize {
        let mut end = at + 2;
        let bytes = self.source.as_bytes();
        while end < body_end && end < at + 5 && bytes[end].is_ascii_digit() {
            end += 1;
        }
        let digits = &self.source[at + 2..end];
        let octal = if digits.len() == 3 && digits.bytes().all(|b| matches!(b, b'0'..=b'7')) {
            u16::from_str_radix(digits, 8)
                .ok()
                .filter(|value| *value <= 255)
        } else {
            None
        };
        if let Some(value) = octal {
            let mut buffer = [0; 4];
            self.replace(
                at..end,
                char::from_u32(u32::from(value))
                    .expect("byte scalar")
                    .encode_utf8(&mut buffer),
            );
        } else {
            // Only the failed unquote's leading zero is removed. Keep every
            // remaining character boundary mapped to its original source.
            self.replace(at + 1..at + 2, "");
        }
        end
    }
}

struct Position {
    at: usize,
    line: usize,
}

struct NodeUnquote {
    backticks: bool,
    close: Option<usize>,
    last_closes: [Option<usize>; 3],
}

impl NodeUnquote {
    fn new(source: &str, body: &Range<usize>, dialect: &Dialect, quote: Quote) -> Option<Self> {
        if dialect.syntax != Syntax::Node || quote != Quote::None || dialect.raw_values {
            return None;
        }
        let mut unquote = Self {
            backticks: dialect.backticks,
            close: None,
            last_closes: [None; 3],
        };
        // Node's outer-unquote regex is multiline with greedy [\s\S]*. Only
        // LS/PS can occur as internal anchors in a physical unquoted value.
        for (relative, c) in source[body.clone()].char_indices() {
            let Some(kind) = unquote.quote_kind(c) else {
                continue;
            };
            let position = body.start + relative;
            if position + c.len_utf8() == body.end
                || source[position + c.len_utf8()..]
                    .chars()
                    .next()
                    .is_some_and(|next| matches!(next, '\u{2028}' | '\u{2029}'))
            {
                unquote.last_closes[kind] = Some(position);
            }
        }
        Some(unquote)
    }

    fn quote_kind(&self, c: char) -> Option<usize> {
        match c {
            '\'' => Some(0),
            '"' => Some(1),
            '`' if self.backticks => Some(2),
            _ => None,
        }
    }

    fn removes(&mut self, source: &str, start: usize, at: usize, c: char) -> bool {
        if self.close == Some(at) {
            self.close = None;
            return true;
        }
        let anchor = at == start
            || source[..at]
                .chars()
                .next_back()
                .is_some_and(|previous| matches!(previous, '\u{2028}' | '\u{2029}'));
        if self.close.is_none() && anchor {
            if let Some(kind) = self.quote_kind(c) {
                if let Some(close) = self.last_closes[kind].filter(|close| *close > at) {
                    self.close = Some(close);
                    return true;
                }
            }
        }
        false
    }
}

fn selected_escape(next: char, dialect: &Dialect, double: bool) -> Option<char> {
    if !double {
        return match dialect.single_escapes {
            SingleEscapes::Quote if next == '\'' => Some('\''),
            SingleEscapes::QuoteAndBackslash if matches!(next, '\'' | '\\') => Some(next),
            _ => None,
        };
    }
    let candidate = match next {
        'n' => Some((DoubleEscapes::LF, '\n')),
        'r' => Some((DoubleEscapes::CR, '\r')),
        't' => Some((DoubleEscapes::TAB, '\t')),
        '\\' => Some((DoubleEscapes::BACKSLASH, '\\')),
        '"' => Some((DoubleEscapes::DOUBLE_QUOTE, '"')),
        '$' => Some((DoubleEscapes::DOLLAR, '$')),
        '\'' => Some((DoubleEscapes::SINGLE_QUOTE, '\'')),
        'a' => Some((DoubleEscapes::BELL, '\u{7}')),
        'b' => Some((DoubleEscapes::BACKSPACE, '\u{8}')),
        'f' => Some((DoubleEscapes::FORM_FEED, '\u{c}')),
        'v' => Some((DoubleEscapes::VERTICAL_TAB, '\u{b}')),
        _ => None,
    };
    candidate
        .filter(|(flag, _)| dialect.double_escapes.contains(*flag))
        .map(|(_, c)| c)
}

impl<const D: bool> Parser<'_, D> {
    pub(super) fn decode(
        &mut self,
        body: Range<usize>,
        quote: Quote,
        tail: Option<&Range<usize>>,
        item_start: usize,
    ) -> (Option<String>, Vec<DecodeSegment>, Option<Diagnostic>) {
        let dialect = self.dialect();
        let source = self.source;
        let bytes = source.as_bytes();
        let mut output = Output::new(source, &body);
        let mut error = None;
        let mut position = Position {
            at: body.start,
            line: self.line + line_breaks(&source[item_start..body.start]),
        };
        let node_literal_double = dialect.syntax == Syntax::Node
            && !dialect.raw_values
            && quote == Quote::None
            && bytes.get(body.start) == Some(&b'"');
        let double = quote == Quote::Double || node_literal_double;
        let mut node = NodeUnquote::new(source, &body, &dialect, quote);
        while position.at < body.end {
            let at = position.at;
            let c = source[at..body.end].chars().next().expect("body character");
            if node
                .as_mut()
                .is_some_and(|node| node.removes(source, body.start, at, c))
            {
                output.replace(at..at + c.len_utf8(), "");
                position.at += c.len_utf8();
                continue;
            }
            if dialect.normalize_newlines && c == '\r' {
                let end = at
                    + if bytes.get(at + 1) == Some(&b'\n') && at + 1 < body.end {
                        2
                    } else {
                        1
                    };
                output.replace(at..end, "\n");
                position.at = end;
                position.line += 1;
                continue;
            }
            if c == '\\'
                && at + 1 < body.end
                && !dialect.raw_values
                && (double || quote == Quote::Single)
            {
                let diagnostic = self.decode_escape(
                    &mut output,
                    &mut position,
                    body.end,
                    quote,
                    double,
                    &dialect,
                );
                if error.is_none() {
                    error = diagnostic;
                }
                continue;
            }
            if D && dialect.syntax == Syntax::Portable
                && quote == Quote::None
                && !dialect.raw_values
            {
                let code = match c {
                    '#' => Some(DiagnosticCode::TightHash),
                    '`' => Some(DiagnosticCode::Backtick),
                    _ => None,
                };
                if let Some(code) = code {
                    self.diagnostics.push(Diagnostic::new(
                        code,
                        at..at + c.len_utf8(),
                        position.line,
                    ));
                }
            }
            if c == '\r' || (c == '\n' && (at == 0 || bytes[at - 1] != b'\r')) {
                position.line += 1;
            }
            position.at += c.len_utf8();
        }
        let (decoded, map) = output.finish(
            &body,
            tail.filter(|_| dialect.quoted_tail == QuotedTail::Append),
        );
        (decoded, map, error)
    }

    fn decode_escape(
        &mut self,
        output: &mut Output<'_>,
        position: &mut Position,
        body_end: usize,
        quote: Quote,
        double: bool,
        dialect: &Dialect,
    ) -> Option<Diagnostic> {
        let source = self.source;
        let at = position.at;
        let next = source[at + 1..body_end]
            .chars()
            .next()
            .expect("escape character");
        if double && next == '0' && dialect.double_escapes.contains(DoubleEscapes::OCTAL) {
            position.at = output.replace_octal(at, body_end);
            return None;
        }
        let end = at + 1 + next.len_utf8();
        let cooked = selected_escape(next, dialect, double);
        let mut error = None;
        if let Some(cooked) = cooked {
            let mut buffer = [0; 4];
            output.replace(at..end, cooked.encode_utf8(&mut buffer));
        } else if double {
            match dialect.unknown_escapes {
                UnknownEscapes::Warn if D => self.diagnostics.push(Diagnostic::new(
                    DiagnosticCode::UnknownEscape,
                    at..end,
                    position.line,
                )),
                UnknownEscapes::Preserve | UnknownEscapes::Warn => {}
                UnknownEscapes::DropBackslash => {
                    if dialect.normalize_newlines && next == '\r' {
                        output.replace(at..at + 1, "");
                    } else {
                        output.replace(at..end, &source[at + 1..end]);
                    }
                }
                UnknownEscapes::Error => {
                    let diagnostic =
                        Diagnostic::new(DiagnosticCode::InvalidEscape, at..end, position.line);
                    if D {
                        self.diagnostics.push(diagnostic.clone());
                    }
                    error = Some(diagnostic);
                }
            }
        }
        // Node's replacements can start at the second backslash of an
        // unknown pair, unlike the pair-consuming Portable/Python rules.
        if cooked.is_none()
            && (quote == Quote::Single
                || (dialect.syntax == Syntax::Node
                    && dialect.unknown_escapes != UnknownEscapes::DropBackslash)
                || (dialect.normalize_newlines && next == '\r'))
        {
            position.at += 1;
        } else {
            position.line += line_breaks(&source[at..end]);
            position.at = end;
        }
        error
    }
}
