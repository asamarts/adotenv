//! Literal reference inspection without resolution or execution.
//!
//! References are reported as raw candidates with original-source spans. A
//! separate eligibility helper applies caller-selected quote, escape, operator
//! and command rules; this module never reads a lookup, environment or file.

use alloc::{collections::BTreeMap, vec, vec::Vec};
use core::ops::Range;

use crate::{Dialect, DoubleEscapes, Entry, Quote, QuotedTail, Syntax};

/// Whether a reference candidate is a variable or a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReferenceKind {
    /// A variable, such as `$VAR`, `${VAR}` or `${VAR:-default}`.
    Variable,
    /// A command substitution, such as `$(git rev-parse HEAD)`.
    Command,
}

/// The operator of a braced variable, such as `:-` in `${VAR:-default}`.
///
/// The text after the operator is the candidate's
/// [`argument`](Reference::argument). Inspection only records the operator;
/// [`Document::resolve`](crate::Document::resolve) applies it when the rules
/// allow operators ([`OperatorSet::Compose`] or [`OperatorSet::DotenvExpand`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReferenceOperator {
    /// `${VAR-default}`: use the argument when the variable is unset.
    DefaultIfUnset,
    /// `${VAR:-default}`: use the argument when the variable is unset or empty.
    DefaultIfUnsetOrEmpty,
    /// `${VAR?message}`: report an error when the variable is unset.
    ErrorIfUnset,
    /// `${VAR:?message}`: report an error when the variable is unset or empty.
    ErrorIfUnsetOrEmpty,
    /// `${VAR+alternative}`: use the argument when the variable is set.
    AlternativeIfSet,
    /// `${VAR:+alternative}`: use the argument when the variable is set and
    /// not empty.
    AlternativeIfSetAndNonEmpty,
}

/// Which variable forms [`ReferenceRules`] accept.
///
/// | Set | `$VAR` | `${VAR}` | Operators, as in `${VAR:-x}` | Nested candidates |
/// |---|---|---|---|---|
/// | [`None`](Self::None) | no | no | no | no |
/// | [`Braced`](Self::Braced) | no | yes | no | no |
/// | [`Basic`](Self::Basic) | yes | yes | no | no |
/// | [`Compose`](Self::Compose) | yes | yes | yes | yes |
/// | [`DotenvExpand`](Self::DotenvExpand) | yes | yes | yes | yes |
///
/// A nested candidate sits inside an operator argument or a command, such as
/// `$B` in `${A:-$B}`. Commands themselves depend on
/// [`ReferenceRules::commands`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OperatorSet {
    /// No variable is eligible.
    ///
    /// A command at the top of a value can still be eligible when
    /// [`ReferenceRules::commands`] is on.
    None,
    /// Only `${VAR}`, without operators or nested candidates.
    Braced,
    /// `$VAR` and `${VAR}`, without operators or nested candidates.
    Basic,
    /// Every variable form and operator, and nested candidates.
    Compose,
    /// The same forms as [`Compose`](Self::Compose), used by
    /// [`ReferenceRules::DOTENV_EXPAND`].
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

/// Rules that decide which reference candidates could expand.
///
/// [`Reference::is_eligible`] checks one candidate against these rules, and
/// [`ExpandOptions`](crate::ExpandOptions) uses them to resolve a document.
/// The rules only classify candidates; nothing is looked up or run.
///
/// | Preset | Operators | Single quotes | Escaped `$` | Commands | Backticks |
/// |---|---|---|---|---|---|
/// | [`NONE`](Self::NONE) | [`None`](OperatorSet::None) | no | no | no | no |
/// | [`PORTABLE`](Self::PORTABLE) (default) | [`Basic`](OperatorSet::Basic) | no | no | no | no |
/// | [`COMPOSE`](Self::COMPOSE) | [`Compose`](OperatorSet::Compose) | no | no | no | no |
/// | [`DOTENV_EXPAND`](Self::DOTENV_EXPAND) | [`DotenvExpand`](OperatorSet::DotenvExpand) | yes | no | yes | no |
///
/// The struct is `#[non_exhaustive]`, so start from a preset and change single
/// fields with the `with_*` builders.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict, OperatorSet, ReferenceRules};
///
/// let doc = parse_strict("GREETING='hello ${USER:-guest}'\n")?;
/// let references = doc.unique("GREETING")?.references();
/// assert!(!references[0].is_eligible(ReferenceRules::PORTABLE));
///
/// let rules = ReferenceRules::PORTABLE
///     .with_operators(OperatorSet::Compose)
///     .with_single_quotes(true);
/// assert!(references[0].is_eligible(rules));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
// These are independent policy switches, not states.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ReferenceRules {
    /// Which variable forms and operators are eligible.
    pub operators: OperatorSet,
    /// Whether candidates inside single-quoted values are eligible.
    pub single_quotes: bool,
    /// Whether escaped candidates are eligible, such as `\$VAR` in double
    /// quotes or `\$` inside a command.
    ///
    /// A backslash inside a `${...}` operand is ordinary text, not an escape.
    /// See [`Reference::escaped`].
    pub escaped_dollar: bool,
    /// Whether `$(command)` candidates are eligible.
    ///
    /// Candidates inside a command need it too.
    pub commands: bool,
    /// Whether candidates inside backtick-quoted values are eligible.
    pub backtick_quotes: bool,
}

impl ReferenceRules {
    /// Sets [`operators`](Self::operators): which variable forms are eligible.
    #[must_use]
    pub const fn with_operators(mut self, value: OperatorSet) -> Self {
        self.operators = value;
        self
    }
    /// Sets [`single_quotes`](Self::single_quotes): whether single-quoted candidates count.
    #[must_use]
    pub const fn with_single_quotes(mut self, value: bool) -> Self {
        self.single_quotes = value;
        self
    }
    /// Sets [`escaped_dollar`](Self::escaped_dollar): whether escaped candidates count.
    #[must_use]
    pub const fn with_escaped_dollar(mut self, value: bool) -> Self {
        self.escaped_dollar = value;
        self
    }
    /// Sets [`commands`](Self::commands): whether `$(command)` candidates count.
    #[must_use]
    pub const fn with_commands(mut self, value: bool) -> Self {
        self.commands = value;
        self
    }
    /// Sets [`backtick_quotes`](Self::backtick_quotes): whether backtick-quoted candidates count.
    #[must_use]
    pub const fn with_backtick_quotes(mut self, value: bool) -> Self {
        self.backtick_quotes = value;
        self
    }
}
impl ReferenceRules {
    /// No candidate is eligible.
    pub const NONE: Self = Self {
        operators: OperatorSet::None,
        single_quotes: false,
        escaped_dollar: false,
        commands: false,
        backtick_quotes: false,
    };
    /// Portable rules: `$VAR` and `${VAR}` outside single quotes and backticks.
    ///
    /// No operators, nesting, commands or escaped candidates. These are
    /// [`ReferenceRules::default`] and the rules of
    /// [`ExpandOptions::portable`](crate::ExpandOptions::portable).
    pub const PORTABLE: Self = Self {
        operators: OperatorSet::Basic,
        single_quotes: false,
        escaped_dollar: false,
        commands: false,
        backtick_quotes: false,
    };
    /// Compose-style rules: every variable form, operator and nesting, but no
    /// commands.
    ///
    /// These are eligibility rules, not a copy of Docker Compose
    /// interpolation. Under the Compose dialect an escaped `$$B` is not
    /// eligible, so resolution leaves `$$B` as written, while Compose prints
    /// `$B`.
    pub const COMPOSE: Self = Self {
        operators: OperatorSet::Compose,
        single_quotes: false,
        escaped_dollar: false,
        commands: false,
        backtick_quotes: false,
    };
    /// dotenv-expand-style rules: every variable form, operator and nesting,
    /// also inside single quotes, plus command substitution.
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

/// A `$` reference candidate found in an entry's value.
///
/// [`Entry::references`] finds candidates in the value as written: `$VAR`,
/// `${VAR}`, the operator forms such as `${VAR:-default}`, and `$(command)`.
/// A candidate is only an observation. Nothing is looked up, expanded or run,
/// and escaped, single-quoted and malformed candidates are reported too.
/// [`is_eligible`](Self::is_eligible) decides whether a candidate could
/// expand under [`ReferenceRules`];
/// [`Document::resolve`](crate::Document::resolve) expands eligible ones.
///
/// Every span is a byte range into the document's source
/// ([`Document::source`](crate::Document::source)), not into the decoded
/// value.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict, ReferenceKind, ReferenceOperator};
///
/// let source = "URL=${HOST:-localhost}:$PORT\n";
/// let doc = parse_strict(source)?;
/// let references = doc.unique("URL")?.references();
/// assert_eq!(references.len(), 2);
///
/// let host = &references[0];
/// assert_eq!(&source[host.span.clone()], "${HOST:-localhost}");
/// assert_eq!(host.name.clone().map(|name| &source[name]), Some("HOST"));
/// assert_eq!(host.operator, Some(ReferenceOperator::DefaultIfUnsetOrEmpty));
/// assert_eq!(host.argument.clone().map(|text| &source[text]), Some("localhost"));
///
/// assert_eq!(&source[references[1].span.clone()], "$PORT");
/// assert_eq!(references[1].kind, ReferenceKind::Variable);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Reference {
    /// Whether this is a variable or a command.
    pub kind: ReferenceKind,
    /// The byte range of the whole candidate in the source, starting at `$`.
    ///
    /// An unclosed form extends to the end of the value, or of the argument
    /// or command that contains it.
    pub span: Range<usize>,
    /// The quote style of the value that contains the candidate.
    ///
    /// Text appended after a closing quote under
    /// [`QuotedTail::Append`] counts as unquoted.
    pub quote: Quote,
    /// Whether the `$` is escaped.
    ///
    /// An odd number of backslashes before the `$` escapes it in a
    /// double-quoted value whose dialect decodes `\$`
    /// ([`DoubleEscapes::DOLLAR`]), and inside a command of an unquoted value.
    /// At the top of an unquoted value a backslash does not escape: in
    /// `A=\$HOME` the candidate `$HOME` is not escaped. A backslash inside a
    /// `${...}` operand is ordinary text, as in Compose and dotenv-expand.
    /// Under the Compose grammar, the second `$` of `$$` is escaped too.
    /// Escaped candidates are eligible only with
    /// [`ReferenceRules::escaped_dollar`].
    pub escaped: bool,
    /// How deeply the candidate is nested.
    ///
    /// It is `0` at the top of the value and `1` inside the argument or
    /// command of a top-level form.
    pub depth: usize,
    /// Whether this is the braced variable form `${...}`.
    pub braced: bool,
    /// Whether the closing `}` or `)` was found.
    ///
    /// `$VAR` is always closed.
    pub closed: bool,
    /// The index of the innermost form that contains this candidate, or `None`
    /// at the top of the value.
    ///
    /// The index points into the list returned by [`Entry::references`]. The
    /// container is an operator form or a command; it comes earlier in the
    /// list, its span contains this candidate's span, and its depth is one
    /// less.
    pub parent: Option<usize>,
    /// The chain of containing forms, summarized for eligibility.
    enclosing: Enclosing,
    /// The byte range of the variable name, or `None` for a command.
    ///
    /// It is empty for `${}`. The spelling of a braced name is not checked.
    pub name: Option<Range<usize>>,
    /// The operator of a braced variable, if it has one.
    pub operator: Option<ReferenceOperator>,
    /// The byte range of the operator, such as `:-`, if there is one.
    pub operator_span: Option<Range<usize>>,
    /// The byte range of the text after the operator: the default, message or
    /// alternative.
    pub argument: Option<Range<usize>>,
    /// The byte range of the command text between `$(` and `)`, or `None` for
    /// a variable.
    pub command: Option<Range<usize>>,
}

impl Reference {
    /// Checks whether this candidate could expand under `rules`.
    ///
    /// The check looks only at the candidate and the forms that contain it.
    /// A candidate is eligible when all of these hold:
    ///
    /// - it is closed, and a variable has a nonempty name;
    /// - [`rules.operators`](ReferenceRules::operators) accepts its form and
    ///   operator, and accepts nesting if the candidate is nested;
    /// - it is single-quoted, backtick-quoted or escaped only where the rules
    ///   allow that;
    /// - it is a command only when [`rules.commands`](ReferenceRules::commands)
    ///   is on;
    /// - every form that contains it is closed, named and allowed by the same
    ///   rules.
    ///
    /// It does not look up variables, check the spelling of braced names or
    /// run commands.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict, ReferenceRules};
    ///
    /// let doc = parse_strict("URL=${HOST:-$FALLBACK}/api\n")?;
    /// let references = doc.unique("URL")?.references();
    /// assert_eq!(references.len(), 2);
    ///
    /// // Portable rules accept neither operators nor nesting.
    /// assert!(!references[0].is_eligible(ReferenceRules::PORTABLE));
    /// assert!(!references[1].is_eligible(ReferenceRules::PORTABLE));
    /// assert!(references[0].is_eligible(ReferenceRules::COMPOSE));
    /// assert!(references[1].is_eligible(ReferenceRules::COMPOSE));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
    /// Returns the `$` reference candidates in this entry's value, in source order.
    ///
    /// The value is scanned as written in the source, with its quote style:
    /// `$VAR`, `${VAR}`, the operator forms such as `${VAR:-default}`, and
    /// `$(command)`, including candidates nested in operator arguments and
    /// commands. Escaped, single-quoted and malformed candidates are returned
    /// too. Nothing is looked up, expanded or run: use
    /// [`Reference::is_eligible`] to apply [`ReferenceRules`], or
    /// [`Document::resolve`](crate::Document::resolve) to expand. A bare key
    /// or a value without `$` has no candidates.
    ///
    /// Inside `${...}`, the operand ends at the first `}` outside a nested
    /// form. Quotes and backslashes are ordinary text there, as in Compose
    /// and dotenv-expand. Inside `$(...)`, shell quoting applies.
    ///
    /// Candidates are sorted by span start. A nested candidate's
    /// [`Reference::parent`] is the index of its innermost containing form,
    /// which comes earlier in the list. The candidates nested in a form, at
    /// any depth, are exactly the candidates after it whose spans start before
    /// its span ends, so they directly follow it.
    ///
    /// The list holds at most one candidate per `$` in the value, and scanning
    /// takes time linear in the value length, up to a logarithmic factor.
    /// Each candidate occupies roughly
    /// [`size_of::<Reference>()`](core::mem::size_of), so a value with many
    /// candidates can allocate much more memory than its own length. Bound the
    /// value before calling this on untrusted input;
    /// [`Document::resolve`](crate::Document::resolve) does so with
    /// [`ExpandOptions::max_references`](crate::ExpandOptions::max_references).
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict, ReferenceKind};
    ///
    /// let source = "REVISION=${TAG:-$(git describe)}\n";
    /// let doc = parse_strict(source)?;
    /// let references = doc.unique("REVISION")?.references();
    ///
    /// let spans: Vec<_> = references.iter().map(|r| &source[r.span.clone()]).collect();
    /// assert_eq!(spans, ["${TAG:-$(git describe)}", "$(git describe)"]);
    /// assert_eq!(references[1].kind, ReferenceKind::Command);
    /// assert_eq!(references[1].parent, Some(0));
    /// assert_eq!(references[1].depth, 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn references(self) -> Vec<Reference> {
        self.references_within_depth(usize::MAX)
    }

    /// Candidates nested at most `max_depth` levels deep. Every returned
    /// candidate equals the corresponding one from [`Self::references`],
    /// except that parent indices address the returned list.
    pub(crate) fn references_within_depth(self, max_depth: usize) -> Vec<Reference> {
        self.scan_references(max_depth, None)
            .expect("an unbounded reference scan cannot exceed its limit")
    }

    /// Like [`Self::references_within_depth`], but stops before allocating
    /// more than `max_references` candidates and returns `None` instead.
    pub(crate) fn references_within_depth_bounded(
        self,
        max_depth: usize,
        max_references: usize,
    ) -> Option<Vec<Reference>> {
        self.scan_references(max_depth, Some(max_references))
    }

    fn scan_references(
        self,
        max_depth: usize,
        max_references: Option<usize>,
    ) -> Option<Vec<Reference>> {
        let tokens = self.tokens();
        let Some(body) = tokens.body.clone() else {
            return Some(Vec::new());
        };
        let dialect = self.document.dialect;
        let source = self.document.source();
        let tail = tokens
            .quoted_tail
            .clone()
            .filter(|_| dialect.quoted_tail == QuotedTail::Append);
        // Every candidate starts at a `$`, so most values need no scan.
        let has_dollar = |range: &Range<usize>| source.as_bytes()[range.clone()].contains(&b'$');
        if !has_dollar(&body) && !tail.as_ref().is_some_and(has_dollar) {
            return Some(Vec::new());
        }
        let mut pending = vec![(body, self.quote(), Level::TOP, true)];
        if let Some(tail) = tail {
            pending.push((tail, Quote::None, Level::TOP, true));
        }
        let closes = Closes {
            limit: max_depth.checked_add(2).filter(|_| max_depth != usize::MAX),
            ..Closes::default()
        };
        let mut scanner = Scanner {
            source,
            dialect,
            max_depth,
            references: Vec::new(),
            pending,
            closes,
            max_references,
            exceeded: false,
        };
        while let Some((range, quote, level, backslash_escapes)) = scanner.pending.pop() {
            scanner.scan(&range, quote, level, backslash_escapes);
            if scanner.exceeded || scanner.closes.exceeded {
                return None;
            }
        }
        Some(in_source_order(scanner.references))
    }
}

/// Sort candidates by start, end and depth, and point each parent index at
/// its container's sorted position.
fn in_source_order(references: Vec<Reference>) -> Vec<Reference> {
    let key = |reference: &Reference| (reference.span.start, reference.span.end, reference.depth);
    // Without nesting the scan already yields source order, and the stable
    // sort below would keep every index, so skip the reordering.
    if references
        .windows(2)
        .all(|pair| key(&pair[0]) <= key(&pair[1]))
    {
        return references;
    }
    let mut order: Vec<usize> = (0..references.len()).collect();
    order.sort_by_key(|&index| key(&references[index]));
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
    pending: Vec<(Range<usize>, Quote, Level, bool)>,
    closes: Closes,
    max_references: Option<usize>,
    exceeded: bool,
}

impl Scanner<'_> {
    /// Records `reference`, or reports that the configured scan limit is
    /// reached. A limit of zero rejects the first candidate.
    fn record(&mut self, reference: Reference) -> bool {
        if self
            .max_references
            .is_some_and(|limit| self.references.len() >= limit)
        {
            self.exceeded = true;
            return false;
        }
        self.references.push(reference);
        true
    }

    fn scan(&mut self, range: &Range<usize>, quote: Quote, level: Level, backslash_escapes: bool) {
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
            let escaped = escaped_at(
                source,
                range,
                at,
                quote,
                dialect,
                EscapeMode {
                    nested: depth > 0,
                    backslash: backslash_escapes,
                },
                dollars,
            );
            if at + 1 < range.end && bytes[at + 1] == b'{' {
                let (reference, nested) =
                    braced(source, range, at, quote, escaped, level, &mut self.closes);
                if self.closes.exceeded {
                    return;
                }
                let end = reference.span.end;
                let inner = level.inside(self.references.len(), &reference);
                if !self.record(reference) {
                    return;
                }
                if let Some(nested) = nested.filter(|_| nest) {
                    self.pending.push((nested, quote, inner, false));
                }
                at = end;
                continue;
            }
            if at + 1 < range.end && bytes[at + 1] == b'(' {
                let (reference, nested) =
                    command(source, range, at, quote, escaped, level, &mut self.closes);
                if self.closes.exceeded {
                    return;
                }
                let end = reference.span.end;
                let inner = level.inside(self.references.len(), &reference);
                if !self.record(reference) {
                    return;
                }
                if let Some(nested) = nested.filter(|_| nest) {
                    self.pending.push((nested, quote, inner, true));
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
                let reference = Reference {
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
                };
                if !self.record(reference) {
                    return;
                }
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
    /// Maximum delimiter nesting kept while scanning, or `None` unbounded.
    limit: Option<usize>,
    /// Set when the delimiter scan exceeded `limit`.
    exceeded: bool,
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

impl Closes {
    /// Report whether another open delimiter would exceed the scan budget.
    fn too_deep(&mut self, depth: usize) -> bool {
        if self.limit.is_some_and(|limit| depth > limit) {
            self.exceeded = true;
            true
        } else {
            false
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
    closes: &mut Closes,
) -> (Option<usize>, usize) {
    if let Some(known) = closes
        .literal
        .get(&(start - 1))
        .and_then(|known| known.within(end))
    {
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
                    if closes.too_deep(stack.len() + 1) {
                        return (None, end);
                    }
                    stack.push(at);
                    at += 1;
                }
                byte if byte == close => {
                    let opened = stack.pop().expect("an open group is pending");
                    record(
                        &mut closes.literal,
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
            &mut closes.literal,
            opened,
            Close {
                at: None,
                bound: end,
            },
        );
    }
    (None, end)
}

#[allow(clippy::too_many_lines)]
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
        // A brace form uses the operand grammar of Compose and dotenv-expand:
        // the first `}` outside a nested form closes it, and quotes and
        // backslashes are ordinary text. A command keeps shell grammar, where
        // quotes and backslashes protect the closing `)`.
        if top(&stack) == Some(Delimiter::Brace) {
            match bytes[at] {
                b'$' if bytes.get(at + 1) == Some(&b'{') => {
                    stack.push((Delimiter::Brace, Some(at)));
                    at += 2;
                }
                b'$' if bytes.get(at + 1) == Some(&b'(') => {
                    stack.push((Delimiter::Paren, Some(at)));
                    at += 2;
                }
                b'}' => {
                    if close_form(&mut stack, at, end, closes) {
                        return (Some(at), at);
                    }
                    at += 1;
                }
                _ => at += 1,
            }
            continue;
        }
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
                    at = skip_escaped_form(source, at + 2, end, closes);
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
                    if closes.too_deep(stack.len() + 1) {
                        return (None, end);
                    }
                    stack.push((Delimiter::Brace, Some(at)));
                    at += 2;
                }
                b'$' if bytes.get(at + 1) == Some(&b'(') => {
                    if closes.too_deep(stack.len() + 1) {
                        return (None, end);
                    }
                    stack.push((Delimiter::Paren, Some(at)));
                    at += 2;
                }
                b'(' if top(&stack) == Some(Delimiter::Paren) => {
                    if closes.too_deep(stack.len() + 1) {
                        return (None, end);
                    }
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
fn skip_escaped_form(source: &str, at: usize, end: usize, closes: &mut Closes) -> usize {
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

#[derive(Clone, Copy)]
struct EscapeMode {
    nested: bool,
    backslash: bool,
}

fn escaped_at(
    source: &str,
    range: &Range<usize>,
    at: usize,
    quote: Quote,
    dialect: Dialect,
    mode: EscapeMode,
    dollars: usize,
) -> bool {
    let bytes = source.as_bytes();
    let backslashes = preceding(bytes, range.start, at, b'\\');
    let backslash = mode.backslash
        && backslashes % 2 == 1
        && ((quote == Quote::Double && dialect.double_escapes.contains(DoubleEscapes::DOLLAR))
            || mode.nested);
    let dollar = dialect.syntax == Syntax::Compose && (dollars + 1) % 2 == 0;
    backslash || dollar
}
