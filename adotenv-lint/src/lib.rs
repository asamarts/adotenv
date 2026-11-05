//! Opt-in lint rules and verified fixes for [`adotenv`] documents.
//!
//! The linter is deliberately separate from the parser. A lint finding is
//! policy, not a parse error, and every suggested fix is applied through
//! `adotenv`'s verified edit or format APIs.
//!
//! The default rule set reports duplicate keys only. Enable ordering, key
//! case, quoting and whitespace rules explicitly.
//!
//! ```
//! use adotenv_lint::{lint, LintOptions};
//!
//! let doc = adotenv::parse_strict("B=2\nA=1\n").unwrap();
//! let defaults = lint(&doc, LintOptions::default());
//! assert!(defaults.is_empty());
//!
//! let findings = lint(
//!     &doc,
//!     LintOptions::default().with_key_order(true),
//! );
//! assert_eq!(findings.len(), 1);
//! assert_eq!(findings[0].rule(), adotenv_lint::Rule::KeyOrder);
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;

use adotenv::{
    parse_strict_with, ColumnEncoding, Document, Edit, EditError, EqualsSpacing, FormatError,
    FormatOptions, FormatResult, LineIndex, Quote, QuoteStyle, Severity, VerifiedEdits,
};

/// The policy rules understood by the linter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Rule {
    /// A key is declared more than once.
    DuplicateKey,
    /// Keys are not in the configured order.
    KeyOrder,
    /// A key does not use the configured ASCII case.
    KeyCase,
    /// A value does not use the configured quote style.
    QuoteStyle,
    /// Spacing around `=` or trailing whitespace differs from the policy.
    Whitespace,
}

impl Rule {
    const fn index(self) -> usize {
        match self {
            Self::DuplicateKey => 0,
            Self::KeyOrder => 1,
            Self::KeyCase => 2,
            Self::QuoteStyle => 3,
            Self::Whitespace => 4,
        }
    }
    /// Returns the stable rule identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateKey => "duplicate-key",
            Self::KeyOrder => "key-order",
            Self::KeyCase => "key-case",
            Self::QuoteStyle => "quote-style",
            Self::Whitespace => "whitespace",
        }
    }
}

/// The desired ASCII case for keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum KeyCase {
    /// Do not check key case.
    #[default]
    Off,
    /// Require lowercase ASCII letters.
    Lower,
    /// Require uppercase ASCII letters.
    Upper,
}

/// Linter policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct LintOptions {
    /// Report later declarations of a repeated key.
    pub duplicate_keys: bool,
    /// Require each group to sort its declarations.
    pub key_order: bool,
    /// Require a key case.
    pub key_case: KeyCase,
    /// Require a quote style for non-empty values.
    pub quote_style: QuoteStyle,
    /// Require a spacing policy around assignment separators.
    pub equals_spacing: EqualsSpacing,
    /// Remove trailing spaces and tabs.
    pub trim_trailing_whitespace: bool,
    severities: [Severity; 5],
}

impl Default for LintOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl LintOptions {
    /// Creates the conservative default policy: duplicate-key checks only.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            duplicate_keys: true,
            key_order: false,
            key_case: KeyCase::Off,
            quote_style: QuoteStyle::Preserve,
            equals_spacing: EqualsSpacing::Preserve,
            trim_trailing_whitespace: false,
            severities: [Severity::Warning; 5],
        }
    }
    /// Enables or disables duplicate-key findings.
    #[must_use]
    pub const fn with_duplicate_keys(mut self, value: bool) -> Self {
        self.duplicate_keys = value;
        self
    }
    /// Enables or disables key-order findings.
    #[must_use]
    pub const fn with_key_order(mut self, value: bool) -> Self {
        self.key_order = value;
        self
    }
    /// Sets the key-case policy.
    #[must_use]
    pub const fn with_key_case(mut self, value: KeyCase) -> Self {
        self.key_case = value;
        self
    }
    /// Sets the quote-style policy.
    #[must_use]
    pub const fn with_quote_style(mut self, value: QuoteStyle) -> Self {
        self.quote_style = value;
        self
    }
    /// Sets the assignment-spacing policy.
    #[must_use]
    pub const fn with_equals_spacing(mut self, value: EqualsSpacing) -> Self {
        self.equals_spacing = value;
        self
    }
    /// Enables or disables trailing-whitespace findings.
    #[must_use]
    pub const fn with_trim_trailing_whitespace(mut self, value: bool) -> Self {
        self.trim_trailing_whitespace = value;
        self
    }
    /// Sets the severity reported for one rule.
    #[must_use]
    pub const fn with_severity(mut self, rule: Rule, severity: Severity) -> Self {
        self.severities[rule.index()] = severity;
        self
    }
    const fn severity_for(self, rule: Rule) -> Severity {
        self.severities[rule.index()]
    }
    fn format_options(self) -> FormatOptions {
        FormatOptions::default()
            .with_quote_style(self.quote_style)
            .with_equals_spacing(self.equals_spacing)
            .with_trim_trailing_whitespace(self.trim_trailing_whitespace)
    }
}

/// A structured lint finding.
///
/// Findings never contain values: only the rule, message, key and source span
/// are retained. A finding may include a fix that can be applied through
/// [`apply_fix`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    rule: Rule,
    message: &'static str,
    severity: Severity,
    span: Range<usize>,
    line: usize,
    key: Option<String>,
    fix: Option<Fix>,
}

impl Finding {
    fn new(
        rule: Rule,
        message: &'static str,
        severity: Severity,
        span: Range<usize>,
        line: usize,
        key: Option<&str>,
        fix: Option<Fix>,
    ) -> Self {
        Self {
            rule,
            message,
            severity,
            span,
            line,
            key: key.map(str::to_owned),
            fix,
        }
    }
    /// Returns the rule that produced this finding.
    #[must_use]
    pub const fn rule(&self) -> Rule {
        self.rule
    }
    /// Returns the stable, value-free message.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
    /// Returns the finding severity. Lint findings are warnings.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.severity
    }
    /// Returns the byte range of the finding in the document source.
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }
    /// Returns the one-based line of the finding.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }
    /// Returns the key named by the finding, when there is one.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }
    /// Returns a verified-fix request for this finding, when one is available.
    #[must_use]
    pub const fn fix(&self) -> Option<&Fix> {
        self.fix.as_ref()
    }
}

/// A fix request for an [`adotenv`] document.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Fix {
    /// Sort each group through [`Document::sort`].
    Sort,
    /// Apply a format policy through [`Document::format`].
    Format(FormatOptions),
    /// Rename one declaration through [`Edit::Rename`].
    Rename {
        /// The zero-based entry ID.
        entry: usize,
        /// The replacement key.
        key: String,
    },
}

/// A successfully applied fix.
#[derive(Debug)]
#[non_exhaustive]
pub enum FixResult {
    /// The verified result of [`Document::sort`] or [`Document::format`].
    Format(FormatResult),
    /// The verified result of an edit batch.
    Edits(VerifiedEdits),
}

impl FixResult {
    /// Returns the complete verified output text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Format(result) => result.as_str(),
            Self::Edits(result) => result.as_str(),
        }
    }
    /// Returns the verified text edits.
    #[must_use]
    pub fn edits(&self) -> &[adotenv::TextEdit] {
        match self {
            Self::Format(result) => result.edits(),
            Self::Edits(result) => result.edits(),
        }
    }
}

/// The result of applying compatible fixes in stages.
///
/// Each stage is verified against the source at the start of that stage. The
/// final [`output`](Self::output) is the complete result; stage edits are
/// relative to their own stage input, not to the original source.
#[derive(Debug)]
pub struct BatchResult {
    output: String,
    stages: Vec<FixResult>,
}

impl BatchResult {
    /// Returns the final verified output text.
    #[must_use]
    pub fn output(&self) -> &str {
        &self.output
    }
    /// Returns the verified stages in application order.
    #[must_use]
    pub fn stages(&self) -> &[FixResult] {
        &self.stages
    }
    /// Returns `true` when no fix changed anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }
}

/// Applies renames, sorting and formatting in compatible stages.
///
/// Renames are applied together in one edit batch. Sorting and one format
/// policy can follow; each stage is verified before the next starts. Different
/// format policies and conflicting replacement keys are rejected. This is the
/// multi-stage companion to [`apply_fixes`].
///
/// # Errors
///
/// Returns [`LintError`] for conflicting fixes, a rejected edit/format, or an
/// intermediate output that cannot be parsed.
pub fn apply_batch(document: &Document<'_>, fixes: &[Fix]) -> Result<BatchResult, LintError> {
    let mut renames: Vec<(usize, String)> = Vec::new();
    let mut format = None;
    let mut sort = false;
    for fix in fixes {
        match fix {
            Fix::Rename { entry, key } => {
                if let Some((_, existing)) = renames.iter().find(|(id, _)| id == entry) {
                    if existing != key {
                        return Err(LintError::Conflict(
                            "one entry has two different replacement keys",
                        ));
                    }
                    continue;
                }
                renames.push((*entry, key.clone()));
            }
            Fix::Format(options) => {
                if format.is_some_and(|existing: FormatOptions| existing != *options) {
                    return Err(LintError::Conflict(
                        "different format policies cannot be combined",
                    ));
                }
                format = Some(*options);
            }
            Fix::Sort => sort = true,
        }
    }

    let mut output = document.source().to_owned();
    let mut stages = Vec::new();
    if !renames.is_empty() {
        let current =
            parse_strict_with(&output, document.dialect()).map_err(|_| LintError::Parse)?;
        let edits: Vec<Edit<'_>> = renames
            .iter()
            .map(|(entry, key)| Edit::Rename { entry: *entry, key })
            .collect();
        let result = current.edit(&edits).map_err(LintError::Edit)?;
        output.clear();
        output.push_str(result.as_str());
        stages.push(FixResult::Edits(result));
    }
    if sort {
        let current =
            parse_strict_with(&output, document.dialect()).map_err(|_| LintError::Parse)?;
        let result = current.sort().map_err(LintError::Format)?;
        output.clear();
        output.push_str(result.as_str());
        stages.push(FixResult::Format(result));
    }
    if let Some(options) = format {
        let current =
            parse_strict_with(&output, document.dialect()).map_err(|_| LintError::Parse)?;
        let result = current.format(&options).map_err(LintError::Format)?;
        output.clear();
        output.push_str(result.as_str());
        stages.push(FixResult::Format(result));
    }
    Ok(BatchResult { output, stages })
}

/// An error while applying a lint fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LintError {
    /// The formatter or sorter refused the output.
    Format(FormatError),
    /// The verified edit batch was rejected.
    Edit(EditError),
    /// The supplied fixes cannot be combined in one verified batch.
    Conflict(&'static str),
    /// An intermediate verified output could not be parsed again.
    Parse,
}

impl fmt::Display for LintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(error) => write!(formatter, "cannot apply fix: {error}"),
            Self::Edit(error) => write!(formatter, "cannot apply fix: {error}"),
            Self::Conflict(message) => write!(formatter, "cannot combine fixes: {message}"),
            Self::Parse => formatter.write_str("cannot parse an intermediate fix output"),
        }
    }
}

impl std::error::Error for LintError {}

/// Applies one fix request through the document's verified APIs.
///
/// # Errors
///
/// Returns [`LintError`] when the formatter/sorter refuses the output or the
/// edit batch is rejected, for example because a rename would collide.
pub fn apply_fix(document: &Document<'_>, fix: &Fix) -> Result<FixResult, LintError> {
    match fix {
        Fix::Sort => document
            .sort()
            .map(FixResult::Format)
            .map_err(LintError::Format),
        Fix::Format(options) => document
            .format(options)
            .map(FixResult::Format)
            .map_err(LintError::Format),
        Fix::Rename { entry, key } => document
            .edit(&[Edit::Rename { entry: *entry, key }])
            .map(FixResult::Edits)
            .map_err(LintError::Edit),
    }
}

/// Applies a batch of compatible fixes in one verified operation.
///
/// Multiple renames are applied as one [`Document::edit`] batch, so swaps and
/// independent renames are verified together. One formatting or sorting fix
/// can be applied without renames. Mixed rename/format/sort batches and
/// multiple different format policies return [`LintError::Conflict`]; apply
/// those in separate rounds and re-lint between them.
///
/// # Errors
///
/// Returns [`LintError`] when the fixes conflict, a rename collides, or the
/// formatter/sorter refuses the output.
pub fn apply_fixes(document: &Document<'_>, fixes: &[Fix]) -> Result<FixResult, LintError> {
    let mut renames: Vec<(usize, String)> = Vec::new();
    let mut format = None;
    let mut sort = false;
    for fix in fixes {
        match fix {
            Fix::Rename { entry, key } => {
                if let Some((_, existing)) = renames.iter().find(|(id, _)| id == entry) {
                    if existing != key {
                        return Err(LintError::Conflict(
                            "one entry has two different replacement keys",
                        ));
                    }
                    continue;
                }
                renames.push((*entry, key.clone()));
            }
            Fix::Format(options) => {
                if format.is_some_and(|existing: FormatOptions| existing != *options) {
                    return Err(LintError::Conflict(
                        "different format policies cannot be combined",
                    ));
                }
                format = Some(*options);
            }
            Fix::Sort => sort = true,
        }
    }
    if !renames.is_empty() && (format.is_some() || sort) {
        return Err(LintError::Conflict(
            "renames cannot be combined with formatting or sorting",
        ));
    }
    if sort && format.is_some() {
        return Err(LintError::Conflict(
            "sorting and formatting cannot be combined in one batch",
        ));
    }
    if !renames.is_empty() {
        let edits: Vec<Edit<'_>> = renames
            .iter()
            .map(|(entry, key)| Edit::Rename { entry: *entry, key })
            .collect();
        return document
            .edit(&edits)
            .map(FixResult::Edits)
            .map_err(LintError::Edit);
    }
    if let Some(options) = format {
        return document
            .format(&options)
            .map(FixResult::Format)
            .map_err(LintError::Format);
    }
    if sort {
        return document
            .sort()
            .map(FixResult::Format)
            .map_err(LintError::Format);
    }
    document
        .edit(&[])
        .map(FixResult::Edits)
        .map_err(LintError::Edit)
}

/// Runs the configured lint rules over `document`.
///
/// Findings are ordered by source position. A finding is policy, not a parse
/// error; parse diagnostics remain available through [`adotenv::parse`].
#[must_use]
pub fn lint(document: &Document<'_>, options: LintOptions) -> Vec<Finding> {
    let line_index = document.line_index();
    let mut findings = Vec::new();

    if options.duplicate_keys {
        let mut entries: BTreeMap<&str, Vec<adotenv::Entry<'_>>> = BTreeMap::new();
        for entry in document.entries() {
            entries.entry(entry.key()).or_default().push(entry);
        }
        for group in entries.values().filter(|group| group.len() > 1) {
            for entry in group.iter().skip(1) {
                findings.push(Finding::new(
                    Rule::DuplicateKey,
                    "key is declared more than once",
                    options.severity_for(Rule::DuplicateKey),
                    entry.key_span(),
                    entry.line(),
                    Some(entry.key()),
                    None,
                ));
            }
        }
    }

    if options.key_order {
        if let Ok(sorted) = document.sort() {
            if let Some(first) = sorted.edits().first() {
                findings.push(Finding::new(
                    Rule::KeyOrder,
                    "declarations are not sorted",
                    options.severity_for(Rule::KeyOrder),
                    first.range.clone(),
                    line_of(&line_index, first.range.start),
                    None,
                    Some(Fix::Sort),
                ));
            }
        }
    }

    for entry in document.entries() {
        if options.key_case != KeyCase::Off {
            let expected = match options.key_case {
                KeyCase::Off => entry.key().to_owned(),
                KeyCase::Lower => entry.key().to_ascii_lowercase(),
                KeyCase::Upper => entry.key().to_ascii_uppercase(),
            };
            if expected != entry.key() {
                findings.push(Finding::new(
                    Rule::KeyCase,
                    "key does not use the configured case",
                    options.severity_for(Rule::KeyCase),
                    entry.key_span(),
                    entry.line(),
                    Some(entry.key()),
                    Some(Fix::Rename {
                        entry: entry.id(),
                        key: expected,
                    }),
                ));
            }
        }

        if let Some(value) = entry.value() {
            if !value.is_empty() {
                let expected = match options.quote_style {
                    QuoteStyle::Double => Quote::Double,
                    QuoteStyle::Single => Quote::Single,
                    QuoteStyle::Unquoted => Quote::None,
                    _ => entry.quote(),
                };
                if entry.quote() != expected {
                    findings.push(Finding::new(
                        Rule::QuoteStyle,
                        "value does not use the configured quote style",
                        options.severity_for(Rule::QuoteStyle),
                        entry.value_span().unwrap_or_else(|| entry.key_span()),
                        entry.line(),
                        Some(entry.key()),
                        Some(Fix::Format(options.format_options())),
                    ));
                }
            }
        }

        if whitespace_differs(document, entry, options) {
            findings.push(Finding::new(
                Rule::Whitespace,
                "assignment spacing or trailing whitespace is not normalized",
                options.severity_for(Rule::Whitespace),
                entry.tokens().key.clone(),
                entry.line(),
                Some(entry.key()),
                Some(Fix::Format(options.format_options())),
            ));
        }
    }

    findings.sort_by_key(|finding| (finding.span.start, finding.rule.as_str()));
    findings
}

fn whitespace_differs(
    document: &Document<'_>,
    entry: adotenv::Entry<'_>,
    options: LintOptions,
) -> bool {
    let tokens = entry.tokens();
    if let Some(_separator) = tokens.eq.as_ref() {
        let before = &document.source()[tokens.space_before_eq.clone()];
        let after = &document.source()[tokens.space_after_eq.clone()];
        match options.equals_spacing {
            EqualsSpacing::None if !before.is_empty() || !after.is_empty() => return true,
            EqualsSpacing::Spaces if before != " " || after != " " => return true,
            _ => {}
        }
    }
    options.trim_trailing_whitespace && !tokens.trailing_space.is_empty()
}

fn line_of(index: &LineIndex<'_>, offset: usize) -> usize {
    index
        .to_position(offset, ColumnEncoding::Utf8)
        .map_or(1, |position| position.line + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adotenv::{parse_strict, parse_with, Dialect};

    #[test]
    fn defaults_report_only_duplicate_keys() {
        let doc = parse_strict("B=2\nA=1\nA=3\n").unwrap();
        let findings = lint(&doc, LintOptions::default());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule(), Rule::DuplicateKey);
        assert_eq!(findings[0].line(), 3);
        assert_eq!(findings[0].key(), Some("A"));
    }

    #[test]
    fn key_order_fix_sorts_through_verified_output() {
        let doc = parse_strict("B=2\nA=1\n").unwrap();
        let findings = lint(&doc, LintOptions::new().with_key_order(true));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule(), Rule::KeyOrder);
        let fixed = apply_fix(&doc, findings[0].fix().unwrap()).unwrap();
        assert_eq!(fixed.as_str(), "A=1\nB=2\n");
    }

    #[test]
    fn key_case_fix_renames_one_declaration() {
        let doc = parse_strict("lower=1\n").unwrap();
        let findings = lint(&doc, LintOptions::new().with_key_case(KeyCase::Upper));
        assert_eq!(findings.len(), 1);
        let fixed = apply_fix(&doc, findings[0].fix().unwrap()).unwrap();
        assert_eq!(fixed.as_str(), "LOWER=1\n");
    }

    #[test]
    fn quote_style_fix_is_verified() {
        let doc = parse_strict("A='one'\n").unwrap();
        let findings = lint(
            &doc,
            LintOptions::new().with_quote_style(QuoteStyle::Double),
        );
        assert_eq!(findings.len(), 1);
        let fixed = apply_fix(&doc, findings[0].fix().unwrap()).unwrap();
        assert_eq!(fixed.as_str(), "A=\"one\"\n");
    }

    #[test]
    fn whitespace_fix_uses_verified_formatting() {
        let doc = parse_strict("A =  one   \n").unwrap();
        let findings = lint(
            &doc,
            LintOptions::new()
                .with_equals_spacing(EqualsSpacing::None)
                .with_trim_trailing_whitespace(true),
        );
        assert_eq!(findings.len(), 1);
        let fixed = apply_fix(&doc, findings[0].fix().unwrap()).unwrap();
        assert_eq!(fixed.as_str(), "A=one\n");
    }

    #[test]
    fn findings_never_contain_values() {
        let doc = parse_strict("TOKEN=secret\nTOKEN=other\n").unwrap();
        let findings = lint(&doc, LintOptions::default());
        let debug = format!("{findings:?}");
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("other"));
        assert!(debug.contains("TOKEN"));
    }

    #[test]
    fn rename_collision_returns_a_structured_error() {
        let doc = parse_strict("A=1\na=2\n").unwrap();
        let findings = lint(&doc, LintOptions::new().with_key_case(KeyCase::Upper));
        let fix = findings
            .iter()
            .find_map(Finding::fix)
            .expect("the lowercase key has a fix");
        assert!(matches!(
            apply_fix(&doc, fix),
            Err(LintError::Edit(EditError::Duplicate(_)))
        ));
    }

    #[test]
    fn python_quoted_keys_keep_their_case_policy() {
        let doc = parse_with("'A B'=1\n", Dialect::PYTHON_DOTENV).0;
        let findings = lint(&doc, LintOptions::new().with_key_case(KeyCase::Upper));
        assert_eq!(findings.len(), 0);
    }
}

#[cfg(test)]
use adotenv::parse_strict;

#[test]
fn apply_fixes_batches_independent_renames() {
    let doc = parse_strict("a=1\nb=2\n").unwrap();
    let findings = lint(&doc, LintOptions::new().with_key_case(KeyCase::Upper));
    let fixes: Vec<Fix> = findings
        .iter()
        .filter_map(|finding| finding.fix().cloned())
        .collect();
    assert_eq!(fixes.len(), 2);
    let result = apply_fixes(&doc, &fixes).unwrap();
    assert_eq!(result.as_str(), "A=1\nB=2\n");
}

#[test]
fn apply_fixes_reports_mixed_fix_conflicts() {
    let doc = parse_strict("A='one'\n").unwrap();
    let fixes = [
        Fix::Rename {
            entry: 0,
            key: "B".to_owned(),
        },
        Fix::Format(FormatOptions::default().with_quote_style(QuoteStyle::Double)),
    ];
    assert!(matches!(
        apply_fixes(&doc, &fixes),
        Err(LintError::Conflict(
            "renames cannot be combined with formatting or sorting"
        ))
    ));
}

#[test]
fn apply_fixes_reports_two_keys_for_one_entry() {
    let doc = parse_strict("a=1\n").unwrap();
    let fixes = [
        Fix::Rename {
            entry: 0,
            key: "A".to_owned(),
        },
        Fix::Rename {
            entry: 0,
            key: "B".to_owned(),
        },
    ];
    assert!(matches!(
        apply_fixes(&doc, &fixes),
        Err(LintError::Conflict(
            "one entry has two different replacement keys"
        ))
    ));
}

#[test]
fn rule_severity_is_configurable() {
    let doc = parse_strict("A=1\nA=2\n").unwrap();
    let findings = lint(
        &doc,
        LintOptions::new().with_severity(Rule::DuplicateKey, Severity::Error),
    );
    assert_eq!(findings[0].severity(), Severity::Error);
}

#[test]
fn apply_batch_combines_renames_sorting_and_formatting() {
    let doc = parse_strict("b='two'\na='one'\n").unwrap();
    let findings = lint(
        &doc,
        LintOptions::new()
            .with_key_case(KeyCase::Upper)
            .with_key_order(true)
            .with_quote_style(QuoteStyle::Double),
    );
    let fixes: Vec<Fix> = findings
        .iter()
        .filter_map(|finding| finding.fix().cloned())
        .collect();
    let result = apply_batch(&doc, &fixes).unwrap();
    assert_eq!(result.output(), "A=\"one\"\nB=\"two\"\n");
    assert_eq!(result.stages().len(), 3);
}
