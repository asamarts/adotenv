//! Opt-in filesystem loading of `.env` cascades.
//!
//! This module is compiled only with the `load` feature. It reads and validates
//! every participating file before returning any values, and performs no
//! process-environment mutation, expansion or command execution.
//!
//! The default cascade is `.env`, `.env.local`, `.env.{mode}` and
//! `.env.{mode}.local`, in that order. Later files override earlier files by
//! effective key.

use std::{
    collections::{BTreeMap, HashMap},
    fmt, fs, io,
    ops::Range,
    path::{Path, PathBuf},
    string::String,
    sync::Arc,
    vec::Vec,
};

use crate::{
    encode_value, parse_strict_with, parse_with, Diagnostic, Dialect, ExpandOptions, ExpandScope,
    KeySyntax, ResolveError, Severity,
};

/// A borrowed file, source span and one-based line identifying a loaded value.
///
/// Origins produced by [`LoadedEnvironment`] borrow the environment's shared
/// file table. Use the accessors to read the path, span and line.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct LoadOrigin<'a> {
    path: &'a Path,
    span: Range<usize>,
    line: usize,
}

impl<'a> LoadOrigin<'a> {
    /// File path used for the load.
    #[must_use]
    pub const fn path(&self) -> &'a Path {
        self.path
    }
    /// Original value span in that file.
    #[must_use]
    pub const fn span(&self) -> &Range<usize> {
        &self.span
    }
    /// One-based source line.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }
}

/// One effective declaration observed during a load.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadedEntry<'a> {
    key: &'a str,
    value: &'a str,
    origin: LoadOrigin<'a>,
}

impl<'a> LoadedEntry<'a> {
    /// Declaration key.
    #[must_use]
    pub const fn key(&self) -> &'a str {
        self.key
    }
    /// Decoded literal value.
    #[must_use]
    pub const fn value(&self) -> &'a str {
        self.value
    }
    /// Where the declaration came from.
    #[must_use]
    pub const fn origin(&self) -> &LoadOrigin<'a> {
        &self.origin
    }
}

/// A later file overriding an earlier effective key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadOverride<'a> {
    key: &'a str,
    previous: LoadOrigin<'a>,
    replacement: LoadOrigin<'a>,
}

impl<'a> LoadOverride<'a> {
    /// Overridden key.
    #[must_use]
    pub const fn key(&self) -> &'a str {
        self.key
    }
    /// Earlier active value.
    #[must_use]
    pub const fn previous(&self) -> &LoadOrigin<'a> {
        &self.previous
    }
    /// Later active value.
    #[must_use]
    pub const fn replacement(&self) -> &LoadOrigin<'a> {
        &self.replacement
    }
}

/// Duplicate declarations for one key within a single file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadRepeated<'a> {
    key: &'a str,
    origins: &'a [CompactOrigin],
    files: &'a [PathBuf],
}

impl<'a> LoadRepeated<'a> {
    /// Repeated key.
    #[must_use]
    pub const fn key(&self) -> &'a str {
        self.key
    }
    /// All occurrences in source order.
    pub fn origins(&self) -> impl Iterator<Item = LoadOrigin<'a>> + '_ {
        self.origins
            .iter()
            .map(|origin| view_origin(self.files, origin))
    }
}

/// Why a declaration was not loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SkipReason {
    /// A key-only declaration has no literal value.
    Bare,
    /// The selected dialect excludes this declaration from lookup.
    NonParticipating,
    /// The declaration key is empty.
    EmptyKey,
}

/// A declaration that was retained in the report but not loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadSkipped<'a> {
    key: Option<&'a str>,
    origin: LoadOrigin<'a>,
    reason: SkipReason,
}

impl<'a> LoadSkipped<'a> {
    /// Key, if one was available.
    #[must_use]
    pub const fn key(&self) -> Option<&'a str> {
        self.key
    }
    /// File and position.
    #[must_use]
    pub const fn origin(&self) -> &LoadOrigin<'a> {
        &self.origin
    }
    /// Why it was skipped.
    #[must_use]
    pub const fn reason(&self) -> SkipReason {
        self.reason
    }
}

/// An owned origin used by resolution errors.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct LoadOriginOwned {
    path: PathBuf,
    span: Range<usize>,
    line: usize,
}

impl LoadOriginOwned {
    /// File path used for the load.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Original value span in that file.
    #[must_use]
    pub const fn span(&self) -> &Range<usize> {
        &self.span
    }
    /// One-based source line.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompactOrigin {
    file: usize,
    span: Range<usize>,
    line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValueRecord {
    value: Arc<str>,
    origin: Option<CompactOrigin>,
    raw: Option<(usize, Range<usize>, Range<usize>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryRecord {
    key: Arc<str>,
    value: Arc<str>,
    origin: CompactOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OverrideRecord {
    key: Arc<str>,
    previous: CompactOrigin,
    replacement: CompactOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepeatedRecord {
    key: Arc<str>,
    origins: Vec<CompactOrigin>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkippedRecord {
    key: Option<Arc<str>>,
    origin: CompactOrigin,
    reason: SkipReason,
}

/// Fully validated result of a file load.
///
/// Values, shared-file origins and raw winning tokens are retained by default.
/// Reports are opt-in through [`LoadOptions::with_reports`] or
/// [`LoadOptions::full`]. Values and report views borrow the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedEnvironment {
    records: BTreeMap<Arc<str>, ValueRecord>,
    raw_source: String,
    files: Vec<PathBuf>,
    dialect: Dialect,
    entries: Vec<EntryRecord>,
    overrides: Vec<OverrideRecord>,
    repeated: Vec<RepeatedRecord>,
    skipped: Vec<SkippedRecord>,
    raw_tokens: bool,
    origins_retained: bool,
}

/// Retention policy for loading.
///
/// [`LoadOptions::new`] retains values, shared-file origins and raw winning
/// tokens while leaving reports empty. [`LoadOptions::full`] additionally
/// retains entries, overrides, repeats and skipped declarations. Applications
/// that only need values should use [`crate::load::load_values`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadOptions {
    /// Dialect used to parse every file.
    pub dialect: Dialect,
    /// Retain raw winning key/value tokens for contextual resolution.
    pub retain_raw_tokens: bool,
    /// Retain loaded-entry, override, repeat and skipped reports.
    pub retain_reports: bool,
    /// Retain the final origin map and cross-file override reports.
    pub retain_origins: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self::new(Dialect::PORTABLE)
    }
}

impl LoadOptions {
    /// Default retention using the selected dialect.
    ///
    /// Values, shared-file origins and raw winning tokens are retained;
    /// reports are disabled.
    #[must_use]
    pub const fn new(dialect: Dialect) -> Self {
        Self {
            dialect,
            retain_raw_tokens: true,
            retain_reports: false,
            retain_origins: true,
        }
    }
    /// Full retention using the selected dialect.
    ///
    /// Includes entries, overrides, repeats and skipped-declaration reports.
    #[must_use]
    pub const fn full(dialect: Dialect) -> Self {
        Self {
            dialect,
            retain_raw_tokens: true,
            retain_reports: true,
            retain_origins: true,
        }
    }
    /// Select a different dialect.
    #[must_use]
    pub const fn with_dialect(mut self, dialect: Dialect) -> Self {
        self.dialect = dialect;
        self
    }
    /// Control retention of raw winning tokens.
    #[must_use]
    pub const fn with_raw_tokens(mut self, retain: bool) -> Self {
        self.retain_raw_tokens = retain;
        self
    }
    /// Control retention of report vectors.
    #[must_use]
    pub const fn with_reports(mut self, retain: bool) -> Self {
        self.retain_reports = retain;
        self
    }
    /// Control retention of the final origin map and override reports.
    #[must_use]
    pub const fn with_origins(mut self, retain: bool) -> Self {
        self.retain_origins = retain;
        self
    }
}

impl LoadedEnvironment {
    /// Iterate final effective literal values in key order.
    pub fn values(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.records
            .iter()
            .map(|(key, record)| (key.as_ref(), record.value.as_ref()))
    }
    /// Whether raw winning tokens were retained for contextual resolution.
    #[must_use]
    pub const fn retains_raw_tokens(&self) -> bool {
        self.raw_tokens
    }
    /// Final value for one key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.records.get(key).map(|record| record.value.as_ref())
    }
    /// Final origin for one effective key.
    ///
    /// Returns `None` when loaded with [`LoadOptions::with_origins`] set to
    /// `false`.
    #[must_use]
    pub fn origin(&self, key: &str) -> Option<LoadOrigin<'_>> {
        if self.origins_retained {
            self.records
                .get(key)
                .and_then(|record| record.origin.as_ref())
                .map(|origin| view_origin(&self.files, origin))
        } else {
            None
        }
    }
    fn owned_origin(&self, key: &str) -> Option<LoadOriginOwned> {
        self.origin(key).map(|origin| LoadOriginOwned {
            path: origin.path.to_path_buf(),
            span: origin.span,
            line: origin.line,
        })
    }
    /// Every loaded declaration, including later-overridden entries.
    ///
    /// Empty when loaded with [`LoadOptions::with_reports`] set to `false`.
    pub fn entries(&self) -> impl Iterator<Item = LoadedEntry<'_>> + '_ {
        self.entries.iter().map(|entry| LoadedEntry {
            key: entry.key.as_ref(),
            value: entry.value.as_ref(),
            origin: view_origin(&self.files, &entry.origin),
        })
    }
    /// Cross-file overrides in load order.
    ///
    /// Empty when reports or origins are disabled.
    pub fn overrides(&self) -> impl Iterator<Item = LoadOverride<'_>> + '_ {
        self.overrides.iter().map(|override_report| LoadOverride {
            key: override_report.key.as_ref(),
            previous: view_origin(&self.files, &override_report.previous),
            replacement: view_origin(&self.files, &override_report.replacement),
        })
    }
    /// Duplicate declarations within individual files.
    ///
    /// Empty when loaded with [`LoadOptions::with_reports`] set to `false`.
    pub fn repeated(&self) -> impl Iterator<Item = LoadRepeated<'_>> + '_ {
        self.repeated.iter().map(|repeated| LoadRepeated {
            key: repeated.key.as_ref(),
            origins: &repeated.origins,
            files: &self.files,
        })
    }
    /// Declarations that were inspected but not loaded.
    ///
    /// Empty when loaded with [`LoadOptions::with_reports`] set to `false`.
    pub fn skipped(&self) -> impl Iterator<Item = LoadSkipped<'_>> + '_ {
        self.skipped.iter().map(|skipped| LoadSkipped {
            key: skipped.key.as_deref(),
            origin: view_origin(&self.files, &skipped.origin),
            reason: skipped.reason,
        })
    }
    /// Number of effective values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }
    /// Whether no effective values were loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    /// Drop provenance and reports, retaining only final effective values.
    #[must_use]
    pub fn into_values(self) -> LoadedValues {
        LoadedValues {
            values: self
                .records
                .into_iter()
                .map(|(key, record)| (key.to_string(), record.value.to_string()))
                .collect(),
        }
    }
}

fn view_origin<'a>(files: &'a [PathBuf], origin: &CompactOrigin) -> LoadOrigin<'a> {
    LoadOrigin {
        path: files[origin.file].as_path(),
        span: origin.span.clone(),
        line: origin.line,
    }
}

/// A compact values-only result of loading one or more files.
///
/// This drops origins, raw winning tokens, skipped/repeated reports and
/// cross-file override reports. Use [`LoadedEnvironment`] when that provenance
/// or contextual resolution is required.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadedValues {
    values: BTreeMap<String, String>,
}

impl LoadedValues {
    /// Final effective literal values.
    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
    /// Final value for one key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    /// Number of effective values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }
    /// Whether no effective values were loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    /// Consume the result and return its map.
    #[must_use]
    pub fn into_values(self) -> BTreeMap<String, String> {
        self.values
    }
}

/// A resolved loaded environment.
///
/// Values have been processed by [`LoadedEnvironment::resolve_contextual`] or
/// [`LoadedEnvironment::resolve_decoded`]. Origins identify the winning
/// declaration that supplied the original value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEnvironment {
    values: BTreeMap<String, String>,
    origins: BTreeMap<String, CompactOrigin>,
    files: Vec<PathBuf>,
}

impl ResolvedEnvironment {
    /// Final resolved values.
    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
    /// One resolved value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    /// Origin of the winning declaration for one key.
    #[must_use]
    pub fn origin(&self, key: &str) -> Option<LoadOrigin<'_>> {
        self.origins
            .get(key)
            .map(|origin| view_origin(&self.files, origin))
    }
    /// Number of resolved values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }
    /// Whether no values remain.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Failure from loaded-environment resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedResolveError {
    key: Option<String>,
    origin: Option<LoadOriginOwned>,
    message: String,
}

impl LoadedResolveError {
    fn new(
        key: Option<String>,
        origin: Option<LoadOriginOwned>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            key,
            origin,
            message: message.into(),
        }
    }
    /// Key being resolved when available.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }
    /// Origin of the winning declaration when available.
    #[must_use]
    pub fn origin(&self) -> Option<&LoadOriginOwned> {
        self.origin.as_ref()
    }
    /// Human-readable failure message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for LoadedResolveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(origin) = &self.origin {
            write!(formatter, "{}:{}: ", origin.path().display(), origin.line())?;
        }
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LoadedResolveError {}

impl LoadedEnvironment {
    /// Resolve variable references in the final decoded values.
    ///
    /// This is the explicit **decoded-value** policy: loaded values are treated
    /// as unquoted decoded text. Original quote, escape and source-line context
    /// is not retained by [`LoadedEnvironment`]; use [`crate::Document::resolve`]
    /// when context-aware resolution is required.
    ///
    /// Only [`ExpandScope::WholeFile`] is accepted. The synthetic resolution
    /// order is lexicographic by key, but whole-file bindings do not depend on
    /// order. Command substitutions remain literal unless the caller supplies a
    /// context-aware document callback.
    ///
    /// Keys take part exactly as loaded, whatever dialect read them, so a
    /// braced reference such as `${A B}` finds a quoted key `A B`. A key is
    /// never reinterpreted as a different key: one that the synthetic source
    /// cannot represent, such as a key that contains both an apostrophe and
    /// whitespace, is an error naming that key.
    ///
    /// # Errors
    /// Returns [`LoadedResolveError`] for unsupported sequential scope, keys or
    /// values that cannot be represented exactly, or resolution failures. The
    /// error retains the winning declaration origin when available.
    pub fn resolve_decoded<F>(
        &self,
        lookup: F,
        options: &ExpandOptions,
    ) -> Result<ResolvedEnvironment, LoadedResolveError>
    where
        F: Fn(&str) -> Option<String>,
    {
        if options.scope != ExpandScope::WholeFile {
            return Err(whole_file_required());
        }
        let unrepresentable = |key: &str, message: &str| {
            LoadedResolveError::new(Some(key.to_string()), self.owned_origin(key), message)
        };
        let mut source = String::new();
        for (key, record) in &self.records {
            let key_token = synthetic_key(key)
                .ok_or_else(|| unrepresentable(key, "key cannot be represented exactly"))?;
            let token = encode_value(record.value.as_ref())
                .ok_or_else(|| unrepresentable(key, "value cannot be represented exactly"))?;
            source.push_str(&key_token);
            source.push('=');
            source.push_str(&token);
            source.push('\n');
        }
        let document = parse_strict_with(&source, SYNTHETIC_DIALECT).map_err(|error| {
            LoadedResolveError::new(None, None, format!("cannot prepare resolution: {error}"))
        })?;
        // Every loaded key must come back exactly, with its exact value, so
        // resolution can never bind a different key.
        let mut entries = document.entries();
        for (key, record) in &self.records {
            let Some(entry) = entries.next().filter(|entry| entry.key() == key.as_ref()) else {
                return Err(unrepresentable(key, "key cannot be represented exactly"));
            };
            if entry.value() != Some(record.value.as_ref()) {
                return Err(unrepresentable(key, "value cannot be represented exactly"));
            }
        }
        if entries.next().is_some() {
            return Err(LoadedResolveError::new(
                None,
                None,
                "cannot prepare resolution: unexpected declaration",
            ));
        }
        let mut options = *options;
        // The synthetic document must treat decoded text as unquoted text. Any
        // quoting introduced by the exact encoder must not change eligibility.
        options.rules.single_quotes = true;
        options.rules.backtick_quotes = true;
        options.rules.escaped_dollar = true;
        let resolved = document.resolve(lookup, &options).map_err(|error| {
            let key = resolve_error_key(&error).to_string();
            let origin = self.owned_origin(&key);
            LoadedResolveError::new(Some(key), origin, error.to_string())
        })?;
        let mut values = BTreeMap::new();
        for entry in resolved.entries() {
            values.insert(entry.key.clone(), entry.value.clone().unwrap_or_default());
        }
        Ok(ResolvedEnvironment {
            values,
            origins: self
                .records
                .iter()
                .filter_map(|(key, record)| {
                    record
                        .origin
                        .as_ref()
                        .map(|origin| (key.to_string(), origin.clone()))
                })
                .collect(),
            files: self.files.clone(),
        })
    }

    /// Resolve references in final values using each winning declaration's
    /// original literal context.
    ///
    /// The winning key/value tokens are reconstructed in cascade order and
    /// parsed with the dialect used at load time. Quote, escape and bare-key
    /// context therefore match the source that won the cascade. The synthetic
    /// source has no original file offsets; errors retain the winning
    /// declaration's [`LoadOrigin`], key and message. It returns an error when
    /// the environment was loaded with raw-token retention disabled; use
    /// [`Self::resolve_decoded`] in that case.
    ///
    /// Only [`ExpandScope::WholeFile`] is accepted, as for
    /// [`Self::resolve_decoded`]. The winning declarations come from different
    /// files, so no single declaration order gives sequential scope its
    /// meaning: a reference to a key that a later file overrides would see no
    /// earlier binding.
    ///
    /// # Errors
    /// Returns [`LoadedResolveError`] for unsupported sequential scope, if raw
    /// tokens were not retained, if the reconstructed source cannot be parsed,
    /// or if resolution fails.
    pub fn resolve_contextual<F>(
        &self,
        lookup: F,
        options: &ExpandOptions,
    ) -> Result<ResolvedEnvironment, LoadedResolveError>
    where
        F: Fn(&str) -> Option<String>,
    {
        if options.scope != ExpandScope::WholeFile {
            return Err(whole_file_required());
        }
        if !self.raw_tokens {
            return Err(LoadedResolveError::new(
                None,
                None,
                "raw token context was not retained; load with raw tokens or use resolve_decoded",
            ));
        }
        let mut entries: Vec<_> = self
            .records
            .values()
            .filter_map(|record| record.raw.clone())
            .collect();
        entries.sort_unstable_by_key(|(order, _, _)| *order);
        let mut source = String::new();
        for (_, key_range, value_range) in entries {
            source.push_str(&self.raw_source[key_range.clone()]);
            source.push('=');
            source.push_str(&self.raw_source[value_range.clone()]);
            source.push('\n');
        }
        let document = parse_strict_with(&source, self.dialect).map_err(|error| {
            LoadedResolveError::new(None, None, format!("cannot prepare resolution: {error}"))
        })?;
        let resolved = document.resolve(lookup, options).map_err(|error| {
            let key = resolve_error_key(&error).to_string();
            let origin = self.owned_origin(&key);
            LoadedResolveError::new(Some(key), origin, error.to_string())
        })?;
        let mut values = BTreeMap::new();
        for entry in resolved.entries() {
            values.insert(entry.key.clone(), entry.value.clone().unwrap_or_default());
        }
        Ok(ResolvedEnvironment {
            values,
            origins: self
                .records
                .iter()
                .filter_map(|(key, record)| {
                    record
                        .origin
                        .as_ref()
                        .map(|origin| (key.to_string(), origin.clone()))
                })
                .collect(),
            files: self.files.clone(),
        })
    }
}

fn whole_file_required() -> LoadedResolveError {
    LoadedResolveError::new(
        None,
        None,
        "loaded environments require whole-file resolution scope",
    )
}

/// Dialect of the synthetic source built by
/// [`LoadedEnvironment::resolve_decoded`]: Portable values, single-quoted keys
/// taken literally, and bare keys that may hold any character except
/// whitespace and `=`.
const SYNTHETIC_DIALECT: Dialect = Dialect::PORTABLE
    .with_quoted_keys(true)
    .with_keys(KeySyntax::NonWhitespace);

/// Spell `key` so [`SYNTHETIC_DIALECT`] reads back exactly `key` on one line.
fn synthetic_key(key: &str) -> Option<String> {
    let line_break = |c: char| c == '\n' || c == '\r';
    if key.is_empty() || key.contains(line_break) {
        return None;
    }
    if !key.contains('\'') {
        return Some(format!("'{key}'"));
    }
    // An apostrophe cannot appear inside a quoted key, so the key must stand
    // bare: no whitespace or `=`, and no leading `#` that starts a comment.
    let bare =
        !key.starts_with(['\'', '#']) && !key.contains(|c: char| c == '=' || c.is_whitespace());
    bare.then(|| key.to_string())
}

fn resolve_error_key(error: &ResolveError) -> &str {
    match error {
        ResolveError::Unresolved { key, .. }
        | ResolveError::Required { key, .. }
        | ResolveError::Cycle { key, .. }
        | ResolveError::DepthExceeded { key, .. }
        | ResolveError::ValueTooLarge { key, .. }
        | ResolveError::OutputTooLarge { key, .. }
        | ResolveError::Command { key, .. } => key,
    }
}

/// File loading failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum LoadError {
    /// A file could not be read.
    Io {
        /// Path that failed.
        path: PathBuf,
        /// OS error.
        source: io::Error,
    },
    /// A file was not valid UTF-8.
    InvalidUtf8 {
        /// Path that failed.
        path: PathBuf,
    },
    /// A file contained a syntax or policy error.
    Invalid {
        /// Path that failed.
        path: PathBuf,
        /// First error in source order.
        diagnostic: Diagnostic,
    },
    /// A mode name could escape the cascade or is otherwise invalid.
    InvalidMode,
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::InvalidUtf8 { path } => {
                write!(formatter, "{} is not valid UTF-8", path.display())
            }
            Self::Invalid { path, diagnostic } => {
                write!(formatter, "{}: {diagnostic}", path.display())
            }
            Self::InvalidMode => formatter.write_str("mode must not contain path separators"),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Load the standard `.env` cascade from `directory` using Portable syntax.
///
/// Missing cascade files are skipped. A file that exists but cannot be read,
/// decoded, or parsed invalidates the complete load.
///
/// # Errors
/// Returns [`LoadError`] for invalid mode names, I/O errors, invalid UTF-8 or
/// syntax/policy errors.
pub fn load_cascade<P: AsRef<Path>>(
    directory: P,
    mode: &str,
) -> Result<LoadedEnvironment, LoadError> {
    load_cascade_with(directory, mode, Dialect::PORTABLE)
}

/// Load the standard cascade with an explicit dialect.
///
/// # Errors
/// Returns [`LoadError`] for invalid mode names, I/O errors, invalid UTF-8 or
/// syntax/policy errors.
pub fn load_cascade_with<P: AsRef<Path>>(
    directory: P,
    mode: &str,
    dialect: Dialect,
) -> Result<LoadedEnvironment, LoadError> {
    load_cascade_with_options(directory, mode, LoadOptions::new(dialect))
}

/// Load the standard cascade with explicit retention options.
///
/// # Errors
/// Returns [`LoadError`] for invalid mode names, I/O errors, invalid UTF-8 or
/// syntax/policy errors.
pub fn load_cascade_with_options<P: AsRef<Path>>(
    directory: P,
    mode: &str,
    options: LoadOptions,
) -> Result<LoadedEnvironment, LoadError> {
    if !valid_mode(mode) {
        return Err(LoadError::InvalidMode);
    }
    let directory = directory.as_ref();
    let mut paths = vec![directory.join(".env"), directory.join(".env.local")];
    if !mode.is_empty() {
        paths.push(directory.join(format!(".env.{mode}")));
        paths.push(directory.join(format!(".env.{mode}.local")));
    }
    load_paths(paths, options, true)
}

/// Load an explicit ordered list of files, with later files overriding earlier
/// files by key.
///
/// Every listed file must exist. Use [`load_cascade`] for optional standard
/// filenames.
///
/// # Errors
/// Returns [`LoadError`] for I/O errors, invalid UTF-8 or syntax/policy errors.
pub fn load_files<I, P>(paths: I, dialect: Dialect) -> Result<LoadedEnvironment, LoadError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    load_files_with_options(paths, LoadOptions::new(dialect))
}

/// Load an explicit ordered file list with explicit retention options.
///
/// # Errors
/// Returns [`LoadError`] for I/O errors, invalid UTF-8 or syntax/policy errors.
pub fn load_files_with_options<I, P>(
    paths: I,
    options: LoadOptions,
) -> Result<LoadedEnvironment, LoadError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    load_paths(paths, options, false)
}

/// Load the standard cascade while retaining only final effective values.
///
/// This is the compact application-loading path. It applies the same strict
/// read/validate and override semantics as [`load_cascade`], then drops
/// origins, raw winning tokens and reports. The shared parser projects only
/// effective values, so no lossless document is retained. Missing cascade files
/// are skipped.
///
/// # Errors
/// Returns [`LoadError`] for invalid mode names, I/O errors, invalid UTF-8 or
/// syntax/policy errors.
pub fn load_cascade_values<P: AsRef<Path>>(
    directory: P,
    mode: &str,
) -> Result<LoadedValues, LoadError> {
    load_cascade_values_with(directory, mode, Dialect::PORTABLE)
}

/// Load the standard cascade with an explicit dialect, retaining only values.
///
/// # Errors
/// Returns [`LoadError`] for invalid mode names, I/O errors, invalid UTF-8 or
/// syntax/policy errors.
pub fn load_cascade_values_with<P: AsRef<Path>>(
    directory: P,
    mode: &str,
    dialect: Dialect,
) -> Result<LoadedValues, LoadError> {
    if !valid_mode(mode) {
        return Err(LoadError::InvalidMode);
    }
    let directory = directory.as_ref();
    let mut paths = vec![directory.join(".env"), directory.join(".env.local")];
    if !mode.is_empty() {
        paths.push(directory.join(format!(".env.{mode}")));
        paths.push(directory.join(format!(".env.{mode}.local")));
    }
    load_value_paths(paths, dialect, true)
}

/// Load an explicit ordered file list while retaining only final effective
/// values.
///
/// Every listed file must exist, and later files override earlier files by
/// key. The shared parser projects only effective values, so no lossless
/// document is retained. Use [`load_cascade_values`] for optional standard
/// filenames.
///
/// # Errors
/// Returns [`LoadError`] for I/O errors, invalid UTF-8 or syntax/policy errors.
pub fn load_values<I, P>(paths: I, dialect: Dialect) -> Result<LoadedValues, LoadError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    load_value_paths(paths, dialect, false)
}

fn load_value_paths<I, P>(
    paths: I,
    dialect: Dialect,
    skip_missing: bool,
) -> Result<LoadedValues, LoadError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut values: Option<BTreeMap<String, String>> = None;
    for path in paths {
        let path = path.as_ref();
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if skip_missing && error.kind() == io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(LoadError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        let text = String::from_utf8(bytes).map_err(|_| LoadError::InvalidUtf8 {
            path: path.to_path_buf(),
        })?;
        let projected = match crate::parse_values_with(&text, dialect) {
            Ok(values) => values,
            Err(diagnostic) => {
                return Err(LoadError::Invalid {
                    path: path.to_path_buf(),
                    diagnostic,
                });
            }
        };
        if let Some(values) = &mut values {
            values.extend(projected);
        } else {
            values = Some(projected);
        }
    }
    Ok(LoadedValues {
        values: values.unwrap_or_default(),
    })
}

fn valid_mode(mode: &str) -> bool {
    !mode.contains(['/', '\\', '\0']) && mode != "." && mode != ".."
}

fn load_paths<I, P>(
    paths: I,
    options: LoadOptions,
    skip_missing: bool,
) -> Result<LoadedEnvironment, LoadError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut loaded = LoadedEnvironment {
        records: BTreeMap::new(),
        raw_source: String::new(),
        files: Vec::new(),
        dialect: options.dialect,
        entries: Vec::new(),
        overrides: Vec::new(),
        repeated: Vec::new(),
        skipped: Vec::new(),
        raw_tokens: options.retain_raw_tokens,
        origins_retained: options.retain_origins,
    };
    let mut scratch = MergeScratch {
        previous_index: BTreeMap::new(),
    };
    let mut next_order = 0;
    for path in paths {
        let path = path.as_ref();
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if skip_missing && error.kind() == io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(LoadError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        let text = String::from_utf8(bytes).map_err(|_| LoadError::InvalidUtf8 {
            path: path.to_path_buf(),
        })?;
        let (document, diagnostics) = parse_with(&text, options.dialect);
        if let Some(diagnostic) = diagnostics
            .into_iter()
            .find(|diagnostic| diagnostic.severity() == Severity::Error)
        {
            return Err(LoadError::Invalid {
                path: path.to_path_buf(),
                diagnostic,
            });
        }
        let file_index = if options.retain_reports || options.retain_origins {
            let index = loaded.files.len();
            loaded.files.push(path.to_path_buf());
            index
        } else {
            0
        };
        merge_document(
            &mut loaded,
            file_index,
            &document,
            options,
            &mut scratch,
            &mut next_order,
        );
    }
    loaded.raw_source.shrink_to_fit();
    loaded.files.shrink_to_fit();
    Ok(loaded)
}

struct MergeScratch {
    previous_index: BTreeMap<Arc<str>, usize>,
}

struct MergeState<'a> {
    loaded: &'a mut LoadedEnvironment,
    file_index: usize,
    options: LoadOptions,
    previous_index: &'a mut BTreeMap<Arc<str>, usize>,
    next_order: usize,
}

fn merge_document(
    loaded: &mut LoadedEnvironment,
    file_index: usize,
    document: &crate::Document<'_>,
    options: LoadOptions,
    scratch: &mut MergeScratch,
    next_order: &mut usize,
) {
    let mut state = MergeState {
        loaded,
        file_index,
        options,
        previous_index: &mut scratch.previous_index,
        next_order: *next_order,
    };
    if options.retain_reports {
        merge_reported(&mut state, document);
    } else {
        merge_effective_entries(&mut state, document);
    }
    *next_order = state.next_order;
}

fn merge_effective_entries(state: &mut MergeState<'_>, document: &crate::Document<'_>) {
    for effective in document.effective_entries() {
        merge_effective(
            state,
            document,
            Arc::from(effective.key()),
            Arc::from(effective.value()),
            effective.entry(),
        );
    }
}

fn merge_reported(state: &mut MergeState<'_>, document: &crate::Document<'_>) {
    let mut first: HashMap<&str, crate::Entry<'_>> = HashMap::new();
    let mut duplicates: HashMap<&str, Vec<crate::Entry<'_>>> = HashMap::new();
    let mut keys = Vec::new();
    for entry in document.entries() {
        let key = entry.key();
        if let Some(previous) = first.get(key).copied() {
            duplicates
                .entry(key)
                .or_insert_with(|| vec![previous])
                .push(entry);
        } else {
            keys.push(key);
            first.insert(key, entry);
        }
    }

    for key in keys {
        let all: &[crate::Entry<'_>] = if let Some(all) = duplicates.get(key) {
            all
        } else {
            std::slice::from_ref(
                first
                    .get(key)
                    .expect("every key in the occurrence order map has an entry"),
            )
        };
        merge_reported_key(state, document, key, all);
    }
}

fn merge_reported_key(
    state: &mut MergeState<'_>,
    document: &crate::Document<'_>,
    key: &str,
    all: &[crate::Entry<'_>],
) {
    if key.is_empty() {
        for entry in all {
            state.loaded.skipped.push(SkippedRecord {
                key: None,
                origin: compact_origin(state.file_index, *entry),
                reason: SkipReason::EmptyKey,
            });
        }
        return;
    }
    let mut first_participating = None;
    let mut last_participating = None;
    let mut participating_count = 0;
    for entry in all {
        if entry.participates_in_lookup() {
            if first_participating.is_none() {
                first_participating = Some(*entry);
            }
            last_participating = Some(*entry);
            participating_count += 1;
        }
    }
    let key: Arc<str> = Arc::from(key);
    if participating_count > 1 {
        state.loaded.repeated.push(RepeatedRecord {
            key: key.clone(),
            origins: all
                .iter()
                .filter(|entry| entry.participates_in_lookup())
                .map(|entry| compact_origin(state.file_index, *entry))
                .collect(),
        });
    }
    let effective = match document.dialect().duplicates {
        crate::Duplicates::First => first_participating,
        crate::Duplicates::Last => last_participating,
    };
    let Some(effective) = effective else {
        state.loaded.skipped.push(SkippedRecord {
            key: Some(key),
            origin: compact_origin(state.file_index, all[0]),
            reason: SkipReason::NonParticipating,
        });
        return;
    };
    let value = if let Some(value) = effective.value() {
        Arc::from(value)
    } else if document.dialect().bare_keys == crate::BareKeys::Empty {
        Arc::from("")
    } else {
        state.loaded.skipped.push(SkippedRecord {
            key: Some(key),
            origin: compact_origin(state.file_index, effective),
            reason: SkipReason::Bare,
        });
        return;
    };
    merge_effective(state, document, key, value, effective);
}

fn merge_effective(
    state: &mut MergeState<'_>,
    document: &crate::Document<'_>,
    key: Arc<str>,
    value: Arc<str>,
    effective: crate::Entry<'_>,
) {
    let compact_origin = compact_origin(state.file_index, effective);
    let order = state.next_order;
    state.next_order += 1;
    let raw = if state.options.retain_raw_tokens {
        let raw_key = &document.source()[effective.tokens().key.clone()];
        let raw_value = effective.raw_value().unwrap_or_default();
        let key_start = state.loaded.raw_source.len();
        state.loaded.raw_source.push_str(raw_key);
        let key_range = key_start..state.loaded.raw_source.len();
        let value_start = state.loaded.raw_source.len();
        state.loaded.raw_source.push_str(raw_value);
        let value_range = value_start..state.loaded.raw_source.len();
        Some((order, key_range, value_range))
    } else {
        None
    };
    if state.options.retain_reports {
        let entry_index = state.loaded.entries.len();
        let previous = if state.options.retain_origins {
            state.previous_index.insert(key.clone(), entry_index)
        } else {
            None
        };
        state.loaded.entries.push(EntryRecord {
            key: key.clone(),
            value: value.clone(),
            origin: compact_origin.clone(),
        });
        if let Some(previous) = previous {
            let previous_origin = state.loaded.entries[previous].origin.clone();
            state.loaded.overrides.push(OverrideRecord {
                key: key.clone(),
                previous: previous_origin,
                replacement: compact_origin.clone(),
            });
        }
    }
    let origin = if state.options.retain_origins {
        Some(compact_origin)
    } else {
        None
    };
    state
        .loaded
        .records
        .insert(key, ValueRecord { value, origin, raw });
}

fn compact_origin(file: usize, entry: crate::Entry<'_>) -> CompactOrigin {
    CompactOrigin {
        file,
        span: entry.value_span().unwrap_or_else(|| entry.key_span()),
        line: entry.line(),
    }
}
