//! A tooling-grade `.env` parser: literal values, exact byte spans, and edits
//! that leave the rest of the file untouched.
//!
//! `adotenv` is for programs that *read or rewrite* `.env` files rather than
//! load them into a process: linters, formatters, editors and language
//! servers, secret scanners, migration tools. It never touches the process
//! environment, never does I/O, has no dependencies, and never expands
//! variables: `${HOME}` stays `${HOME}`.
//!
//! ```
//! let text = "# app\nexport PORT=8080\nURL=\"${HOST}/api\" # base\n";
//! let doc = adotenv::parse(text).unwrap();
//!
//! assert_eq!(doc.get("PORT"), Some("8080"));
//! assert_eq!(doc.get("URL"), Some("${HOST}/api")); // literal, never expanded
//!
//! // Every entry knows where it lives in the source.
//! let url = &doc.entries()[1];
//! assert_eq!(url.line(), 3);
//! assert_eq!(&text[url.value_span()], "\"${HOST}/api\"");
//!
//! // Edits splice one value and keep every other byte, comments included.
//! let edited = adotenv::set(text, "URL", "https://example.com").unwrap();
//! assert_eq!(edited, "# app\nexport PORT=8080\nURL=https://example.com # base\n");
//! ```
//!
//! # Syntax
//!
//! - `KEY=VALUE`, one per line. Lines end at LF or CRLF (a lone CR does not
//!   end a line).
//! - Blank lines and lines whose first non-blank character is `#` are ignored.
//! - An optional `export ` prefix (with a following space; `export=x` is a key
//!   named `export`).
//! - The key is the text before the first `=`, trimmed; it must be non-empty
//!   and contain no whitespace. Everything after the first `=` is the value, so
//!   `URL=a?b=c` keeps `a?b=c`.
//! - **Unquoted** values are trimmed. An inline comment starts at ` #` (a space
//!   then `#`); a `#` with no space before it is part of the value.
//! - **Single-quoted** values are fully literal, up to the next `'`.
//! - **Double-quoted** values honor `\n`, `\r`, `\t`, `\\` and `\"`; any other
//!   backslash pair is kept as written.
//! - Text after a closing quote is ignored (typically a comment).
//! - Duplicate keys are all kept as entries; lookups use the last one.
//! - A leading UTF-8 byte-order mark is skipped.
//!
//! Not supported (a parse error, never a silent guess): a line with no `=`, an
//! empty key, a key containing whitespace, an unterminated quote (so no
//! multi-line values), and a trailing backslash inside double quotes.
//!
//! # Spans
//!
//! All spans are byte ranges into the original text, BOM included, so they can
//! be used to slice it directly or to place editor diagnostics.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt;
use std::ops::Range;

/// How a value was quoted in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quote {
    /// `KEY=value`
    None,
    /// `KEY='value'`
    Single,
    /// `KEY="value"`
    Double,
}

/// One `KEY=VALUE` assignment and where it sits in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    key: String,
    value: String,
    quote: Quote,
    exported: bool,
    line: usize,
    line_span: Range<usize>,
    key_span: Range<usize>,
    value_span: Range<usize>,
}

impl Entry {
    /// The key, as written.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The value, unquoted and unescaped but never variable-expanded.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// How the value was quoted.
    #[must_use]
    pub fn quote(&self) -> Quote {
        self.quote
    }

    /// Whether the line had an `export ` prefix.
    #[must_use]
    pub fn exported(&self) -> bool {
        self.exported
    }

    /// The 1-based line number.
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }

    /// The whole line, including its line ending if it has one, but excluding
    /// a file-leading BOM. Deleting this range can change how the remaining
    /// text parses (for example, by exposing another BOM). Use [`remove`] for
    /// a deletion that verifies the remaining entries are unchanged.
    #[must_use]
    pub fn line_span(&self) -> Range<usize> {
        self.line_span.clone()
    }

    /// The key.
    #[must_use]
    pub fn key_span(&self) -> Range<usize> {
        self.key_span.clone()
    }

    /// The value as written: the whole quoted region (quotes included) for a
    /// quoted value, the trimmed token for an unquoted one, and an empty range
    /// right after `=` for an empty value. A trailing comment is outside it.
    /// A replacement token can interact with the surrounding bytes, especially
    /// text after a closing quote. Use [`set`] for a replacement that verifies
    /// the requested value reads back correctly.
    #[must_use]
    pub fn value_span(&self) -> Range<usize> {
        self.value_span.clone()
    }
}

/// A parsed `.env` file: its assignments in source order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    entries: Vec<Entry>,
    bom: bool,
}

impl Document {
    /// Every assignment in source order, duplicates included.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The value of `key`; the last assignment wins.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.key == key)
            .map(Entry::value)
    }

    /// Every assignment of `key`, in source order.
    pub fn entries_for<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Entry> + 'a {
        self.entries.iter().filter(move |e| e.key == key)
    }

    /// The single assignment of `key`. Fails when the key is missing or
    /// assigned more than once, where an edit would have to guess which line
    /// is meant.
    ///
    /// # Errors
    ///
    /// [`EditError::Missing`] or [`EditError::Duplicate`].
    pub fn unique(&self, key: &str) -> Result<&Entry, EditError> {
        let mut found = self.entries.iter().filter(|e| e.key == key);
        let first = found.next().ok_or(EditError::Missing)?;
        match found.count() {
            0 => Ok(first),
            more => Err(EditError::Duplicate(more + 1)),
        }
    }

    /// Whether the text started with a UTF-8 byte-order mark.
    #[must_use]
    pub fn has_bom(&self) -> bool {
        self.bom
    }
}

/// A line that is not valid `.env` syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    line: usize,
    message: String,
}

impl ParseError {
    /// The 1-based line number.
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }

    /// What is wrong, without the line number.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Why [`set`] or [`remove`] could not edit the text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditError {
    /// The text does not parse.
    Parse(ParseError),
    /// The key is not assigned.
    Missing,
    /// The key is assigned this many times, so the edit is ambiguous.
    Duplicate(usize),
    /// The new value cannot be encoded exactly in the existing context.
    Unencodable,
    /// The edit would make the remaining text invalid or change its BOM state
    /// or the ordered keys, values, quoting, or export status of other entries.
    WouldChangeMeaning,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e}"),
            Self::Missing => f.write_str("key is not assigned"),
            Self::Duplicate(n) => write!(f, "key is assigned {n} times"),
            Self::Unencodable => f.write_str("value cannot be encoded exactly in this context"),
            Self::WouldChangeMeaning => {
                f.write_str("edit would change the meaning of the remaining text")
            }
        }
    }
}

impl std::error::Error for EditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ParseError> for EditError {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

const BOM: char = '\u{feff}';

/// Parse `.env` text. See the [crate docs](crate) for the accepted syntax.
///
/// # Errors
///
/// A [`ParseError`] for the first line that is not valid syntax.
pub fn parse(text: &str) -> Result<Document, ParseError> {
    let (body, base) = match text.strip_prefix(BOM) {
        Some(rest) => (rest, BOM.len_utf8()),
        None => (text, 0),
    };
    let mut entries = Vec::new();
    let mut start = base;
    for (i, chunk) in body.split_inclusive('\n').enumerate() {
        let line_span = start..start + chunk.len();
        start += chunk.len();
        let content = chunk
            .strip_suffix('\n')
            .map_or(chunk, |c| c.strip_suffix('\r').unwrap_or(c));
        if let Some(entry) = parse_line(content, i + 1, line_span)? {
            entries.push(entry);
        }
    }
    Ok(Document {
        entries,
        bom: base > 0,
    })
}

fn parse_line(
    content: &str,
    line: usize,
    line_span: Range<usize>,
) -> Result<Option<Entry>, ParseError> {
    let err = |message: String| ParseError { line, message };
    let at = |r: Range<usize>| line_span.start + r.start..line_span.start + r.end;

    let trimmed = content.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let mut pos = content.len() - trimmed.len();
    let exported = trimmed.starts_with("export ");
    if exported {
        let rest = &content[pos + "export ".len()..];
        pos = content.len() - rest.trim_start().len();
    }
    let assignment = &content[pos..];
    let Some(eq) = assignment.find('=') else {
        return Err(err(format!(
            "expected `KEY=VALUE`, found `{}`",
            assignment.trim_end()
        )));
    };
    let key = assignment[..eq].trim_end();
    if key.is_empty() {
        return Err(err("empty key before `=`".to_string()));
    }
    if key.contains(char::is_whitespace) {
        return Err(err(format!("key `{key}` contains whitespace")));
    }
    let raw_start = pos + eq + 1;
    let (value, quote, region) =
        parse_value(&content[raw_start..]).map_err(|m| err(m.to_string()))?;
    Ok(Some(Entry {
        key: key.to_string(),
        value,
        quote,
        exported,
        line,
        key_span: at(pos..pos + key.len()),
        value_span: at(raw_start + region.start..raw_start + region.end),
        line_span,
    }))
}

/// The value after `=`: its decoded text, its quoting, and the byte range
/// within `raw` that [`Entry::value_span`] reports.
fn parse_value(raw: &str) -> Result<(String, Quote, Range<usize>), &'static str> {
    let v = raw.trim_start();
    let lead = raw.len() - v.len();
    if let Some(rest) = v.strip_prefix('\'') {
        let end = rest.find('\'').ok_or("unterminated single-quoted value")?;
        return Ok((rest[..end].to_string(), Quote::Single, lead..lead + end + 2));
    }
    if let Some(rest) = v.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.char_indices();
        loop {
            match chars.next() {
                None => return Err("unterminated double-quoted value"),
                Some((i, '"')) => return Ok((out, Quote::Double, lead..lead + i + 2)),
                Some((_, '\\')) => match chars.next() {
                    Some((_, 'n')) => out.push('\n'),
                    Some((_, 'r')) => out.push('\r'),
                    Some((_, 't')) => out.push('\t'),
                    Some((_, '\\')) => out.push('\\'),
                    Some((_, '"')) => out.push('"'),
                    Some((_, other)) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => return Err("trailing backslash in value"),
                },
                Some((_, c)) => out.push(c),
            }
        }
    }
    // Unquoted: find ` #` on the raw text, before trimming, so `KEY= # note`
    // is an empty value plus a comment rather than the value `# note`.
    let region = &raw[..raw.find(" #").unwrap_or(raw.len())];
    let token = region.trim();
    if token.is_empty() {
        return Ok((String::new(), Quote::None, 0..0));
    }
    let start = region.len() - region.trim_start().len();
    Ok((token.to_string(), Quote::None, start..start + token.len()))
}

/// Write `value` as a complete `.env` value token that parses back to exactly
/// `value` after `=`: bare when that is safe, otherwise double-quoted with
/// `\\`, `\"`, `\n`, `\r` and `\t` escaped. `None` when `value` holds any other
/// control character, which this encoder deliberately declines. Existing
/// surrounding bytes can affect readback; use [`set`] for a verified
/// contextual replacement.
#[must_use]
pub fn encode_value(value: &str) -> Option<String> {
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return None;
    }
    // A leading `#` is quoted too: after `KEY = ` it would start a comment.
    let bare_ok = value == value.trim()
        && !value.contains(" #")
        && !value.starts_with(['\'', '"', '#'])
        && !value.contains(['\n', '\r', '\t']);
    if bare_ok {
        return Some(value.to_string());
    }
    Some(double_quoted(value))
}

fn double_quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Set `key` to `value`, changing only that value's bytes (quoting is chosen
/// by [`encode_value`]; a trailing comment is kept). The key must be assigned
/// exactly once.
///
/// The result is checked: it parses and reads back `value`. Text after a
/// closing quote is ignored by the parser but would join a bare value, so
/// `KEY="old"tail` gets a quoted value.
///
/// # Errors
///
/// [`EditError::Parse`] if `text` does not parse, [`EditError::Missing`] or
/// [`EditError::Duplicate`] unless `key` is assigned exactly once, and
/// [`EditError::Unencodable`] if `value` cannot be written.
pub fn set(text: &str, key: &str, value: &str) -> Result<String, EditError> {
    let doc = parse(text)?;
    let span = doc.unique(key)?.value_span();
    let encoded = encode_value(value).ok_or(EditError::Unencodable)?;
    let out = splice(text, span.clone(), &encoded);
    if parse(&out).is_ok_and(|d| d.get(key) == Some(value)) {
        return Ok(out);
    }
    // Encoding already chose double quotes when the bare form was unsafe.
    // Only try a different candidate if surrounding text defeated a bare one.
    if !encoded.starts_with('"') {
        let out = splice(text, span, &double_quoted(value));
        if parse(&out).is_ok_and(|d| d.get(key) == Some(value)) {
            return Ok(out);
        }
    }
    Err(EditError::Unencodable)
}

/// Remove the line assigning `key`, line ending included. The key must be
/// assigned exactly once. All other bytes, including a leading BOM, are kept.
///
/// The result is checked: it parses with the same BOM state and the same
/// remaining entries in source order, including their quoting and export
/// status. In particular, removal cannot expose an embedded BOM as a new
/// file-leading BOM.
///
/// # Errors
///
/// [`EditError::Parse`] if `text` does not parse, [`EditError::Missing`] or
/// [`EditError::Duplicate`] unless `key` is assigned exactly once, or
/// [`EditError::WouldChangeMeaning`] if deletion would change how the remaining
/// text parses.
pub fn remove(text: &str, key: &str) -> Result<String, EditError> {
    let doc = parse(text)?;
    let span = doc.unique(key)?.line_span();
    let out = splice(text, span, "");
    let after = parse(&out).map_err(|_| EditError::WouldChangeMeaning)?;
    let remaining = doc
        .entries()
        .iter()
        .filter(|e| e.key() != key)
        .map(|e| (e.key(), e.value(), e.quote(), e.exported()));
    let reparsed = after
        .entries()
        .iter()
        .map(|e| (e.key(), e.value(), e.quote(), e.exported()));
    if doc.has_bom() != after.has_bom() || !remaining.eq(reparsed) {
        return Err(EditError::WouldChangeMeaning);
    }
    Ok(out)
}

fn splice(text: &str, span: Range<usize>, with: &str) -> String {
    let mut out = String::with_capacity(text.len() - span.len() + with.len());
    out.push_str(&text[..span.start]);
    out.push_str(with);
    out.push_str(&text[span.end..]);
    out
}

#[cfg(test)]
mod tests;

/// Keeps the README example compiling and passing.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
