//! Exact inverses of the configured literal grammar.

use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::fmt;

use crate::{
    Dialect, DoubleEscapes, LineBreaks, Quote, QuotedTail, SingleEscapes, Syntax, UnknownEscapes,
    Whitespace,
};

/// A value has no exact literal spelling in the selected grammar and context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodeError {
    /// No spelling preserves the value, statement boundaries and resource limits.
    Unrepresentable,
}
impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("value cannot be represented exactly in the selected dialect and context")
    }
}
impl core::error::Error for EncodeError {}

/// Encode an exact standalone value under an explicit literal dialect.
///
/// The returned token is verified in both `A=<token>\n` and `A=<token>`.
/// The canonical `A=` header consumes two bytes of the first physical line's
/// byte budget. A Node token may begin with a line break when a first-line cap
/// cannot fit the opening quote; that line break is trivia between `=` and the
/// value. This is not a context-safe edit or an interpolation guarantee.
/// LF-only raw grammars may require an extra terminal CR as syntax padding;
/// appending CRLF instead of LF requires context-aware native encoding.
///
/// # Errors
/// Returns [`EncodeError::Unrepresentable`] when no exact spelling is available.
pub fn encode_value_with(value: &str, dialect: Dialect) -> Result<String, EncodeError> {
    let mut context = Context::standalone(value, dialect);
    match encode(context.clone(), Quote::None) {
        Ok((token, _)) => return Ok(token),
        Err(EncodeError::Unrepresentable) => {}
    }
    // Node permits whitespace, including a physical line break, between `=`
    // and an opening quote. When a first-line cap cannot fit the opener, try
    // the same encoding with the opener on the next physical line. The prefix
    // is part of the returned standalone token; native edits remain context
    // bound and do not use this form.
    if dialect.syntax == Syntax::Node && dialect.line_byte_limit.is_some() {
        let mut before = String::with_capacity(context.before.len() + 1);
        before.push_str(context.before);
        before.push('\n');
        let moved = Context {
            value,
            dialect,
            before: &before,
            after: context.after,
            needs_eq: context.needs_eq,
            separate_comment: context.separate_comment,
            eof: context.eof,
            scratch: String::new(),
        };
        if let Ok((token, _)) = encode(moved, Quote::None) {
            let mut prefixed = String::with_capacity(token.len() + 1);
            prefixed.push('\n');
            prefixed.push_str(&token);
            if context.verify(&prefixed).is_some() {
                return Ok(prefixed);
            }
        }
    }
    Err(EncodeError::Unrepresentable)
}

/// Encode a standalone Portable value, retaining the original control policy.
///
/// Controls other than CR, LF and tab return `None`. Use [`encode_value_with`]
/// to encode every control the selected literal grammar can represent.
#[must_use]
pub fn encode_value(value: &str) -> Option<String> {
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return None;
    }
    let preferred = if !value.starts_with(['\'', '"'])
        && !value
            .chars()
            .any(|c| c.is_whitespace() || c == '#' || c.is_control())
    {
        Quote::None
    } else {
        Quote::Double
    };
    encode(Context::standalone(value, Dialect::PORTABLE), preferred)
        .ok()
        .map(|(token, _)| token)
}

pub(crate) fn encode_native(
    value: &str,
    preferred: Quote,
    dialect: Dialect,
    before: &str,
    after: &str,
    needs_eq: bool,
    separate_comment: bool,
) -> Result<(String, Quote), EncodeError> {
    encode(
        Context {
            value,
            dialect,
            before,
            after,
            needs_eq,
            separate_comment,
            eof: false,
            scratch: String::new(),
        },
        preferred,
    )
}

#[derive(Clone)]
struct Context<'a> {
    value: &'a str,
    dialect: Dialect,
    before: &'a str,
    after: &'a str,
    needs_eq: bool,
    separate_comment: bool,
    eof: bool,
    scratch: String,
}
impl<'a> Context<'a> {
    fn standalone(value: &'a str, dialect: Dialect) -> Self {
        Self {
            value,
            dialect,
            before: "A=",
            after: "\n",
            needs_eq: false,
            separate_comment: false,
            eof: true,
            scratch: String::new(),
        }
    }
    fn verify(&mut self, token: &str) -> Option<Quote> {
        self.scratch.clear();
        self.scratch.push_str(self.before);
        if self.needs_eq {
            self.scratch.push('=');
        }
        self.scratch.push_str(token);
        if self.separate_comment {
            self.scratch.push(' ');
        }
        self.scratch.push_str(self.after);
        let quote = if self.eof {
            let quote = exact(&self.scratch, self.value, self.dialect)?;
            self.scratch.pop();
            exact(&self.scratch, self.value, self.dialect)?;
            quote
        } else {
            let start = self.before.len() + usize::from(self.needs_eq);
            exact_native(
                &self.scratch,
                self.value,
                self.dialect,
                start..start + token.len(),
            )?
        };
        Some(quote)
    }
    fn initial_line(&self, quote: char) -> Option<usize> {
        let mut line = 0;
        advance_line(&mut line, self.before, self.dialect)?;
        if self.needs_eq {
            advance_line(&mut line, "=", self.dialect)?;
        }
        let mut buffer = [0; 4];
        advance_line(&mut line, quote.encode_utf8(&mut buffer), self.dialect)?;
        Some(line)
    }
}

fn exact(source: &str, value: &str, dialect: Dialect) -> Option<Quote> {
    let document = crate::parser::parse_document(source, dialect);
    if document.entries.len() != 1
        || document.items.iter().any(|item| {
            item.invalid
                .as_ref()
                .is_some_and(|diagnostic| diagnostic.severity() == crate::Severity::Error)
        })
    {
        return None;
    }
    let entry = document.entries().next()?;
    (entry.value() == Some(value)).then_some(entry.quote())
}

fn native_entry<'a>(document: &'a crate::Document<'_>, at: usize) -> Option<crate::Entry<'a>> {
    document
        .entries()
        .rev()
        .find(|entry| entry.tokens().eq.as_ref().is_some_and(|eq| eq.end <= at))
}

fn exact_native(
    source: &str,
    value: &str,
    dialect: Dialect,
    token: core::ops::Range<usize>,
) -> Option<Quote> {
    let document = crate::parser::parse_document(source, dialect);
    let entry = native_entry(&document, token.start)?;
    if entry.value() != Some(value)
        || document.entries().any(|other| {
            let start = other.tokens().key.start;
            start >= token.start && start < token.end
        })
        || document.items().any(|item| {
            let span = item.span();
            span.start < token.end
                && span.end > token.start
                && item
                    .diagnostic()
                    .is_some_and(|d| d.severity() == crate::Severity::Error)
        })
    {
        return None;
    }
    Some(entry.quote())
}

fn encode(mut context: Context<'_>, preferred: Quote) -> Result<(String, Quote), EncodeError> {
    if context.dialect.raw_values {
        return encode_raw(&mut context);
    }
    let styles = quote_styles(preferred);
    for (index, quote) in styles.iter().copied().enumerate() {
        if styles[..index].contains(&quote) {
            continue;
        }
        let encoded = if quote == Quote::None {
            encode_unquoted(&mut context)
        } else {
            encode_quoted(&mut context, quote)
        };
        if let Some(encoded) = encoded {
            return Ok(encoded);
        }
    }
    Err(EncodeError::Unrepresentable)
}

fn quote_styles(preferred: Quote) -> [Quote; 5] {
    // Keep a quoted boundary when its original style needs replacement.
    // Dropping all delimiters can absorb an unselected following statement.
    if preferred == Quote::None {
        [
            Quote::None,
            Quote::Double,
            Quote::Single,
            Quote::Backtick,
            Quote::None,
        ]
    } else {
        [
            preferred,
            Quote::Double,
            Quote::Single,
            Quote::Backtick,
            Quote::None,
        ]
    }
}

fn encode_unquoted(context: &mut Context<'_>) -> Option<(String, Quote)> {
    let value = context.value;
    let dialect = context.dialect;
    if let Some(encoded) = verify_with_cr_padding(context, value.to_string()) {
        return Some(encoded);
    }
    if dialect.syntax == Syntax::Node && value.starts_with('"') {
        let mut token = conventional(value, Quote::Double, dialect);
        token.pop();
        token.remove(0);
        if let Some(actual) = context.verify(&token) {
            return Some((token, actual));
        }
    }
    if dialect.syntax == Syntax::Node && dialect.quoted_tail == QuotedTail::LiteralFallback {
        if let Some(encoded) = encode_node_literal_fallback(context) {
            return Some(encoded);
        }
    }
    // LiteralFallback is deliberately tried as literal input, including
    // Node's outer-unquote and double-substitution behavior.
    if dialect.normalize_newlines && value.contains('\n') && dialect.line_breaks == LineBreaks::Lf {
        if let Some(encoded) = verify_with_cr_padding(context, value.replace('\n', "\r")) {
            return Some(encoded);
        }
    }
    literal_fallback_padding(context, value)
}

fn encode_node_literal_fallback(context: &mut Context<'_>) -> Option<(String, Quote)> {
    let value = context.value;
    let dialect = context.dialect;
    if let Some(suffix) = value.strip_prefix('"') {
        for scan_quotes in [true, false] {
            let mode = InverseMode {
                quote: Quote::Double,
                decoder: Quote::Double,
                append: false,
                closing: Closing::None,
                scan_quotes,
            };
            let Some(token) = inverse(context, suffix, mode) else {
                continue;
            };
            if let Some(encoded) = verify_with_cr_padding(context, token) {
                return Some(encoded);
            }
        }
    }
    for quote in [Quote::Single, Quote::Double, Quote::Backtick] {
        if quote == Quote::Backtick && !dialect.backticks {
            continue;
        }
        for (closing, scan_quotes) in [(Closing::Escaped, true), (Closing::Parsed, false)] {
            let mode = InverseMode {
                quote,
                decoder: if quote == Quote::Double {
                    Quote::Double
                } else {
                    Quote::Backtick
                },
                append: false,
                closing,
                scan_quotes,
            };
            if let Some(token) = inverse(context, value, mode) {
                if let Some(actual) = context.verify(&token) {
                    return Some((token, actual));
                }
            }
        }
    }
    None
}

fn encode_quoted(context: &mut Context<'_>, quote: Quote) -> Option<(String, Quote)> {
    let dialect = context.dialect;
    if quote == Quote::Backtick && !dialect.backticks {
        return None;
    }
    let value = context.value;
    let body = if dialect.quoted_tail == QuotedTail::Append {
        &value[..appended_body_end(context, quote).unwrap_or(value.len())]
    } else {
        value
    };
    let token = conventional(body, quote, dialect);
    if let Some(actual) = context.verify(&token) {
        return Some((token, actual));
    }
    for append in [false, true] {
        if append && dialect.quoted_tail != QuotedTail::Append {
            continue;
        }
        for closing in [Closing::Parsed, Closing::Escaped] {
            if matches!(closing, Closing::Escaped) && dialect.syntax != Syntax::Node {
                continue;
            }
            let mode = InverseMode {
                quote,
                decoder: quote,
                append,
                closing,
                scan_quotes: true,
            };
            if let Some(token) = inverse(context, body, mode) {
                if let Some(encoded) = verify_with_cr_padding(context, token) {
                    return Some(encoded);
                }
            }
        }
    }
    None
}

fn verify_with_cr_padding(context: &mut Context<'_>, mut token: String) -> Option<(String, Quote)> {
    if let Some(actual) = context.verify(&token) {
        return Some((token, actual));
    }
    if context.dialect.line_breaks == LineBreaks::Lf && token.ends_with('\r') {
        token.push('\r');
        if let Some(actual) = context.verify(&token) {
            return Some((token, actual));
        }
    }
    None
}

fn literal_fallback_padding(context: &mut Context<'_>, value: &str) -> Option<(String, Quote)> {
    if context.dialect.quoted_tail != QuotedTail::LiteralFallback {
        return None;
    }
    // A trailing comment can force quote-prefixed input through the configured
    // unquoted fallback without becoming part of its value. Candidate parsing
    // decides which comment delimiter form, if either, is valid.
    let mut token = String::with_capacity(value.len() + 2);
    token.push_str(value);
    token.push('#');
    if let Some(quote) = context.verify(&token) {
        return Some((token, quote));
    }
    token.pop();
    token.push_str(" #");
    context.verify(&token).map(|quote| (token, quote))
}

fn encode_raw(context: &mut Context<'_>) -> Result<(String, Quote), EncodeError> {
    let mut token = raw(context.value, context.dialect).ok_or(EncodeError::Unrepresentable)?;
    if let Some(quote) = context.verify(&token) {
        return Ok((token, quote));
    }
    // One terminal CR is physical-ending syntax in LF-only scanning, at EOF too.
    if context.dialect.line_breaks == LineBreaks::Lf {
        token.push('\r');
        if let Some(quote) = context.verify(&token) {
            return Ok((token, quote));
        }
    }
    Err(EncodeError::Unrepresentable)
}

fn appended_body_end(context: &mut Context<'_>, quote: Quote) -> Option<usize> {
    context.scratch.clear();
    context.scratch.push_str(context.before);
    if context.needs_eq {
        context.scratch.push('=');
    }
    context.scratch.push(delimiter(quote));
    context.scratch.push(delimiter(quote));
    if context.separate_comment {
        context.scratch.push(' ');
    }
    context.scratch.push_str(context.after);
    let document = crate::parser::parse_document(&context.scratch, context.dialect);
    let entry = if context.eof {
        (document.entries.len() == 1)
            .then(|| document.entries().next())
            .flatten()?
    } else {
        native_entry(
            &document,
            context.before.len() + usize::from(context.needs_eq),
        )?
    };
    if entry.quote() != quote {
        return None;
    }
    let tail = entry.value()?;
    context.value.strip_suffix(tail).map(str::len)
}

fn raw(value: &str, dialect: Dialect) -> Option<String> {
    if dialect.normalize_newlines {
        if value.contains('\r') {
            return None;
        }
        if value.contains('\n') {
            return (dialect.line_breaks == LineBreaks::Lf).then(|| value.replace('\n', "\r"));
        }
    }
    Some(value.to_string())
}

fn delimiter(quote: Quote) -> char {
    match quote {
        Quote::Single => '\'',
        Quote::Double => '"',
        Quote::Backtick => '`',
        Quote::None => unreachable!(),
    }
}

fn escape(c: char, dialect: Dialect, quote: Quote) -> Option<char> {
    if quote == Quote::Single {
        return match dialect.single_escapes {
            SingleEscapes::Quote if c == '\'' => Some('\''),
            SingleEscapes::QuoteAndBackslash if matches!(c, '\'' | '\\') => Some(c),
            _ => None,
        };
    }
    if quote != Quote::Double {
        return None;
    }
    let (flag, cooked) = match c {
        'n' => (DoubleEscapes::LF, '\n'),
        'r' => (DoubleEscapes::CR, '\r'),
        't' => (DoubleEscapes::TAB, '\t'),
        '\\' => (DoubleEscapes::BACKSLASH, '\\'),
        '"' => (DoubleEscapes::DOUBLE_QUOTE, '"'),
        '$' => (DoubleEscapes::DOLLAR, '$'),
        '\'' => (DoubleEscapes::SINGLE_QUOTE, '\''),
        'a' => (DoubleEscapes::BELL, '\u{7}'),
        'b' => (DoubleEscapes::BACKSPACE, '\u{8}'),
        'f' => (DoubleEscapes::FORM_FEED, '\u{c}'),
        'v' => (DoubleEscapes::VERTICAL_TAB, '\u{b}'),
        _ => return None,
    };
    dialect.double_escapes.contains(flag).then_some(cooked)
}

fn conventional(value: &str, quote: Quote, dialect: Dialect) -> String {
    let mark = delimiter(quote);
    let mut token = String::with_capacity(value.len() + 2);
    token.push(mark);
    for c in value.chars() {
        let spelling = ['n', 'r', 't', '\\', '"', '\'', '$', 'a', 'b', 'f', 'v']
            .into_iter()
            .find(|next| escape(*next, dialect, quote) == Some(c));
        if let Some(next) = spelling {
            if c == mark || c == '\\' || c.is_control() {
                token.push('\\');
                token.push(next);
                continue;
            }
        }
        token.push(c);
    }
    token.push(mark);
    token
}

// The inverse is a finite transducer, not a search over candidate strings.
// Its eight states are scanner backslash, decoder backslash and normalized-CR.
// Each edge emits at most two cooked scalars. Only the next two target scalars
// and the fixed escape alphabet can be source characters. Zero-output edges
// are closed over eight states; all other edges advance the target cursor.
#[derive(Clone, Copy)]
struct Cell {
    path: usize,
    total: usize,
    line: usize,
}
#[derive(Clone, Copy)]
struct Link {
    previous: usize,
    raw: char,
    octal: bool,
}
const ROOT: usize = usize::MAX;

#[derive(Clone, Copy)]
enum Closing {
    None,
    Parsed,
    Escaped,
}

#[derive(Clone, Copy)]
struct InverseMode {
    quote: Quote,
    decoder: Quote,
    append: bool,
    closing: Closing,
    scan_quotes: bool,
}

struct InverseSearch<'a> {
    chars: &'a [char],
    mode: InverseMode,
    dialect: Dialect,
    ring: [[Option<Cell>; 8]; 3],
    links: Vec<Link>,
}

impl<'a> InverseSearch<'a> {
    fn new(chars: &'a [char], mode: InverseMode, dialect: Dialect, line: usize) -> Self {
        let mut ring = [[None; 8]; 3];
        ring[0][0] = Some(Cell {
            path: ROOT,
            total: 0,
            line,
        });
        Self {
            chars,
            mode,
            dialect,
            ring,
            links: Vec::new(),
        }
    }

    fn close_zero_output(&mut self, at: usize) {
        let slot = at % 3;
        // A zero-output path cannot use a state twice beneficially: every edge
        // adds source bytes and a discarded LF can occur only after a CR.
        for _ in 0..8 {
            let mut changed = false;
            for state in 0..8 {
                let Some(cell) = self.ring[slot][state] else {
                    continue;
                };
                changed |= self.advance_plain(at, state, cell);
                self.advance_octal(at, state, cell);
            }
            if !changed {
                break;
            }
        }
    }

    fn advance_plain(&mut self, at: usize, state: usize, cell: Cell) -> bool {
        let mut alphabet = [
            '\\', 'n', 'r', 't', '"', '\'', '$', 'a', 'b', 'f', 'v', '0', '\r', '\n', '`', '\0',
            '\0',
        ];
        alphabet[15] = self.chars.get(at).copied().unwrap_or('\0');
        alphabet[16] = self.chars.get(at + 1).copied().unwrap_or('\0');
        let mut changed = false;
        for (which, raw) in alphabet.into_iter().enumerate() {
            if alphabet[..which].contains(&raw) {
                continue;
            }
            let Some((next, output, count)) = transition(
                state,
                raw,
                self.mode.quote,
                self.mode.decoder,
                self.dialect,
                self.mode.scan_quotes,
            ) else {
                continue;
            };
            if self.chars.get(at..at + count) != Some(&output[..count]) {
                continue;
            }
            let mut line = cell.line;
            let mut buffer = [0; 4];
            if advance_line(&mut line, raw.encode_utf8(&mut buffer), self.dialect).is_none() {
                continue;
            }
            if extend_path(
                &mut self.ring[(at + count) % 3][next],
                &mut self.links,
                cell,
                raw,
                false,
                line,
                self.dialect,
            ) && count == 0
            {
                changed = true;
            }
        }
        changed
    }

    fn advance_octal(&mut self, at: usize, state: usize, cell: Cell) {
        if self.mode.decoder != Quote::Double
            || state & 6 != 0
            || !self.dialect.double_escapes.contains(DoubleEscapes::OCTAL)
        {
            return;
        }
        let Some(raw) = self.chars.get(at).copied().filter(|c| u32::from(*c) <= 255) else {
            return;
        };
        let Some((next, line)) = octal_step(
            state,
            raw,
            self.mode.quote,
            self.dialect,
            self.mode.scan_quotes,
            cell.line,
        ) else {
            return;
        };
        extend_path(
            &mut self.ring[(at + 1) % 3][next],
            &mut self.links,
            cell,
            raw,
            true,
            line,
            self.dialect,
        );
    }
}

fn inverse(context: &Context<'_>, value: &str, mode: InverseMode) -> Option<String> {
    let dialect = context.dialect;
    let mark = delimiter(mode.quote);
    let close = !matches!(mode.closing, Closing::None);
    let chars: Vec<char> = value.chars().collect();
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    offsets.extend(value.char_indices().map(|(offset, _)| offset));
    offsets.push(value.len());
    let tails = mode
        .append
        .then(|| tail_starts(value, &chars, &offsets, dialect));
    let line = context.initial_line(mark)?;
    let mut search = InverseSearch::new(&chars, mode, dialect, line);
    let mut finish = None;
    let suffix_budget = suffix_budget(context).ok()?;
    for at in 0..=chars.len() {
        let slot = at % 3;
        search.close_zero_output(at);
        for (state, cell) in search.ring[slot].iter().copied().enumerate() {
            let Some(cell) = cell else {
                continue;
            };
            match mode.closing {
                Closing::Parsed if state & 1 != 0 => continue,
                Closing::Escaped if state & 1 == 0 => continue,
                Closing::None | Closing::Parsed | Closing::Escaped => {}
            }
            let end = at + usize::from(state & 2 != 0);
            if state & 2 != 0 && chars.get(at) != Some(&'\\') {
                continue;
            }
            if end > chars.len()
                || (end != chars.len() && !tails.as_ref().is_some_and(|tails| tails[end]))
                || (mode.append && end == chars.len())
            {
                continue;
            }
            let line =
                cell.line + usize::from(close) * mark.len_utf8() + value.len() - offsets[end];
            if suffix_budget.is_some_and(|budget| line > budget) {
                continue;
            }
            let score = if dialect.line_byte_limit.is_some() {
                (line, cell.total)
            } else {
                (cell.total, line)
            };
            if finish.map_or(true, |(_, old_end, old_score)| {
                end > old_end || (end == old_end && score < old_score)
            }) {
                finish = Some((cell.path, end, score));
            }
        }
        search.ring[slot] = [None; 8];
    }
    let (path, end, _) = finish?;
    Some(quoted_path(
        search.links,
        path,
        &value[offsets[end]..],
        mark,
        close,
    ))
}

fn suffix_budget(context: &Context<'_>) -> Result<Option<usize>, EncodeError> {
    let Some(limit) = context.dialect.line_byte_limit else {
        return Ok(None);
    };
    let mut line = 0;
    advance_line(&mut line, context.after, context.dialect).ok_or(EncodeError::Unrepresentable)?;
    let first = context
        .after
        .bytes()
        .take_while(|byte| {
            *byte != b'\n'
                && !(*byte == b'\r' && context.dialect.line_breaks == LineBreaks::Universal)
        })
        .count();
    let cr = usize::from(context.after.as_bytes().get(first) == Some(&b'\r'));
    let budget = limit
        .checked_sub(first + cr + usize::from(context.separate_comment))
        .ok_or(EncodeError::Unrepresentable)?;
    Ok(Some(budget))
}

fn extend_path(
    destination: &mut Option<Cell>,
    links: &mut Vec<Link>,
    previous: Cell,
    raw: char,
    octal: bool,
    line: usize,
    dialect: Dialect,
) -> bool {
    let bytes = if octal { 5 } else { raw.len_utf8() };
    if !improve(destination, previous, line, bytes, dialect) {
        return false;
    }
    let path = links.len();
    links.push(Link {
        previous: previous.path,
        raw,
        octal,
    });
    *destination = Some(Cell {
        path,
        total: previous.total + bytes,
        line,
    });
    true
}

fn octal_step(
    mut state: usize,
    raw: char,
    quote: Quote,
    dialect: Dialect,
    scan_quotes: bool,
    mut line: usize,
) -> Option<(usize, usize)> {
    let spelling = octal(raw);
    for byte in spelling {
        state = if scan_quotes {
            usize::from(scan(state & 1 != 0, char::from(byte), quote, dialect)?)
        } else {
            0
        };
    }
    advance_line(
        &mut line,
        core::str::from_utf8(&spelling).expect("ASCII octal"),
        dialect,
    )?;
    Some((state, line))
}

fn quoted_path(
    mut links: Vec<Link>,
    mut path: usize,
    tail: &str,
    mark: char,
    close: bool,
) -> String {
    // Compact the chosen monotonic predecessor path into the same allocation.
    // Each destination is at or above its source; predecessors remain intact.
    let mut start = links.len();
    let mut bytes = 1 + usize::from(close) + tail.len();
    while path != ROOT {
        let link = links[path];
        start -= 1;
        links[start] = link;
        bytes += if link.octal { 5 } else { link.raw.len_utf8() };
        path = link.previous;
    }
    let mut token = String::with_capacity(bytes);
    token.push(mark);
    for link in &links[start..] {
        if link.octal {
            for byte in octal(link.raw) {
                token.push(char::from(byte));
            }
        } else {
            token.push(link.raw);
        }
    }
    if close {
        token.push(mark);
    }
    token.push_str(tail);
    token
}

fn improve(
    destination: &mut Option<Cell>,
    previous: Cell,
    line: usize,
    bytes: usize,
    dialect: Dialect,
) -> bool {
    destination.map_or(true, |old| {
        if dialect.line_byte_limit.is_some() {
            (line, previous.total + bytes) < (old.line, old.total)
        } else {
            previous.total + bytes < old.total
        }
    })
}

fn octal(c: char) -> [u8; 5] {
    let value = u8::try_from(u32::from(c)).expect("octal inverse is limited to byte scalars");
    [
        b'\\',
        b'0',
        b'0' + (value >> 6),
        b'0' + ((value >> 3) & 7),
        b'0' + (value & 7),
    ]
}

fn transition(
    state: usize,
    raw: char,
    quote: Quote,
    decoder: Quote,
    dialect: Dialect,
    scan_quotes: bool,
) -> Option<(usize, [char; 2], usize)> {
    let scanner = if scan_quotes {
        scan(state & 1 != 0, raw, quote, dialect)?
    } else {
        false
    };
    let multiline = match quote {
        Quote::Single => dialect.multiline.single,
        Quote::Double => dialect.multiline.double,
        _ => dialect.multiline.backtick,
    };
    let physical_break =
        raw == '\n' || (raw == '\r' && dialect.line_breaks == LineBreaks::Universal);
    if physical_break && (!scan_quotes || !multiline) {
        return None;
    }
    let mut pending = state & 2 != 0;
    let mut cr = state & 4 != 0;
    let mut output = ['\0'; 2];
    let mut count = 0;
    if pending {
        pending = false;
        if let Some(cooked) = escape(raw, dialect, decoder) {
            output[0] = cooked;
            return Some((usize::from(scanner), output, 1));
        }
        if decoder == Quote::Double
            && raw == '0'
            && dialect.double_escapes.contains(DoubleEscapes::OCTAL)
        {
            return None;
        }
        let preserve = decoder == Quote::Single
            || matches!(
                dialect.unknown_escapes,
                UnknownEscapes::Preserve | UnknownEscapes::Warn
            );
        if !preserve && dialect.unknown_escapes == UnknownEscapes::Error {
            return None;
        }
        if preserve {
            output[count] = '\\';
            count += 1;
        }
        let revisit = decoder == Quote::Single
            || (dialect.syntax == Syntax::Node
                && dialect.unknown_escapes != UnknownEscapes::DropBackslash)
            || (dialect.normalize_newlines && raw == '\r');
        if !revisit {
            output[count] = raw;
            return Some((usize::from(scanner), output, count + 1));
        }
    }
    if cr && raw == '\n' {
        cr = false;
    } else {
        cr = false;
        if dialect.normalize_newlines && raw == '\r' {
            output[count] = '\n';
            count += 1;
            cr = true;
        } else if raw == '\\' && decoder != Quote::Backtick {
            pending = true;
        } else {
            output[count] = raw;
            count += 1;
        }
    }
    Some((
        usize::from(scanner) | (usize::from(pending) << 1) | (usize::from(cr) << 2),
        output,
        count,
    ))
}

fn scan(pending: bool, raw: char, quote: Quote, dialect: Dialect) -> Option<bool> {
    let mark = delimiter(quote);
    if dialect.syntax == Syntax::Node {
        if raw == mark && !pending {
            return None;
        }
        return Some(raw == '\\');
    }
    if quote == Quote::Single && dialect.syntax != Syntax::Python {
        match dialect.single_escapes {
            SingleEscapes::None => return (raw != mark).then_some(false),
            SingleEscapes::Quote => {
                if raw == mark {
                    return pending.then_some(false);
                }
                return Some(raw == '\\' && !(pending && dialect.syntax == Syntax::Compose));
            }
            SingleEscapes::QuoteAndBackslash => {}
        }
    }
    if pending {
        Some(false)
    } else if raw == mark {
        None
    } else {
        Some(raw == '\\')
    }
}

fn advance_line(line: &mut usize, text: &str, dialect: Dialect) -> Option<()> {
    for byte in text.bytes() {
        if byte == b'\n' {
            *line = 0;
        } else {
            *line += 1;
            if dialect.line_byte_limit.is_some_and(|limit| *line > limit) {
                return None;
            }
            if byte == b'\r' && dialect.line_breaks == LineBreaks::Universal {
                *line = 0;
            }
        }
    }
    Some(())
}

fn space(c: char, mode: Whitespace) -> bool {
    match mode {
        Whitespace::Ascii => matches!(c, ' ' | '\t'),
        Whitespace::Unicode => c.is_whitespace(),
        Whitespace::EcmaScript => c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}'),
        Whitespace::Python => c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'),
        Whitespace::Compose => matches!(
            c,
            ' ' | '\t' | '\u{b}' | '\u{c}' | '\r' | '\u{85}' | '\u{a0}'
        ),
    }
}

fn tail_starts(value: &str, chars: &[char], offsets: &[usize], dialect: Dialect) -> Vec<bool> {
    let mut valid = vec![false; chars.len() + 1];
    let mut safe = chars.last().is_some_and(|c| !space(*c, dialect.whitespace));
    let mut nonspace = false;
    for at in (0..chars.len()).rev() {
        let c = chars[at];
        if c == '\n'
            || (c == '\r' && dialect.line_breaks == LineBreaks::Universal)
            || (dialect.syntax == Syntax::Node && matches!(c, '\u{2028}' | '\u{2029}'))
        {
            safe = false;
        }
        let previous = value[..offsets[at]].chars().next_back();
        let comment = c == '#'
            && match dialect.inline_comments {
                crate::InlineComments::None => false,
                crate::InlineComments::AnyHash => true,
                crate::InlineComments::Space => previous == Some(' '),
                crate::InlineComments::Whitespace | crate::InlineComments::Python => {
                    previous.is_some_and(|c| space(c, dialect.whitespace))
                }
            };
        // At the boundary a quote replaces the preceding cooked character.
        valid[at] = safe && nonspace && !comment;
        if !space(c, dialect.whitespace) {
            nonspace = true;
            valid[at] = safe
                && !(c == '#'
                    && (dialect.quoted_comments
                        || dialect.inline_comments == crate::InlineComments::AnyHash)
                    && dialect.inline_comments != crate::InlineComments::None);
        }
        if comment {
            safe = false;
        }
    }
    valid
}
