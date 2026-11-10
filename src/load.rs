//! Reads `.env` files from disk and merges them into one set of values.
//!
//! This module needs the `load` feature:
//!
//! ```toml
//! adotenv = { version = "1", features = ["load"] }
//! ```
//!
//! Loading only reads files. It never changes the process environment, never
//! runs commands, and expands `${VAR}` references only when you call a resolve
//! method. To give the values to a child process, pass
//! [`LoadedEnvironment::values`] to [`Command::envs`](std::process::Command::envs).
//! To set them in the current process, use the
//! [`adotenv-process`](https://docs.rs/adotenv-process) crate.
//!
//! # Cascades
//!
//! [`load_cascade`] reads these files from one directory, in this order:
//!
//! 1. `.env`
//! 2. `.env.local`
//! 3. `.env.{mode}`
//! 4. `.env.{mode}.local`
//!
//! Every file is parsed with the same dialect. Within one file, the dialect's
//! duplicate-key and bare-key rules choose the effective value of each key.
//! Across files, a later file's effective value replaces an earlier one, key by
//! key. When the declaration that wins in a file is a bare key, the Portable
//! dialect gives the key no value there, so the value from earlier files stays:
//! with `PORT=3000` in `.env`, and `PORT=8080` followed by a bare `PORT` in
//! `.env.local`, the result is `3000`.
//!
//! With `PORT=3000` and `DATABASE_URL=postgres://localhost/dev` in `.env`, and
//! `PORT=8080` in `.env.production`:
//!
//! ```
//! use adotenv::load::load_cascade;
//!
//! # let dir = std::env::temp_dir().join(format!("adotenv-doc-load-{}", std::process::id()));
//! # std::fs::create_dir_all(&dir)?;
//! # std::fs::write(dir.join(".env"), "PORT=3000\nDATABASE_URL=postgres://localhost/dev\n")?;
//! # std::fs::write(dir.join(".env.production"), "PORT=8080\n")?;
//! let loaded = load_cascade(&dir, "production")?;
//! assert_eq!(loaded.get("PORT"), Some("8080"));
//! assert_eq!(loaded.get("DATABASE_URL"), Some("postgres://localhost/dev"));
//!
//! let origin = loaded.origin("PORT").unwrap();
//! assert_eq!(origin.path(), dir.join(".env.production"));
//! assert_eq!(origin.line(), 1);
//! # std::fs::remove_dir_all(&dir)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## Missing files and failures
//!
//! A cascade file that does not exist is skipped, so a directory with none of
//! the files, or a directory that does not exist, loads an empty environment.
//! A file that exists but cannot be read, is not valid UTF-8 or has an
//! error-severity [`Diagnostic`] fails the whole load: the function returns a
//! [`LoadError`] and no values. Warnings never fail a load.
//!
//! The functions that take an explicit file list, such as [`load_files`], read
//! the files in the order given, and a file that does not exist is an error.
//!
//! ## Mode names
//!
//! The mode names the last two cascade files, for example `production` or
//! `test`. It must not contain `/`, `\` or NUL and must not be exactly `.` or
//! `..`, so it cannot name a file outside the directory. Such a mode returns
//! [`LoadError::InvalidMode`] before any file is read. Other dots are allowed:
//! mode `prod.eu` reads `.env.prod.eu` and `.env.prod.eu.local`. An empty mode
//! reads only `.env` and `.env.local`.
//!
//! # Entry points
//!
//! | Function | Reads | Returns |
//! |---|---|---|
//! | [`load_cascade`] | Cascade, Portable dialect | [`LoadedEnvironment`] |
//! | [`load_cascade_with`] | Cascade, chosen dialect | [`LoadedEnvironment`] |
//! | [`load_cascade_with_options`] | Cascade, chosen [`LoadOptions`] | [`LoadedEnvironment`] |
//! | [`load_files`] | File list, chosen dialect | [`LoadedEnvironment`] |
//! | [`load_files_with_options`] | File list, chosen [`LoadOptions`] | [`LoadedEnvironment`] |
//! | [`load_cascade_values`] | Cascade, Portable dialect | [`LoadedValues`] |
//! | [`load_cascade_values_with`] | Cascade, chosen dialect | [`LoadedValues`] |
//! | [`load_values`] | File list, chosen dialect | [`LoadedValues`] |
//!
//! A [`LoadedEnvironment`] holds the final values sorted by key and, by
//! default, the [`LoadOrigin`] of each one: its file, the byte span of its
//! value and its one-based line. With the `serde` feature,
//! `LoadedEnvironment::deserialize` turns the values into a typed struct.
//!
//! # Options and reports
//!
//! [`LoadOptions`] chooses the dialect and what a [`LoadedEnvironment`] keeps.
//! [`LoadOptions::new`], which [`load_cascade`], [`load_cascade_with`] and
//! [`load_files`] use, keeps the values, their origins and the raw source text
//! that [`LoadedEnvironment::resolve_contextual`] needs. [`LoadOptions::full`]
//! also keeps four reports that explain how the values were chosen:
//!
//! - [`LoadedEnvironment::entries`]: the effective declaration of each key in
//!   each file, including the ones a later file replaced ([`LoadedEntry`]).
//! - [`LoadedEnvironment::overrides`]: each time a later file replaced a key
//!   ([`LoadOverride`]).
//! - [`LoadedEnvironment::repeated`]: keys declared more than once in one file
//!   ([`LoadRepeated`]).
//! - [`LoadedEnvironment::skipped`]: declarations that supplied no value, with
//!   a [`SkipReason`] ([`LoadSkipped`]).
//!
//! # Resolution
//!
//! Loaded values are literal: `${HOST}` is the text `${HOST}`. Two methods
//! resolve references across the merged values and return a
//! [`ResolvedEnvironment`]:
//!
//! - [`LoadedEnvironment::resolve_contextual`] parses the raw source text of
//!   each winning declaration again with the load dialect, so quotes and
//!   escapes keep their meaning: a single-quoted `'${HOST}'` stays literal.
//!   Prefer it unless the raw text was dropped with
//!   [`LoadOptions::with_raw_tokens`].
//! - [`LoadedEnvironment::resolve_decoded`] treats every final value as
//!   unquoted text, so a reference resolves wherever it appears in the value,
//!   even if the file quoted it.
//!
//! Both use whole-file scope only: a reference sees the final value of each
//! key, whichever file set it. Every [`ExpandOptions`] preset uses sequential
//! scope, so select [`ExpandScope::WholeFile`] explicitly; any other scope is
//! an error. Names are also looked up with the `lookup` closure you pass; with
//! the presets, the loaded values win over it. The process environment is read
//! only if that closure reads it. Command substitutions such as `$(whoami)`
//! stay literal. To run them, parse the file yourself and call
//! [`Document::resolve_with_commands`](crate::Document::resolve_with_commands).
//!
//! ```
//! use adotenv::load::load_cascade;
//! use adotenv::{ExpandOptions, ExpandScope};
//!
//! # let dir = std::env::temp_dir().join(format!("adotenv-doc-load-resolve-{}", std::process::id()));
//! # std::fs::create_dir_all(&dir)?;
//! # std::fs::write(dir.join(".env"), "DB_HOST=localhost\nDATABASE_URL=postgres://${DB_HOST}/app\n")?;
//! # std::fs::write(dir.join(".env.production"), "DB_HOST=db.internal\n")?;
//! let loaded = load_cascade(&dir, "production")?;
//! assert_eq!(loaded.get("DATABASE_URL"), Some("postgres://${DB_HOST}/app"));
//!
//! let options = ExpandOptions::portable().with_scope(ExpandScope::WholeFile);
//! let resolved = loaded.resolve_contextual(|_| None, &options)?;
//! assert_eq!(resolved.get("DATABASE_URL"), Some("postgres://db.internal/app"));
//! # std::fs::remove_dir_all(&dir)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Values only
//!
//! [`load_cascade_values`], [`load_cascade_values_with`] and [`load_values`]
//! read the same files with the same rules and errors, but return a
//! [`LoadedValues`] that keeps only the final strings: no origins, reports or
//! raw text, and no resolution. Use them when an application needs only the
//! values; they use less memory.
//!
//! # Errors
//!
//! Loading returns a [`LoadError`]:
//!
//! - [`LoadError::Io`]: a file exists but cannot be read, or a file in an
//!   explicit list does not exist.
//! - [`LoadError::InvalidUtf8`]: a file is not valid UTF-8.
//! - [`LoadError::Invalid`]: a file has an error under the dialect; the error
//!   carries the first error-severity [`Diagnostic`].
//! - [`LoadError::Limit`]: a configured read or parse limit was exceeded.
//! - [`LoadError::InvalidMode`]: the mode name is not allowed.
//!
//! Resolution returns a [`LoadedResolveError`], which names the key and the
//! origin of its winning declaration when they are known.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt, fs,
    io::{self, Read},
    ops::Range,
    path::{Path, PathBuf},
    string::String,
    sync::Arc,
    vec::Vec,
};

use crate::{
    encode_value, parse_strict_with, parse_values_with_options, parse_with_options, Diagnostic,
    Dialect, ExpandOptions, ExpandScope, KeySyntax, LimitExceeded, LineBreaks, ParseOptions,
    ResolveError, Severity,
};

/// Where a loaded declaration is: its file, byte span and one-based line.
///
/// Returned by [`LoadedEnvironment::origin`], [`ResolvedEnvironment::origin`]
/// and the report views, such as [`LoadedEntry::origin`]. It borrows the
/// environment that returned it. [`LoadOriginOwned`] is the owned form that
/// [`LoadedResolveError`] carries.
///
/// The span is a byte range into the file's text. It covers the value token as
/// written, including any quotes. For a bare key, such as one that
/// [`BareKeys::Empty`](crate::BareKeys::Empty) reads as an empty value, it
/// covers the key name. The line is the one-based line of the declaration's
/// key.
///
/// See [`LoadedEnvironment::origin`] for an example.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct LoadOrigin<'a> {
    path: &'a Path,
    span: Range<usize>,
    line: usize,
}

impl<'a> LoadOrigin<'a> {
    /// Returns the path of the file, as the load built or received it.
    ///
    /// For a cascade this is the directory joined with the file name, for an
    /// explicit list the path as given. It is not canonicalized.
    #[must_use]
    pub const fn path(&self) -> &'a Path {
        self.path
    }
    /// Returns the byte range of the value token in the file, or of the key
    /// for a bare key.
    #[must_use]
    pub const fn span(&self) -> &Range<usize> {
        &self.span
    }
    /// Returns the one-based line of the declaration's key.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }
}

/// The effective declaration of one key in one file, from
/// [`LoadedEnvironment::entries`].
///
/// Unlike [`LoadedEnvironment::get`], the entries also include values that a
/// later file replaced. See [`LoadedEnvironment::entries`] for an example.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadedEntry<'a> {
    key: &'a str,
    value: &'a str,
    origin: LoadOrigin<'a>,
}

impl<'a> LoadedEntry<'a> {
    /// Returns the decoded key.
    #[must_use]
    pub const fn key(&self) -> &'a str {
        self.key
    }
    /// Returns the decoded value, without any reference expanded.
    #[must_use]
    pub const fn value(&self) -> &'a str {
        self.value
    }
    /// Returns where the declaration is.
    #[must_use]
    pub const fn origin(&self) -> &LoadOrigin<'a> {
        &self.origin
    }
}

/// A later file replacing the value of a key, from
/// [`LoadedEnvironment::overrides`].
///
/// When three files set the same key, there are two overrides: the second file
/// replaces the first, then the third replaces the second. See
/// [`LoadedEnvironment::overrides`] for an example.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadOverride<'a> {
    key: &'a str,
    previous: LoadOrigin<'a>,
    replacement: LoadOrigin<'a>,
}

impl<'a> LoadOverride<'a> {
    /// Returns the key whose value was replaced.
    #[must_use]
    pub const fn key(&self) -> &'a str {
        self.key
    }
    /// Returns the origin of the value that was replaced.
    #[must_use]
    pub const fn previous(&self) -> &LoadOrigin<'a> {
        &self.previous
    }
    /// Returns the origin of the value that replaced it.
    #[must_use]
    pub const fn replacement(&self) -> &LoadOrigin<'a> {
        &self.replacement
    }
}

/// A key declared more than once in one file, from
/// [`LoadedEnvironment::repeated`].
///
/// Only declarations that take part in lookup count, and the key is reported
/// only when at least two of them do. The dialect's duplicate-key rule chooses
/// which one supplies the value: the last one under the Portable dialect. See
/// [`LoadedEnvironment::repeated`] for an example.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadRepeated<'a> {
    key: &'a str,
    origins: &'a [CompactOrigin],
    files: &'a [PathBuf],
}

impl<'a> LoadRepeated<'a> {
    /// Returns the repeated key.
    #[must_use]
    pub const fn key(&self) -> &'a str {
        self.key
    }
    /// Returns the origins of the declarations that take part in lookup, in
    /// source order.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn origins(&self) -> impl Iterator<Item = LoadOrigin<'a>> + '_ {
        self.origins
            .iter()
            .map(|origin| view_origin(self.files, origin))
    }
}

/// Why a declaration supplied no value, from [`LoadSkipped::reason`].
///
/// See [`LoadedEnvironment::skipped`] for an example.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SkipReason {
    /// The declaration is a bare key, which has no value under the dialect.
    ///
    /// Under [`BareKeys::Empty`](crate::BareKeys::Empty) a bare key loads as
    /// an empty value instead and is not skipped.
    Bare,
    /// The dialect excludes every declaration of this key from lookup.
    ///
    /// Examples are a bare key under [`BareKeys::Ignore`](crate::BareKeys::Ignore)
    /// and `__proto__` under [`Dialect::NODE_DOTENV`].
    NonParticipating,
    /// The key is empty, which some dialects, such as [`Dialect::COMPOSE`],
    /// accept without an error.
    EmptyKey,
}

/// A declaration that supplied no value, from [`LoadedEnvironment::skipped`].
///
/// See [`LoadedEnvironment::skipped`] for an example.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadSkipped<'a> {
    key: Option<&'a str>,
    origin: LoadOrigin<'a>,
    reason: SkipReason,
}

impl<'a> LoadSkipped<'a> {
    /// Returns the key, or `None` for an empty key.
    #[must_use]
    pub const fn key(&self) -> Option<&'a str> {
        self.key
    }
    /// Returns where the declaration is.
    ///
    /// For [`SkipReason::NonParticipating`] this is the first declaration of
    /// the key in the file. Each empty key is reported on its own.
    #[must_use]
    pub const fn origin(&self) -> &LoadOrigin<'a> {
        &self.origin
    }
    /// Returns why the declaration supplied no value.
    #[must_use]
    pub const fn reason(&self) -> SkipReason {
        self.reason
    }
}

/// An owned [`LoadOrigin`], carried by [`LoadedResolveError::origin`].
///
/// It has the same path, span and line as the [`LoadOrigin`] of the key's
/// winning declaration, but does not borrow the environment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct LoadOriginOwned {
    path: PathBuf,
    span: Range<usize>,
    line: usize,
}

impl LoadOriginOwned {
    /// Returns the path of the file, as the load built or received it.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Returns the byte range of the value token in the file, or of the key
    /// for a bare key.
    #[must_use]
    pub const fn span(&self) -> &Range<usize> {
        &self.span
    }
    /// Returns the one-based line of the declaration's key.
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

/// The merged values of a load, with their origins and optional reports.
///
/// Returned by [`load_cascade`], [`load_files`] and their variants. Every file
/// was read and checked before it was built, so it always describes a complete
/// load. The values are literal and sorted by key.
///
/// What else it keeps depends on the [`LoadOptions`]. By default it keeps the
/// [`LoadOrigin`] of each value and the raw source text that
/// [`resolve_contextual`](Self::resolve_contextual) needs. With
/// [`LoadOptions::full`] it also keeps the reports returned by
/// [`entries`](Self::entries), [`overrides`](Self::overrides),
/// [`repeated`](Self::repeated) and [`skipped`](Self::skipped). The views it
/// returns borrow it. [`into_values`](Self::into_values) keeps only the values.
///
/// # Examples
///
/// With `PORT=3000` and `DATABASE_URL=postgres://localhost/dev` in `.env`, and
/// `PORT=8080` in `.env.production`:
///
/// ```
/// use adotenv::load::load_cascade;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedEnvironment-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "PORT=3000\nDATABASE_URL=postgres://localhost/dev\n")?;
/// # std::fs::write(dir.join(".env.production"), "PORT=8080\n")?;
/// let loaded = load_cascade(&dir, "production")?;
/// assert_eq!(loaded.len(), 2);
/// assert_eq!(loaded.get("PORT"), Some("8080"));
/// assert_eq!(loaded.get("DEBUG"), None);
///
/// let values: Vec<_> = loaded.values().collect();
/// assert_eq!(values, [("DATABASE_URL", "postgres://localhost/dev"), ("PORT", "8080")]);
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
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

/// The dialect of a load and what the [`LoadedEnvironment`] keeps.
///
/// Start from [`LoadOptions::new`], which keeps values, origins and raw source
/// text but no reports, or from [`LoadOptions::full`], which keeps everything.
/// Then change single parts with the `with_*` methods.
/// [`LoadOptions::default`] is `LoadOptions::new(Dialect::PORTABLE)`.
///
/// | Part | `new` | `full` | Needed for |
/// |---|---|---|---|
/// | Origins | Kept | Kept | [`LoadedEnvironment::origin`], [`LoadedEnvironment::overrides`], resolution and Serde error locations |
/// | Raw source text | Kept | Kept | [`LoadedEnvironment::resolve_contextual`] |
/// | Reports | Not kept | Kept | [`LoadedEnvironment::entries`], [`overrides`](LoadedEnvironment::overrides), [`repeated`](LoadedEnvironment::repeated), [`skipped`](LoadedEnvironment::skipped) |
///
/// Keeping less uses less memory. An application that needs only the values
/// can use [`load_cascade_values`] or [`load_values`] instead.
///
/// # Examples
///
/// ```
/// use adotenv::load::{load_cascade_with_options, LoadOptions};
/// use adotenv::Dialect;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadOptions-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "PORT=3000\n")?;
/// # std::fs::write(dir.join(".env.local"), "PORT=8080\n")?;
/// let options = LoadOptions::default()
///     .with_dialect(Dialect::NODE_DOTENV)
///     .with_reports(true)
///     .with_raw_tokens(false);
/// let loaded = load_cascade_with_options(&dir, "", options)?;
/// assert_eq!(loaded.get("PORT"), Some("8080"));
/// assert_eq!(loaded.overrides().count(), 1);
/// assert!(!loaded.retains_raw_tokens());
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadOptions {
    /// The dialect used to parse every file.
    pub dialect: Dialect,
    /// Whether to keep the raw source text of each winning declaration, which
    /// [`LoadedEnvironment::resolve_contextual`] needs.
    pub retain_raw_tokens: bool,
    /// Whether to keep the reports: [`LoadedEnvironment::entries`],
    /// [`overrides`](LoadedEnvironment::overrides),
    /// [`repeated`](LoadedEnvironment::repeated) and
    /// [`skipped`](LoadedEnvironment::skipped).
    pub retain_reports: bool,
    /// Whether to keep the origin of each final value, and with reports the
    /// [`overrides`](LoadedEnvironment::overrides) report.
    pub retain_origins: bool,
    /// Optional parse and read limits for each file.
    pub limits: ParseOptions,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self::new(Dialect::PORTABLE)
    }
}

impl LoadOptions {
    /// Returns options that keep values, origins and raw source text, but no
    /// reports.
    ///
    /// [`load_cascade`], [`load_cascade_with`] and [`load_files`] use these
    /// options.
    #[must_use]
    pub const fn new(dialect: Dialect) -> Self {
        Self {
            dialect,
            retain_raw_tokens: true,
            retain_reports: false,
            retain_origins: true,
            limits: ParseOptions::new(),
        }
    }
    /// Returns options that keep values, origins, raw source text and every
    /// report.
    #[must_use]
    pub const fn full(dialect: Dialect) -> Self {
        Self {
            dialect,
            retain_raw_tokens: true,
            retain_reports: true,
            retain_origins: true,
            limits: ParseOptions::new(),
        }
    }
    /// Sets the dialect used to parse every file.
    #[must_use]
    pub const fn with_dialect(mut self, dialect: Dialect) -> Self {
        self.dialect = dialect;
        self
    }
    /// Sets whether to keep the raw source text of each winning declaration.
    ///
    /// Without it, [`LoadedEnvironment::resolve_contextual`] returns an error;
    /// [`LoadedEnvironment::resolve_decoded`] still works.
    #[must_use]
    pub const fn with_raw_tokens(mut self, retain: bool) -> Self {
        self.retain_raw_tokens = retain;
        self
    }
    /// Sets whether to keep the reports.
    ///
    /// Without them, [`LoadedEnvironment::entries`],
    /// [`overrides`](LoadedEnvironment::overrides),
    /// [`repeated`](LoadedEnvironment::repeated) and
    /// [`skipped`](LoadedEnvironment::skipped) yield nothing.
    #[must_use]
    pub const fn with_reports(mut self, retain: bool) -> Self {
        self.retain_reports = retain;
        self
    }
    /// Sets whether to keep the origin of each final value.
    ///
    /// Without origins, [`LoadedEnvironment::origin`] and
    /// [`ResolvedEnvironment::origin`] return `None`,
    /// [`LoadedEnvironment::overrides`] yields nothing, a
    /// [`LoadedResolveError`] has no origin, and a Serde error from
    /// `LoadedEnvironment::deserialize` reports line `0`. The
    /// [`entries`](LoadedEnvironment::entries),
    /// [`repeated`](LoadedEnvironment::repeated) and
    /// [`skipped`](LoadedEnvironment::skipped) reports keep their origins.
    #[must_use]
    pub const fn with_origins(mut self, retain: bool) -> Self {
        self.retain_origins = retain;
        self
    }
    /// Sets optional parse and read limits for every file.
    ///
    /// [`ParseOptions::max_input_bytes`](crate::ParseOptions::max_input_bytes)
    /// also bounds the number of bytes read from a file. A failure is
    /// [`LoadError::Limit`].
    #[must_use]
    pub const fn with_limits(mut self, limits: ParseOptions) -> Self {
        self.limits = limits;
        self
    }
}

impl LoadedEnvironment {
    /// Returns the final values as `(key, value)` pairs, sorted by key.
    ///
    /// The pairs fit [`Command::envs`](std::process::Command::envs), which
    /// passes them to a child process without changing the current one.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::process::Command;
    ///
    /// use adotenv::load::load_cascade;
    ///
    /// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedEnvironment-values-{}", std::process::id()));
    /// # std::fs::create_dir_all(&dir)?;
    /// # std::fs::write(dir.join(".env"), "PORT=8080\nHOST=0.0.0.0\n")?;
    /// let loaded = load_cascade(&dir, "")?;
    /// assert_eq!(loaded.values().collect::<Vec<_>>(), [("HOST", "0.0.0.0"), ("PORT", "8080")]);
    ///
    /// let mut server = Command::new("my-server");
    /// server.envs(loaded.values());
    /// # assert_eq!(server.get_envs().count(), 2);
    /// # std::fs::remove_dir_all(&dir)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn values(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.records
            .iter()
            .map(|(key, record)| (key.as_ref(), record.value.as_ref()))
    }
    /// Returns `true` if the raw source text of each winning declaration was
    /// kept.
    ///
    /// [`resolve_contextual`](Self::resolve_contextual) needs it. It is kept
    /// unless the load used [`LoadOptions::with_raw_tokens`] with `false`.
    #[must_use]
    pub const fn retains_raw_tokens(&self) -> bool {
        self.raw_tokens
    }
    /// Returns the final value of `key`, or `None` if no file set it.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.records.get(key).map(|record| record.value.as_ref())
    }
    /// Returns where the final value of `key` was declared.
    ///
    /// Returns `None` if no file set `key`, or if the load used
    /// [`LoadOptions::with_origins`] with `false`.
    ///
    /// # Examples
    ///
    /// With a comment on line 1 and `DATABASE_URL="postgres://localhost/app"` on
    /// line 2 of `.env`:
    ///
    /// ```
    /// use adotenv::load::load_cascade;
    ///
    /// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedEnvironment-origin-{}", std::process::id()));
    /// # std::fs::create_dir_all(&dir)?;
    /// # std::fs::write(dir.join(".env"), "# database\nDATABASE_URL=\"postgres://localhost/app\"\n")?;
    /// let loaded = load_cascade(&dir, "")?;
    /// let origin = loaded.origin("DATABASE_URL").unwrap();
    /// assert_eq!(origin.path(), dir.join(".env"));
    /// assert_eq!(origin.line(), 2);
    ///
    /// let source = std::fs::read_to_string(origin.path())?;
    /// assert_eq!(&source[origin.span().clone()], "\"postgres://localhost/app\"");
    /// # std::fs::remove_dir_all(&dir)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
    /// Returns the effective declaration of each key in each file, including
    /// the ones a later file replaced.
    ///
    /// Entries come file by file, in load order. Within a file, each key
    /// appears once, at the position of its first declaration, with its
    /// effective value. A key that supplied no value is in
    /// [`skipped`](Self::skipped) instead. Yields nothing unless the load kept
    /// reports, for example with [`LoadOptions::full`].
    ///
    /// # Examples
    ///
    /// With `PORT=3000` and `HOST=localhost` in `.env`, and `PORT=8080` in
    /// `.env.local`:
    ///
    /// ```
    /// use adotenv::load::{load_cascade_with_options, LoadOptions};
    /// use adotenv::Dialect;
    ///
    /// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedEnvironment-entries-{}", std::process::id()));
    /// # std::fs::create_dir_all(&dir)?;
    /// # std::fs::write(dir.join(".env"), "PORT=3000\nHOST=localhost\n")?;
    /// # std::fs::write(dir.join(".env.local"), "PORT=8080\n")?;
    /// let loaded = load_cascade_with_options(&dir, "", LoadOptions::full(Dialect::PORTABLE))?;
    /// let entries: Vec<_> = loaded.entries().map(|entry| (entry.key(), entry.value())).collect();
    /// assert_eq!(entries, [("PORT", "3000"), ("HOST", "localhost"), ("PORT", "8080")]);
    /// assert_eq!(loaded.get("PORT"), Some("8080"));
    /// # std::fs::remove_dir_all(&dir)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn entries(&self) -> impl Iterator<Item = LoadedEntry<'_>> + '_ {
        self.entries.iter().map(|entry| LoadedEntry {
            key: entry.key.as_ref(),
            value: entry.value.as_ref(),
            origin: view_origin(&self.files, &entry.origin),
        })
    }
    /// Returns each replacement of a key's value by a later file, in load
    /// order.
    ///
    /// Yields nothing unless the load kept both reports and origins, as
    /// [`LoadOptions::full`] does.
    ///
    /// # Examples
    ///
    /// With `PORT=3000` in `.env` and `PORT=8080` in `.env.local`:
    ///
    /// ```
    /// use adotenv::load::{load_cascade_with_options, LoadOptions};
    /// use adotenv::Dialect;
    ///
    /// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedEnvironment-overrides-{}", std::process::id()));
    /// # std::fs::create_dir_all(&dir)?;
    /// # std::fs::write(dir.join(".env"), "PORT=3000\n")?;
    /// # std::fs::write(dir.join(".env.local"), "PORT=8080\n")?;
    /// let loaded = load_cascade_with_options(&dir, "", LoadOptions::full(Dialect::PORTABLE))?;
    /// let change = loaded.overrides().next().unwrap();
    /// assert_eq!(change.key(), "PORT");
    /// assert_eq!(change.previous().path(), dir.join(".env"));
    /// assert_eq!(change.replacement().path(), dir.join(".env.local"));
    /// # std::fs::remove_dir_all(&dir)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn overrides(&self) -> impl Iterator<Item = LoadOverride<'_>> + '_ {
        self.overrides.iter().map(|override_report| LoadOverride {
            key: override_report.key.as_ref(),
            previous: view_origin(&self.files, &override_report.previous),
            replacement: view_origin(&self.files, &override_report.replacement),
        })
    }
    /// Returns the keys declared more than once within one file.
    ///
    /// Each [`LoadRepeated`] covers one key in one file. Yields nothing unless
    /// the load kept reports, for example with [`LoadOptions::full`].
    ///
    /// # Examples
    ///
    /// With `PORT=3000` on line 1 and `PORT=8080` on line 2 of `.env`:
    ///
    /// ```
    /// use adotenv::load::{load_cascade_with_options, LoadOptions};
    /// use adotenv::Dialect;
    ///
    /// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedEnvironment-repeated-{}", std::process::id()));
    /// # std::fs::create_dir_all(&dir)?;
    /// # std::fs::write(dir.join(".env"), "PORT=3000\nPORT=8080\n")?;
    /// let loaded = load_cascade_with_options(&dir, "", LoadOptions::full(Dialect::PORTABLE))?;
    /// let repeated = loaded.repeated().next().unwrap();
    /// assert_eq!(repeated.key(), "PORT");
    /// assert_eq!(repeated.origins().map(|origin| origin.line()).collect::<Vec<_>>(), [1, 2]);
    /// assert_eq!(loaded.get("PORT"), Some("8080"));
    /// # std::fs::remove_dir_all(&dir)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn repeated(&self) -> impl Iterator<Item = LoadRepeated<'_>> + '_ {
        self.repeated.iter().map(|repeated| LoadRepeated {
            key: repeated.key.as_ref(),
            origins: &repeated.origins,
            files: &self.files,
        })
    }
    /// Returns the declarations that supplied no value, in load order.
    ///
    /// Each [`LoadSkipped`] says why with a [`SkipReason`]. Yields nothing
    /// unless the load kept reports, for example with [`LoadOptions::full`].
    ///
    /// # Examples
    ///
    /// With a bare `DEBUG` and `PORT=8080` in `.env`:
    ///
    /// ```
    /// use adotenv::load::{load_cascade_with_options, LoadOptions, SkipReason};
    /// use adotenv::Dialect;
    ///
    /// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedEnvironment-skipped-{}", std::process::id()));
    /// # std::fs::create_dir_all(&dir)?;
    /// # std::fs::write(dir.join(".env"), "DEBUG\nPORT=8080\n")?;
    /// let loaded = load_cascade_with_options(&dir, "", LoadOptions::full(Dialect::PORTABLE))?;
    /// let skipped = loaded.skipped().next().unwrap();
    /// assert_eq!(skipped.key(), Some("DEBUG"));
    /// assert_eq!(skipped.reason(), SkipReason::Bare);
    /// assert_eq!(loaded.get("DEBUG"), None);
    /// # std::fs::remove_dir_all(&dir)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn skipped(&self) -> impl Iterator<Item = LoadSkipped<'_>> + '_ {
        self.skipped.iter().map(|skipped| LoadSkipped {
            key: skipped.key.as_deref(),
            origin: view_origin(&self.files, &skipped.origin),
            reason: skipped.reason,
        })
    }
    /// Returns the number of final values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }
    /// Returns `true` if no file supplied a value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    /// Converts the environment into a [`LoadedValues`] with only the final
    /// values.
    ///
    /// Origins, reports and raw source text are dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::load::load_cascade;
    ///
    /// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedEnvironment-into_values-{}", std::process::id()));
    /// # std::fs::create_dir_all(&dir)?;
    /// # std::fs::write(dir.join(".env"), "PORT=8080\n")?;
    /// let loaded = load_cascade(&dir, "")?;
    /// let values = loaded.into_values();
    /// assert_eq!(values.get("PORT"), Some("8080"));
    /// # std::fs::remove_dir_all(&dir)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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

/// The merged values of a load, without origins, reports or raw source text.
///
/// Returned by [`load_cascade_values`], [`load_cascade_values_with`],
/// [`load_values`] and [`LoadedEnvironment::into_values`]. It holds the same
/// final values that a [`LoadedEnvironment`] from the same files holds, sorted
/// by key, and uses less memory. Use a [`LoadedEnvironment`] to find where a
/// value came from or to resolve references.
///
/// # Examples
///
/// With `PORT=3000` in `.env` and `PORT=8080` in `.env.production`:
///
/// ```
/// use adotenv::load::load_cascade_values;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedValues-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "PORT=3000\n")?;
/// # std::fs::write(dir.join(".env.production"), "PORT=8080\n")?;
/// let values = load_cascade_values(&dir, "production")?;
/// assert_eq!(values.len(), 1);
/// assert_eq!(values.get("PORT"), Some("8080"));
///
/// let map = values.into_values();
/// assert_eq!(map["PORT"], "8080");
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadedValues {
    values: BTreeMap<String, String>,
}

impl LoadedValues {
    /// Returns the final values as a map sorted by key.
    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
    /// Returns the final value of `key`, or `None` if no file set it.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    /// Returns the number of final values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }
    /// Returns `true` if no file supplied a value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    /// Returns the map of final values, consuming `self`.
    #[must_use]
    pub fn into_values(self) -> BTreeMap<String, String> {
        self.values
    }
}

/// The values of a [`LoadedEnvironment`] after their references were
/// resolved.
///
/// Returned by [`LoadedEnvironment::resolve_contextual`] and
/// [`LoadedEnvironment::resolve_decoded`]. It has every loaded key, sorted by
/// key. See those methods for examples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEnvironment {
    values: BTreeMap<String, String>,
    origins: BTreeMap<String, CompactOrigin>,
    files: Vec<PathBuf>,
}

impl ResolvedEnvironment {
    /// Returns the resolved values as a map sorted by key.
    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
    /// Returns the resolved value of `key`, or `None` if no file set it.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    /// Returns where the winning declaration of `key` is.
    ///
    /// The origin locates the value as written in the file, before
    /// resolution. Returns `None` if no file set `key`, or if the load used
    /// [`LoadOptions::with_origins`] with `false`.
    #[must_use]
    pub fn origin(&self, key: &str) -> Option<LoadOrigin<'_>> {
        self.origins
            .get(key)
            .map(|origin| view_origin(&self.files, origin))
    }
    /// Returns the number of resolved values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }
    /// Returns `true` if there are no values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// An error from [`LoadedEnvironment::resolve_contextual`] or
/// [`LoadedEnvironment::resolve_decoded`].
///
/// When the error is about one key, [`key`](Self::key) names it and
/// [`origin`](Self::origin) locates its winning declaration, if origins were
/// kept. Its `Display` is `{path}:{line}: {message}` when the origin is known,
/// and the message alone otherwise.
///
/// # Examples
///
/// With `DATABASE_URL=postgres://${DB_HOST}/app` in `.env` and no `DB_HOST`
/// anywhere:
///
/// ```
/// use adotenv::load::load_cascade;
/// use adotenv::{ExpandOptions, ExpandScope, Unresolved};
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedResolveError-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "DATABASE_URL=postgres://${DB_HOST}/app\n")?;
/// let loaded = load_cascade(&dir, "")?;
/// let options = ExpandOptions::portable()
///     .with_scope(ExpandScope::WholeFile)
///     .with_unresolved(Unresolved::Error);
/// let error = loaded.resolve_contextual(|_| None, &options).unwrap_err();
/// assert_eq!(error.key(), Some("DATABASE_URL"));
/// assert_eq!(error.message(), "unresolved variable DB_HOST while resolving DATABASE_URL");
///
/// let origin = error.origin().unwrap();
/// assert_eq!(origin.path(), dir.join(".env"));
/// assert_eq!(origin.line(), 1);
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
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
    /// Returns the key that failed, or `None` when the error is not about one
    /// key.
    ///
    /// The key is `None` for a scope other than whole-file, for missing raw
    /// source text and when the merged values cannot be prepared for
    /// resolution.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }
    /// Returns where the winning declaration of the failed key is.
    ///
    /// Returns `None` when [`key`](Self::key) is `None`, or if the load used
    /// [`LoadOptions::with_origins`] with `false`.
    #[must_use]
    pub fn origin(&self) -> Option<&LoadOriginOwned> {
        self.origin.as_ref()
    }
    /// Returns the error message, without the file and line.
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
    /// Resolves references in the final values, treating each value as
    /// unquoted text.
    ///
    /// The quotes and escapes of the original files are ignored: a reference
    /// resolves wherever it appears in the decoded value, even in a value the
    /// file single-quoted. Use [`resolve_contextual`](Self::resolve_contextual)
    /// to keep their meaning. This method works without raw source text, so it
    /// also works after [`LoadOptions::with_raw_tokens`] with `false`.
    ///
    /// A reference sees the final value of every loaded key, whichever file
    /// set it. Names are looked up in those values and in `lookup`, in the
    /// `lookup_order` of `options`; the presets try the values first. `lookup`
    /// returns `None` for a name it does not know. Only
    /// [`ExpandScope::WholeFile`] is accepted, and every [`ExpandOptions`]
    /// preset uses sequential scope, so set the scope explicitly. The reference
    /// forms, unresolved-reference handling and limits also come from
    /// `options`. Command substitutions stay literal; to run them, parse the
    /// file and call
    /// [`Document::resolve_with_commands`](crate::Document::resolve_with_commands).
    ///
    /// Keys take part exactly as loaded, whatever dialect read them, so
    /// `${A B}` finds a key `A B` that a dialect with quoted keys read. A key
    /// is never read as a different key: one that cannot be written back
    /// exactly, such as a key with both an apostrophe and whitespace, is an
    /// error naming that key.
    ///
    /// # Errors
    ///
    /// Returns a [`LoadedResolveError`]:
    ///
    /// - without a key, if `options.scope` is not [`ExpandScope::WholeFile`];
    /// - naming the key, if a key or value cannot be written back exactly,
    ///   such as a value with a control character other than tab, CR or LF;
    /// - without a key, if the written-back values cannot be parsed again;
    /// - naming the key, if resolution fails, for example an unset name under
    ///   [`Unresolved::Error`](crate::Unresolved::Error), a cycle or a size
    ///   limit; the message is the [`ResolveError`] message.
    ///
    /// The error carries the origin of the key's winning declaration when
    /// origins were kept.
    ///
    /// # Examples
    ///
    /// With `HOST=db.internal` and `TEMPLATE='${HOST}'` in `.env`:
    ///
    /// ```
    /// use adotenv::load::load_cascade;
    /// use adotenv::{ExpandOptions, ExpandScope};
    ///
    /// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedEnvironment-resolve_decoded-{}", std::process::id()));
    /// # std::fs::create_dir_all(&dir)?;
    /// # std::fs::write(dir.join(".env"), "HOST=db.internal\nTEMPLATE='${HOST}'\n")?;
    /// let loaded = load_cascade(&dir, "")?;
    /// assert_eq!(loaded.get("TEMPLATE"), Some("${HOST}"));
    ///
    /// let options = ExpandOptions::portable().with_scope(ExpandScope::WholeFile);
    /// let decoded = loaded.resolve_decoded(|_| None, &options)?;
    /// assert_eq!(decoded.get("TEMPLATE"), Some("db.internal"));
    ///
    /// // The single quotes keep the reference literal in contextual resolution.
    /// let contextual = loaded.resolve_contextual(|_| None, &options)?;
    /// assert_eq!(contextual.get("TEMPLATE"), Some("${HOST}"));
    /// # std::fs::remove_dir_all(&dir)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
        // Keep a leading U+FEFF in a key from being read as the document BOM.
        let mut source = String::from("\n");
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

    /// Resolves references in the final values, keeping the quotes and escapes
    /// of the files they came from.
    ///
    /// The raw source text of each winning declaration is parsed again with
    /// the load dialect, so a value resolves as it would in its own file: a
    /// single-quoted `'${HOST}'` stays literal and an escaped `\$HOST` in
    /// double quotes stays `$HOST`. This needs the raw source text, which
    /// [`LoadOptions::new`] and [`LoadOptions::full`] keep; without it, use
    /// [`resolve_decoded`](Self::resolve_decoded).
    ///
    /// The rebuilt declarations are checked against the loaded keys and
    /// values. A key or value that cannot be reproduced exactly is an error
    /// naming the key, so contextual resolution never binds a different key or
    /// starts from a changed value.
    ///
    /// A reference sees the final value of every loaded key, whichever file
    /// set it. Names are looked up in those values and in `lookup`, in the
    /// `lookup_order` of `options`; the presets try the values first. `lookup`
    /// returns `None` for a name it does not know. The reference forms,
    /// unresolved-reference handling and limits also come from `options`.
    /// Command substitutions stay literal; to run them, parse the file and
    /// call
    /// [`Document::resolve_with_commands`](crate::Document::resolve_with_commands).
    ///
    /// Only [`ExpandScope::WholeFile`] is accepted, and every [`ExpandOptions`]
    /// preset uses sequential scope, so set the scope explicitly. The winning
    /// declarations come from different files, so no single order gives
    /// sequential scope a meaning. With `B=1` and `A=${B}/x` in `.env` and
    /// `B=2` in `.env.local`, a sequential reference from `A` would see no
    /// earlier `B`; whole-file scope resolves `A` to `2/x`.
    ///
    /// # Errors
    ///
    /// Returns a [`LoadedResolveError`]:
    ///
    /// - without a key, if `options.scope` is not [`ExpandScope::WholeFile`];
    /// - without a key, if the raw source text was not kept;
    /// - without a key, if the winning declarations cannot be parsed again;
    /// - naming the key, if a loaded key or value cannot be reproduced
    ///   exactly;
    /// - naming the key, if resolution fails, for example an unset name under
    ///   [`Unresolved::Error`](crate::Unresolved::Error), a cycle or a size
    ///   limit; the message is the [`ResolveError`] message.
    ///
    /// The error carries the origin of the key's winning declaration when
    /// origins were kept.
    ///
    /// # Examples
    ///
    /// With `DB_HOST=localhost` and `DATABASE_URL=postgres://${DB_USER}@${DB_HOST}/app`
    /// in `.env`, and `DB_HOST=db.internal` in `.env.production`:
    ///
    /// ```
    /// use adotenv::load::load_cascade;
    /// use adotenv::{ExpandOptions, ExpandScope};
    ///
    /// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedEnvironment-resolve_contextual-{}", std::process::id()));
    /// # std::fs::create_dir_all(&dir)?;
    /// # std::fs::write(dir.join(".env"), "DB_HOST=localhost\nDATABASE_URL=postgres://${DB_USER}@${DB_HOST}/app\n")?;
    /// # std::fs::write(dir.join(".env.production"), "DB_HOST=db.internal\n")?;
    /// let loaded = load_cascade(&dir, "production")?;
    /// let options = ExpandOptions::portable().with_scope(ExpandScope::WholeFile);
    /// let lookup = |name: &str| (name == "DB_USER").then(|| "app".to_string());
    /// let resolved = loaded.resolve_contextual(lookup, &options)?;
    /// assert_eq!(resolved.get("DATABASE_URL"), Some("postgres://app@db.internal/app"));
    /// assert_eq!(resolved.origin("DATABASE_URL").unwrap().path(), dir.join(".env"));
    /// # std::fs::remove_dir_all(&dir)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
        let unrepresentable = |key: &str, message: &str| {
            LoadedResolveError::new(Some(key.to_string()), self.owned_origin(key), message)
        };
        let mut entries: Vec<_> = self
            .records
            .values()
            .filter_map(|record| record.raw.clone())
            .collect();
        entries.sort_unstable_by_key(|(order, _, _)| *order);
        let mut source = String::new();
        // A leading blank line keeps a byte-order mark that belongs to the
        // first key from being read as the file's mark.
        source.push('\n');
        for (_, key_range, value_range) in entries {
            source.push_str(&self.raw_source[key_range.clone()]);
            source.push('=');
            let value = &self.raw_source[value_range.clone()];
            source.push_str(value);
            // In an LF-only dialect a CR right before the line feed belongs to
            // the line ending. Add one CR so a value that ends with CR keeps it.
            if self.dialect.line_breaks == LineBreaks::Lf && value.ends_with('\r') {
                source.push('\r');
            }
            source.push('\n');
        }
        let document = parse_strict_with(&source, self.dialect).map_err(|error| {
            LoadedResolveError::new(None, None, format!("cannot prepare resolution: {error}"))
        })?;
        // Every loaded key must come back exactly, with its exact value, so
        // resolution can never bind a different key or start from a changed
        // value. This matches the check in `resolve_decoded`.
        let mut parsed = BTreeMap::new();
        for entry in document.entries() {
            let Some(record) = self.records.get(entry.key()) else {
                return Err(LoadedResolveError::new(
                    Some(entry.key().to_string()),
                    self.owned_origin(entry.key()),
                    "cannot prepare resolution: unexpected declaration",
                ));
            };
            if entry.value() != Some(record.value.as_ref()) {
                return Err(unrepresentable(
                    entry.key(),
                    "value cannot be represented exactly",
                ));
            }
            if parsed.insert(entry.key(), entry.value()).is_some() {
                return Err(LoadedResolveError::new(
                    Some(entry.key().to_string()),
                    self.owned_origin(entry.key()),
                    "cannot prepare resolution: duplicate declaration",
                ));
            }
        }
        if parsed.len() != self.records.len() {
            for key in self.records.keys() {
                if !parsed.contains_key(&**key) {
                    return Err(unrepresentable(key, "key cannot be represented exactly"));
                }
            }
            return Err(LoadedResolveError::new(
                None,
                None,
                "cannot prepare resolution: unexpected declaration",
            ));
        }
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
        | ResolveError::TooManyReferences { key, .. }
        | ResolveError::ValueTooLarge { key, .. }
        | ResolveError::OutputTooLarge { key, .. }
        | ResolveError::Command { key, .. } => key,
    }
}

/// An error that stops a load.
///
/// A load either reads and checks every file or returns this error and no
/// values. The `Display` of a file error names the file, for example
/// `/srv/my-app/.env: line 2: key does not match the selected dialect (dotenv.invalid-key)`.
/// [`LoadError::InvalidMode`] has a fixed message.
///
/// # Examples
///
/// With `PORT=8080` on line 1 and `BAD KEY=1` on line 2 of `.env`:
///
/// ```
/// use adotenv::load::{load_cascade, LoadError};
/// use adotenv::DiagnosticCode;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadError-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "PORT=8080\nBAD KEY=1\n")?;
/// match load_cascade(&dir, "") {
///     Err(LoadError::Invalid { path, diagnostic }) => {
///         assert_eq!(path, dir.join(".env"));
///         assert_eq!(diagnostic.code(), DiagnosticCode::InvalidKey);
///         assert_eq!(diagnostic.line(), 2);
///     }
///     other => panic!("expected an invalid file, got {other:?}"),
/// }
///
/// assert!(matches!(load_cascade(&dir, "../secrets"), Err(LoadError::InvalidMode)));
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub enum LoadError {
    /// A file could not be read.
    ///
    /// A cascade skips a file that does not exist, but a file in an explicit
    /// list that does not exist is this error, with
    /// [`ErrorKind::NotFound`](std::io::ErrorKind::NotFound).
    Io {
        /// The file that could not be read.
        path: PathBuf,
        /// The I/O error, also returned by [`Error::source`](std::error::Error::source).
        source: io::Error,
    },
    /// A file is not valid UTF-8.
    InvalidUtf8 {
        /// The file that is not valid UTF-8.
        path: PathBuf,
    },
    /// A file has an error-severity diagnostic under the dialect.
    ///
    /// Warnings never cause this error.
    Invalid {
        /// The file that has the error.
        path: PathBuf,
        /// The first error-severity diagnostic in the file, in source order.
        diagnostic: Diagnostic,
    },
    /// The mode contains `/`, `\` or NUL, or is exactly `.` or `..`.
    InvalidMode,
    /// A configured read or parse limit was exceeded.
    Limit {
        /// The file whose limit was exceeded.
        path: PathBuf,
        /// The structured limit and observed size or count.
        limit: LimitExceeded,
    },
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
            Self::Limit { path, limit } => {
                write!(formatter, "{}: {limit}", path.display())
            }
            Self::InvalidMode => formatter
                .write_str("mode must not contain path separators or NUL and must not be . or .."),
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

/// Loads the `.env` cascade from `directory` with the Portable dialect.
///
/// Reads `.env`, `.env.local`, `.env.{mode}` and `.env.{mode}.local` from
/// `directory`, in that order. A later file's value replaces an earlier one,
/// key by key. Files that do not exist are skipped, and an empty `mode` reads
/// only `.env` and `.env.local`. The [module documentation](crate::load)
/// describes the cascade in full.
///
/// The result keeps origins and the raw source text for
/// [`LoadedEnvironment::resolve_contextual`], but no reports. Use
/// [`load_cascade_with`] for another dialect, [`load_cascade_with_options`]
/// for reports and [`load_cascade_values`] when only the values are needed.
///
/// # Errors
///
/// - [`LoadError::InvalidMode`] if `mode` contains `/`, `\` or NUL, or is
///   `.` or `..`. No file is read.
/// - [`LoadError::Io`] if a cascade file exists but cannot be read.
/// - [`LoadError::InvalidUtf8`] if a cascade file is not valid UTF-8.
/// - [`LoadError::Invalid`] if a cascade file has an error-severity
///   diagnostic.
///
/// # Examples
///
/// With `PORT=3000` in `.env` and `PORT=8080` in `.env.production`:
///
/// ```
/// use adotenv::load::load_cascade;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-load_cascade-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "PORT=3000\n")?;
/// # std::fs::write(dir.join(".env.production"), "PORT=8080\n")?;
/// let loaded = load_cascade(&dir, "production")?;
/// assert_eq!(loaded.get("PORT"), Some("8080"));
///
/// let development = load_cascade(&dir, "development")?;
/// assert_eq!(development.get("PORT"), Some("3000"));
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn load_cascade<P: AsRef<Path>>(
    directory: P,
    mode: &str,
) -> Result<LoadedEnvironment, LoadError> {
    load_cascade_with(directory, mode, Dialect::PORTABLE)
}

/// Loads the `.env` cascade from `directory` with the given dialect.
///
/// Works like [`load_cascade`], but parses every file with `dialect`.
///
/// # Errors
///
/// - [`LoadError::InvalidMode`] if `mode` contains `/`, `\` or NUL, or is
///   `.` or `..`. No file is read.
/// - [`LoadError::Io`] if a cascade file exists but cannot be read.
/// - [`LoadError::InvalidUtf8`] if a cascade file is not valid UTF-8.
/// - [`LoadError::Invalid`] if a cascade file has an error-severity
///   diagnostic under `dialect`.
///
/// # Examples
///
/// Node dotenv reads backticks as quotes. With `` GREETING=`hello world` `` in
/// `.env`:
///
/// ```
/// use adotenv::load::load_cascade_with;
/// use adotenv::Dialect;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-load_cascade_with-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "GREETING=`hello world`\n")?;
/// let loaded = load_cascade_with(&dir, "", Dialect::NODE_DOTENV)?;
/// assert_eq!(loaded.get("GREETING"), Some("hello world"));
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn load_cascade_with<P: AsRef<Path>>(
    directory: P,
    mode: &str,
    dialect: Dialect,
) -> Result<LoadedEnvironment, LoadError> {
    load_cascade_with_options(directory, mode, LoadOptions::new(dialect))
}

/// Loads the `.env` cascade from `directory` with the given options.
///
/// Works like [`load_cascade`], but parses every file with
/// `options.dialect` and keeps what `options` selects, such as the reports of
/// [`LoadOptions::full`].
///
/// # Errors
///
/// - [`LoadError::InvalidMode`] if `mode` contains `/`, `\` or NUL, or is
///   `.` or `..`. No file is read.
/// - [`LoadError::Io`] if a cascade file exists but cannot be read.
/// - [`LoadError::InvalidUtf8`] if a cascade file is not valid UTF-8.
/// - [`LoadError::Invalid`] if a cascade file has an error-severity
///   diagnostic under `options.dialect`.
/// - [`LoadError::Limit`] if a configured read or parse limit is exceeded.
///
/// # Examples
///
/// With `PORT=3000` in `.env` and `PORT=8080` in `.env.production`:
///
/// ```
/// use adotenv::load::{load_cascade_with_options, LoadOptions};
/// use adotenv::Dialect;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-load_cascade_with_options-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "PORT=3000\n")?;
/// # std::fs::write(dir.join(".env.production"), "PORT=8080\n")?;
/// let options = LoadOptions::full(Dialect::PORTABLE);
/// let loaded = load_cascade_with_options(&dir, "production", options)?;
///
/// let change = loaded.overrides().next().unwrap();
/// assert_eq!(change.key(), "PORT");
/// assert_eq!(change.previous().path(), dir.join(".env"));
/// assert_eq!(change.replacement().path(), dir.join(".env.production"));
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn load_cascade_with_options<P: AsRef<Path>>(
    directory: P,
    mode: &str,
    options: LoadOptions,
) -> Result<LoadedEnvironment, LoadError> {
    if !valid_mode(mode) {
        return Err(LoadError::InvalidMode);
    }
    let directory = directory.as_ref();
    load_paths(cascade_paths(directory, mode), options, true)
}

/// Loads the listed files in order with the given dialect.
///
/// A later file's value replaces an earlier one, key by key, as in a cascade.
/// Every listed file must exist; use [`load_cascade`] for the standard file
/// names, which may be missing. An empty list loads an empty environment.
///
/// # Errors
///
/// - [`LoadError::Io`] if a listed file does not exist or cannot be read.
/// - [`LoadError::InvalidUtf8`] if a listed file is not valid UTF-8.
/// - [`LoadError::Invalid`] if a listed file has an error-severity
///   diagnostic under `dialect`.
///
/// # Examples
///
/// With `PORT=3000` in `base.env` and `PORT=8080` in `production.env`:
///
/// ```
/// use adotenv::load::{load_files, LoadError};
/// use adotenv::Dialect;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-load_files-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join("base.env"), "PORT=3000\n")?;
/// # std::fs::write(dir.join("production.env"), "PORT=8080\n")?;
/// let files = [dir.join("base.env"), dir.join("production.env")];
/// let loaded = load_files(&files, Dialect::PORTABLE)?;
/// assert_eq!(loaded.get("PORT"), Some("8080"));
///
/// let missing = load_files([dir.join("missing.env")], Dialect::PORTABLE);
/// assert!(matches!(missing, Err(LoadError::Io { .. })));
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn load_files<I, P>(paths: I, dialect: Dialect) -> Result<LoadedEnvironment, LoadError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    load_files_with_options(paths, LoadOptions::new(dialect))
}

/// Loads the listed files in order with the given options.
///
/// Works like [`load_files`], but parses every file with `options.dialect` and
/// keeps what `options` selects.
///
/// # Errors
///
/// - [`LoadError::Io`] if a listed file does not exist or cannot be read.
/// - [`LoadError::InvalidUtf8`] if a listed file is not valid UTF-8.
/// - [`LoadError::Invalid`] if a listed file has an error-severity
///   diagnostic under `options.dialect`.
/// - [`LoadError::Limit`] if a configured read or parse limit is exceeded.
///
/// # Examples
///
/// With `PORT=3000` in `base.env` and `PORT=8080` in `production.env`:
///
/// ```
/// use adotenv::load::{load_files_with_options, LoadOptions};
/// use adotenv::Dialect;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-load_files_with_options-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join("base.env"), "PORT=3000\n")?;
/// # std::fs::write(dir.join("production.env"), "PORT=8080\n")?;
/// let files = [dir.join("base.env"), dir.join("production.env")];
/// let options = LoadOptions::new(Dialect::PORTABLE).with_origins(false);
/// let loaded = load_files_with_options(&files, options)?;
/// assert_eq!(loaded.get("PORT"), Some("8080"));
/// assert!(loaded.origin("PORT").is_none());
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
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

/// Loads the `.env` cascade from `directory` with the Portable dialect,
/// keeping only the final values.
///
/// Reads the same files with the same rules and errors as [`load_cascade`]
/// and returns the same values, but keeps no origins, reports or raw source
/// text, and builds no [`Document`](crate::Document) for any file. This uses
/// less memory, so prefer it when an application needs only the values.
///
/// # Errors
///
/// - [`LoadError::InvalidMode`] if `mode` contains `/`, `\` or NUL, or is
///   `.` or `..`. No file is read.
/// - [`LoadError::Io`] if a cascade file exists but cannot be read.
/// - [`LoadError::InvalidUtf8`] if a cascade file is not valid UTF-8.
/// - [`LoadError::Invalid`] if a cascade file has an error-severity
///   diagnostic.
///
/// # Examples
///
/// With `PORT=3000` in `.env` and `PORT=8080` in `.env.production`:
///
/// ```
/// use adotenv::load::load_cascade_values;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-load_cascade_values-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "PORT=3000\n")?;
/// # std::fs::write(dir.join(".env.production"), "PORT=8080\n")?;
/// let values = load_cascade_values(&dir, "production")?;
/// assert_eq!(values.get("PORT"), Some("8080"));
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn load_cascade_values<P: AsRef<Path>>(
    directory: P,
    mode: &str,
) -> Result<LoadedValues, LoadError> {
    load_cascade_values_with(directory, mode, Dialect::PORTABLE)
}

/// Loads the `.env` cascade from `directory` with the given dialect, keeping
/// only the final values.
///
/// Works like [`load_cascade_values`], but parses every file with `dialect`.
///
/// # Errors
///
/// - [`LoadError::InvalidMode`] if `mode` contains `/`, `\` or NUL, or is
///   `.` or `..`. No file is read.
/// - [`LoadError::Io`] if a cascade file exists but cannot be read.
/// - [`LoadError::InvalidUtf8`] if a cascade file is not valid UTF-8.
/// - [`LoadError::Invalid`] if a cascade file has an error-severity
///   diagnostic under `dialect`.
///
/// # Examples
///
/// Node dotenv reads backticks as quotes. With `` GREETING=`hello world` `` in
/// `.env`:
///
/// ```
/// use adotenv::load::load_cascade_values_with;
/// use adotenv::Dialect;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-load_cascade_values_with-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join(".env"), "GREETING=`hello world`\n")?;
/// let values = load_cascade_values_with(&dir, "", Dialect::NODE_DOTENV)?;
/// assert_eq!(values.get("GREETING"), Some("hello world"));
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn load_cascade_values_with<P: AsRef<Path>>(
    directory: P,
    mode: &str,
    dialect: Dialect,
) -> Result<LoadedValues, LoadError> {
    if !valid_mode(mode) {
        return Err(LoadError::InvalidMode);
    }
    let directory = directory.as_ref();
    load_value_paths(
        cascade_paths(directory, mode),
        LoadOptions::new(dialect),
        true,
    )
}

/// Loads the `.env` cascade with options, keeping only the final values.
///
/// This is [`load_cascade_values`] with [`LoadOptions`]. Report-retention
/// settings do not apply to the values-only result; the dialect and limits do.
///
/// # Errors
///
/// Returns [`LoadError::InvalidMode`], [`LoadError::Io`],
/// [`LoadError::InvalidUtf8`], [`LoadError::Invalid`] or
/// [`LoadError::Limit`] under the same rules as the full loader.
pub fn load_cascade_values_with_options<P: AsRef<Path>>(
    directory: P,
    mode: &str,
    options: LoadOptions,
) -> Result<LoadedValues, LoadError> {
    if !valid_mode(mode) {
        return Err(LoadError::InvalidMode);
    }
    let directory = directory.as_ref();
    load_value_paths(cascade_paths(directory, mode), options, true)
}

/// Loads the listed files in order with the given dialect, keeping only the
/// final values.
///
/// Reads the same files with the same rules and errors as [`load_files`] and
/// returns the same values, but keeps no origins, reports or raw source text.
/// Every listed file must exist; use [`load_cascade_values`] for the standard
/// file names, which may be missing.
///
/// # Errors
///
/// - [`LoadError::Io`] if a listed file does not exist or cannot be read.
/// - [`LoadError::InvalidUtf8`] if a listed file is not valid UTF-8.
/// - [`LoadError::Invalid`] if a listed file has an error-severity
///   diagnostic under `dialect`.
///
/// # Examples
///
/// With `PORT=3000` and `HOST=localhost` in `base.env`, and `PORT=8080` in
/// `production.env`:
///
/// ```
/// use std::collections::BTreeMap;
///
/// use adotenv::load::load_values;
/// use adotenv::Dialect;
///
/// # let dir = std::env::temp_dir().join(format!("adotenv-doc-load_values-{}", std::process::id()));
/// # std::fs::create_dir_all(&dir)?;
/// # std::fs::write(dir.join("base.env"), "PORT=3000\nHOST=localhost\n")?;
/// # std::fs::write(dir.join("production.env"), "PORT=8080\n")?;
/// let files = [dir.join("base.env"), dir.join("production.env")];
/// let map: BTreeMap<String, String> = load_values(&files, Dialect::PORTABLE)?.into_values();
/// assert_eq!(map["HOST"], "localhost");
/// assert_eq!(map["PORT"], "8080");
/// # std::fs::remove_dir_all(&dir)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn load_values<I, P>(paths: I, dialect: Dialect) -> Result<LoadedValues, LoadError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    load_values_with_options(paths, LoadOptions::new(dialect))
}

/// Loads listed files with options, keeping only the final values.
///
/// This is [`load_values`] with [`LoadOptions`]. Report-retention settings do
/// not apply to the values-only result; the dialect and limits do.
///
/// # Errors
///
/// Returns [`LoadError::Io`], [`LoadError::InvalidUtf8`],
/// [`LoadError::Invalid`] or [`LoadError::Limit`] under the same rules as
/// [`load_files_with_options`].
pub fn load_values_with_options<I, P>(
    paths: I,
    options: LoadOptions,
) -> Result<LoadedValues, LoadError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    load_value_paths(paths, options, false)
}

#[allow(clippy::too_many_lines)]
fn read_load_path(
    path: &Path,
    skip_missing: bool,
    max_input_bytes: Option<usize>,
) -> Result<Option<Vec<u8>>, LoadError> {
    if max_input_bytes.is_none() {
        match fs::metadata(path) {
            Ok(metadata) if !metadata.is_file() => {
                return Err(LoadError::Io {
                    path: path.to_path_buf(),
                    source: io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "path is not a regular file",
                    ),
                });
            }
            Ok(metadata) if metadata.len() == 0 => return Ok(Some(Vec::new())),
            _ => {}
        }
        let mut file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound && skip_missing => {
                return match fs::symlink_metadata(path) {
                    Ok(_) => Err(LoadError::Io {
                        path: path.to_path_buf(),
                        source: io::Error::new(io::ErrorKind::NotFound, "dangling symbolic link"),
                    }),
                    Err(metadata_error) if metadata_error.kind() == io::ErrorKind::NotFound => {
                        Ok(None)
                    }
                    Err(metadata_error) => Err(LoadError::Io {
                        path: path.to_path_buf(),
                        source: metadata_error,
                    }),
                };
            }
            Err(source) => {
                return Err(LoadError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|source| LoadError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        return Ok(Some(bytes));
    }
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound && skip_missing => {
            // A missing cascade file is optional. A dangling symlink is not:
            // the directory entry exists but its target cannot be read.
            return match fs::symlink_metadata(path) {
                Ok(_) => Err(LoadError::Io {
                    path: path.to_path_buf(),
                    source: io::Error::new(io::ErrorKind::NotFound, "dangling symbolic link"),
                }),
                Err(metadata_error) if metadata_error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(metadata_error) => Err(LoadError::Io {
                    path: path.to_path_buf(),
                    source: metadata_error,
                }),
            };
        }
        Err(source) => {
            return Err(LoadError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let metadata = file.metadata().map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(LoadError::Io {
            path: path.to_path_buf(),
            source: io::Error::new(io::ErrorKind::InvalidInput, "path is not a regular file"),
        });
    }
    if metadata.len() == 0 {
        return Ok(Some(Vec::new()));
    }
    if let Some(limit) = max_input_bytes {
        let observed = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        if observed > limit {
            return Err(LoadError::Limit {
                path: path.to_path_buf(),
                limit: LimitExceeded::new(crate::LimitKind::InputBytes, limit, observed),
            });
        }
    }
    let mut bytes = Vec::new();
    let read_result = match max_input_bytes {
        Some(limit) => file
            .take(limit.saturating_add(1) as u64)
            .read_to_end(&mut bytes),
        None => file.read_to_end(&mut bytes),
    };
    read_result.map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if let Some(limit) = max_input_bytes {
        if bytes.len() > limit {
            return Err(LoadError::Limit {
                path: path.to_path_buf(),
                limit: LimitExceeded::new(crate::LimitKind::InputBytes, limit, bytes.len()),
            });
        }
    }
    Ok(Some(bytes))
}

fn load_value_paths<I, P>(
    paths: I,
    options: LoadOptions,
    skip_missing: bool,
) -> Result<LoadedValues, LoadError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut values: Option<BTreeMap<String, String>> = None;
    for path in paths {
        let path = path.as_ref();
        let Some(bytes) = read_load_path(path, skip_missing, options.limits.max_input_bytes)?
        else {
            continue;
        };
        let text = String::from_utf8(bytes).map_err(|_| LoadError::InvalidUtf8 {
            path: path.to_path_buf(),
        })?;
        if text.is_empty() {
            continue;
        }
        let projected = match parse_values_with_options(&text, options.dialect, options.limits) {
            Ok(values) => values,
            Err(diagnostic) => {
                return Err(load_parse_error(path, diagnostic));
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

fn load_parse_error(path: &Path, diagnostic: Diagnostic) -> LoadError {
    match diagnostic.limit() {
        Some(limit) => LoadError::Limit {
            path: path.to_path_buf(),
            limit,
        },
        None => LoadError::Invalid {
            path: path.to_path_buf(),
            diagnostic,
        },
    }
}

fn valid_mode(mode: &str) -> bool {
    !mode.contains(['/', '\\', '\0']) && mode != "." && mode != ".."
}

/// The standard cascade paths, in load order, without duplicates.
///
/// Mode `local` names `.env.local` for both the base-local and mode-local
/// entries. Removing the duplicate keeps the report views from claiming that
/// the file overrides itself.
fn cascade_paths(directory: &Path, mode: &str) -> Vec<PathBuf> {
    let mut paths = vec![directory.join(".env"), directory.join(".env.local")];
    if !mode.is_empty() {
        paths.push(directory.join(format!(".env.{mode}")));
        paths.push(directory.join(format!(".env.{mode}.local")));
    }
    let mut seen = BTreeSet::new();
    paths.retain(|path| {
        // Windows and some macOS filesystems resolve differently spelled names
        // to the same file. Canonicalize what exists so the cascade does not
        // read it twice; missing files keep their spelling.
        let key = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        seen.insert(key)
    });
    paths
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
        let Some(bytes) = read_load_path(path, skip_missing, options.limits.max_input_bytes)?
        else {
            continue;
        };
        let text = String::from_utf8(bytes).map_err(|_| LoadError::InvalidUtf8 {
            path: path.to_path_buf(),
        })?;
        let (document, diagnostics) = parse_with_options(&text, options.dialect, options.limits);
        if let Some(diagnostic) = diagnostics
            .into_iter()
            .find(|diagnostic| diagnostic.severity() == Severity::Error)
        {
            return Err(load_parse_error(path, diagnostic));
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
