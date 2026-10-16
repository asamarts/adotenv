use alloc::{borrow::Cow, string::String, vec::Vec};
use core::ops::Range;

use crate::model::{DecodeSegment, EntryData, ItemData};
use crate::{
    AssignmentState, Diagnostic, DiagnosticCode, Document, EntryTokens, ItemKind, LineEnding,
    Quote, Severity,
};

/// Parse Portable syntax, borrowing the complete UTF-8 input and recovering errors.
///
/// Diagnostics are in source order. Every byte remains in the document, including
/// malformed statements. Lookup is literal and never consults the environment.
#[must_use]
pub fn parse(source: &str) -> (Document<'_>, Vec<Diagnostic>) {
    parse_impl::<true>(source)
}

fn parse_impl<const DIAGNOSTICS: bool>(source: &str) -> (Document<'_>, Vec<Diagnostic>) {
    let bom = source.starts_with('\u{feff}');
    let mut parser = Parser::<DIAGNOSTICS> {
        source,
        document: Document {
            source: Cow::Borrowed(source),
            items: Vec::new(),
            entries: Vec::new(),
            bom,
        },
        diagnostics: Vec::new(),
        keys: KeyIndex::default(),
        failed_quotes: [None, None],
        line: 1,
    };
    let mut at = if bom { 3 } else { 0 };
    if DIAGNOSTICS && bom {
        parser
            .diagnostics
            .push(Diagnostic::new(DiagnosticCode::ByteOrderMark, 0..3, 1));
    }
    while at < source.len() {
        let first = physical_line(source, at);
        let end = parser.statement(at, first);
        parser.line += line_breaks(&source[at..end]);
        at = end;
    }
    let diagnostics = if DIAGNOSTICS {
        merge_lone_cr(source, parser.diagnostics)
    } else {
        parser.diagnostics
    };
    (parser.document, diagnostics)
}

/// Consume owned UTF-8 source without copying its bytes or retaining self-references.
#[must_use]
pub fn parse_owned(source: String) -> (Document<'static>, Vec<Diagnostic>) {
    let (items, entries, bom, diagnostics) = {
        let (document, diagnostics) = parse(&source);
        (document.items, document.entries, document.bom, diagnostics)
    };
    (
        Document {
            source: Cow::Owned(source),
            items,
            entries,
            bom,
        },
        diagnostics,
    )
}

/// Parse Portable syntax and return the first error-severity diagnostic, if any.
///
/// Warnings and informational diagnostics do not make strict parsing fail. Use
/// [`parse`] to inspect them or retain a document with malformed statements.
///
/// # Errors
/// Returns the first malformed statement's diagnostic in source order.
pub fn parse_strict(source: &str) -> Result<Document<'_>, Diagnostic> {
    let document = parse_document(source);
    let error = document
        .items
        .iter()
        .filter_map(|item| item.invalid.as_ref())
        .find(|diagnostic| diagnostic.severity() == Severity::Error)
        .cloned();
    match error {
        Some(error) => Err(error),
        None => Ok(document),
    }
}

/// Recover the document without a diagnostic list; errors remain on invalid items.
pub(crate) fn parse_document(source: &str) -> Document<'_> {
    parse_impl::<false>(source).0
}

pub(crate) fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

fn posix_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

fn horizontal(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

fn skip_space(bytes: &[u8], mut at: usize, end: usize) -> usize {
    while at < end && horizontal(bytes[at]) {
        at += 1;
    }
    at
}

fn trim_space(bytes: &[u8], start: usize, mut end: usize) -> usize {
    while end > start && horizontal(bytes[end - 1]) {
        end -= 1;
    }
    end
}

#[derive(Clone, Copy)]
struct PhysicalLine {
    content_end: usize,
    end: usize,
    ending: LineEnding,
}

fn physical_line(source: &str, start: usize) -> PhysicalLine {
    let bytes = source.as_bytes();
    let mut at = start;
    while at < bytes.len() {
        match bytes[at] {
            b'\n' => {
                return PhysicalLine {
                    content_end: at,
                    end: at + 1,
                    ending: LineEnding::Lf,
                }
            }
            b'\r' => {
                let crlf = bytes.get(at + 1) == Some(&b'\n');
                return PhysicalLine {
                    content_end: at,
                    end: at + if crlf { 2 } else { 1 },
                    ending: if crlf {
                        LineEnding::CrLf
                    } else {
                        LineEnding::Cr
                    },
                };
            }
            _ => at += 1,
        }
    }
    PhysicalLine {
        content_end: at,
        end: at,
        ending: LineEnding::None,
    }
}

fn line_breaks(source: &str) -> usize {
    let bytes = source.as_bytes();
    bytes
        .iter()
        .enumerate()
        .filter(|&(index, byte)| {
            *byte == b'\r' || (*byte == b'\n' && (index == 0 || bytes[index - 1] != b'\r'))
        })
        .count()
}

struct Parser<'src, const DIAGNOSTICS: bool> {
    source: &'src str,
    document: Document<'src>,
    diagnostics: Vec<Diagnostic>,
    keys: KeyIndex,
    failed_quotes: [Option<usize>; 2],
    line: usize,
}

impl<const DIAGNOSTICS: bool> Parser<'_, DIAGNOSTICS> {
    fn plain_item(
        &mut self,
        kind: ItemKind,
        span: Range<usize>,
        ending: LineEnding,
        comment: Option<Range<usize>>,
    ) {
        self.document.items.push(ItemData {
            kind,
            span,
            ending,
            entry: None,
            comment,
            invalid: None,
        });
    }

    fn invalid(
        &mut self,
        start: usize,
        physical: PhysicalLine,
        code: DiagnosticCode,
        span: Range<usize>,
        line: usize,
    ) -> usize {
        let diagnostic = Diagnostic::new(code, span, line);
        if DIAGNOSTICS {
            self.diagnostics.push(diagnostic.clone());
        }
        self.document.items.push(ItemData {
            kind: ItemKind::Invalid,
            span: start..physical.end,
            ending: physical.ending,
            entry: None,
            comment: None,
            invalid: Some(diagnostic),
        });
        physical.end
    }

    fn statement(&mut self, start: usize, first: PhysicalLine) -> usize {
        let bytes = self.source.as_bytes();
        let mut at = skip_space(bytes, start, first.content_end);
        let indent = start..at;
        if at == first.content_end {
            self.plain_item(ItemKind::Blank, start..first.end, first.ending, None);
            return first.end;
        }
        if bytes[at] == b'#' {
            self.plain_item(
                ItemKind::Comment,
                start..first.end,
                first.ending,
                Some(at + 1..first.content_end),
            );
            return first.end;
        }
        let export = if self.source[at..first.content_end].starts_with("export")
            && at + 6 < first.content_end
            && horizontal(bytes[at + 6])
        {
            let range = at..at + 6;
            at += 6;
            Some(range)
        } else {
            None
        };
        let export_space_start = at;
        if export.is_some() {
            at = skip_space(bytes, at, first.content_end);
        }
        let space_after_export = export_space_start..at;
        let key_start = at;
        while at < first.content_end && !horizontal(bytes[at]) && bytes[at] != b'=' {
            at += 1;
        }
        let key = key_start..at;
        let key_end = at;
        at = skip_space(bytes, at, first.content_end);
        if !valid_key(&self.source[key.clone()])
            || (at < first.content_end && !matches!(bytes[at], b'=' | b'#'))
        {
            let end = self.source[key_start..first.content_end]
                .find('=')
                .map_or(first.content_end, |relative| key_start + relative);
            return self.invalid(
                start,
                first,
                DiagnosticCode::InvalidKey,
                key_start..trim_space(bytes, key_start, end),
                self.line,
            );
        }
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
            trailing_space: at..at,
            inline_comment: None,
        };
        if at == first.content_end || bytes[at] == b'#' {
            tokens.inline_comment = (at < first.content_end).then_some(at..first.content_end);
            self.entry(start, first, tokens, Quote::None);
            return first.end;
        }
        tokens.eq = Some(at..at + 1);
        at += 1;
        let after_eq = at;
        at = skip_space(bytes, at, first.content_end);
        tokens.space_after_eq = after_eq..at;
        let quote = match bytes.get(at).filter(|_| at < first.content_end) {
            Some(b'\'') => Quote::Single,
            Some(b'"') => Quote::Double,
            _ => Quote::None,
        };
        match quote {
            Quote::None => self.unquoted_statement(start, first, tokens),
            _ => self.quoted_statement(start, first, tokens, quote),
        }
    }

    fn unquoted_statement(
        &mut self,
        start: usize,
        first: PhysicalLine,
        mut tokens: EntryTokens,
    ) -> usize {
        let at = tokens.space_after_eq.end;
        let bytes = self.source.as_bytes();
        let mut comment_start = first.content_end;
        for index in at..first.content_end {
            if bytes[index] == b'#' && index > 0 && horizontal(bytes[index - 1]) {
                comment_start = index;
                break;
            }
        }
        let value_end = trim_space(bytes, at, comment_start);
        tokens.value = Some(at..value_end);
        tokens.body = Some(at..value_end);
        tokens.trailing_space = value_end..comment_start;
        tokens.inline_comment =
            (comment_start < first.content_end).then_some(comment_start..first.content_end);
        self.entry(start, first, tokens, Quote::None);
        first.end
    }

    fn quoted_statement(
        &mut self,
        start: usize,
        first: PhysicalLine,
        mut tokens: EntryTokens,
        quote: Quote,
    ) -> usize {
        let at = tokens.space_after_eq.end;
        let Some(close) = self.closing_quote(at, quote) else {
            return self.invalid(
                start,
                first,
                DiagnosticCode::UnterminatedQuote,
                at..at + 1,
                self.line,
            );
        };
        let last = physical_line(self.source, close + 1);
        let tail = skip_space(self.source.as_bytes(), close + 1, last.content_end);
        if tail < last.content_end && !(self.source.as_bytes()[tail] == b'#' && tail > close + 1) {
            let line = self.line + line_breaks(&self.source[start..tail]);
            return self.invalid(
                start,
                last,
                DiagnosticCode::TrailingText,
                tail..last.content_end,
                line,
            );
        }
        tokens.value = Some(at..close + 1);
        tokens.body = Some(at + 1..close);
        tokens.trailing_space = close + 1..tail;
        tokens.inline_comment = (tail < last.content_end).then_some(tail..last.content_end);
        self.entry(start, last, tokens, quote);
        last.end
    }

    fn closing_quote(&mut self, open: usize, quote: Quote) -> Option<usize> {
        let kind = usize::from(quote == Quote::Double);
        if self.failed_quotes[kind].is_some_and(|failed| open >= failed) {
            return None;
        }
        let delimiter = if quote == Quote::Double { b'"' } else { b'\'' };
        let bytes = self.source.as_bytes();
        let mut at = open + 1;
        while at < bytes.len() {
            if bytes[at] == delimiter {
                return Some(at);
            }
            if quote == Quote::Double && bytes[at] == b'\\' && at + 1 < bytes.len() {
                at += 2;
            } else {
                at += 1;
            }
        }
        self.failed_quotes[kind] = Some(open);
        None
    }

    fn entry(&mut self, start: usize, last: PhysicalLine, tokens: EntryTokens, quote: Quote) {
        let index = self.document.entries.len();
        let key = &self.source[tokens.key.clone()];
        if DIAGNOSTICS && !posix_key(key) {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticCode::NonPosixKey,
                tokens.key.clone(),
                self.line,
            ));
        }
        // A one-entry document cannot contain duplicates and needs no key index.
        if DIAGNOSTICS && index == 1 {
            let first = &self.document.entries[0].tokens.key;
            self.keys.insert(&self.source[first.clone()], 0);
        }
        if DIAGNOSTICS && index > 0 {
            if let Some(previous) = self.keys.insert(key, index) {
                let mut diagnostic =
                    Diagnostic::new(DiagnosticCode::DuplicateKey, tokens.key.clone(), self.line);
                diagnostic.secondary = Some(self.document.entries[previous].tokens.key.clone());
                self.diagnostics.push(diagnostic);
            }
        }
        let (state, decoded, map) = match &tokens.body {
            None => {
                if DIAGNOSTICS {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticCode::BareKey,
                        tokens.key.clone(),
                        self.line,
                    ));
                }
                (AssignmentState::Bare, None, Vec::new())
            }
            Some(body) => {
                let (decoded, map) = if quote == Quote::Double {
                    self.decode_double(body.clone())
                } else {
                    if DIAGNOSTICS && quote == Quote::None {
                        for at in body.clone() {
                            let code = match self.source.as_bytes()[at] {
                                b'#' => Some(DiagnosticCode::TightHash),
                                b'`' => Some(DiagnosticCode::Backtick),
                                _ => None,
                            };
                            if let Some(code) = code {
                                self.diagnostics
                                    .push(Diagnostic::new(code, at..at + 1, self.line));
                            }
                        }
                    }
                    (None, Vec::new())
                };
                let empty = decoded.as_ref().map_or(body.is_empty(), String::is_empty);
                (
                    if empty {
                        AssignmentState::Empty
                    } else {
                        AssignmentState::Value
                    },
                    decoded,
                    map,
                )
            }
        };
        let item = self.document.items.len();
        self.document.entries.push(EntryData {
            item,
            line: self.line,
            tokens,
            quote,
            state,
            decoded,
            map,
        });
        self.document.items.push(ItemData {
            kind: ItemKind::Entry,
            span: start..last.end,
            ending: last.ending,
            entry: Some(index),
            comment: None,
            invalid: None,
        });
    }

    fn decode_double(&mut self, body: Range<usize>) -> (Option<String>, Vec<DecodeSegment>) {
        let mut decoded: Option<String> = None;
        let mut map = Vec::new();
        let mut copied_until = body.start;
        let mut at = body.start;
        let mut line = self.line;
        let bytes = self.source.as_bytes();
        while at < body.end {
            if bytes[at] == b'\\' && at + 1 < body.end {
                let next = self.source[at + 1..body.end]
                    .chars()
                    .next()
                    .expect("non-empty escape suffix");
                let cooked = match next {
                    'n' => Some('\n'),
                    'r' => Some('\r'),
                    't' => Some('\t'),
                    '\\' => Some('\\'),
                    '"' => Some('"'),
                    '$' => Some('$'),
                    _ => None,
                };
                let end = at + 1 + next.len_utf8();
                if let Some(cooked) = cooked {
                    let output = decoded.get_or_insert_with(|| String::with_capacity(body.len()));
                    output.push_str(&self.source[copied_until..at]);
                    let begin = output.len();
                    output.push(cooked);
                    map.push(DecodeSegment {
                        decoded: begin..output.len(),
                        raw: at..end,
                    });
                    copied_until = end;
                } else if DIAGNOSTICS {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticCode::UnknownEscape,
                        at..end,
                        line,
                    ));
                    line += line_breaks(&self.source[at..end]);
                }
                at = end;
            } else {
                if DIAGNOSTICS
                    && (bytes[at] == b'\r'
                        || (bytes[at] == b'\n' && (at == 0 || bytes[at - 1] != b'\r')))
                {
                    line += 1;
                }
                at += 1;
            }
        }
        if let Some(output) = &mut decoded {
            output.push_str(&self.source[copied_until..body.end]);
        }
        (decoded, map)
    }
}

// The key alphabet has a fixed 65 symbols. A sparse trie bounds sibling search
// by that alphabet rather than by input size, so duplicate diagnostics remain
// linear even for deliberately colliding/long keys. It stores no key copies and
// is discarded after parsing. Shared prefixes use one node each.
const ABSENT: usize = usize::MAX;
struct KeyNode {
    byte: u8,
    child: usize,
    sibling: usize,
    entry: usize,
}
#[derive(Default)]
struct KeyIndex {
    nodes: Vec<KeyNode>,
}
impl KeyIndex {
    fn insert(&mut self, key: &str, entry: usize) -> Option<usize> {
        if self.nodes.is_empty() {
            self.nodes.push(KeyNode {
                byte: 0,
                child: ABSENT,
                sibling: ABSENT,
                entry: ABSENT,
            });
        }
        let mut parent = 0;
        for byte in key.bytes() {
            let mut child = self.nodes[parent].child;
            while child != ABSENT && self.nodes[child].byte != byte {
                child = self.nodes[child].sibling;
            }
            if child == ABSENT {
                child = self.nodes.len();
                let sibling = self.nodes[parent].child;
                self.nodes.push(KeyNode {
                    byte,
                    child: ABSENT,
                    sibling,
                    entry: ABSENT,
                });
                self.nodes[parent].child = child;
            }
            parent = child;
        }
        let previous = self.nodes[parent].entry;
        if previous == ABSENT {
            self.nodes[parent].entry = entry;
            None
        } else {
            Some(previous)
        }
    }
}

// Source newline diagnostics are independent of statement recovery. Both input
// lists are already ordered, so merging preserves linear parsing and allocation
// proportional to diagnostics rather than sorting or indexing every source line.
fn merge_lone_cr(source: &str, diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut carriage_returns = Vec::new();
    let mut line = 1;
    for (at, byte) in bytes.iter().enumerate() {
        if *byte == b'\r' {
            if bytes.get(at + 1) != Some(&b'\n') {
                carriage_returns.push(Diagnostic::new(
                    DiagnosticCode::LoneCarriageReturn,
                    at..at + 1,
                    line,
                ));
            }
            line += 1;
        } else if *byte == b'\n' && (at == 0 || bytes[at - 1] != b'\r') {
            line += 1;
        }
    }
    if carriage_returns.is_empty() {
        return diagnostics;
    }
    let mut output = Vec::with_capacity(diagnostics.len() + carriage_returns.len());
    let mut ordinary = diagnostics.into_iter().peekable();
    let mut carriage = carriage_returns.into_iter().peekable();
    while let (Some(a), Some(b)) = (ordinary.peek(), carriage.peek()) {
        if a.span.start <= b.span.start {
            output.push(ordinary.next().expect("peeked diagnostic"));
        } else {
            output.push(carriage.next().expect("peeked carriage return"));
        }
    }
    output.extend(ordinary);
    output.extend(carriage);
    output
}
