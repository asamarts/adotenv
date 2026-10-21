//! Opt-in literal variable resolution with explicit lookup and origins.
//!
//! Resolution never reads the process environment, files or commands. A caller
//! supplies a lookup closure; command substitutions remain literal in this
//! slice. Every substituted value records its direct source.

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{fmt, ops::Range};

use crate::{
    AssignmentState, BareKeys, Document, Entry, Reference, ReferenceKind, ReferenceOperator,
    ReferenceRules,
};

/// Whether later definitions can see earlier definitions or the final file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExpandScope {
    /// A reference sees the latest participating declaration before it.
    Sequential,
    /// A reference sees the final participating declaration in the file.
    WholeFile,
}

/// Whether the caller lookup or the document's own bindings win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LookupOrder {
    /// Consult the caller lookup before document bindings.
    CallerFirst,
    /// Consult document bindings before the caller lookup. A participating
    /// declaration with no value shadows the caller lookup.
    FileFirst,
}

/// Treatment of an eligible reference with no value from the lookup order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Unresolved {
    /// Leave the reference exactly as written.
    Keep,
    /// Substitute an empty string.
    Empty,
    /// Return [`ResolveError::Unresolved`].
    Error,
}

/// Explicit resolution policy and resource limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ExpandOptions {
    /// Quote, escape and operator eligibility rules.
    pub rules: ReferenceRules,
    /// Sequential or whole-file bindings.
    pub scope: ExpandScope,
    /// Caller lookup versus file binding precedence.
    pub lookup_order: LookupOrder,
    /// Treatment of an eligible reference with no value.
    pub unresolved: Unresolved,
    /// Maximum active binding and reference nesting depth.
    pub max_depth: usize,
    /// Maximum resolved bytes for one entry value.
    pub max_value_bytes: usize,
}

impl ExpandOptions {
    /// Use the given reference eligibility rules.
    #[must_use]
    pub const fn with_rules(mut self, value: ReferenceRules) -> Self {
        self.rules = value;
        self
    }
    /// Use the given resolution scope.
    #[must_use]
    pub const fn with_scope(mut self, value: ExpandScope) -> Self {
        self.scope = value;
        self
    }
    /// Use the given lookup precedence.
    #[must_use]
    pub const fn with_lookup_order(mut self, value: LookupOrder) -> Self {
        self.lookup_order = value;
        self
    }
    /// Use the given unresolved-reference policy.
    #[must_use]
    pub const fn with_unresolved(mut self, value: Unresolved) -> Self {
        self.unresolved = value;
        self
    }
    /// Set the maximum nesting depth.
    #[must_use]
    pub const fn with_max_depth(mut self, value: usize) -> Self {
        self.max_depth = value;
        self
    }
    /// Set the maximum resolved value size.
    #[must_use]
    pub const fn with_max_value_bytes(mut self, value: usize) -> Self {
        self.max_value_bytes = value;
        self
    }
}
impl ExpandOptions {
    /// Portable-style basic variables with file-first sequential bindings.
    #[must_use]
    pub const fn portable() -> Self {
        Self {
            rules: ReferenceRules::PORTABLE,
            scope: ExpandScope::Sequential,
            lookup_order: LookupOrder::FileFirst,
            unresolved: Unresolved::Keep,
            max_depth: 64,
            max_value_bytes: 1_048_576,
        }
    }
    /// Compose-style operators and nesting with file-first sequential bindings.
    #[must_use]
    pub const fn compose() -> Self {
        Self {
            rules: ReferenceRules::COMPOSE,
            ..Self::portable()
        }
    }
    /// dotenv-expand-style rules with file-first sequential bindings.
    #[must_use]
    pub const fn dotenv_expand() -> Self {
        Self {
            rules: ReferenceRules::DOTENV_EXPAND,
            ..Self::portable()
        }
    }
    /// No variable forms are eligible; values remain literal.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            rules: ReferenceRules::NONE,
            ..Self::portable()
        }
    }
}

impl Default for ExpandOptions {
    fn default() -> Self {
        Self::portable()
    }
}

/// Structured input passed to a command-substitution callback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommandContext<'a> {
    /// Command text exactly as written between `$(` and `)`.
    pub command: &'a str,
    /// Whole `$(...)` reference span in the original source.
    pub reference: Range<usize>,
    /// Raw command payload span in the original source.
    pub payload: Range<usize>,
    /// Optional caller-supplied source identity; never inferred from a path.
    pub source: Option<&'a str>,
}

/// An error returned by a command-substitution callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandError {
    message: String,
}

impl CommandError {
    /// Create a callback error with a caller-provided message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
    /// The caller-provided message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl core::error::Error for CommandError {}

/// Recorded outcome of one invoked command callback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CommandOutcome {
    /// The callback returned this output.
    Output(String),
    /// The callback returned this error message.
    Error(String),
}

/// Provenance for one invoked command callback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommandCall {
    /// Whole `$(...)` reference span in the original source.
    pub reference: Range<usize>,
    /// Raw command payload span in the original source.
    pub payload: Range<usize>,
    /// Caller-supplied source identity, if any.
    pub source: Option<String>,
    /// Recorded output or error.
    pub outcome: CommandOutcome,
}

/// A resolution failure that retains command calls already made.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolveFailure {
    /// The structured resolution error.
    pub error: ResolveError,
    /// Command callbacks invoked before the failure, in invocation order.
    pub commands: Vec<CommandCall>,
}

impl fmt::Display for ResolveFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl core::error::Error for ResolveFailure {}

/// Where one resolved variable value came from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValueOrigin {
    /// A participating declaration in this document.
    File {
        /// Snapshot-local entry identity.
        entry: usize,
        /// Binding key.
        key: String,
        /// Original value-token or line span.
        span: Range<usize>,
    },
    /// The caller-supplied lookup.
    Lookup,
}

/// One resolved value part.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolvedSegment {
    /// Exact text contributed by this part.
    pub value: String,
    /// Provenance and structural role.
    pub kind: ResolvedSegmentKind,
}

/// The role of one resolved segment.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolvedSegmentKind {
    /// Literal decoded source text.
    Literal {
        /// Original-source region, when representable.
        span: Range<usize>,
    },
    /// A substituted variable reference.
    Variable {
        /// Referenced name.
        name: String,
        /// Whole reference span in the original source.
        reference: Range<usize>,
        /// Variable operator, when present.
        operator: Option<ReferenceOperator>,
        /// Direct value origin; absent for an empty unresolved substitution.
        origin: Option<ValueOrigin>,
        /// Nested provenance of a file binding; empty for caller lookup.
        parts: Vec<ResolvedSegment>,
    },
    /// An eligible reference left literal by [`Unresolved::Keep`].
    Unresolved {
        /// Referenced name.
        name: String,
        /// Whole reference span in the original source.
        reference: Range<usize>,
    },
    /// Output of a caller-supplied command callback.
    Command {
        /// Whole `$(...)` reference span in the original source.
        reference: Range<usize>,
        /// Raw command payload span in the original source.
        payload: Range<usize>,
        /// Caller-supplied source identity, if any.
        source: Option<String>,
    },
}

/// One entry's resolved value and provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolvedEntry {
    /// Snapshot-local entry identity from the originating document.
    pub entry: usize,
    /// Literal key.
    pub key: String,
    /// Original assignment state.
    pub state: AssignmentState,
    /// Resolved assigned value; `None` for a bare declaration.
    pub value: Option<String>,
    /// Effective literal lookup value after participation and bare-key policy.
    pub effective_value: Option<String>,
    /// Ordered provenance segments; concatenation equals `value`.
    pub segments: Vec<ResolvedSegment>,
}

/// All resolved entries in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDocument {
    entries: Vec<ResolvedEntry>,
    commands: Vec<CommandCall>,
}

impl ResolvedDocument {
    /// Resolved entries in source order.
    #[must_use]
    pub fn entries(&self) -> &[ResolvedEntry] {
        &self.entries
    }
    /// Command callbacks invoked during resolution, in invocation order.
    #[must_use]
    pub fn commands(&self) -> &[CommandCall] {
        &self.commands
    }
    /// Number of resolved entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Whether there are no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A resolution failure.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveError {
    /// An eligible variable had no value under [`Unresolved::Error`].
    Unresolved {
        /// Entry being resolved.
        key: String,
        /// Referenced name.
        name: String,
        /// Reference span in the original source.
        span: Range<usize>,
    },
    /// An `:?` or `?` operator required a value.
    Required {
        /// Entry being resolved.
        key: String,
        /// Referenced name.
        name: String,
        /// Operator message.
        message: String,
        /// Reference span in the original source.
        span: Range<usize>,
    },
    /// Reachable bindings formed a cycle.
    Cycle {
        /// Entry where the cycle was detected.
        key: String,
        /// Keys in cycle order, including the repeated final key.
        chain: Vec<String>,
    },
    /// The configured depth limit was reached.
    DepthExceeded {
        /// Entry being resolved.
        key: String,
        /// Configured limit.
        limit: usize,
        /// Reference or entry span.
        span: Range<usize>,
    },
    /// One resolved value exceeded the configured byte limit.
    ValueTooLarge {
        /// Entry being resolved.
        key: String,
        /// Configured limit.
        limit: usize,
        /// Entry value or line span.
        span: Range<usize>,
    },
    /// A command callback failed.
    Command {
        /// Entry being resolved.
        key: String,
        /// Whole `$(...)` reference span.
        reference: Range<usize>,
        /// Callback error message.
        message: String,
    },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unresolved { key, name, .. } => {
                write!(
                    formatter,
                    "unresolved variable {name} while resolving {key}"
                )
            }
            Self::Required {
                key, name, message, ..
            } => write!(
                formatter,
                "required variable {name} is unset or empty while resolving {key}: {message}"
            ),
            Self::Cycle { key, chain } => {
                write!(formatter, "reference cycle while resolving {key}: ")?;
                for (index, name) in chain.iter().enumerate() {
                    if index != 0 {
                        formatter.write_str(" -> ")?;
                    }
                    formatter.write_str(name)?;
                }
                Ok(())
            }
            Self::DepthExceeded { key, limit, .. } => {
                write!(
                    formatter,
                    "reference depth limit {limit} exceeded while resolving {key}"
                )
            }
            Self::ValueTooLarge { key, limit, .. } => {
                write!(formatter, "resolved value for {key} exceeds {limit} bytes")
            }
            Self::Command { key, message, .. } => {
                write!(
                    formatter,
                    "command callback failed while resolving {key}: {message}"
                )
            }
        }
    }
}

impl core::error::Error for ResolveError {}

impl Document<'_> {
    /// Resolve eligible variable references with an explicit caller lookup.
    ///
    /// The lookup closure is consulted only when the selected order reaches it;
    /// no process environment, file or command access occurs. Command forms and
    /// ineligible references remain literal in this slice. Every substituted
    /// variable records its direct file or caller origin and, for file bindings,
    /// nested provenance.
    ///
    /// # Errors
    /// Returns a structured error for required variables, unresolved variables
    /// under [`Unresolved::Error`], reachable cycles, or configured limits.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict, ExpandOptions};
    ///
    /// let doc = parse_strict("A=base\nB=${A}-suffix\n").unwrap();
    /// let resolved = doc
    ///     .resolve(|_| None, &ExpandOptions::portable())
    ///     .unwrap();
    /// assert_eq!(resolved.entries()[1].value.as_deref(), Some("base-suffix"));
    /// ```
    pub fn resolve<F>(
        &self,
        lookup: F,
        options: &ExpandOptions,
    ) -> Result<ResolvedDocument, ResolveError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let mut resolver = Resolver {
            document: self,
            lookup,
            options: *options,
            bindings: BTreeMap::new(),
            cache: BTreeMap::new(),
            stack: Vec::new(),
            commands: None,
            source_identity: None,
            command_calls: Vec::new(),
        };
        resolver.resolve_document()
    }

    /// Resolve with an explicit command-substitution callback.
    ///
    /// Commands are invoked only when both [`ReferenceRules::commands`] and this
    /// callback permit them. The callback receives the raw command text, source
    /// spans and the supplied source identity. Outputs are literal resolved data.
    /// Every call is recorded; [`ResolveFailure::commands`] retains calls made
    /// before a later error. No shell, environment or file access is implied.
    ///
    /// # Errors
    /// Returns the same structured errors as [`Self::resolve`], wraps callback
    /// failures, and retains prior command-call provenance in [`ResolveFailure`].
    pub fn resolve_with_commands<F, C>(
        &self,
        lookup: F,
        mut commands: C,
        source_identity: Option<&str>,
        options: &ExpandOptions,
    ) -> Result<ResolvedDocument, ResolveFailure>
    where
        F: Fn(&str) -> Option<String>,
        C: FnMut(CommandContext<'_>) -> Result<String, CommandError>,
    {
        let mut resolver = Resolver {
            document: self,
            lookup,
            options: *options,
            bindings: BTreeMap::new(),
            cache: BTreeMap::new(),
            stack: Vec::new(),
            commands: Some(&mut commands),
            source_identity: source_identity.map(str::to_string),
            command_calls: Vec::new(),
        };
        match resolver.resolve_document() {
            Ok(mut document) => {
                document.commands = core::mem::take(&mut resolver.command_calls);
                Ok(document)
            }
            Err(error) => Err(ResolveFailure {
                error,
                commands: core::mem::take(&mut resolver.command_calls),
            }),
        }
    }
}

type CommandCallback<'a> = dyn FnMut(CommandContext<'_>) -> Result<String, CommandError> + 'a;

struct Resolver<'a, 'src, F> {
    document: &'a Document<'src>,
    lookup: F,
    options: ExpandOptions,
    bindings: BTreeMap<&'a str, usize>,
    cache: BTreeMap<usize, ResolvedEntry>,
    stack: Vec<usize>,
    commands: Option<&'a mut CommandCallback<'a>>,
    source_identity: Option<String>,
    command_calls: Vec<CommandCall>,
}

enum Bound {
    Unset,
    Value {
        text: String,
        parts: Vec<ResolvedSegment>,
        origin: ValueOrigin,
    },
}

impl<'a, F> Resolver<'a, '_, F>
where
    F: Fn(&str) -> Option<String>,
{
    fn resolve_document(&mut self) -> Result<ResolvedDocument, ResolveError> {
        let entries = match self.options.scope {
            ExpandScope::Sequential => {
                let mut entries = Vec::new();
                for entry in self.document.entries() {
                    let entry_result = self.resolve_entry(entry.id())?;
                    if entry.participates_in_lookup() {
                        self.bindings.insert(entry.key(), entry.id());
                    }
                    entries.push(entry_result);
                }
                entries
            }
            ExpandScope::WholeFile => {
                for entry in self.document.entries() {
                    if entry.participates_in_lookup() {
                        self.bindings.insert(entry.key(), entry.id());
                    }
                }
                let mut entries = Vec::new();
                for entry in self.document.entries() {
                    entries.push(self.resolve_entry(entry.id())?);
                }
                entries
            }
        };
        Ok(ResolvedDocument {
            entries,
            commands: Vec::new(),
        })
    }

    fn resolve_entry(&mut self, id: usize) -> Result<ResolvedEntry, ResolveError> {
        if let Some(cached) = self.cache.get(&id) {
            return Ok(cached.clone());
        }
        let entry = self
            .document
            .entry(id)
            .expect("resolved entry id belongs to the document");
        if let Some(position) = self.stack.iter().position(|current| *current == id) {
            let mut chain = self.stack[position..]
                .iter()
                .map(|current| {
                    self.document
                        .entry(*current)
                        .expect("resolved stack id belongs to the document")
                        .key()
                        .to_string()
                })
                .collect::<Vec<_>>();
            chain.push(entry.key().to_string());
            return Err(ResolveError::Cycle {
                key: entry.key().to_string(),
                chain,
            });
        }
        if self.stack.len() > self.options.max_depth {
            return Err(ResolveError::DepthExceeded {
                key: entry.key().to_string(),
                limit: self.options.max_depth,
                span: entry.line_span(),
            });
        }
        self.stack.push(id);
        let result = self.resolve_entry_value(entry);
        self.stack.pop();
        let resolved = result?;
        self.cache.insert(id, resolved.clone());
        Ok(resolved)
    }

    fn resolve_entry_value(&mut self, entry: Entry<'a>) -> Result<ResolvedEntry, ResolveError> {
        let key = entry.key().to_string();
        let state = entry.state();
        let Some(value) = entry.value() else {
            return Ok(ResolvedEntry {
                entry: entry.id(),
                key,
                state,
                value: None,
                effective_value: effective_binding(entry, None),
                segments: Vec::new(),
            });
        };
        let references = entry.references();
        let segments = self.resolve_segments(entry, value, 0..value.len(), None, &references)?;
        let resolved = segments
            .iter()
            .map(|segment| segment.value.as_str())
            .collect::<String>();
        if resolved.len() > self.options.max_value_bytes {
            return Err(ResolveError::ValueTooLarge {
                key,
                limit: self.options.max_value_bytes,
                span: entry.value_span().unwrap_or_else(|| entry.line_span()),
            });
        }
        Ok(ResolvedEntry {
            entry: entry.id(),
            effective_value: effective_binding(entry, Some(&resolved)),
            key,
            state,
            value: Some(resolved),
            segments,
        })
    }

    fn resolve_segments(
        &mut self,
        entry: Entry<'a>,
        decoded: &str,
        range: Range<usize>,
        parent: Option<&Reference>,
        references: &[Reference],
    ) -> Result<Vec<ResolvedSegment>, ResolveError> {
        let mut segments = Vec::new();
        let mut length = 0usize;
        let mut cursor = range.start;
        for reference in references {
            if let Some(parent) = parent {
                if reference.depth != parent.depth.saturating_add(1)
                    || !reference.span_within(parent.argument.as_ref())
                {
                    continue;
                }
            } else if !reference.containers.is_empty() {
                continue;
            }
            if !reference.is_eligible(self.options.rules) {
                continue;
            }
            if reference.kind == ReferenceKind::Command && self.commands.is_none() {
                continue;
            }
            if reference.depth > self.options.max_depth {
                return Err(ResolveError::DepthExceeded {
                    key: entry.key().to_string(),
                    limit: self.options.max_depth,
                    span: reference.span.clone(),
                });
            }
            let Some(start) = decoded_start_for_raw(entry, reference.span.start) else {
                continue;
            };
            let Some(end) = decoded_end_for_raw(entry, reference.span.end) else {
                continue;
            };
            if start < range.start || end > range.end || start < cursor || start > end {
                continue;
            }
            if start > cursor {
                let literal = literal_segment(entry, decoded, cursor..start);
                account(&mut length, &literal, self.options.max_value_bytes, entry)?;
                segments.push(literal);
            }
            let replacement = match reference.kind {
                ReferenceKind::Variable => {
                    self.resolve_reference(entry, reference, decoded, start..end, references)?
                }
                ReferenceKind::Command => self.run_command(entry, reference)?,
            };
            for segment in replacement {
                account(&mut length, &segment, self.options.max_value_bytes, entry)?;
                segments.push(segment);
            }
            cursor = end;
        }
        if cursor < range.end {
            let literal = literal_segment(entry, decoded, cursor..range.end);
            account(&mut length, &literal, self.options.max_value_bytes, entry)?;
            segments.push(literal);
        }
        Ok(segments)
    }

    fn run_command(
        &mut self,
        entry: Entry<'a>,
        reference: &Reference,
    ) -> Result<Vec<ResolvedSegment>, ResolveError> {
        let payload = reference
            .command
            .clone()
            .expect("eligible command has a payload");
        let command = self.document.source()[payload.clone()].to_string();
        let reference_span = reference.span.clone();
        let source = self.source_identity.clone();
        let result = {
            let context = CommandContext {
                command: &command,
                reference: reference_span.clone(),
                payload: payload.clone(),
                source: source.as_deref(),
            };
            let callback = self
                .commands
                .as_mut()
                .expect("command callback was checked before invocation");
            callback(context)
        };
        match result {
            Ok(output) => {
                self.command_calls.push(CommandCall {
                    reference: reference_span.clone(),
                    payload: payload.clone(),
                    source: source.clone(),
                    outcome: CommandOutcome::Output(output.clone()),
                });
                Ok(vec![ResolvedSegment {
                    value: output,
                    kind: ResolvedSegmentKind::Command {
                        reference: reference_span,
                        payload,
                        source,
                    },
                }])
            }
            Err(error) => {
                let message = error.message().to_string();
                self.command_calls.push(CommandCall {
                    reference: reference_span.clone(),
                    payload,
                    source,
                    outcome: CommandOutcome::Error(message.clone()),
                });
                Err(ResolveError::Command {
                    key: entry.key().to_string(),
                    reference: reference_span,
                    message,
                })
            }
        }
    }

    fn resolve_reference(
        &mut self,
        entry: Entry<'a>,
        reference: &Reference,
        decoded: &str,
        decoded_span: Range<usize>,
        references: &[Reference],
    ) -> Result<Vec<ResolvedSegment>, ResolveError> {
        let name_span = reference
            .name
            .as_ref()
            .expect("eligible variable has a name");
        let name = self.document.source()[name_span.clone()].to_string();
        let bound = self.lookup_variable(&name)?;
        let (set, empty, text, parts, origin) = match bound {
            Some(Bound::Value {
                text,
                parts,
                origin,
            }) => (true, text.is_empty(), text, parts, Some(origin)),
            _ => (false, true, String::new(), Vec::new(), None),
        };
        let variable = |text: String, parts: Vec<ResolvedSegment>, origin: Option<ValueOrigin>| {
            ResolvedSegment {
                value: text,
                kind: ResolvedSegmentKind::Variable {
                    name: name.clone(),
                    reference: reference.span.clone(),
                    operator: reference.operator,
                    origin,
                    parts,
                },
            }
        };
        match reference.operator {
            None => {
                if set {
                    Ok(vec![variable(text, parts, origin)])
                } else {
                    self.unresolved(reference, decoded, decoded_span, &name)
                }
            }
            Some(ReferenceOperator::DefaultIfUnset) => {
                if set {
                    Ok(vec![variable(text, parts, origin)])
                } else {
                    self.argument(entry, reference, decoded, references)
                }
            }
            Some(ReferenceOperator::DefaultIfUnsetOrEmpty) => {
                if set && !empty {
                    Ok(vec![variable(text, parts, origin)])
                } else {
                    self.argument(entry, reference, decoded, references)
                }
            }
            Some(ReferenceOperator::AlternativeIfSet) => {
                if set {
                    self.argument(entry, reference, decoded, references)
                } else {
                    Ok(Vec::new())
                }
            }
            Some(ReferenceOperator::AlternativeIfSetAndNonEmpty) => {
                if set && !empty {
                    self.argument(entry, reference, decoded, references)
                } else {
                    Ok(Vec::new())
                }
            }
            Some(ReferenceOperator::ErrorIfUnset) => {
                if set {
                    Ok(vec![variable(text, parts, origin)])
                } else {
                    Err(self.required(entry, reference, decoded, name))
                }
            }
            Some(ReferenceOperator::ErrorIfUnsetOrEmpty) => {
                if set && !empty {
                    Ok(vec![variable(text, parts, origin)])
                } else {
                    Err(self.required(entry, reference, decoded, name))
                }
            }
        }
    }

    fn unresolved(
        &self,
        reference: &Reference,
        decoded: &str,
        decoded_span: Range<usize>,
        name: &str,
    ) -> Result<Vec<ResolvedSegment>, ResolveError> {
        Ok(match self.options.unresolved {
            Unresolved::Keep => vec![ResolvedSegment {
                value: decoded[decoded_span].to_string(),
                kind: ResolvedSegmentKind::Unresolved {
                    name: name.to_string(),
                    reference: reference.span.clone(),
                },
            }],
            Unresolved::Empty => Vec::new(),
            Unresolved::Error => {
                return Err(ResolveError::Unresolved {
                    key: self.current_key(),
                    name: name.to_string(),
                    span: reference.span.clone(),
                });
            }
        })
    }

    fn required(
        &self,
        entry: Entry<'a>,
        reference: &Reference,
        decoded: &str,
        name: String,
    ) -> ResolveError {
        let message = reference
            .argument
            .as_ref()
            .map_or_else(String::new, |argument| {
                decoded_start_for_raw(entry, argument.start)
                    .zip(decoded_end_for_raw(entry, argument.end))
                    .map_or_else(
                        || self.document.source()[argument.clone()].to_string(),
                        |(start, end)| decoded[start..end].to_string(),
                    )
            });
        ResolveError::Required {
            key: entry.key().to_string(),
            name,
            message,
            span: reference.span.clone(),
        }
    }

    fn argument(
        &mut self,
        entry: Entry<'a>,
        reference: &Reference,
        decoded: &str,
        references: &[Reference],
    ) -> Result<Vec<ResolvedSegment>, ResolveError> {
        let Some(argument) = reference.argument.as_ref() else {
            return Ok(Vec::new());
        };
        let Some(start) = decoded_start_for_raw(entry, argument.start) else {
            return Ok(vec![ResolvedSegment {
                value: self.document.source()[argument.clone()].to_string(),
                kind: ResolvedSegmentKind::Literal {
                    span: argument.clone(),
                },
            }]);
        };
        let Some(end) = decoded_end_for_raw(entry, argument.end) else {
            return Ok(vec![ResolvedSegment {
                value: self.document.source()[argument.clone()].to_string(),
                kind: ResolvedSegmentKind::Literal {
                    span: argument.clone(),
                },
            }]);
        };
        self.resolve_segments(entry, decoded, start..end, Some(reference), references)
    }

    fn lookup_variable(&mut self, name: &str) -> Result<Option<Bound>, ResolveError> {
        match self.options.lookup_order {
            LookupOrder::CallerFirst => {
                if let Some(value) = (self.lookup)(name) {
                    return Ok(Some(Bound::Value {
                        text: value,
                        parts: Vec::new(),
                        origin: ValueOrigin::Lookup,
                    }));
                }
                self.file_binding(name)
            }
            LookupOrder::FileFirst => {
                let file = self.file_binding(name)?;
                if file.is_some() {
                    return Ok(file);
                }
                Ok((self.lookup)(name).map(|value| Bound::Value {
                    text: value,
                    parts: Vec::new(),
                    origin: ValueOrigin::Lookup,
                }))
            }
        }
    }

    fn file_binding(&mut self, name: &str) -> Result<Option<Bound>, ResolveError> {
        let Some(id) = self.bindings.get(name).copied() else {
            return Ok(None);
        };
        let entry = self
            .document
            .entry(id)
            .expect("binding id belongs to the document");
        if entry.value().is_none() {
            if entry.state() == AssignmentState::Bare
                && self.document.dialect().bare_keys == BareKeys::Empty
            {
                return Ok(Some(Bound::Value {
                    text: String::new(),
                    parts: Vec::new(),
                    origin: file_origin(entry),
                }));
            }
            return Ok(Some(Bound::Unset));
        }
        let resolved = self.resolve_entry(id)?;
        Ok(Some(Bound::Value {
            text: resolved.value.unwrap_or_default(),
            parts: resolved.segments,
            origin: file_origin(entry),
        }))
    }

    fn current_key(&self) -> String {
        self.stack
            .last()
            .and_then(|id| self.document.entry(*id))
            .map_or_else(String::new, |entry| entry.key().to_string())
    }
}

impl Reference {
    fn span_within(&self, parent: Option<&Range<usize>>) -> bool {
        parent.is_some_and(|parent| parent.start <= self.span.start && self.span.end <= parent.end)
    }
}

fn effective_binding(entry: Entry<'_>, value: Option<&str>) -> Option<String> {
    if !entry.participates_in_lookup() {
        return None;
    }
    match value {
        Some(value) => Some(value.to_string()),
        None if entry.state() == AssignmentState::Bare
            && entry.document.dialect().bare_keys == BareKeys::Empty =>
        {
            Some(String::new())
        }
        None => None,
    }
}

fn file_origin(entry: Entry<'_>) -> ValueOrigin {
    ValueOrigin::File {
        entry: entry.id(),
        key: entry.key().to_string(),
        span: entry.value_span().unwrap_or_else(|| entry.line_span()),
    }
}

fn literal_segment(entry: Entry<'_>, decoded: &str, range: Range<usize>) -> ResolvedSegment {
    let span = entry
        .decoded_offset_to_raw(range.start)
        .ok()
        .zip(entry.decoded_offset_to_raw(range.end).ok())
        .map_or_else(
            || entry.value_span().unwrap_or_else(|| entry.line_span()),
            |(start, end)| start..end,
        );
    ResolvedSegment {
        value: decoded[range].to_string(),
        kind: ResolvedSegmentKind::Literal { span },
    }
}

fn account(
    length: &mut usize,
    segment: &ResolvedSegment,
    limit: usize,
    entry: Entry<'_>,
) -> Result<(), ResolveError> {
    *length = length.saturating_add(segment.value.len());
    if *length > limit {
        return Err(ResolveError::ValueTooLarge {
            key: entry.key().to_string(),
            limit,
            span: entry.line_span(),
        });
    }
    Ok(())
}

fn decoded_start_for_raw(entry: Entry<'_>, raw: usize) -> Option<usize> {
    let value = entry.value()?;
    let mut start = None;
    for (offset, _) in value.char_indices() {
        let mapped = entry.decoded_offset_to_raw(offset).ok()?;
        if mapped <= raw {
            start = Some(offset);
        } else {
            break;
        }
    }
    if entry.decoded_offset_to_raw(value.len()).ok()? <= raw {
        start = Some(value.len());
    }
    start
}

fn decoded_end_for_raw(entry: Entry<'_>, raw: usize) -> Option<usize> {
    let value = entry.value()?;
    for (offset, _) in value.char_indices() {
        if entry.decoded_offset_to_raw(offset).ok()? >= raw {
            return Some(offset);
        }
    }
    Some(value.len())
}
