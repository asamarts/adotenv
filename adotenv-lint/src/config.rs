//! Shared `.adotenv-lint.toml` loading, validation and policy resolution.
//!
//! The CLI and the language server use the same model so they cannot drift on
//! discovery, unknown fields, rule severities or ignored paths.
//!
//! ```
//! use adotenv_lint::config::{ConfigExplicit, LintPolicy, LintConfig};
//!
//! let config = LintConfig::from_toml("key_order = true\n", std::path::Path::new("test.toml"))?;
//! let mut policy = LintPolicy::default();
//! config.apply(&mut policy, ConfigExplicit::default());
//! assert!(policy.lint.key_order);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use adotenv::{Dialect, EqualsSpacing, QuoteStyle, Severity};
use serde::Deserialize;

use crate::{KeyCase, LintOptions, Rule};

/// The conventional lint configuration file name.
pub const CONFIG_FILE_NAME: &str = ".adotenv-lint.toml";

/// Human, JSON or GitHub output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OutputFormat {
    /// Human-readable text.
    Human,
    /// One JSON array.
    Json,
    /// GitHub Actions workflow commands.
    Github,
}

impl OutputFormat {
    /// Returns the stable configuration name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Json => "json",
            Self::Github => "github",
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            "github" => Ok(Self::Github),
            other => Err(format!("unknown format: {other}")),
        }
    }
}

/// A rule severity in configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RuleSeverity {
    /// Disable the rule.
    Off,
    /// Report a warning.
    Warning,
    /// Report an error.
    Error,
}

impl RuleSeverity {
    /// Returns the corresponding lint severity, or `None` when disabled.
    #[must_use]
    pub const fn severity(self) -> Option<Severity> {
        match self {
            Self::Off => None,
            Self::Warning => Some(Severity::Warning),
            Self::Error => Some(Severity::Error),
        }
    }
}

impl std::str::FromStr for RuleSeverity {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "off" => Ok(Self::Off),
            "warning" => Ok(Self::Warning),
            "error" => Ok(Self::Error),
            other => Err(format!("unknown severity: {other}")),
        }
    }
}

/// A loaded and validated `.adotenv-lint.toml`.
///
/// All enum values are parsed while loading. Unknown fields and rules are
/// rejected before a policy can be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintConfig {
    /// Parser dialect selected by the file.
    pub dialect: Option<Dialect>,
    /// Output format selected by the file.
    pub format: Option<OutputFormat>,
    /// Whether directory paths are searched recursively.
    pub recursive: Option<bool>,
    /// Whether declarations must be sorted.
    pub key_order: Option<bool>,
    /// Key-case policy.
    pub key_case: Option<KeyCase>,
    /// Quote-style policy.
    pub quote_style: Option<QuoteStyle>,
    /// Equals-spacing policy.
    pub equals_spacing: Option<EqualsSpacing>,
    /// Whether trailing whitespace must be removed.
    pub trim_trailing_whitespace: Option<bool>,
    /// Glob patterns excluded from discovery, when present.
    pub ignore: Option<Vec<String>>,
    /// Per-rule severity overrides, keyed by the stable rule name.
    pub rules: Option<BTreeMap<String, RuleSeverity>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    dialect: Option<String>,
    format: Option<String>,
    recursive: Option<bool>,
    key_order: Option<bool>,
    key_case: Option<String>,
    quote_style: Option<String>,
    equals_spacing: Option<String>,
    trim_trailing_whitespace: Option<bool>,
    ignore: Option<Vec<String>>,
    rules: Option<BTreeMap<String, String>>,
}

/// A validated lint configuration error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    path: Option<PathBuf>,
    message: String,
}

impl ConfigError {
    fn at(path: &Path, message: impl Into<String>) -> Self {
        Self {
            path: Some(path.to_path_buf()),
            message: message.into(),
        }
    }

    /// Returns the path associated with the error, when one is known.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Returns the error message without the path prefix.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.path {
            Some(path) => write!(formatter, "{}: {}", path.display(), self.message),
            None => formatter.write_str(&self.message),
        }
    }
}

impl std::error::Error for ConfigError {}

fn unknown(name: &str, value: &str) -> String {
    // Kept separate so every configuration error has the same spelling.
    let mut message = String::new();
    message.push_str("unknown ");
    message.push_str(name);
    message.push_str(": ");
    message.push_str(value);
    message
}

impl LintConfig {
    /// Parses and validates a configuration from TOML.
    ///
    /// # Errors
    ///
    /// Returns a [`ConfigError`] for TOML syntax errors, unknown fields,
    /// unknown dialect/format/key-case/quote/spacing values or unknown rules
    /// and severities.
    pub fn from_toml(source: &str, path: &Path) -> Result<Self, ConfigError> {
        let raw: RawConfig =
            toml::from_str(source).map_err(|error| ConfigError::at(path, error.to_string()))?;
        Self::from_raw(raw, path)
    }

    /// Loads and validates a configuration file.
    ///
    /// # Errors
    ///
    /// Returns a [`ConfigError`] when the file cannot be read or does not
    /// validate.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let source =
            fs::read_to_string(path).map_err(|error| ConfigError::at(path, error.to_string()))?;
        Self::from_toml(&source, path)
    }

    /// Applies this configuration to a policy, honoring explicit CLI fields.
    ///
    /// Command-line values represented as `true` in `explicit` are not
    /// overwritten by the file. Rules apply after the corresponding top-level
    /// options, matching the historical CLI behavior.
    pub fn apply(&self, policy: &mut LintPolicy, explicit: ConfigExplicit) {
        if !explicit.dialect {
            if let Some(value) = self.dialect {
                policy.dialect = value;
            }
        }
        if !explicit.format {
            if let Some(value) = self.format {
                policy.format = value;
            }
        }
        if !explicit.recursive {
            if let Some(value) = self.recursive {
                policy.recursive = value;
            }
        }
        if !explicit.key_order {
            if let Some(value) = self.key_order {
                policy.lint = policy.lint.with_key_order(value);
            }
        }
        if !explicit.key_case {
            if let Some(value) = self.key_case {
                policy.lint = policy.lint.with_key_case(value);
            }
        }
        if !explicit.quote_style {
            if let Some(value) = self.quote_style {
                policy.lint = policy.lint.with_quote_style(value);
            }
        }
        if !explicit.equals_spacing {
            if let Some(value) = self.equals_spacing {
                policy.lint = policy.lint.with_equals_spacing(value);
            }
        }
        if !explicit.trim_trailing_whitespace {
            if let Some(value) = self.trim_trailing_whitespace {
                policy.lint = policy.lint.with_trim_trailing_whitespace(value);
            }
        }
        if !explicit.ignore {
            if let Some(ignore) = &self.ignore {
                policy.ignore.clone_from(ignore);
            }
        }
        if let Some(rules) = &self.rules {
            apply_rules(policy, rules, explicit);
        }
    }

    fn from_raw(raw: RawConfig, path: &Path) -> Result<Self, ConfigError> {
        let dialect = raw
            .dialect
            .as_deref()
            .map(|value| {
                parse_dialect(value).map_err(|_| ConfigError::at(path, unknown("dialect", value)))
            })
            .transpose()?;
        let format = raw
            .format
            .as_deref()
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| ConfigError::at(path, unknown("format", value)))
            })
            .transpose()?;
        let key_case = raw
            .key_case
            .as_deref()
            .map(|value| {
                parse_key_case(value).map_err(|_| ConfigError::at(path, unknown("key case", value)))
            })
            .transpose()?;
        let quote_style = raw
            .quote_style
            .as_deref()
            .map(|value| {
                parse_quote_style(value)
                    .map_err(|_| ConfigError::at(path, unknown("quote style", value)))
            })
            .transpose()?;
        let equals_spacing = raw
            .equals_spacing
            .as_deref()
            .map(|value| {
                parse_equals_spacing(value)
                    .map_err(|_| ConfigError::at(path, unknown("equals spacing", value)))
            })
            .transpose()?;
        let mut rules = None;
        if let Some(raw_rules) = raw.rules {
            let mut parsed = BTreeMap::new();
            for (name, value) in raw_rules {
                let rule = parse_rule(&name)
                    .ok_or_else(|| ConfigError::at(path, format!("unknown rule: {name}")))?;
                let severity = value.parse::<RuleSeverity>().map_err(|_| {
                    ConfigError::at(path, unknown(&format!("severity for {name}"), &value))
                })?;
                parsed.insert(rule.as_str().to_owned(), severity);
            }
            rules = Some(parsed);
        }
        Ok(Self {
            dialect,
            format,
            recursive: raw.recursive,
            key_order: raw.key_order,
            key_case,
            quote_style,
            equals_spacing,
            trim_trailing_whitespace: raw.trim_trailing_whitespace,
            ignore: raw.ignore,
            rules,
        })
    }
}

fn apply_rules(
    policy: &mut LintPolicy,
    rules: &BTreeMap<String, RuleSeverity>,
    explicit: ConfigExplicit,
) {
    for (name, severity) in rules {
        let rule = parse_rule(name).expect("validated rule name");
        let enabled = severity.severity().is_some();
        match rule {
            Rule::DuplicateKey => {
                policy.lint = policy.lint.with_duplicate_keys(enabled);
            }
            Rule::KeyOrder if !explicit.key_order => {
                policy.lint = policy.lint.with_key_order(enabled);
            }
            Rule::KeyCase if !explicit.key_case && !enabled => {
                policy.lint = policy.lint.with_key_case(KeyCase::Off);
            }
            Rule::QuoteStyle if !explicit.quote_style && !enabled => {
                policy.lint = policy.lint.with_quote_style(QuoteStyle::Preserve);
            }
            Rule::Whitespace if !explicit.equals_spacing && !enabled => {
                policy.lint = policy
                    .lint
                    .with_equals_spacing(EqualsSpacing::Preserve)
                    .with_trim_trailing_whitespace(false);
            }
            _ => {}
        }
        if let Some(severity) = severity.severity() {
            policy.lint = policy.lint.with_severity(rule, severity);
        }
    }
}

/// The policy resolved from a config plus caller-selected defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintPolicy {
    /// Parser dialect.
    pub dialect: Dialect,
    /// Output format.
    pub format: OutputFormat,
    /// Whether directories are searched recursively.
    pub recursive: bool,
    /// Lint rules and severities.
    pub lint: LintOptions,
    /// Ignored glob patterns.
    pub ignore: Vec<String>,
}

impl Default for LintPolicy {
    fn default() -> Self {
        Self {
            dialect: Dialect::PORTABLE,
            format: OutputFormat::Human,
            recursive: false,
            lint: LintOptions::default(),
            ignore: Vec::new(),
        }
    }
}

/// Which policy fields were set explicitly on the command line.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConfigExplicit {
    /// Dialect was selected explicitly.
    pub dialect: bool,
    /// Format was selected explicitly.
    pub format: bool,
    /// Recursive mode was selected explicitly.
    pub recursive: bool,
    /// Key-order mode was selected explicitly.
    pub key_order: bool,
    /// Key-case policy was selected explicitly.
    pub key_case: bool,
    /// Quote-style policy was selected explicitly.
    pub quote_style: bool,
    /// Equals-spacing policy was selected explicitly.
    pub equals_spacing: bool,
    /// Trailing-whitespace mode was selected explicitly.
    pub trim_trailing_whitespace: bool,
    /// Ignore patterns were selected explicitly.
    pub ignore: bool,
}

/// Finds the nearest ancestor configuration without reading it.
///
/// `start` may be a file or a directory. The search begins at its directory
/// and walks toward the filesystem root.
#[must_use]
pub fn discover_config(start: &Path) -> Option<PathBuf> {
    let mut directory = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };
    loop {
        let candidate = directory.join(CONFIG_FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !directory.pop() {
            return None;
        }
    }
}

/// Loads an explicit configuration, or discovers one from `start`.
///
/// Returns `Ok(None)` when no explicit path is supplied and no configuration
/// file exists in `start` or its ancestors.
///
/// # Errors
///
/// Returns a [`ConfigError`] when a file exists or is explicit but cannot be
/// read or validated.
pub fn load_effective_config(
    explicit: Option<&Path>,
    start: &Path,
) -> Result<Option<LintConfig>, ConfigError> {
    let path = match explicit {
        Some(path) => path.to_path_buf(),
        None => match discover_config(start) {
            Some(path) => path,
            None => return Ok(None),
        },
    };
    LintConfig::load(&path).map(Some)
}

fn parse_rule(value: &str) -> Option<Rule> {
    match value {
        "duplicate-key" => Some(Rule::DuplicateKey),
        "key-order" => Some(Rule::KeyOrder),
        "key-case" => Some(Rule::KeyCase),
        "quote-style" => Some(Rule::QuoteStyle),
        "whitespace" => Some(Rule::Whitespace),
        _ => None,
    }
}

/// Parses a dialect configuration/CLI value.
///
/// # Errors
///
/// Returns an error for an unknown dialect name.
pub fn parse_dialect(value: &str) -> Result<Dialect, String> {
    match value {
        "portable" => Ok(Dialect::PORTABLE),
        "node" => Ok(Dialect::NODE_DOTENV),
        "python" => Ok(Dialect::PYTHON_DOTENV),
        "compose" => Ok(Dialect::COMPOSE),
        "docker" | "docker-raw" => Ok(Dialect::DOCKER_RAW),
        other => Err(format!("unknown dialect: {other}")),
    }
}

/// Parses a key-case configuration/CLI value.
///
/// # Errors
///
/// Returns an error for an unknown key-case name.
pub fn parse_key_case(value: &str) -> Result<KeyCase, String> {
    match value {
        "lower" => Ok(KeyCase::Lower),
        "upper" => Ok(KeyCase::Upper),
        other => Err(format!("unknown key case: {other}")),
    }
}

/// Parses a quote-style configuration/CLI value.
///
/// # Errors
///
/// Returns an error for an unknown quote-style name.
pub fn parse_quote_style(value: &str) -> Result<QuoteStyle, String> {
    match value {
        "preserve" => Ok(QuoteStyle::Preserve),
        "double" => Ok(QuoteStyle::Double),
        "single" => Ok(QuoteStyle::Single),
        "unquoted" => Ok(QuoteStyle::Unquoted),
        other => Err(format!("unknown quote style: {other}")),
    }
}

/// Parses an equals-spacing configuration/CLI value.
///
/// # Errors
///
/// Returns an error for an unknown spacing name.
pub fn parse_equals_spacing(value: &str) -> Result<EqualsSpacing, String> {
    match value {
        "preserve" => Ok(EqualsSpacing::Preserve),
        "none" => Ok(EqualsSpacing::None),
        "spaces" => Ok(EqualsSpacing::Spaces),
        other => Err(format!("unknown equals spacing: {other}")),
    }
}

/// Parses a format configuration/CLI value.
///
/// # Errors
///
/// Returns an error for an unknown output format.
pub fn parse_output_format(value: &str) -> Result<OutputFormat, String> {
    value.parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "adotenv-lint-config-{name}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn file(&self, name: &str, source: &str) -> PathBuf {
            let path = self.0.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, source).unwrap();
            path
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn no_config_uses_defaults() {
        let dir = TempDir::new("defaults");
        let config = load_effective_config(None, dir.path()).unwrap();
        assert!(config.is_none());
        let policy = LintPolicy::default();
        assert_eq!(policy.dialect, Dialect::PORTABLE);
        assert_eq!(policy.format, OutputFormat::Human);
        assert_eq!(policy.ignore.len(), 0);
    }

    #[test]
    fn explicit_path_takes_precedence_over_discovery() {
        let dir = TempDir::new("explicit");
        dir.file(CONFIG_FILE_NAME, "key_order = true\n");
        let explicit = dir.file("other.toml", "key_case = \"upper\"\n");
        let config = load_effective_config(Some(&explicit), dir.path())
            .unwrap()
            .unwrap();
        assert_eq!(config.key_order, None);
        assert_eq!(config.key_case, Some(KeyCase::Upper));
    }

    #[test]
    fn discovery_walks_ancestors() {
        let dir = TempDir::new("discovery");
        dir.file(CONFIG_FILE_NAME, "key_order = true\n");
        let nested = dir.path().join("nested");
        fs::create_dir_all(&nested).unwrap();
        assert_eq!(
            discover_config(&nested.join("app.env")),
            Some(dir.path().join(CONFIG_FILE_NAME))
        );
    }

    #[test]
    fn unknown_fields_and_rules_are_rejected() {
        let path = Path::new("test.toml");
        assert!(LintConfig::from_toml("unknown = true\n", path).is_err());
        assert!(LintConfig::from_toml("[rules]\nunknown = \"warning\"\n", path).is_err());
        assert!(LintConfig::from_toml("[rules]\nkey-order = \"nope\"\n", path).is_err());
    }

    #[test]
    fn severity_and_ignores_apply() {
        let config = LintConfig::from_toml(
            "ignore = [\".env.skip\"]\n[rules]\nduplicate-key = \"error\"\n",
            Path::new("test.toml"),
        )
        .unwrap();
        let mut policy = LintPolicy::default();
        config.apply(&mut policy, ConfigExplicit::default());
        assert_eq!(policy.ignore, [".env.skip"]);
        assert_eq!(
            policy.lint.severity_for_rule(Rule::DuplicateKey),
            Severity::Error
        );
    }

    #[test]
    fn explicit_fields_are_not_overridden() {
        let config = LintConfig::from_toml(
            "format = \"json\"\nkey_order = true\nignore = [\"config\"]\n",
            Path::new("test.toml"),
        )
        .unwrap();
        let mut policy = LintPolicy {
            format: OutputFormat::Human,
            ignore: vec!["cli".to_owned()],
            ..LintPolicy::default()
        };
        config.apply(
            &mut policy,
            ConfigExplicit {
                format: true,
                key_order: true,
                ignore: true,
                ..ConfigExplicit::default()
            },
        );
        assert_eq!(policy.format, OutputFormat::Human);
        assert!(!policy.lint.key_order);
        assert_eq!(policy.ignore, ["cli"]);
    }

    #[test]
    fn parses_dialects_and_spacing() {
        assert_eq!(parse_dialect("node").unwrap(), Dialect::NODE_DOTENV);
        assert_eq!(parse_dialect("docker").unwrap(), Dialect::DOCKER_RAW);
        assert_eq!(
            parse_equals_spacing("spaces").unwrap(),
            EqualsSpacing::Spaces
        );
        assert_eq!(parse_output_format("github").unwrap(), OutputFormat::Github);
    }
}
