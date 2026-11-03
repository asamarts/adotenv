//! Opt-in variable resolution with a lookup closure supplied by the caller.
//!
//! [`Document::resolve`] substitutes references from the file and from a lookup
//! closure; it never reads the process environment or files. Commands run only
//! through the callback passed to [`Document::resolve_with_commands`]. Every
//! substituted part records where it came from.

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::{fmt, ops::Range};

use crate::{
    AssignmentState, BareKeys, Document, Duplicates, Entry, Reference, ReferenceKind,
    ReferenceOperator, ReferenceRules,
};

/// Which declarations in the file a reference can see during resolution.
///
/// Set it with [`ExpandOptions::with_scope`]; every preset uses
/// [`ExpandScope::Sequential`]. Only declarations that take part in lookup
/// count. When a key is declared more than once, a reference follows the
/// dialect's duplicate-key rule ([`Dialect::duplicates`](crate::Dialect::duplicates))
/// among the declarations it can see: the first one under
/// [`Duplicates::First`], the latest one under [`Duplicates::Last`]. In
/// whole-file scope this is the declaration that [`Document::get`] returns.
///
/// # Examples
///
/// In sequential scope a line can extend a value from the lookup closure. In
/// whole-file scope the same line refers to itself, which is a cycle:
///
/// ```
/// use adotenv::{parse_strict, ExpandOptions, ExpandScope, ResolveError};
///
/// let doc = parse_strict("PATH=${PATH}:/opt/app/bin\n")?;
/// let lookup = |name: &str| (name == "PATH").then(|| "/usr/bin".to_string());
///
/// let sequential = doc.resolve(lookup, &ExpandOptions::portable())?;
/// assert_eq!(sequential.entries()[0].value.as_deref(), Some("/usr/bin:/opt/app/bin"));
///
/// let whole_file = ExpandOptions::portable().with_scope(ExpandScope::WholeFile);
/// let error = doc.resolve(lookup, &whole_file).unwrap_err();
/// assert!(matches!(error, ResolveError::Cycle { .. }));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExpandScope {
    /// Each entry sees only the declarations before it.
    ///
    /// A reference uses the earlier declaration of the name that the
    /// dialect's duplicate-key rule selects, so with the presets `A=base`
    /// followed by `A=${A}x` gives `basex`. Declarations later in the file are
    /// not visible. Sequential scope cannot form a cycle.
    Sequential,
    /// Each entry sees, for every name, the declaration that [`Document::get`] uses.
    ///
    /// Entries may refer to declarations that come after them. A loop of
    /// references through file values, including a line such as `A=${A}x`,
    /// fails with [`ResolveError::Cycle`] unless the lookup closure supplies the
    /// value first ([`LookupOrder::CallerFirst`]).
    WholeFile,
}

/// Whether the file or the lookup closure supplies a variable's value first.
///
/// Set it with [`ExpandOptions::with_lookup_order`]; every preset uses
/// [`LookupOrder::FileFirst`].
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict, ExpandOptions, LookupOrder};
///
/// let doc = parse_strict("HOST=localhost\nURL=http://${HOST}/\n")?;
/// let lookup = |name: &str| (name == "HOST").then(|| "db.internal".to_string());
///
/// let file_first = doc.resolve(lookup, &ExpandOptions::portable())?;
/// assert_eq!(file_first.entries()[1].value.as_deref(), Some("http://localhost/"));
///
/// let options = ExpandOptions::portable().with_lookup_order(LookupOrder::CallerFirst);
/// let caller_first = doc.resolve(lookup, &options)?;
/// assert_eq!(caller_first.entries()[1].value.as_deref(), Some("http://db.internal/"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LookupOrder {
    /// The lookup closure first, then the file.
    ///
    /// The closure is called first for each reference that is resolved. A
    /// declaration in the file is used only when the closure returns `None`.
    CallerFirst,
    /// The file first, then the lookup closure.
    ///
    /// The closure is called only when no declaration of the name is visible in
    /// the selected [`ExpandScope`]. A visible declaration always wins, even a
    /// bare key (`NAME` without `=`): the name is then unset, or empty under
    /// [`BareKeys::Empty`], and the closure is not called.
    FileFirst,
}

/// What happens to a plain reference whose variable has no value.
///
/// This applies to `$NAME` and `${NAME}` without an operator. Operator forms
/// such as `${NAME:-default}` decide for themselves; see [`Document::resolve`].
/// A variable has no value when neither a visible declaration nor the lookup
/// closure supplies one; an empty value is still a value. Set it with
/// [`ExpandOptions::with_unresolved`]; every preset uses [`Unresolved::Keep`].
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict, ExpandOptions, ResolveError, Unresolved};
///
/// let doc = parse_strict("URL=https://${API_HOST}/v1\n")?;
/// let kept = doc.resolve(|_| None, &ExpandOptions::portable())?;
/// assert_eq!(kept.entries()[0].value.as_deref(), Some("https://${API_HOST}/v1"));
///
/// let strict = ExpandOptions::portable().with_unresolved(Unresolved::Error);
/// let error = doc.resolve(|_| None, &strict).unwrap_err();
/// assert!(matches!(error, ResolveError::Unresolved { name, .. } if name == "API_HOST"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Unresolved {
    /// The reference stays in the value as written.
    ///
    /// It is recorded as a [`ResolvedSegmentKind::Unresolved`] segment.
    Keep,
    /// The reference is replaced with an empty string.
    ///
    /// No segment is recorded for it.
    Empty,
    /// Resolution fails with [`ResolveError::Unresolved`].
    Error,
}

/// Options for [`Document::resolve`]: eligible references, scope, lookup order
/// and limits.
///
/// Start from a preset and adjust it with the `with_*` methods. The struct is
/// `#[non_exhaustive]`, so code outside this crate cannot build it with a
/// struct literal, but its public fields can be read and assigned.
///
/// The presets differ only in their [`rules`](Self::rules), which decide which
/// references are eligible for substitution:
///
/// | Preset | Rules | Eligible forms |
/// |---|---|---|
/// | [`ExpandOptions::portable`] (the [`Default`]) | [`ReferenceRules::PORTABLE`] | `$VAR` and `${VAR}` |
/// | [`ExpandOptions::compose`] | [`ReferenceRules::COMPOSE`] | also every operator and nested references |
/// | [`ExpandOptions::dotenv_expand`] | [`ReferenceRules::DOTENV_EXPAND`] | also single-quoted values and `$(command)` |
/// | [`ExpandOptions::none`] | [`ReferenceRules::NONE`] | none |
///
/// All presets share the other settings:
///
/// | Option | Builder | Preset value |
/// |---|---|---|
/// | [`scope`](Self::scope) | [`with_scope`](Self::with_scope) | [`ExpandScope::Sequential`] |
/// | [`lookup_order`](Self::lookup_order) | [`with_lookup_order`](Self::with_lookup_order) | [`LookupOrder::FileFirst`] |
/// | [`unresolved`](Self::unresolved) | [`with_unresolved`](Self::with_unresolved) | [`Unresolved::Keep`] |
/// | [`max_depth`](Self::max_depth) | [`with_max_depth`](Self::with_max_depth) | 64 |
/// | [`max_references`](Self::max_references) | [`with_max_references`](Self::with_max_references) | 65,536 |
/// | [`max_value_bytes`](Self::max_value_bytes) | [`with_max_value_bytes`](Self::with_max_value_bytes) | 1 MiB (1,048,576 bytes) |
/// | [`max_output_bytes`](Self::max_output_bytes) | [`with_max_output_bytes`](Self::with_max_output_bytes) | 64 MiB (67,108,864 bytes) |
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict, ExpandOptions, ExpandScope, Unresolved};
///
/// let options = ExpandOptions::compose()
///     .with_scope(ExpandScope::WholeFile)
///     .with_unresolved(Unresolved::Empty)
///     .with_max_value_bytes(4096);
///
/// // HOST is declared later, SCHEME uses its default and SUFFIX has no value.
/// let doc = parse_strict("URL=${SCHEME:-https}://${HOST}${SUFFIX}\nHOST=example.com\n")?;
/// let resolved = doc.resolve(|_| None, &options)?;
/// assert_eq!(resolved.entries()[0].value.as_deref(), Some("https://example.com"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ExpandOptions {
    /// The rules that decide which references are eligible for substitution.
    ///
    /// Ineligible references, such as single-quoted ones under
    /// [`ReferenceRules::PORTABLE`], stay as written. See
    /// [`Reference::is_eligible`].
    pub rules: ReferenceRules,
    /// Which declarations a reference can see; [`ExpandScope::Sequential`] in
    /// every preset.
    pub scope: ExpandScope,
    /// Whether the file or the lookup closure is consulted first;
    /// [`LookupOrder::FileFirst`] in every preset.
    pub lookup_order: LookupOrder,
    /// What happens to a plain reference with no value; [`Unresolved::Keep`] in
    /// every preset.
    pub unresolved: Unresolved,
    /// The maximum nesting depth; 64 in every preset.
    ///
    /// Two kinds of nesting count, each checked on its own. A reference inside
    /// an operator argument is one level deeper than the reference around it:
    /// in `${A:-${B}}`, `${B}` is at depth 1. Under [`ExpandScope::WholeFile`],
    /// a file value that has to be resolved while another one is being resolved
    /// is also one level deeper; in sequential scope, earlier values are already
    /// resolved. Going deeper than the limit fails with
    /// [`ResolveError::DepthExceeded`].
    pub max_depth: usize,
    /// The maximum number of reference candidates scanned in one value;
    /// 65,536 in every preset.
    ///
    /// Resolution stops before it builds a longer candidate list and fails
    /// with [`ResolveError::TooManyReferences`]. This bounds the memory a scan
    /// can allocate, which is roughly the size of one [`Reference`] per
    /// candidate. `0` rejects every value that contains a possible reference.
    pub max_references: usize,
    /// The maximum length in bytes of one resolved value; 1 MiB in every preset.
    ///
    /// Every entry's value is checked as its parts are added. Exceeding the
    /// limit fails with [`ResolveError::ValueTooLarge`]; `0` allows only empty
    /// values.
    pub max_value_bytes: usize,
    /// The maximum approximate size in bytes of one resolution's result; 64 MiB
    /// in every preset.
    ///
    /// Every resolved value, segment and recorded command call counts, with a
    /// fixed overhead for each. A reference to a file value is charged again for
    /// its copy of that value and of its nested segments, so the limit holds
    /// even when values that repeat each other make the result grow much faster
    /// than the file. The budget is checked before each copy is made. Exceeding
    /// it fails with [`ResolveError::OutputTooLarge`].
    pub max_output_bytes: usize,
}

impl ExpandOptions {
    /// Sets the rules that decide which references are eligible.
    ///
    /// See [`ExpandOptions::rules`].
    #[must_use]
    pub const fn with_rules(mut self, value: ReferenceRules) -> Self {
        self.rules = value;
        self
    }
    /// Sets which declarations a reference can see.
    ///
    /// See [`ExpandOptions::scope`].
    #[must_use]
    pub const fn with_scope(mut self, value: ExpandScope) -> Self {
        self.scope = value;
        self
    }
    /// Sets whether the file or the lookup closure is consulted first.
    ///
    /// See [`ExpandOptions::lookup_order`].
    #[must_use]
    pub const fn with_lookup_order(mut self, value: LookupOrder) -> Self {
        self.lookup_order = value;
        self
    }
    /// Sets what happens to a plain reference with no value.
    ///
    /// See [`ExpandOptions::unresolved`].
    #[must_use]
    pub const fn with_unresolved(mut self, value: Unresolved) -> Self {
        self.unresolved = value;
        self
    }
    /// Sets the maximum nesting depth.
    ///
    /// See [`ExpandOptions::max_depth`].
    #[must_use]
    pub const fn with_max_depth(mut self, value: usize) -> Self {
        self.max_depth = value;
        self
    }
    /// Sets the maximum number of reference candidates scanned in one value.
    ///
    /// See [`ExpandOptions::max_references`].
    #[must_use]
    pub const fn with_max_references(mut self, value: usize) -> Self {
        self.max_references = value;
        self
    }
    /// Sets the maximum length in bytes of one resolved value.
    ///
    /// See [`ExpandOptions::max_value_bytes`].
    #[must_use]
    pub const fn with_max_value_bytes(mut self, value: usize) -> Self {
        self.max_value_bytes = value;
        self
    }
    /// Sets the maximum approximate size in bytes of one resolution's result.
    ///
    /// See [`ExpandOptions::max_output_bytes`].
    #[must_use]
    pub const fn with_max_output_bytes(mut self, value: usize) -> Self {
        self.max_output_bytes = value;
        self
    }
}
impl ExpandOptions {
    /// Default options with the Portable reference rules: `$VAR` and `${VAR}`
    /// only.
    ///
    /// Uses [`ReferenceRules::PORTABLE`]: no operators, nothing inside single
    /// quotes and no commands. The other settings have the preset values listed
    /// on [`ExpandOptions`]. [`Default::default`] returns the same options.
    #[must_use]
    pub const fn portable() -> Self {
        Self {
            rules: ReferenceRules::PORTABLE,
            scope: ExpandScope::Sequential,
            lookup_order: LookupOrder::FileFirst,
            unresolved: Unresolved::Keep,
            max_depth: 64,
            max_references: 65_536,
            max_value_bytes: 1_048_576,
            max_output_bytes: 67_108_864,
        }
    }
    /// Options with the Compose reference rules: every operator and nested
    /// references.
    ///
    /// Uses [`ReferenceRules::COMPOSE`]: `$VAR`, `${VAR}`, every operator such
    /// as `${VAR:-default}`, and references inside operator arguments. The
    /// other settings have the preset values listed on [`ExpandOptions`].
    ///
    /// This selects which references are eligible; it does not emulate Docker
    /// Compose interpolation. For example, under the Compose dialect an escaped
    /// `$$B` stays `$$B` here, while Compose interpolation replaces `$$` with a
    /// single `$`.
    #[must_use]
    pub const fn compose() -> Self {
        Self {
            rules: ReferenceRules::COMPOSE,
            ..Self::portable()
        }
    }
    /// Options with the dotenv-expand reference rules, which also allow single
    /// quotes and commands.
    ///
    /// Uses [`ReferenceRules::DOTENV_EXPAND`]: every operator and nested
    /// references, references inside single-quoted values, and `$(command)`
    /// forms. Commands run only through [`Document::resolve_with_commands`];
    /// [`Document::resolve`] leaves them as written. The other settings have
    /// the preset values listed on [`ExpandOptions`].
    #[must_use]
    pub const fn dotenv_expand() -> Self {
        Self {
            rules: ReferenceRules::DOTENV_EXPAND,
            ..Self::portable()
        }
    }
    /// Options under which no reference is eligible, so every value stays
    /// literal.
    ///
    /// Uses [`ReferenceRules::NONE`]. Resolution still returns a
    /// [`ResolvedDocument`], with each non-empty value as one literal segment.
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

/// The details of one `$(command)` form, passed to a command callback.
///
/// [`Document::resolve_with_commands`] passes one to the callback for each
/// eligible command it reaches. The struct is `#[non_exhaustive]`, so code
/// outside this crate receives it but cannot build it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommandContext<'a> {
    /// The command text exactly as written between `$(` and `)`.
    ///
    /// This is the source text: references inside it are not substituted and
    /// escapes are not decoded.
    pub command: &'a str,
    /// The byte range of the whole `$(...)` form in the source.
    pub reference: Range<usize>,
    /// The byte range of the command text in the source.
    pub payload: Range<usize>,
    /// The source identity passed to [`Document::resolve_with_commands`], if
    /// any.
    ///
    /// The crate passes it through unchanged and never derives it from a path.
    pub source: Option<&'a str>,
}

/// An error that a command callback returns to stop resolution.
///
/// Resolution then fails with [`ResolveError::Command`], which carries the
/// message, and the failed call is recorded with [`CommandOutcome::Error`].
/// The [`Display`](fmt::Display) output is the message.
///
/// # Examples
///
/// A callback that refuses commands it does not know:
///
/// ```
/// use adotenv::{parse_strict, CommandError, ExpandOptions, ResolveError};
///
/// let doc = parse_strict("TOKEN=$(cat /run/secrets/token)\n")?;
/// let failure = doc
///     .resolve_with_commands(
///         |_| None,
///         |context| Err(CommandError::new(format!("not allowed: {}", context.command))),
///         None,
///         &ExpandOptions::dotenv_expand(),
///     )
///     .unwrap_err();
/// assert!(matches!(
///     failure.error,
///     ResolveError::Command { key, message, .. }
///         if key == "TOKEN" && message == "not allowed: cat /run/secrets/token"
/// ));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandError {
    message: String,
}

impl CommandError {
    /// Creates an error with the given message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
    /// Returns the message.
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

/// What one command callback call returned.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CommandOutcome {
    /// The output the callback returned.
    Output(String),
    /// The message of the [`CommandError`] the callback returned.
    Error(String),
}

/// A record of one command callback call made during resolution.
///
/// Calls are listed in [`ResolvedDocument::commands`] or, when resolution
/// fails, in [`ResolveFailure::commands`]. The command text itself is the
/// document's source text at [`payload`](Self::payload). See
/// [`Document::resolve_with_commands`] for an example.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommandCall {
    /// The byte range of the whole `$(...)` form in the source.
    pub reference: Range<usize>,
    /// The byte range of the command text in the source.
    pub payload: Range<usize>,
    /// The source identity passed to [`Document::resolve_with_commands`], if
    /// any.
    pub source: Option<String>,
    /// The output or error message the callback returned.
    pub outcome: CommandOutcome,
}

/// A failed [`Document::resolve_with_commands`], with the command calls made
/// before the failure.
///
/// Commands that already ran are not undone, so [`commands`](Self::commands)
/// shows which of them happened. The [`Display`](fmt::Display) output is that
/// of [`error`](Self::error).
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict, CommandOutcome, ExpandOptions, ResolveError};
///
/// let doc = parse_strict("REVISION=$(git rev-parse HEAD)\nURL=${HOST:?HOST is required}\n")?;
/// let failure = doc
///     .resolve_with_commands(
///         |_| None,
///         |_| Ok("4f2a9c1".to_string()),
///         None,
///         &ExpandOptions::dotenv_expand(),
///     )
///     .unwrap_err();
/// assert!(matches!(failure.error, ResolveError::Required { .. }));
/// assert_eq!(failure.commands.len(), 1);
/// assert_eq!(failure.commands[0].outcome, CommandOutcome::Output("4f2a9c1".into()));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolveFailure {
    /// The error that stopped resolution.
    pub error: ResolveError,
    /// The command calls made before the failure, including a failed call, in
    /// call order.
    pub commands: Vec<CommandCall>,
}

impl fmt::Display for ResolveFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(formatter)
    }
}

impl core::error::Error for ResolveFailure {}

/// Where a substituted variable value came from.
///
/// Found in [`ResolvedSegmentKind::Variable`]; see there for an example.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValueOrigin {
    /// A declaration in the document.
    File {
        /// The zero-based entry ID of the declaration; see [`Document::entry`].
        entry: usize,
        /// The declaration's key.
        key: String,
        /// The byte range of the declaration's raw value, quotes included, or of
        /// its whole statement for a bare key.
        span: Range<usize>,
    },
    /// The lookup closure passed to resolution.
    Lookup,
}

/// One part of a resolved value: its text and where the text came from.
///
/// The segments of a [`ResolvedEntry`] concatenate to its value. See
/// [`ResolvedSegmentKind`] for an example.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolvedSegment {
    /// The text this part adds to the resolved value.
    pub value: String,
    /// Where the text came from.
    pub kind: ResolvedSegmentKind,
}

/// Where the text of one [`ResolvedSegment`] came from.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict, ExpandOptions, ResolvedSegmentKind, ValueOrigin};
///
/// let doc = parse_strict("HOST=localhost\nURL=http://${HOST}:${PORT}/${BASE_PATH}\n")?;
/// let lookup = |name: &str| (name == "PORT").then(|| "8080".to_string());
/// let resolved = doc.resolve(lookup, &ExpandOptions::portable())?;
///
/// let described: Vec<String> = resolved.entries()[1]
///     .segments
///     .iter()
///     .map(|segment| match &segment.kind {
///         ResolvedSegmentKind::Literal { .. } => format!("text {:?}", segment.value),
///         ResolvedSegmentKind::Variable { name, origin: Some(ValueOrigin::File { .. }), .. } => {
///             format!("{name} from the file")
///         }
///         ResolvedSegmentKind::Variable { name, .. } => format!("{name} from the lookup"),
///         ResolvedSegmentKind::Unresolved { name, .. } => format!("{name} unresolved"),
///         _ => "other".to_string(),
///     })
///     .collect();
/// assert_eq!(
///     described,
///     [
///         "text \"http://\"",
///         "HOST from the file",
///         "text \":\"",
///         "PORT from the lookup",
///         "text \"/\"",
///         "BASE_PATH unresolved",
///     ]
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolvedSegmentKind {
    /// Text from the entry's own value, after quote and escape decoding.
    Literal {
        /// The source byte range the text was decoded from.
        ///
        /// When the text does not map to an exact range, this is the span of the
        /// whole value.
        span: Range<usize>,
    },
    /// The value of a variable, substituted for a reference.
    ///
    /// This appears when the variable's own value is used, with or without an
    /// operator. When an operator uses its argument instead, as in
    /// `${NAME:-default}` with `NAME` unset, the argument's segments appear in
    /// its place.
    Variable {
        /// The variable name.
        name: String,
        /// The byte range of the whole reference, such as `${HOST}`, in the
        /// source.
        reference: Range<usize>,
        /// The reference's operator, such as
        /// [`ReferenceOperator::DefaultIfUnsetOrEmpty`] for `${HOST:-localhost}`;
        /// `None` for a plain reference.
        operator: Option<ReferenceOperator>,
        /// Where the value came from; always `Some(..)` in this release.
        origin: Option<ValueOrigin>,
        /// The segments of the file value, nested; empty for a value from the
        /// lookup closure or for a bare key read as empty.
        parts: Vec<ResolvedSegment>,
    },
    /// A reference with no value, kept as written under [`Unresolved::Keep`].
    Unresolved {
        /// The variable name.
        name: String,
        /// The byte range of the whole reference in the source.
        reference: Range<usize>,
    },
    /// The output of a command callback, substituted for a `$(command)` form.
    Command {
        /// The byte range of the whole `$(...)` form in the source.
        reference: Range<usize>,
        /// The byte range of the command text in the source.
        payload: Range<usize>,
        /// The source identity passed to [`Document::resolve_with_commands`],
        /// if any.
        source: Option<String>,
    },
}

/// One entry's resolved value, with a record of where each part came from.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict, AssignmentState, ExpandOptions};
///
/// let doc = parse_strict("DEBUG\nLOG_DIR=${APP_DIR}/logs\n")?;
/// let lookup = |name: &str| (name == "APP_DIR").then(|| "/srv/app".to_string());
/// let resolved = doc.resolve(lookup, &ExpandOptions::portable())?;
///
/// let debug = &resolved.entries()[0];
/// assert_eq!((debug.state, debug.value.as_deref()), (AssignmentState::Bare, None));
///
/// let log_dir = &resolved.entries()[1];
/// assert_eq!(log_dir.value.as_deref(), Some("/srv/app/logs"));
/// assert_eq!(log_dir.effective_value.as_deref(), Some("/srv/app/logs"));
/// let joined: String = log_dir.segments.iter().map(|segment| segment.value.as_str()).collect();
/// assert_eq!(joined, "/srv/app/logs");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolvedEntry {
    /// The zero-based entry ID in the resolved document; see [`Entry::id`] and
    /// [`Document::entry`].
    pub entry: usize,
    /// The key, as returned by [`Entry::key`].
    pub key: String,
    /// The entry's state in the source, as returned by [`Entry::state`].
    ///
    /// It describes the literal value, not the resolved one: `A=${MISSING}`
    /// is [`AssignmentState::Value`] even when it resolves to an empty string.
    pub state: AssignmentState,
    /// The resolved value; `None` for a bare key.
    pub value: Option<String>,
    /// The resolved value this entry gives to lookup, after the dialect's
    /// bare-key rule.
    ///
    /// It is `None` when the entry does not take part in lookup, and for a bare
    /// key that stays unset; it is `Some("")` for a bare key under
    /// [`BareKeys::Empty`]. The dialect's duplicate-key rule is not applied
    /// here: each declaration of a repeated key has its own value.
    pub effective_value: Option<String>,
    /// The parts of the resolved value in order; their texts concatenate to
    /// [`value`](Self::value). Empty for a bare key.
    pub segments: Vec<ResolvedSegment>,
}

/// The result of resolution: every entry's resolved value, in source order.
///
/// Returned by [`Document::resolve`] and [`Document::resolve_with_commands`].
/// The parsed [`Document`] itself is not changed.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict, ExpandOptions};
///
/// let doc = parse_strict("# Service\nHOST=localhost\nURL=http://${HOST}/\n")?;
/// let resolved = doc.resolve(|_| None, &ExpandOptions::portable())?;
///
/// assert_eq!(resolved.len(), 2);
/// let values: Vec<_> = resolved.entries().iter().map(|entry| entry.value.as_deref()).collect();
/// assert_eq!(values, [Some("localhost"), Some("http://localhost/")]);
/// assert!(resolved.commands().is_empty());
/// assert_eq!(doc.get("URL"), Some("http://${HOST}/"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDocument {
    entries: Vec<ResolvedEntry>,
    commands: Vec<CommandCall>,
}

impl ResolvedDocument {
    /// Returns the resolved entries in source order, one per entry of the
    /// document.
    #[must_use]
    pub fn entries(&self) -> &[ResolvedEntry] {
        &self.entries
    }
    /// Returns the command callback calls made during resolution, in call
    /// order.
    ///
    /// Always empty for [`Document::resolve`].
    #[must_use]
    pub fn commands(&self) -> &[CommandCall] {
        &self.commands
    }
    /// Returns the number of resolved entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Returns `true` when there are no resolved entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// An error from [`Document::resolve`] or [`Document::resolve_with_commands`].
///
/// Resolution stops at the first error. Each variant names the `key` of the
/// entry that was being resolved; under [`ExpandScope::WholeFile`] this can be
/// a declaration reached through a reference rather than the entry the
/// resolution started from. Spans are byte ranges in the source.
///
/// # Examples
///
/// ```
/// use adotenv::{parse_strict, ExpandOptions, ResolveError};
///
/// let doc = parse_strict("DATABASE_URL=\"postgres://${DB_HOST:?set DB_HOST}/app\"\n")?;
/// let error = doc.resolve(|_| None, &ExpandOptions::compose()).unwrap_err();
///
/// match &error {
///     ResolveError::Required { key, name, message, .. } => {
///         assert_eq!((key.as_str(), name.as_str()), ("DATABASE_URL", "DB_HOST"));
///         assert_eq!(message, "set DB_HOST");
///     }
///     other => panic!("unexpected error: {other}"),
/// }
/// assert_eq!(
///     error.to_string(),
///     "required variable DB_HOST has no value while resolving DATABASE_URL: set DB_HOST"
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveError {
    /// A plain reference had no value under [`Unresolved::Error`].
    Unresolved {
        /// The key of the entry being resolved.
        key: String,
        /// The variable name.
        name: String,
        /// The byte range of the reference.
        span: Range<usize>,
    },
    /// A `${NAME:?message}` reference found its variable unset or empty, or a
    /// `${NAME?message}` reference found it unset.
    Required {
        /// The key of the entry being resolved.
        key: String,
        /// The variable name.
        name: String,
        /// The text after `?`, decoded like the rest of the value but with any
        /// references left as written; empty when there is none.
        message: String,
        /// The byte range of the reference.
        span: Range<usize>,
    },
    /// File values refer to each other in a loop, under
    /// [`ExpandScope::WholeFile`].
    Cycle {
        /// The key where the loop closes; the first and last key of `chain`.
        key: String,
        /// The keys along the loop in resolution order, starting and ending
        /// with `key`: `["A", "B", "A"]` for `A=${B}` and `B=${A}`.
        chain: Vec<String>,
    },
    /// Nesting went deeper than [`ExpandOptions::max_depth`].
    DepthExceeded {
        /// The key of the entry that holds the nested reference, or of the
        /// entry that would be resolved too deep.
        key: String,
        /// The configured [`ExpandOptions::max_depth`].
        limit: usize,
        /// The byte range of the nested reference, or the statement of the
        /// entry that would be resolved too deep.
        span: Range<usize>,
    },
    /// Reference scanning found more candidates than
    /// [`ExpandOptions::max_references`].
    TooManyReferences {
        /// The key of the entry being resolved.
        key: String,
        /// The configured [`ExpandOptions::max_references`].
        limit: usize,
        /// The byte range of the entry's value as written, quotes included.
        span: Range<usize>,
    },
    /// A resolved value grew longer than [`ExpandOptions::max_value_bytes`].
    ValueTooLarge {
        /// The key of the entry being resolved.
        key: String,
        /// The configured [`ExpandOptions::max_value_bytes`].
        limit: usize,
        /// The byte range of the entry's value as written, quotes included.
        span: Range<usize>,
    },
    /// The whole result grew larger than [`ExpandOptions::max_output_bytes`].
    OutputTooLarge {
        /// The key of the entry being resolved when the budget ran out.
        key: String,
        /// The configured [`ExpandOptions::max_output_bytes`].
        limit: usize,
        /// The byte range of the entry's value, or of its statement for a bare
        /// key.
        span: Range<usize>,
    },
    /// A command callback returned a [`CommandError`].
    Command {
        /// The key of the entry being resolved.
        key: String,
        /// The byte range of the whole `$(...)` form.
        reference: Range<usize>,
        /// The message of the [`CommandError`].
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
                "required variable {name} has no value while resolving {key}: {message}"
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
            Self::TooManyReferences { key, limit, .. } => {
                write!(
                    formatter,
                    "reference scan for {key} exceeds {limit} candidates"
                )
            }
            Self::ValueTooLarge { key, limit, .. } => {
                write!(formatter, "resolved value for {key} exceeds {limit} bytes")
            }
            Self::OutputTooLarge { key, limit, .. } => {
                write!(
                    formatter,
                    "resolution output exceeds {limit} bytes while resolving {key}"
                )
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
    /// Substitutes variable references using the file's declarations and a
    /// lookup closure.
    ///
    /// Parsing keeps every value literal; this method is the explicit, opt-in
    /// step that expands references such as `$HOST` or `${PORT:-8080}`. It never
    /// reads the process environment, files or the working directory, and never
    /// runs commands: `$(command)` forms stay as written even when the rules
    /// allow them (see [`Document::resolve_with_commands`]). The document is not
    /// changed; the result holds one [`ResolvedEntry`] per entry, in source
    /// order, and records where each part of every value came from.
    ///
    /// [`ExpandOptions::rules`] decides which references are eligible; the
    /// others stay as written. For an eligible reference, the value of a name
    /// comes from two places:
    ///
    /// - **The file**: a declaration of the name that takes part in lookup and
    ///   is visible in the [`ExpandScope`] and selected by the dialect's
    ///   duplicate-key rule: among the declarations before the entry in
    ///   sequential scope (the preset), or among all declarations in whole-file
    ///   scope. Its value is resolved first.
    /// - **The lookup closure**: `lookup` is called with the variable name and
    ///   returns `Some(value)` or `None`. Its values are used as they are and
    ///   are not expanded again. It may be called more than once for the same
    ///   name. To read the process environment, pass
    ///   `|name| std::env::var(name).ok()`.
    ///
    /// [`LookupOrder`] picks which one is tried first: the file
    /// ([`LookupOrder::FileFirst`], the preset) or the closure
    /// ([`LookupOrder::CallerFirst`]). A variable is *unset* when this order
    /// finds no value for it. A visible bare key has no value, so under
    /// [`LookupOrder::FileFirst`] it leaves the variable unset without calling
    /// the closure; under [`BareKeys::Empty`] it reads as empty instead. An
    /// empty value still counts as set. Operator forms, which need rules such
    /// as those of [`ExpandOptions::compose`], work as follows:
    ///
    /// | Form | Result |
    /// |---|---|
    /// | `${NAME:-default}` | `default` when `NAME` is unset or empty, else its value |
    /// | `${NAME-default}` | `default` when `NAME` is unset, else its value |
    /// | `${NAME:+alternative}` | `alternative` when `NAME` is set and not empty, else empty |
    /// | `${NAME+alternative}` | `alternative` when `NAME` is set, else empty |
    /// | `${NAME:?message}` | [`ResolveError::Required`] when `NAME` is unset or empty, else its value |
    /// | `${NAME?message}` | [`ResolveError::Required`] when `NAME` is unset, else its value |
    ///
    /// An argument is resolved only when it is used, and may contain further
    /// references. A plain `$NAME` or `${NAME}` that is unset follows
    /// [`ExpandOptions::unresolved`]: by default it stays as written.
    ///
    /// Every entry is resolved, including entries that nothing refers to.
    /// Resolution never loops: in whole-file scope, file values that refer to
    /// each other in a loop fail with [`ResolveError::Cycle`]. The limits
    /// [`ExpandOptions::max_depth`], [`ExpandOptions::max_references`],
    /// [`ExpandOptions::max_value_bytes`] and
    /// [`ExpandOptions::max_output_bytes`] bound the nesting depth, the
    /// reference scan, the length of each value and the total size of the
    /// result.
    ///
    /// # Errors
    ///
    /// Resolution stops at the first error and returns it:
    ///
    /// - [`ResolveError::Unresolved`] when a plain reference is unset and the
    ///   policy is [`Unresolved::Error`];
    /// - [`ResolveError::Required`] when a `${NAME?message}` reference finds
    ///   its variable unset, or a `${NAME:?message}` reference finds it unset
    ///   or empty;
    /// - [`ResolveError::Cycle`] when file values refer to each other in a loop
    ///   (whole-file scope only);
    /// - [`ResolveError::DepthExceeded`] when nesting goes deeper than
    ///   [`ExpandOptions::max_depth`];
    /// - [`ResolveError::TooManyReferences`] when one value has more reference
    ///   candidates than [`ExpandOptions::max_references`];
    /// - [`ResolveError::ValueTooLarge`] when a resolved value grows longer
    ///   than [`ExpandOptions::max_value_bytes`];
    /// - [`ResolveError::OutputTooLarge`] when the whole result grows larger
    ///   than [`ExpandOptions::max_output_bytes`].
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::{parse_strict, ExpandOptions};
    ///
    /// let doc = parse_strict("HOST=localhost\nURL=http://${HOST}:${PORT}/\n")?;
    /// let lookup = |name: &str| (name == "PORT").then(|| "8080".to_string());
    ///
    /// let resolved = doc.resolve(lookup, &ExpandOptions::portable())?;
    /// assert_eq!(resolved.entries()[1].value.as_deref(), Some("http://localhost:8080/"));
    ///
    /// // The document still holds the literal text.
    /// assert_eq!(doc.get("URL"), Some("http://${HOST}:${PORT}/"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// Operators need rules that accept them, such as the Compose rules:
    ///
    /// ```
    /// use adotenv::{parse_strict, ExpandOptions};
    ///
    /// let doc = parse_strict("LOG_LEVEL=\nA=${LOG_LEVEL:-info}\nB=${LOG_LEVEL-info}\n")?;
    ///
    /// // `:-` replaces an empty value too; `-` replaces only an unset one.
    /// let resolved = doc.resolve(|_| None, &ExpandOptions::compose())?;
    /// assert_eq!(resolved.entries()[1].value.as_deref(), Some("info"));
    /// assert_eq!(resolved.entries()[2].value.as_deref(), Some(""));
    ///
    /// let literal = doc.resolve(|_| None, &ExpandOptions::portable())?;
    /// assert_eq!(literal.entries()[1].value.as_deref(), Some("${LOG_LEVEL:-info}"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn resolve<F>(
        &self,
        lookup: F,
        options: &ExpandOptions,
    ) -> Result<ResolvedDocument, ResolveError>
    where
        F: Fn(&str) -> Option<String>,
    {
        Resolver::new(self, lookup, options, None, None).resolve_document()
    }

    /// Substitutes references like [`Document::resolve`] and passes `$(command)`
    /// forms to a callback.
    ///
    /// Variables resolve exactly as in [`Document::resolve`], with the same
    /// options. In addition, each eligible `$(command)` form is passed to
    /// `commands`, and the string it returns replaces the form. Command forms
    /// are eligible only when [`ReferenceRules::commands`] is on, as in
    /// [`ExpandOptions::dotenv_expand`]; otherwise they stay as written and the
    /// callback is never called.
    ///
    /// This crate never starts a process itself: the callback decides what a
    /// command means. It receives a [`CommandContext`] with the command text as
    /// written between `$(` and `)`, its byte ranges and `source_identity`.
    /// References inside the command text are not substituted, and the output
    /// is used as it is, not expanded again. A command inside an operator
    /// argument, as in `${REVISION:-$(git rev-parse HEAD)}`, runs only when the
    /// argument is used. With the `exec` feature,
    /// [`exec::Shell`](crate::exec::Shell) provides a callback that runs
    /// commands through the system shell.
    ///
    /// `source_identity` is an optional name for the file, such as its path. It
    /// is passed to the callback and recorded with each call unchanged; the
    /// crate never fills it in itself.
    ///
    /// Every call is recorded as a [`CommandCall`], in call order: in
    /// [`ResolvedDocument::commands`] on success and in
    /// [`ResolveFailure::commands`] on failure. Calls made before an error are
    /// not undone.
    ///
    /// # Errors
    ///
    /// Returns a [`ResolveFailure`] whose [`error`](ResolveFailure::error) is
    /// any error of [`Document::resolve`], or [`ResolveError::Command`] when the
    /// callback returns a [`CommandError`]. Its
    /// [`commands`](ResolveFailure::commands) lists the calls made before the
    /// failure, including a failed call.
    ///
    /// # Examples
    ///
    /// The callback here returns a fixed string instead of running anything:
    ///
    /// ```
    /// use adotenv::{parse_strict, CommandOutcome, ExpandOptions};
    ///
    /// let doc = parse_strict("REVISION=$(git rev-parse --short HEAD)\n")?;
    /// let resolved = doc.resolve_with_commands(
    ///     |_| None,
    ///     |context| {
    ///         assert_eq!(context.command, "git rev-parse --short HEAD");
    ///         Ok("4f2a9c1".to_string())
    ///     },
    ///     Some("app.env"),
    ///     &ExpandOptions::dotenv_expand(),
    /// )?;
    ///
    /// assert_eq!(resolved.entries()[0].value.as_deref(), Some("4f2a9c1"));
    /// let call = &resolved.commands()[0];
    /// assert_eq!(call.outcome, CommandOutcome::Output("4f2a9c1".into()));
    /// assert_eq!(call.source.as_deref(), Some("app.env"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
        let mut resolver = Resolver::new(
            self,
            lookup,
            options,
            Some(&mut commands),
            source_identity.map(str::to_string),
        );
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

/// Fixed charge for one segment against [`ExpandOptions::max_output_bytes`].
const SEGMENT_BYTES: usize = core::mem::size_of::<ResolvedSegment>();
/// Fixed charge for one entry against [`ExpandOptions::max_output_bytes`].
const ENTRY_BYTES: usize = core::mem::size_of::<ResolvedEntry>();
/// Fixed charge for one recorded command call.
const COMMAND_BYTES: usize = core::mem::size_of::<CommandCall>();

struct Resolver<'a, 'src, F> {
    document: &'a Document<'src>,
    lookup: F,
    options: ExpandOptions,
    bindings: BTreeMap<&'a str, usize>,
    /// Resolved entries by entry id. Entries move into the output at the end.
    cache: Vec<Option<Cached>>,
    stack: Vec<usize>,
    commands: Option<&'a mut CommandCallback<'a>>,
    source_identity: Option<String>,
    command_calls: Vec<CommandCall>,
    /// Bytes charged against [`ExpandOptions::max_output_bytes`].
    output_bytes: usize,
}

struct Cached {
    entry: ResolvedEntry,
    /// Charged size of `entry.segments`, nested parts included.
    weight: usize,
}

/// Segments with their charged size, nested parts included.
#[derive(Default)]
struct Parts {
    segments: Vec<ResolvedSegment>,
    weight: usize,
}

impl Parts {
    fn one(segment: ResolvedSegment, weight: usize) -> Self {
        Self {
            segments: vec![segment],
            weight,
        }
    }
}

/// A variable binding found by lookup. File bindings stay in the cache until
/// a segment actually needs a copy of their provenance.
enum Bound {
    Unset,
    Lookup(String),
    /// A bare declaration read as empty under [`BareKeys::Empty`].
    Empty(usize),
    /// A resolved, cached file binding.
    File(usize),
}

impl<'a, 'src, F> Resolver<'a, 'src, F>
where
    F: Fn(&str) -> Option<String>,
{
    fn new(
        document: &'a Document<'src>,
        lookup: F,
        options: &ExpandOptions,
        commands: Option<&'a mut CommandCallback<'a>>,
        source_identity: Option<String>,
    ) -> Self {
        Self {
            document,
            lookup,
            options: *options,
            bindings: BTreeMap::new(),
            cache: core::iter::repeat_with(|| None)
                .take(document.entries.len())
                .collect(),
            stack: Vec::new(),
            commands,
            source_identity,
            command_calls: Vec::new(),
            output_bytes: 0,
        }
    }

    fn resolve_document(&mut self) -> Result<ResolvedDocument, ResolveError> {
        match self.options.scope {
            ExpandScope::Sequential => {
                for entry in self.document.entries() {
                    self.resolve_entry(entry.id())?;
                    self.bind(entry);
                }
            }
            ExpandScope::WholeFile => {
                for entry in self.document.entries() {
                    self.bind(entry);
                }
                for entry in self.document.entries() {
                    self.resolve_entry(entry.id())?;
                }
            }
        }
        let entries = self
            .cache
            .iter_mut()
            .map(|cached| cached.take().expect("every entry was resolved").entry)
            .collect();
        Ok(ResolvedDocument {
            entries,
            commands: Vec::new(),
        })
    }

    /// Charge `bytes` against the output budget for the entry being resolved.
    fn charge(&mut self, entry: Entry<'_>, bytes: usize) -> Result<(), ResolveError> {
        self.output_bytes = self.output_bytes.saturating_add(bytes);
        if self.output_bytes > self.options.max_output_bytes {
            return Err(ResolveError::OutputTooLarge {
                key: entry.key().to_string(),
                limit: self.options.max_output_bytes,
                span: entry.value_span().unwrap_or_else(|| entry.line_span()),
            });
        }
        Ok(())
    }

    fn resolve_entry(&mut self, id: usize) -> Result<(), ResolveError> {
        if self.cache[id].is_some() {
            return Ok(());
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
        self.cache[id] = Some(result?);
        Ok(())
    }

    fn resolve_entry_value(&mut self, entry: Entry<'a>) -> Result<Cached, ResolveError> {
        let key = entry.key().to_string();
        let state = entry.state();
        let Some(value) = entry.value() else {
            let effective_value = effective_binding(entry, None);
            self.charge(
                entry,
                ENTRY_BYTES + key.len() + effective_value.as_ref().map_or(0, String::len),
            )?;
            return Ok(Cached {
                entry: ResolvedEntry {
                    entry: entry.id(),
                    key,
                    state,
                    value: None,
                    effective_value,
                    segments: Vec::new(),
                },
                weight: 0,
            });
        };
        // References nested past the depth limit can never be resolved.
        let references = if self.options.rules == ReferenceRules::NONE {
            Vec::new()
        } else {
            entry
                .references_within_depth_bounded(
                    self.options.max_depth.saturating_add(1),
                    self.options.max_references,
                )
                .ok_or_else(|| ResolveError::TooManyReferences {
                    key: key.clone(),
                    limit: self.options.max_references,
                    span: entry.value_span().unwrap_or_else(|| entry.line_span()),
                })?
        };
        let parts = self.resolve_segments(entry, value, 0..value.len(), None, &references)?;
        let resolved = parts
            .segments
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
        let effective_value = effective_binding(entry, Some(&resolved));
        self.charge(
            entry,
            ENTRY_BYTES
                + key.len()
                + resolved.len()
                + effective_value.as_ref().map_or(0, String::len),
        )?;
        Ok(Cached {
            entry: ResolvedEntry {
                entry: entry.id(),
                effective_value,
                key,
                state,
                value: Some(resolved),
                segments: parts.segments,
            },
            weight: parts.weight,
        })
    }

    /// Resolve the top-level references of `range`, or the direct children of
    /// `references[parent]` when a parent is given.
    fn resolve_segments(
        &mut self,
        entry: Entry<'a>,
        decoded: &str,
        range: Range<usize>,
        parent: Option<usize>,
        references: &[Reference],
    ) -> Result<Parts, ResolveError> {
        let mut parts = Parts::default();
        let mut length = 0usize;
        let mut cursor = range.start;
        let (mut index, last) = match parent {
            Some(parent) => (parent + 1, subtree_end(references, parent)),
            None => (0, references.len()),
        };
        while index < last {
            let current = index;
            // Skip the candidates nested inside this one; they are resolved,
            // if at all, as part of its argument.
            index = subtree_end(references, current);
            let reference = &references[current];
            if let Some(parent) = parent.map(|parent| &references[parent]) {
                if reference.depth != parent.depth.saturating_add(1)
                    || !reference.span_within(parent.argument.as_ref())
                {
                    continue;
                }
            } else if reference.parent.is_some() {
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
                let literal = self.literal(entry, decoded, cursor..start)?;
                self.append(&mut parts, &mut length, literal, entry)?;
            }
            let replacement = match reference.kind {
                ReferenceKind::Variable => {
                    self.resolve_reference(entry, current, decoded, start..end, references)?
                }
                ReferenceKind::Command => self.run_command(entry, reference)?,
            };
            self.append(&mut parts, &mut length, replacement, entry)?;
            cursor = end;
        }
        if cursor < range.end {
            let literal = self.literal(entry, decoded, cursor..range.end)?;
            self.append(&mut parts, &mut length, literal, entry)?;
        }
        Ok(parts)
    }

    /// Append already charged segments, enforcing the per-value byte limit.
    fn append(
        &self,
        parts: &mut Parts,
        length: &mut usize,
        replacement: Parts,
        entry: Entry<'_>,
    ) -> Result<(), ResolveError> {
        for segment in &replacement.segments {
            account(length, segment, self.options.max_value_bytes, entry)?;
        }
        parts.segments.extend(replacement.segments);
        parts.weight = parts.weight.saturating_add(replacement.weight);
        Ok(())
    }

    fn literal(
        &mut self,
        entry: Entry<'a>,
        decoded: &str,
        range: Range<usize>,
    ) -> Result<Parts, ResolveError> {
        let weight = SEGMENT_BYTES + range.len();
        self.charge(entry, weight)?;
        Ok(Parts::one(literal_segment(entry, decoded, range), weight))
    }

    fn run_command(
        &mut self,
        entry: Entry<'a>,
        reference: &Reference,
    ) -> Result<Parts, ResolveError> {
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
                let identity = source.as_ref().map_or(0, String::len);
                let weight = SEGMENT_BYTES + output.len() + identity;
                self.charge(entry, weight + COMMAND_BYTES + output.len() + identity)?;
                Ok(Parts::one(
                    ResolvedSegment {
                        value: output,
                        kind: ResolvedSegmentKind::Command {
                            reference: reference_span,
                            payload,
                            source,
                        },
                    },
                    weight,
                ))
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
        index: usize,
        decoded: &str,
        decoded_span: Range<usize>,
        references: &[Reference],
    ) -> Result<Parts, ResolveError> {
        let reference = &references[index];
        let name_span = reference
            .name
            .as_ref()
            .expect("eligible variable has a name");
        let name = self.document.source()[name_span.clone()].to_string();
        let bound = self.lookup_variable(&name)?;
        let (set, empty) = self.bound_state(bound.as_ref());
        match reference.operator {
            None => {
                if set {
                    self.variable(entry, reference, name, bound)
                } else {
                    self.unresolved(entry, reference, decoded, decoded_span, &name)
                }
            }
            Some(ReferenceOperator::DefaultIfUnset) => {
                if set {
                    self.variable(entry, reference, name, bound)
                } else {
                    self.argument(entry, index, decoded, references)
                }
            }
            Some(ReferenceOperator::DefaultIfUnsetOrEmpty) => {
                if set && !empty {
                    self.variable(entry, reference, name, bound)
                } else {
                    self.argument(entry, index, decoded, references)
                }
            }
            Some(ReferenceOperator::AlternativeIfSet) => {
                if set {
                    self.argument(entry, index, decoded, references)
                } else {
                    Ok(Parts::default())
                }
            }
            Some(ReferenceOperator::AlternativeIfSetAndNonEmpty) => {
                if set && !empty {
                    self.argument(entry, index, decoded, references)
                } else {
                    Ok(Parts::default())
                }
            }
            Some(ReferenceOperator::ErrorIfUnset) => {
                if set {
                    self.variable(entry, reference, name, bound)
                } else {
                    Err(self.required(entry, reference, decoded, name))
                }
            }
            Some(ReferenceOperator::ErrorIfUnsetOrEmpty) => {
                if set && !empty {
                    self.variable(entry, reference, name, bound)
                } else {
                    Err(self.required(entry, reference, decoded, name))
                }
            }
        }
    }

    /// Whether a binding is set and whether its value is empty.
    fn bound_state(&self, bound: Option<&Bound>) -> (bool, bool) {
        match bound {
            None | Some(Bound::Unset) => (false, true),
            Some(Bound::Lookup(text)) => (true, text.is_empty()),
            Some(Bound::Empty(_)) => (true, true),
            Some(Bound::File(id)) => (
                true,
                self.cached(*id)
                    .entry
                    .value
                    .as_deref()
                    .map_or(true, str::is_empty),
            ),
        }
    }

    fn cached(&self, id: usize) -> &Cached {
        self.cache[id]
            .as_ref()
            .expect("a file binding is resolved before use")
    }

    /// Build the substituted segment, charging a copy of any file provenance
    /// before it is made.
    fn variable(
        &mut self,
        entry: Entry<'a>,
        reference: &Reference,
        name: String,
        bound: Option<Bound>,
    ) -> Result<Parts, ResolveError> {
        let (value, origin, parts, weight) = match bound {
            Some(Bound::Lookup(text)) => {
                let weight = SEGMENT_BYTES + name.len() + text.len();
                self.charge(entry, weight)?;
                (text, ValueOrigin::Lookup, Vec::new(), weight)
            }
            Some(Bound::Empty(id)) => {
                let binding = self
                    .document
                    .entry(id)
                    .expect("binding id belongs to the document");
                let weight = SEGMENT_BYTES + name.len() + binding.key().len();
                self.charge(entry, weight)?;
                (String::new(), file_origin(binding), Vec::new(), weight)
            }
            Some(Bound::File(id)) => {
                let binding = self
                    .document
                    .entry(id)
                    .expect("binding id belongs to the document");
                let cached = self.cached(id);
                let weight = (SEGMENT_BYTES + name.len() + binding.key().len())
                    .saturating_add(cached.entry.value.as_ref().map_or(0, String::len))
                    .saturating_add(cached.weight);
                self.charge(entry, weight)?;
                let cached = self.cached(id);
                (
                    cached.entry.value.clone().unwrap_or_default(),
                    file_origin(binding),
                    cached.entry.segments.clone(),
                    weight,
                )
            }
            None | Some(Bound::Unset) => unreachable!("only set bindings are substituted"),
        };
        Ok(Parts::one(
            ResolvedSegment {
                value,
                kind: ResolvedSegmentKind::Variable {
                    name,
                    reference: reference.span.clone(),
                    operator: reference.operator,
                    origin: Some(origin),
                    parts,
                },
            },
            weight,
        ))
    }

    fn unresolved(
        &mut self,
        entry: Entry<'a>,
        reference: &Reference,
        decoded: &str,
        decoded_span: Range<usize>,
        name: &str,
    ) -> Result<Parts, ResolveError> {
        match self.options.unresolved {
            Unresolved::Keep => {
                let weight = SEGMENT_BYTES + name.len() + decoded_span.len();
                self.charge(entry, weight)?;
                Ok(Parts::one(
                    ResolvedSegment {
                        value: decoded[decoded_span].to_string(),
                        kind: ResolvedSegmentKind::Unresolved {
                            name: name.to_string(),
                            reference: reference.span.clone(),
                        },
                    },
                    weight,
                ))
            }
            Unresolved::Empty => Ok(Parts::default()),
            Unresolved::Error => Err(ResolveError::Unresolved {
                key: self.current_key(),
                name: name.to_string(),
                span: reference.span.clone(),
            }),
        }
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
        index: usize,
        decoded: &str,
        references: &[Reference],
    ) -> Result<Parts, ResolveError> {
        let Some(argument) = references[index].argument.as_ref() else {
            return Ok(Parts::default());
        };
        let decoded_range = decoded_start_for_raw(entry, argument.start)
            .and_then(|start| decoded_end_for_raw(entry, argument.end).map(|end| start..end));
        let Some(range) = decoded_range else {
            let weight = SEGMENT_BYTES + argument.len();
            self.charge(entry, weight)?;
            return Ok(Parts::one(
                ResolvedSegment {
                    value: self.document.source()[argument.clone()].to_string(),
                    kind: ResolvedSegmentKind::Literal {
                        span: argument.clone(),
                    },
                },
                weight,
            ));
        };
        self.resolve_segments(entry, decoded, range, Some(index), references)
    }

    fn lookup_variable(&mut self, name: &str) -> Result<Option<Bound>, ResolveError> {
        match self.options.lookup_order {
            LookupOrder::CallerFirst => {
                if let Some(value) = (self.lookup)(name) {
                    return Ok(Some(Bound::Lookup(value)));
                }
                self.file_binding(name)
            }
            LookupOrder::FileFirst => {
                let file = self.file_binding(name)?;
                if file.is_some() {
                    return Ok(file);
                }
                Ok((self.lookup)(name).map(Bound::Lookup))
            }
        }
    }

    /// Make `entry` the file binding of its key when lookup would select it.
    fn bind(&mut self, entry: Entry<'a>) {
        if !entry.participates_in_lookup() {
            return;
        }
        match self.document.dialect().duplicates {
            Duplicates::First => {
                self.bindings.entry(entry.key()).or_insert(entry.id());
            }
            Duplicates::Last => {
                self.bindings.insert(entry.key(), entry.id());
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
                return Ok(Some(Bound::Empty(id)));
            }
            return Ok(Some(Bound::Unset));
        }
        self.resolve_entry(id)?;
        Ok(Some(Bound::File(id)))
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
            span: entry.value_span().unwrap_or_else(|| entry.line_span()),
        });
    }
    Ok(())
}

/// The index just past `references[index]` and the candidates nested in it.
///
/// Candidates are sorted by start, and a nested candidate starts inside its
/// container's span, so they form one contiguous run after the container.
fn subtree_end(references: &[Reference], index: usize) -> usize {
    let end = references[index].span.end;
    let rest = &references[index + 1..];
    index + 1 + rest.partition_point(|reference| reference.span.start < end)
}

/// The last decoded boundary that maps to original offset `raw` or earlier.
///
/// Original offsets never decrease as decoded offsets grow, so a binary search
/// finds the boundary a forward scan would stop at.
fn decoded_start_for_raw(entry: Entry<'_>, raw: usize) -> Option<usize> {
    let value = entry.value()?;
    // Find the first byte offset whose enclosing boundary maps past `raw`.
    let (mut low, mut high) = (0, value.len() + 1);
    while low < high {
        let middle = low + (high - low) / 2;
        if entry
            .decoded_offset_to_raw(floor_boundary(value, middle))
            .ok()?
            <= raw
        {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low.checked_sub(1)
        .map(|offset| floor_boundary(value, offset))
}

/// The first decoded boundary before the end of the value that maps to
/// original offset `raw` or later, or the end of the value.
fn decoded_end_for_raw(entry: Entry<'_>, raw: usize) -> Option<usize> {
    let value = entry.value()?;
    let (mut low, mut high) = (0, value.len());
    while low < high {
        let middle = low + (high - low) / 2;
        let boundary = ceil_boundary(value, middle);
        if boundary == value.len() || entry.decoded_offset_to_raw(boundary).ok()? >= raw {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Some(ceil_boundary(value, low))
}

fn floor_boundary(value: &str, mut offset: usize) -> usize {
    while !value.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn ceil_boundary(value: &str, mut offset: usize) -> usize {
    while !value.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}
