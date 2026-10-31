//! Literal reference inspection without resolution or execution.
//!
//! References are reported as raw candidates with original-source spans. A
//! separate eligibility helper applies caller-selected quote, escape, operator
//! and command rules; this module never reads a lookup, environment or file.

use alloc::{collections::BTreeMap, vec, vec::Vec};
use core::ops::Range;

use crate::{Dialect, DoubleEscapes, Entry, Quote, QuotedTail, Syntax};

/// The kind of a raw reference candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReferenceKind {
    /// A variable reference such as `$VAR` or `${VAR:-default}`.
    Variable,
    /// A command substitution such as `$(command)`.
    Command,
}

/// A recognized variable-reference operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReferenceOperator {
    /// `${VAR-default}`: use the argument when the variable is unset.
    DefaultIfUnset,
    /// `${VAR:-default}`: use the argument when unset or empty.
    DefaultIfUnsetOrEmpty,
    /// `${VAR?message}`: report an error when the variable is unset.
    ErrorIfUnset,
    /// `${VAR:?message}`: report an error when unset or empty.
    ErrorIfUnsetOrEmpty,
    /// `${VAR+alternative}`: use the argument when the variable is set.
    AlternativeIfSet,
    /// `${VAR:+alternative}`: use the argument when set and non-empty.
    AlternativeIfSetAndNonEmpty,
}

/// Variable-reference forms and operators eligible under one expansion policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OperatorSet {
    /// No variable references are eligible.
    None,
    /// Only braced variables such as `${VAR}`; no short or operator forms.
    Braced,
    /// `$VAR` and `${VAR}` without operators.
    Basic,
    /// The full operator set, including nested defaults.
    Compose,
    /// dotenv-expand-style variables and operators, including nested forms.
    DotenvExpand,
}

impl OperatorSet {
    fn accepts_variable(self, braced: bool) -> bool {
        match self {
            Self::None => false,
            Self::Braced => braced,
            Self::Basic | Self::Compose | Self::DotenvExpand => true,
        }
    }
    fn accepts_operator(self, _operator: ReferenceOperator) -> bool {
        matches!(self, Self::Compose | Self::DotenvExpand)
    }
    fn accepts_nested(self) -> bool {
        matches!(self, Self::Compose | Self::DotenvExpand)
    }
}

/// Caller-selected rules for deciding whether a raw candidate could expand.
// These are independent policy switches, not states.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ReferenceRules {
    /// Variable forms and operators considered eligible.
    pub operators: OperatorSet,
    /// Whether references inside single-quoted values are eligible.
    pub single_quotes: bool,
    /// Whether escaped-dollar candidates are eligible.
    pub escaped_dollar: bool,
    /// Whether `$(command)` candidates are eligible.
    pub commands: bool,
    /// Whether references inside backtick-quoted values are eligible.
    pub backtick_quotes: bool,
}

impl ReferenceRules {
    /// Use the given variable/operator eligibility set.
    #[must_use]
    pub const fn with_operators(mut self, value: OperatorSet) -> Self {
        self.operators = value;
        self
    }
    /// Enable or disable references inside single quotes.
    #[must_use]
    pub const fn with_single_quotes(mut self, value: bool) -> Self {
        self.single_quotes = value;
        self
    }
    /// Enable or disable escaped-dollar candidates.
    #[must_use]
    pub const fn with_escaped_dollar(mut self, value: bool) -> Self {
        self.escaped_dollar = value;
        self
    }
    /// Enable or disable command candidates.
    #[must_use]
    pub const fn with_commands(mut self, value: bool) -> Self {
        self.commands = value;
        self
    }
    /// Enable or disable references inside backticks.
    #[must_use]
    pub const fn with_backtick_quotes(mut self, value: bool) -> Self {
        self.backtick_quotes = value;
        self
    }
}
impl ReferenceRules {
    /// No references are eligible.
    pub const NONE: Self = Self {
        operators: OperatorSet::None,
        single_quotes: false,
        escaped_dollar: false,
        commands: false,
        backtick_quotes: false,
    };
    /// Conservative Portable rules: basic variables only, no quotes or commands.
    pub const PORTABLE: Self = Self {
        operators: OperatorSet::Basic,
        single_quotes: false,
        escaped_dollar: false,
        commands: false,
        backtick_quotes: false,
    };
    /// Compose-style variable operators and nesting; no command execution.
    ///
    /// These are eligibility rules, not Docker Compose interpolation: an
    /// escaped `$$` is not eligible and resolution leaves it as written.
    pub const COMPOSE: Self = Self {
        operators: OperatorSet::Compose,
        single_quotes: false,
        escaped_dollar: false,
        commands: false,
        backtick_quotes: false,
    };
    /// dotenv-expand-style variables, single quotes and command substitution.
    pub const DOTENV_EXPAND: Self = Self {
        operators: OperatorSet::DotenvExpand,
        single_quotes: true,
        escaped_dollar: false,
        commands: true,
        backtick_quotes: false,
    };
}

impl Default for ReferenceRules {
    fn default() -> Self {
        Self::PORTABLE
    }
}

/// What eligibility needs to know about the variable-operator and command
/// forms that contain a candidate, independent of any rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// These summarize independent properties of the containing forms.
#[allow(clippy::struct_excessive_bools)]
struct Enclosing {
    /// Some containing form's dollar is escaped.
    escaped: bool,
    /// Some containing form has no closing delimiter.
    unclosed: bool,
    /// Some containing form is a command.
    command: bool,
    /// Some containing variable has an empty name.
    unnamed: bool,
    /// The operators of the containing variables, one bit per operator.
    operators: u8,
}

impl Enclosing {
    /// The summary for a candidate that no form contains.
    const NONE: Self = Self {
        escaped: false,
        unclosed: false,
        command: false,
        unnamed: false,
        operators: 0,
    };

    /// The summary for candidates nested directly inside `container`.
    fn within(self, container: &Reference) -> Self {
        let variable = container.kind == ReferenceKind::Variable;
        Self {
            escaped: self.escaped || container.escaped,
            unclosed: self.unclosed || !container.closed,
            command: self.command || container.kind == ReferenceKind::Command,
            unnamed: self.unnamed
                || (variable && container.name.as_ref().map_or(true, Range::is_empty)),
            operators: self.operators | container.operator.map_or(0, operator_bit),
        }
    }

    /// Whether a containing variable has an operator that `operators` rejects.
    fn rejects_operator(self, operators: OperatorSet) -> bool {
        OPERATORS.iter().any(|&operator| {
            self.operators & operator_bit(operator) != 0 && !operators.accepts_operator(operator)
        })
    }
}

const OPERATORS: [ReferenceOperator; 6] = [
    ReferenceOperator::DefaultIfUnset,
    ReferenceOperator::DefaultIfUnsetOrEmpty,
    ReferenceOperator::ErrorIfUnset,
    ReferenceOperator::ErrorIfUnsetOrEmpty,
    ReferenceOperator::AlternativeIfSet,
    ReferenceOperator::AlternativeIfSetAndNonEmpty,
];

fn operator_bit(operator: ReferenceOperator) -> u8 {
    match operator {
        ReferenceOperator::DefaultIfUnset => 1,
        ReferenceOperator::DefaultIfUnsetOrEmpty => 1 << 1,
        ReferenceOperator::ErrorIfUnset => 1 << 2,
        ReferenceOperator::ErrorIfUnsetOrEmpty => 1 << 3,
        ReferenceOperator::AlternativeIfSet => 1 << 4,
        ReferenceOperator::AlternativeIfSetAndNonEmpty => 1 << 5,
    }
}

/// A raw reference candidate in an entry's literal value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Reference {
    /// Variable or command form.
    pub kind: ReferenceKind,
    /// Whole candidate in original-source bytes, starting at `$`.
    pub span: Range<usize>,
    /// Quote context of the containing value.
    pub quote: Quote,
    /// Whether the dollar is escaped by backslashes under the selected quote
    /// grammar, inside a nested expansion argument, or by Compose's `$$` escape.
    pub escaped: bool,
    /// Nesting depth; zero at the value's top level.
    pub depth: usize,
    /// Whether this is a braced `${...}` variable form.
    pub braced: bool,
    /// Whether the closing `}` or `)` was found.
    pub closed: bool,
    /// Index of the innermost containing variable-operator or command form in
    /// the list returned with this candidate, or `None` at the value's top
    /// level. The container comes earlier in the list, its span contains this
    /// candidate's span, and its depth is one less.
    pub parent: Option<usize>,
    /// The chain of containing forms, summarized for eligibility.
    enclosing: Enclosing,
    /// Variable-name span for variable forms.
    pub name: Option<Range<usize>>,
    /// Variable operator, when present.
    pub operator: Option<ReferenceOperator>,
    /// Operator span, when present.
    pub operator_span: Option<Range<usize>>,
    /// Default, message or alternative span for an operator form.
    pub argument: Option<Range<usize>>,
    /// Raw command payload span between `$(` and `)`.
    pub command: Option<Range<usize>>,
}

impl Reference {
    /// Whether this candidate is eligible under the selected rules.
    ///
    /// Eligibility is local to the candidate's form, quote, escape and nesting
    /// context. It does not resolve variables, validate braced-name spelling or
    /// execute commands. Malformed candidates are never eligible.
    #[must_use]
    pub fn is_eligible(&self, rules: ReferenceRules) -> bool {
        if !self.closed {
            return false;
        }
        if self.escaped && !rules.escaped_dollar {
            return false;
        }
        if self.quote == Quote::Single && !rules.single_quotes {
            return false;
        }
        if self.quote == Quote::Backtick && !rules.backtick_quotes {
            return false;
        }
        if self.depth > 0 && !rules.operators.accepts_nested() {
            return false;
        }
        let enclosing = self.enclosing;
        if enclosing.escaped && !rules.escaped_dollar {
            return false;
        }
        if enclosing.unclosed || enclosing.unnamed {
            return false;
        }
        if enclosing.command && !rules.commands {
            return false;
        }
        if enclosing.rejects_operator(rules.operators) {
            return false;
        }
        match self.kind {
            ReferenceKind::Command => rules.commands,
            ReferenceKind::Variable => {
                if self.name.as_ref().map_or(true, Range::is_empty) {
                    return false;
                }
                if !rules.operators.accepts_variable(self.braced) {
                    return false;
                }
                self.operator
                    .map_or(true, |operator| rules.operators.accepts_operator(operator))
            }
        }
    }
}

impl Entry<'_> {
    /// Recognized variable and command candidates in this entry's literal value.
    ///
    /// Candidates are returned in source order, including escaped dollars,
    /// single-quoted observations, malformed unclosed forms and nested defaults.
    /// Spans address the original source. No lookup, expansion, I/O or command
    /// execution occurs; use [`Reference::is_eligible`] for a policy decision.
    ///
    /// Candidates are sorted by span start. A nested candidate's
    /// [`Reference::parent`] is the index of its innermost containing form,
    /// which comes earlier in the list. The candidates nested in a form, at
    /// any depth, are exactly the candidates after it whose spans start before
    /// its span ends, so they directly follow it.
    ///
    /// The result holds at most one candidate per `$` in the value, and
    /// scanning time is linear in the value length up to a logarithmic
    /// factor. Resolution scans only one level past its depth limit.
    #[must_use]
    pub fn references(self) -> Vec<Reference> {
        self.references_within_depth(usize::MAX)
    }

    /// Candidates nested at most `max_depth` levels deep. Every returned
    /// candidate equals the corresponding one from [`Self::references`],
    /// except that parent indices address the returned list.
    pub(crate) fn references_within_depth(self, max_depth: usize) -> Vec<Reference> {
        let tokens = self.tokens();
        let Some(body) = tokens.body.clone() else {
            return Vec::new();
        };
        let dialect = self.document.dialect;
        let source = self.document.source();
        let mut pending = vec![(body, self.quote(), Level::TOP)];
        if dialect.quoted_tail == QuotedTail::Append {
            if let Some(tail) = tokens.quoted_tail.clone() {
                pending.push((tail, Quote::None, Level::TOP));
            }
        }
        let mut scanner = Scanner {
            source,
            dialect,
            max_depth,
            references: Vec::new(),
            pending,
            closes: Closes::default(),
        };
        while let Some((range, quote, level)) = scanner.pending.pop() {
            scanner.scan(&range, quote, level);
        }
        in_source_order(scanner.references)
    }
}

/// Sort candidates by start, end and depth, and point each parent index at
/// its container's sorted position.
fn in_source_order(references: Vec<Reference>) -> Vec<Reference> {
    let mut order: Vec<usize> = (0..references.len()).collect();
    order.sort_by_key(|&index| {
        let reference = &references[index];
        (reference.span.start, reference.span.end, reference.depth)
    });
    let mut position = vec![0; references.len()];
    for (sorted, &index) in order.iter().enumerate() {
        position[index] = sorted;
    }
    let mut slots: Vec<Option<Reference>> = references.into_iter().map(Some).collect();
    order
        .iter()
        .map(|&index| {
            let mut reference = slots[index].take().expect("each candidate moves once");
            reference.parent = reference.parent.map(|parent| position[parent]);
            reference
        })
        .collect()
}

/// Where a scanned range sits among the forms that contain it.
#[derive(Clone, Copy)]
struct Level {
    depth: usize,
    /// The containing form's index in scan order.
    parent: Option<usize>,
    enclosing: Enclosing,
}

impl Level {
    const TOP: Self = Self {
        depth: 0,
        parent: None,
        enclosing: Enclosing::NONE,
    };

    /// The level of a range nested directly inside `container`, the
    /// candidate at scan-order `index`.
    fn inside(self, index: usize, container: &Reference) -> Self {
        Self {
            depth: self.depth.saturating_add(1),
            parent: Some(index),
            enclosing: self.enclosing.within(container),
        }
    }
}

struct Scanner<'a> {
    source: &'a str,
    dialect: Dialect,
    max_depth: usize,
    references: Vec<Reference>,
    pending: Vec<(Range<usize>, Quote, Level)>,
    closes: Closes,
}

impl Scanner<'_> {
    fn scan(&mut self, range: &Range<usize>, quote: Quote, level: Level) {
        let source = self.source;
        let dialect = self.dialect;
        let bytes = source.as_bytes();
        let depth = level.depth;
        let nest = depth < self.max_depth;
        // The last dollar seen and how many dollars directly precede it, so a
        // run of dollars is counted once instead of once per dollar.
        let mut previous_dollar: Option<(usize, usize)> = None;
        let mut at = range.start;
        while at < range.end {
            if bytes[at] != b'$' {
                at += 1;
                continue;
            }
            let dollars = match previous_dollar {
                Some((position, before)) if position + 1 == at => before + 1,
                _ => preceding(bytes, range.start, at, b'$'),
            };
            previous_dollar = Some((at, dollars));
            let escaped = escaped_at(source, range, at, quote, dialect, depth > 0, dollars);
            if at + 1 < range.end && bytes[at + 1] == b'{' {
                let (reference, nested) =
                    braced(source, range, at, quote, escaped, level, &mut self.closes);
                let end = reference.span.end;
                let inner = level.inside(self.references.len(), &reference);
                self.references.push(reference);
                if let Some(nested) = nested.filter(|_| nest) {
                    self.pending.push((nested, quote, inner));
                }
                at = end;
                continue;
            }
            if at + 1 < range.end && bytes[at + 1] == b'(' {
                let (reference, nested) =
                    command(source, range, at, quote, escaped, level, &mut self.closes);
                let end = reference.span.end;
                let inner = level.inside(self.references.len(), &reference);
                self.references.push(reference);
                if let Some(nested) = nested.filter(|_| nest) {
                    self.pending.push((nested, quote, inner));
                }
                at = end;
                continue;
            }
            if at + 1 < range.end && (bytes[at + 1].is_ascii_alphabetic() || bytes[at + 1] == b'_')
            {
                let name_start = at + 1;
                let mut name_end = name_start + 1;
                while name_end < range.end
                    && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
                {
                    name_end += 1;
                }
                self.references.push(Reference {
                    kind: ReferenceKind::Variable,
                    span: at..name_end,
                    quote,
                    escaped,
                    depth,
                    braced: false,
                    closed: true,
                    parent: level.parent,
                    enclosing: level.enclosing,
                    name: Some(name_start..name_end),
                    operator: None,
                    operator_span: None,
                    argument: None,
                    command: None,
                });
                at = name_end;
                continue;
            }
            at += 1;
        }
    }
}

/// Closing delimiters found by earlier delimiter scans, so nested forms are
/// not scanned again at every level.
///
/// A scan that opens a form at `p` while unquoted behaves exactly like a fresh
/// scan starting at `p` until it closes that form or reaches its own bound.
/// Its outcome therefore answers a later scan from `p`: a close before the
/// later bound is the same close, and an open form at a bound no smaller than
/// the later one stays open.
#[derive(Default)]
struct Closes {
    /// `${` and `$(` forms, keyed by the position of `$`.
    forms: BTreeMap<usize, Close>,
    /// Literal brace or parenthesis groups, keyed by the opening delimiter.
    literal: BTreeMap<usize, Close>,
}

#[derive(Clone, Copy)]
struct Close {
    /// The closing delimiter, when the recording scan found it.
    at: Option<usize>,
    /// The bound of the recording scan.
    bound: usize,
}

impl Close {
    /// The result of a fresh scan bounded by `end`, when this outcome decides it.
    fn within(self, end: usize) -> Option<(Option<usize>, usize)> {
        match self.at {
            Some(at) if at < end => Some((Some(at), at)),
            Some(_) => Some((None, end)),
            None if self.bound >= end => Some((None, end)),
            None => None,
        }
    }
}

fn record(closes: &mut BTreeMap<usize, Close>, open: usize, close: Close) {
    let known = closes.get(&open).is_some_and(|existing| {
        existing.at.is_some() || (close.at.is_none() && existing.bound >= close.bound)
    });
    if !known {
        closes.insert(open, close);
    }
}

fn preceding(bytes: &[u8], start: usize, at: usize, byte: u8) -> usize {
    bytes[start..at]
        .iter()
        .rev()
        .take_while(|current| **current == byte)
        .count()
}

fn braced(
    source: &str,
    range: &Range<usize>,
    at: usize,
    quote: Quote,
    escaped: bool,
    level: Level,
    closes: &mut Closes,
) -> (Reference, Option<Range<usize>>) {
    let body_start = at + 2;
    let (close, body_end) = closes
        .forms
        .get(&at)
        .and_then(|known| known.within(range.end))
        .unwrap_or_else(|| {
            matching_delimited(source, body_start, range.end, Delimiter::Brace, closes)
        });
    let end = close.map_or(range.end, |close| close + 1);
    let (name_end, operator, operator_span) = name_operator(source, body_start, body_end);
    let argument = operator_span.as_ref().map(|span| span.end..body_end);
    (
        Reference {
            kind: ReferenceKind::Variable,
            span: at..end,
            quote,
            escaped,
            depth: level.depth,
            braced: true,
            closed: close.is_some(),
            parent: level.parent,
            enclosing: level.enclosing,
            name: Some(body_start..name_end),
            operator,
            operator_span,
            argument: argument.clone(),
            command: None,
        },
        argument.filter(|argument| !argument.is_empty()),
    )
}

fn command(
    source: &str,
    range: &Range<usize>,
    at: usize,
    quote: Quote,
    escaped: bool,
    level: Level,
    closes: &mut Closes,
) -> (Reference, Option<Range<usize>>) {
    let payload_start = at + 2;
    let (close, payload_end) = closes
        .forms
        .get(&at)
        .and_then(|known| known.within(range.end))
        .unwrap_or_else(|| {
            matching_delimited(source, payload_start, range.end, Delimiter::Paren, closes)
        });
    let end = close.map_or(range.end, |close| close + 1);
    let payload = payload_start..payload_end;
    (
        Reference {
            kind: ReferenceKind::Command,
            span: at..end,
            quote,
            escaped,
            depth: level.depth,
            braced: false,
            closed: close.is_some(),
            parent: level.parent,
            enclosing: level.enclosing,
            name: None,
            operator: None,
            operator_span: None,
            argument: None,
            command: Some(payload.clone()),
        },
        (!payload.is_empty()).then_some(payload),
    )
}

fn name_operator(
    source: &str,
    start: usize,
    end: usize,
) -> (usize, Option<ReferenceOperator>, Option<Range<usize>>) {
    let bytes = source.as_bytes();
    let mut at = start;
    while at < end {
        let found = match (bytes[at], bytes.get(at + 1).copied()) {
            (b':', Some(b'-')) => Some((ReferenceOperator::DefaultIfUnsetOrEmpty, 2)),
            (b'-', _) => Some((ReferenceOperator::DefaultIfUnset, 1)),
            (b':', Some(b'?')) => Some((ReferenceOperator::ErrorIfUnsetOrEmpty, 2)),
            (b'?', _) => Some((ReferenceOperator::ErrorIfUnset, 1)),
            (b':', Some(b'+')) => Some((ReferenceOperator::AlternativeIfSetAndNonEmpty, 2)),
            (b'+', _) => Some((ReferenceOperator::AlternativeIfSet, 1)),
            _ => None,
        };
        if let Some((operator, length)) = found {
            return (at, Some(operator), Some(at..at + length));
        }
        at += 1;
    }
    (end, None, None)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Delimiter {
    Brace,
    Paren,
}

/// Find the delimiter closing the group opened at `start - 1`, counting only
/// that delimiter pair and skipping quoted text and backslash escapes.
fn literal_balanced(
    source: &str,
    start: usize,
    end: usize,
    open: u8,
    close: u8,
    closes: &mut BTreeMap<usize, Close>,
) -> (Option<usize>, usize) {
    if let Some(known) = closes.get(&(start - 1)).and_then(|known| known.within(end)) {
        return known;
    }
    let bytes = source.as_bytes();
    // Positions of the open delimiters; the length is the nesting depth.
    let mut stack = vec![start - 1];
    let mut at = start;
    let mut quote = None;
    while at < end {
        match quote {
            Some(b'\'') => {
                if bytes[at] == b'\'' {
                    quote = None;
                }
                at += 1;
            }
            Some(_) => {
                if bytes[at] == b'\\' {
                    at = at.saturating_add(2);
                } else {
                    if bytes[at] == b'"' {
                        quote = None;
                    }
                    at += 1;
                }
            }
            None => match bytes[at] {
                b'\\' => at = at.saturating_add(2),
                b'\'' => {
                    quote = Some(b'\'');
                    at += 1;
                }
                b'"' => {
                    quote = Some(b'"');
                    at += 1;
                }
                byte if byte == open => {
                    stack.push(at);
                    at += 1;
                }
                byte if byte == close => {
                    let opened = stack.pop().expect("an open group is pending");
                    record(
                        closes,
                        opened,
                        Close {
                            at: Some(at),
                            bound: end,
                        },
                    );
                    if stack.is_empty() {
                        return (Some(at), at);
                    }
                    at += 1;
                }
                _ => at += 1,
            },
        }
    }
    for opened in stack {
        record(
            closes,
            opened,
            Close {
                at: None,
                bound: end,
            },
        );
    }
    (None, end)
}

fn matching_delimited(
    source: &str,
    start: usize,
    end: usize,
    initial: Delimiter,
    closes: &mut Closes,
) -> (Option<usize>, usize) {
    let bytes = source.as_bytes();
    // Open delimiters, with the position of `$` for `${` and `$(` forms.
    let mut stack = vec![(initial, None)];
    let mut at = start;
    let mut quote = None;
    while at < end {
        match quote {
            Some(b'\'') => {
                if bytes[at] == b'\'' {
                    quote = None;
                }
                at += 1;
            }
            Some(_) => {
                if bytes[at] == b'\\' {
                    at = at.saturating_add(2);
                } else {
                    if bytes[at] == b'"' {
                        quote = None;
                    }
                    at += 1;
                }
            }
            None => match bytes[at] {
                b'\\' if bytes.get(at + 1) == Some(&b'$') => {
                    at = skip_escaped_form(source, at + 2, end, &mut closes.literal);
                }
                b'\\' => at = at.saturating_add(2),
                b'\'' => {
                    quote = Some(b'\'');
                    at += 1;
                }
                b'"' => {
                    quote = Some(b'"');
                    at += 1;
                }
                b'$' if bytes.get(at + 1) == Some(&b'{') => {
                    stack.push((Delimiter::Brace, Some(at)));
                    at += 2;
                }
                b'$' if bytes.get(at + 1) == Some(&b'(') => {
                    stack.push((Delimiter::Paren, Some(at)));
                    at += 2;
                }
                b'(' if top(&stack) == Some(Delimiter::Paren) => {
                    stack.push((Delimiter::Paren, None));
                    at += 1;
                }
                b'}' if top(&stack) == Some(Delimiter::Brace) => {
                    if close_form(&mut stack, at, end, closes) {
                        return (Some(at), at);
                    }
                    at += 1;
                }
                b')' if top(&stack) == Some(Delimiter::Paren) => {
                    if close_form(&mut stack, at, end, closes) {
                        return (Some(at), at);
                    }
                    at += 1;
                }
                _ => at += 1,
            },
        }
    }
    for (_, opened) in stack {
        if let Some(opened) = opened {
            record(
                &mut closes.forms,
                opened,
                Close {
                    at: None,
                    bound: end,
                },
            );
        }
    }
    (None, end)
}

/// Skip the literal group of an escaped `\${` or `\$(` form whose opening
/// delimiter, if any, is at `at`; returns where scanning resumes.
fn skip_escaped_form(
    source: &str,
    at: usize,
    end: usize,
    closes: &mut BTreeMap<usize, Close>,
) -> usize {
    let (open, close) = match source.as_bytes().get(at) {
        Some(b'{') => (b'{', b'}'),
        Some(b'(') => (b'(', b')'),
        _ => return at,
    };
    let (found, end) = literal_balanced(source, at + 1, end, open, close, closes);
    found.map_or(end, |found| found + 1)
}

fn top(stack: &[(Delimiter, Option<usize>)]) -> Option<Delimiter> {
    stack.last().map(|(delimiter, _)| *delimiter)
}

/// Pop the innermost delimiter closed at `at`, recording a `$` form's close.
/// Returns whether the scan's initial delimiter closed.
fn close_form(
    stack: &mut Vec<(Delimiter, Option<usize>)>,
    at: usize,
    end: usize,
    closes: &mut Closes,
) -> bool {
    if let Some((_, Some(opened))) = stack.pop() {
        record(
            &mut closes.forms,
            opened,
            Close {
                at: Some(at),
                bound: end,
            },
        );
    }
    stack.is_empty()
}

fn escaped_at(
    source: &str,
    range: &Range<usize>,
    at: usize,
    quote: Quote,
    dialect: Dialect,
    nested: bool,
    dollars: usize,
) -> bool {
    let bytes = source.as_bytes();
    let backslashes = preceding(bytes, range.start, at, b'\\');
    let backslash = backslashes % 2 == 1
        && ((quote == Quote::Double && dialect.double_escapes.contains(DoubleEscapes::DOLLAR))
            || (quote == Quote::None && nested));
    let dollar = dialect.syntax == Syntax::Compose && (dollars + 1) % 2 == 0;
    backslash || dollar
}
