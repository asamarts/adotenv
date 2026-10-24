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
    sync::OnceLock,
    vec::Vec,
};

use crate::{
    encode_value, parse_strict, parse_strict_with, parse_with, Diagnostic, Dialect, ExpandOptions,
    ExpandScope, ResolveError, Severity,
};

/// A file, source span and one-based line identifying a loaded value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct LoadOrigin {
    /// File path used for the load.
    pub path: PathBuf,
    /// Original value span in that file.
    pub span: Range<usize>,
    /// One-based source line.
    pub line: usize,
}

/// One effective declaration observed during a load.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadedEntry {
    /// Declaration key.
    pub key: String,
    /// Decoded literal value.
    pub value: String,
    /// Where the declaration came from.
    pub origin: LoadOrigin,
}

/// A later file overriding an earlier effective key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadOverride {
    /// Overridden key.
    pub key: String,
    /// Earlier active value.
    pub previous: LoadOrigin,
    /// Later active value.
    pub replacement: LoadOrigin,
}

/// Duplicate declarations for one key within a single file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadRepeated {
    /// Repeated key.
    pub key: String,
    /// All occurrences in source order.
    pub origins: Vec<LoadOrigin>,
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
pub struct LoadSkipped {
    /// Key, if one was available.
    pub key: Option<String>,
    /// File and position.
    pub origin: LoadOrigin,
    /// Why it was skipped.
    pub reason: SkipReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompactEntry {
    key: String,
    origin: LoadOrigin,
    value: Option<String>,
}

/// Fully validated result of a file load.
#[derive(Debug, Clone)]
pub struct LoadedEnvironment {
    values: BTreeMap<String, String>,
    origins: BTreeMap<String, LoadOrigin>,
    raw_entries: BTreeMap<String, (usize, Range<usize>, Range<usize>)>,
    raw_source: String,
    dialect: Dialect,
    entry_records: Vec<CompactEntry>,
    entries_cache: OnceLock<Vec<LoadedEntry>>,
    overrides: Vec<LoadOverride>,
    repeated: Vec<LoadRepeated>,
    skipped: Vec<LoadSkipped>,
    raw_tokens: bool,
    origins_in_entries: bool,
    origin_index: OnceLock<BTreeMap<String, usize>>,
}

impl PartialEq for LoadedEnvironment {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
            && self.origins == other.origins
            && self.raw_entries == other.raw_entries
            && self.dialect == other.dialect
            && self.entry_records == other.entry_records
            && self.overrides == other.overrides
            && self.repeated == other.repeated
            && self.skipped == other.skipped
            && self.raw_tokens == other.raw_tokens
            && self.origins_in_entries == other.origins_in_entries
    }
}

impl Eq for LoadedEnvironment {}

/// Retention policy for the full loader.
///
/// The default retains every report and the raw winning tokens required by
/// [`LoadedEnvironment::resolve_contextual`]. Applications that only need
/// values, or that never need contextual resolution, can opt out of the
/// corresponding allocations. Disabling reports leaves
/// [`LoadedEnvironment::entries`], [`LoadedEnvironment::overrides`],
/// [`LoadedEnvironment::repeated`] and [`LoadedEnvironment::skipped`] empty.
/// Disabling origins omits the final [`LoadedEnvironment::origin`] map and
/// override reports; reports that remain enabled still carry their own origins.
/// Disabling raw tokens makes [`LoadedEnvironment::resolve_contextual`] return
/// an error; decoded-value resolution remains available. Use
/// [`crate::load::load_values`] when no origins or reports are needed at all.
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
    /// Full retention using the selected dialect.
    #[must_use]
    pub const fn new(dialect: Dialect) -> Self {
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
    /// Final effective literal values.
    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
    /// Whether raw winning tokens were retained for contextual resolution.
    #[must_use]
    pub const fn retains_raw_tokens(&self) -> bool {
        self.raw_tokens
    }
    /// Final value for one key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    /// Final origin for one effective key.
    ///
    /// When reports are retained, the origin is derived from the final matching
    /// [`LoadedEntry`] instead of storing a duplicate map. Returns `None` when
    /// loaded with [`LoadOptions::with_origins`] set to `false`.
    #[must_use]
    pub fn origin(&self, key: &str) -> Option<&LoadOrigin> {
        if self.origins_in_entries {
            let index = self.origin_index.get_or_init(|| {
                self.entry_records
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| (entry.key.clone(), index))
                    .collect()
            });
            index
                .get(key)
                .and_then(|index| self.entry_records.get(*index))
                .map(|entry| &entry.origin)
        } else {
            self.origins.get(key)
        }
    }
    fn origins_map(&self) -> BTreeMap<String, LoadOrigin> {
        if self.origins_in_entries {
            self.entry_records
                .iter()
                .map(|entry| (entry.key.clone(), entry.origin.clone()))
                .collect()
        } else {
            self.origins.clone()
        }
    }

    /// Every loaded declaration, including later-overridden entries.
    ///
    /// Empty when loaded with [`LoadOptions::with_reports`] set to `false`.
    /// The owned report is materialized lazily on the first call.
    #[must_use]
    pub fn entries(&self) -> &[LoadedEntry] {
        self.entries_cache.get_or_init(|| {
            self.entry_records
                .iter()
                .map(|entry| LoadedEntry {
                    key: entry.key.clone(),
                    value: entry
                        .value
                        .clone()
                        .or_else(|| self.values.get(&entry.key).cloned())
                        .unwrap_or_default(),
                    origin: entry.origin.clone(),
                })
                .collect()
        })
    }
    /// Cross-file overrides in load order.
    ///
    /// Empty when loaded with [`LoadOptions::with_reports`] set to `false`.
    #[must_use]
    pub fn overrides(&self) -> &[LoadOverride] {
        &self.overrides
    }
    /// Duplicate declarations within individual files.
    ///
    /// Empty when loaded with [`LoadOptions::with_reports`] set to `false`.
    #[must_use]
    pub fn repeated(&self) -> &[LoadRepeated] {
        &self.repeated
    }
    /// Declarations that were inspected but not loaded.
    ///
    /// Empty when loaded with [`LoadOptions::with_reports`] set to `false`.
    #[must_use]
    pub fn skipped(&self) -> &[LoadSkipped] {
        &self.skipped
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

impl LoadedEnvironment {
    /// Drop provenance and reports, retaining only final effective values.
    #[must_use]
    pub fn into_values(self) -> LoadedValues {
        LoadedValues {
            values: self.values,
        }
    }
}

/// A resolved decoded loaded environment.
///
/// Values have been processed by [`LoadedEnvironment::resolve_contextual`] or
/// [`LoadedEnvironment::resolve_decoded`]. Origins identify the winning
/// declaration that supplied the original value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEnvironment {
    values: BTreeMap<String, String>,
    origins: BTreeMap<String, LoadOrigin>,
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
    pub fn origin(&self, key: &str) -> Option<&LoadOrigin> {
        self.origins.get(key)
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
    origin: Option<LoadOrigin>,
    message: String,
}

impl LoadedResolveError {
    fn new(key: Option<String>, origin: Option<LoadOrigin>, message: impl Into<String>) -> Self {
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
    pub fn origin(&self) -> Option<&LoadOrigin> {
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
            write!(formatter, "{}:{}: ", origin.path.display(), origin.line)?;
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
    /// # Errors
    /// Returns [`LoadedResolveError`] for unsupported sequential scope, values
    /// that cannot be encoded, or resolution failures. The error retains the
    /// winning declaration origin when available.
    pub fn resolve_decoded<F>(
        &self,
        lookup: F,
        options: &ExpandOptions,
    ) -> Result<ResolvedEnvironment, LoadedResolveError>
    where
        F: Fn(&str) -> Option<String>,
    {
        if options.scope != ExpandScope::WholeFile {
            return Err(LoadedResolveError::new(
                None,
                None,
                "loaded environments require whole-file resolution scope",
            ));
        }
        let mut source = String::new();
        for (key, value) in &self.values {
            let token = encode_value(value).ok_or_else(|| {
                LoadedResolveError::new(
                    Some(key.clone()),
                    self.origin(key).cloned(),
                    "value cannot be represented exactly",
                )
            })?;
            source.push_str(key);
            source.push('=');
            source.push_str(&token);
            source.push('\n');
        }
        let document = parse_strict(&source).map_err(|error| {
            LoadedResolveError::new(None, None, format!("cannot prepare resolution: {error}"))
        })?;
        let mut options = *options;
        // The synthetic document must treat decoded text as unquoted text. Any
        // quoting introduced by the exact encoder must not change eligibility.
        options.rules.single_quotes = true;
        options.rules.backtick_quotes = true;
        options.rules.escaped_dollar = true;
        let resolved = document.resolve(lookup, &options).map_err(|error| {
            let key = resolve_error_key(&error).to_string();
            let origin = self.origin(&key).cloned();
            LoadedResolveError::new(Some(key), origin, error.to_string())
        })?;
        let mut values = BTreeMap::new();
        for entry in resolved.entries() {
            values.insert(entry.key.clone(), entry.value.clone().unwrap_or_default());
        }
        Ok(ResolvedEnvironment {
            values,
            origins: self.origins_map(),
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
    /// # Errors
    /// Returns [`LoadedResolveError`] if raw tokens were not retained, if the
    /// reconstructed source cannot be parsed, or if resolution fails.
    pub fn resolve_contextual<F>(
        &self,
        lookup: F,
        options: &ExpandOptions,
    ) -> Result<ResolvedEnvironment, LoadedResolveError>
    where
        F: Fn(&str) -> Option<String>,
    {
        if !self.raw_tokens {
            return Err(LoadedResolveError::new(
                None,
                None,
                "raw token context was not retained; load with raw tokens or use resolve_decoded",
            ));
        }
        let mut entries: Vec<_> = self.raw_entries.iter().collect();
        entries.sort_unstable_by_key(|(_, (order, _, _))| *order);
        let mut source = String::new();
        for (_, (_, key_range, value_range)) in entries {
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
            let origin = self.origin(&key).cloned();
            LoadedResolveError::new(Some(key), origin, error.to_string())
        })?;
        let mut values = BTreeMap::new();
        for entry in resolved.entries() {
            values.insert(entry.key.clone(), entry.value.clone().unwrap_or_default());
        }
        Ok(ResolvedEnvironment {
            values,
            origins: self.origins_map(),
        })
    }
}

fn resolve_error_key(error: &ResolveError) -> &str {
    match error {
        ResolveError::Unresolved { key, .. }
        | ResolveError::Required { key, .. }
        | ResolveError::Cycle { key, .. }
        | ResolveError::DepthExceeded { key, .. }
        | ResolveError::ValueTooLarge { key, .. }
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
        let projected = match crate::parser::parse_projected_values(&text, dialect) {
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
        values: BTreeMap::new(),
        origins: BTreeMap::new(),
        raw_entries: BTreeMap::new(),
        raw_source: String::new(),
        dialect: options.dialect,
        entry_records: Vec::new(),
        entries_cache: OnceLock::new(),
        overrides: Vec::new(),
        repeated: Vec::new(),
        skipped: Vec::new(),
        raw_tokens: options.retain_raw_tokens,
        origins_in_entries: options.retain_reports && options.retain_origins,
        origin_index: OnceLock::new(),
    };
    let mut previous_origins = BTreeMap::new();
    let mut previous_index = BTreeMap::new();
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
        merge_document(
            &mut loaded,
            path,
            &document,
            options,
            &mut previous_origins,
            &mut previous_index,
            &mut next_order,
        );
    }
    if !loaded.origins_in_entries && options.retain_origins {
        loaded.origins = previous_origins;
    }
    loaded.raw_source.shrink_to_fit();
    Ok(loaded)
}

struct MergeState<'a> {
    loaded: &'a mut LoadedEnvironment,
    path: &'a Path,
    options: LoadOptions,
    previous_origins: &'a mut BTreeMap<String, LoadOrigin>,
    previous_index: &'a mut BTreeMap<String, usize>,
    next_order: usize,
}

fn merge_document(
    loaded: &mut LoadedEnvironment,
    path: &Path,
    document: &crate::Document<'_>,
    options: LoadOptions,
    previous_origins: &mut BTreeMap<String, LoadOrigin>,
    previous_index: &mut BTreeMap<String, usize>,
    next_order: &mut usize,
) {
    let mut state = MergeState {
        loaded,
        path,
        options,
        previous_origins,
        previous_index,
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
            effective.key().to_string(),
            effective.value().to_string(),
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
            state.loaded.skipped.push(LoadSkipped {
                key: None,
                origin: origin(state.path, *entry),
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
    if participating_count > 1 {
        state.loaded.repeated.push(LoadRepeated {
            key: key.to_string(),
            origins: all
                .iter()
                .filter(|entry| entry.participates_in_lookup())
                .map(|entry| origin(state.path, *entry))
                .collect(),
        });
    }
    let effective = match document.dialect().duplicates {
        crate::Duplicates::First => first_participating,
        crate::Duplicates::Last => last_participating,
    };
    let Some(effective) = effective else {
        state.loaded.skipped.push(LoadSkipped {
            key: Some(key.to_string()),
            origin: origin(state.path, all[0]),
            reason: SkipReason::NonParticipating,
        });
        return;
    };
    let value = if let Some(value) = effective.value() {
        value.to_string()
    } else if document.dialect().bare_keys == crate::BareKeys::Empty {
        String::new()
    } else {
        state.loaded.skipped.push(LoadSkipped {
            key: Some(key.to_string()),
            origin: origin(state.path, effective),
            reason: SkipReason::Bare,
        });
        return;
    };
    merge_effective(state, document, key.to_string(), value, effective);
}

fn merge_effective(
    state: &mut MergeState<'_>,
    document: &crate::Document<'_>,
    key: String,
    value: String,
    effective: crate::Entry<'_>,
) {
    let entry_origin = origin(state.path, effective);
    let order = state.next_order;
    state.next_order += 1;
    if state.options.retain_raw_tokens {
        let raw_key = &document.source()[effective.tokens().key.clone()];
        let raw_value = effective.raw_value().unwrap_or_default();
        let key_start = state.loaded.raw_source.len();
        state.loaded.raw_source.push_str(raw_key);
        let key_range = key_start..state.loaded.raw_source.len();
        let value_start = state.loaded.raw_source.len();
        state.loaded.raw_source.push_str(raw_value);
        let value_range = value_start..state.loaded.raw_source.len();
        state
            .loaded
            .raw_entries
            .insert(key.clone(), (order, key_range, value_range));
    }
    if state.options.retain_reports {
        let entry_index = state.loaded.entry_records.len();
        let previous = if state.options.retain_origins {
            state.previous_index.insert(key.clone(), entry_index)
        } else {
            None
        };
        let previous_value = state.loaded.values.get(&key).cloned();
        state.loaded.entry_records.push(CompactEntry {
            key: key.clone(),
            origin: entry_origin.clone(),
            value: None,
        });
        if let (Some(previous), true) = (previous, state.options.retain_origins) {
            let previous_origin = state.loaded.entry_records[previous].origin.clone();
            if let Some(value) = previous_value {
                state.loaded.entry_records[previous].value = Some(value);
            }
            state.loaded.overrides.push(LoadOverride {
                key: key.clone(),
                previous: previous_origin,
                replacement: entry_origin,
            });
        }
    } else if state.options.retain_origins {
        state.previous_origins.insert(key.clone(), entry_origin);
    }
    state.loaded.values.insert(key, value);
}

fn origin(path: &Path, entry: crate::Entry<'_>) -> LoadOrigin {
    LoadOrigin {
        path: path.to_path_buf(),
        span: entry.value_span().unwrap_or_else(|| entry.key_span()),
        line: entry.line(),
    }
}
