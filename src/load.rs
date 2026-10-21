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
    collections::{BTreeMap, BTreeSet},
    fmt, fs, io,
    ops::Range,
    path::{Path, PathBuf},
    string::String,
    vec::Vec,
};

use crate::{
    encode_value, parse_strict, parse_strict_with, parse_with, Diagnostic, Dialect, ExpandOptions,
    ExpandScope, ResolveError, Severity,
};

/// A file, source span and one-based line identifying a loaded value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
pub struct LoadSkipped {
    /// Key, if one was available.
    pub key: Option<String>,
    /// File and position.
    pub origin: LoadOrigin,
    /// Why it was skipped.
    pub reason: SkipReason,
}

/// Fully validated result of a file load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedEnvironment {
    values: BTreeMap<String, String>,
    origins: BTreeMap<String, LoadOrigin>,
    raw_entries: BTreeMap<String, (usize, String, String)>,
    dialect: Dialect,
    entries: Vec<LoadedEntry>,
    overrides: Vec<LoadOverride>,
    repeated: Vec<LoadRepeated>,
    skipped: Vec<LoadSkipped>,
}

impl LoadedEnvironment {
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
    /// Final origin for one effective key.
    #[must_use]
    pub fn origin(&self, key: &str) -> Option<&LoadOrigin> {
        self.origins.get(key)
    }
    /// Every loaded declaration, including later-overridden entries.
    #[must_use]
    pub fn entries(&self) -> &[LoadedEntry] {
        &self.entries
    }
    /// Cross-file overrides in load order.
    #[must_use]
    pub fn overrides(&self) -> &[LoadOverride] {
        &self.overrides
    }
    /// Duplicate declarations within individual files.
    #[must_use]
    pub fn repeated(&self) -> &[LoadRepeated] {
        &self.repeated
    }
    /// Declarations that were inspected but not loaded.
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

/// A resolved decoded loaded environment.
///
/// Values have been processed by [`LoadedEnvironment::resolve_decoded`]. Origins
/// identify the winning declaration that supplied the original value.
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

/// Failure from [`LoadedEnvironment::resolve_decoded`].
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
            origins: self.origins.clone(),
        })
    }

    /// Resolve references in final values using each winning declaration's
    /// original literal context.
    ///
    /// The winning key/value tokens are reconstructed in cascade order and
    /// parsed with the dialect used at load time. Quote, escape and bare-key
    /// context therefore match the source that won the cascade. The synthetic
    /// source has no original file offsets; errors retain the winning
    /// declaration's [`LoadOrigin`], key and message.
    ///
    /// # Errors
    /// Returns [`LoadedResolveError`] if the reconstructed source cannot be
    /// parsed or resolution fails.
    pub fn resolve_contextual<F>(
        &self,
        lookup: F,
        options: &ExpandOptions,
    ) -> Result<ResolvedEnvironment, LoadedResolveError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let mut entries: Vec<_> = self.raw_entries.iter().collect();
        entries.sort_unstable_by_key(|(_, (order, _, _))| *order);
        let mut source = String::new();
        for (_, (_, raw_key, raw_value)) in entries {
            source.push_str(raw_key);
            source.push('=');
            source.push_str(raw_value);
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
            origins: self.origins.clone(),
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
    if !valid_mode(mode) {
        return Err(LoadError::InvalidMode);
    }
    let directory = directory.as_ref();
    let mut paths = vec![directory.join(".env"), directory.join(".env.local")];
    if !mode.is_empty() {
        paths.push(directory.join(format!(".env.{mode}")));
        paths.push(directory.join(format!(".env.{mode}.local")));
    }
    load_paths(paths, dialect, true)
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
    load_paths(paths, dialect, false)
}

fn valid_mode(mode: &str) -> bool {
    !mode.contains(['/', '\\', '\0']) && mode != "." && mode != ".."
}

fn load_paths<I, P>(
    paths: I,
    dialect: Dialect,
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
        dialect,
        entries: Vec::new(),
        overrides: Vec::new(),
        repeated: Vec::new(),
        skipped: Vec::new(),
    };
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
        let (document, diagnostics) = parse_with(&text, dialect);
        if let Some(diagnostic) = diagnostics
            .into_iter()
            .find(|diagnostic| diagnostic.severity() == Severity::Error)
        {
            return Err(LoadError::Invalid {
                path: path.to_path_buf(),
                diagnostic,
            });
        }
        merge_document(&mut loaded, path, &document);
    }
    Ok(loaded)
}

fn merge_document(loaded: &mut LoadedEnvironment, path: &Path, document: &crate::Document<'_>) {
    let mut seen = BTreeSet::new();
    for entry in document.entries() {
        let key = entry.key().to_string();
        if key.is_empty() {
            loaded.skipped.push(LoadSkipped {
                key: None,
                origin: origin(path, entry),
                reason: SkipReason::EmptyKey,
            });
            continue;
        }
        if !seen.insert(key.clone()) {
            continue;
        }
        let occurrences: Vec<_> = document.get_all(&key).collect();
        if occurrences.len() > 1 {
            loaded.repeated.push(LoadRepeated {
                key: key.clone(),
                origins: occurrences
                    .iter()
                    .map(|entry| origin(path, *entry))
                    .collect(),
            });
        }
        let Some(effective) = document.get_entry(&key) else {
            loaded.skipped.push(LoadSkipped {
                key: Some(key),
                origin: origin(path, entry),
                reason: SkipReason::NonParticipating,
            });
            continue;
        };
        if !effective.participates_in_lookup() {
            loaded.skipped.push(LoadSkipped {
                key: Some(key),
                origin: origin(path, effective),
                reason: SkipReason::NonParticipating,
            });
            continue;
        }
        let value = if let Some(value) = effective.value() {
            value.to_string()
        } else if document.dialect().bare_keys == crate::BareKeys::Empty {
            String::new()
        } else {
            loaded.skipped.push(LoadSkipped {
                key: Some(key),
                origin: origin(path, effective),
                reason: SkipReason::Bare,
            });
            continue;
        };
        let entry_origin = origin(path, effective);
        let order = loaded.entries.len();
        let raw_key = document.source()[effective.tokens().key.clone()].to_string();
        let raw_value = effective.raw_value().unwrap_or_default().to_string();
        loaded
            .raw_entries
            .insert(key.clone(), (order, raw_key, raw_value));
        loaded.entries.push(LoadedEntry {
            key: key.clone(),
            value: value.clone(),
            origin: entry_origin.clone(),
        });
        if let Some(previous) = loaded.origins.insert(key.clone(), entry_origin.clone()) {
            loaded.overrides.push(LoadOverride {
                key: key.clone(),
                previous,
                replacement: entry_origin,
            });
        }
        loaded.values.insert(key, value);
    }
}

fn origin(path: &Path, entry: crate::Entry<'_>) -> LoadOrigin {
    LoadOrigin {
        path: path.to_path_buf(),
        span: entry.value_span().unwrap_or_else(|| entry.key_span()),
        line: entry.line(),
    }
}
