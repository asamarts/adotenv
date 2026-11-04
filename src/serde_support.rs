//! Serde deserialization of documents and loaded environments, behind the
//! `serde` feature.
//!
//! The effective declarations are presented as a map from key to literal
//! value. A value converts to a scalar type when the target asks for one;
//! sequences, nested maps and structs, tuples and enums are rejected. A value
//! or key that does not convert reports the line of its declaration.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

use serde::de::{self, DeserializeOwned, Deserializer, MapAccess, Visitor};

#[cfg(feature = "load")]
use crate::load::LoadedEnvironment;
use crate::{Document, Mask, REDACTED};

/// An error from deserializing a [`Document`] or loaded environment with Serde.
///
/// Returned by [`Document::deserialize`] and, with the `load` feature,
/// `LoadedEnvironment::deserialize`. [`line`](Self::line) and
/// [`key`](Self::key) locate the problem:
///
/// - A value that does not convert has the one-based line of its declaration
///   and its key. A value consumed through `#[serde(flatten)]` is decoded
///   after map access; serde then reports line `0` and no key for that value.
/// - A key that does not convert, such as an unknown field under
///   `#[serde(deny_unknown_fields)]`, has the line of its declaration and an
///   empty key, `Some("")`. Keys are always passed as strings, so any key read
///   into a map with non-string keys, such as `BTreeMap<u16, String>`, fails
///   this way, even `8080`.
/// - An error about the whole map, such as a missing field, has line `0` and
///   no key.
/// - From a loaded environment loaded without origins, a value error has
///   line `0` but still names the key.
///
/// Its `Display` is `line {line}: {message} (key "{key}")`, without the
/// `line` part when the line is `0` and without the `key` part when there is
/// no key. Messages include the
/// value that failed to convert, such as `expected a u16, found "abc"`, so
/// treat them as secret when the values are secret.
///
/// # Examples
///
/// ```
/// use adotenv::parse_strict;
///
/// #[derive(Debug, serde::Deserialize)]
/// #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
/// struct Config {
///     host: String,
///     port: u16,
/// }
///
/// let doc = parse_strict("HOST=localhost\nPORT=abc\n")?;
/// let error = doc.deserialize::<Config>().unwrap_err();
/// assert_eq!(error.line(), 2);
/// assert_eq!(error.key(), Some("PORT"));
/// assert_eq!(error.message(), r#"expected a u16, found "abc""#);
/// assert_eq!(error.to_string(), r#"line 2: expected a u16, found "abc" (key "PORT")"#);
///
/// let missing = parse_strict("HOST=localhost\n")?.deserialize::<Config>().unwrap_err();
/// assert_eq!((missing.line(), missing.key()), (0, None));
/// assert_eq!(missing.message(), "missing field `PORT`");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeserializeError {
    line: usize,
    key: Option<String>,
    message: String,
}

impl DeserializeError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            line: 0,
            key: None,
            message: message.into(),
        }
    }
    fn at(line: usize, key: String, message: impl Into<String>) -> Self {
        Self {
            line,
            key: Some(key),
            message: message.into(),
        }
    }
    fn with_context(mut self, line: usize, key: String) -> Self {
        if self.line == 0 {
            self.line = line;
            self.key = Some(key);
        }
        self
    }
    /// Returns a form that masks the value-bearing message.
    ///
    /// The line and key are kept. Use this when the values may be secret and
    /// the error is logged or displayed.
    ///
    /// ```
    /// use adotenv::parse_strict;
    ///
    /// # #[allow(dead_code)]
    /// # #[derive(Debug, serde::Deserialize)]
    /// # #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    /// # struct Config { port: u16 }
    /// let doc = parse_strict("PORT=abc\n")?;
    /// let error = doc.deserialize::<Config>().unwrap_err();
    /// assert!(error.to_string().contains("abc"));
    /// assert!(!error.masked().to_string().contains("abc"));
    /// assert_eq!(error.masked().to_string(), "line 1: <redacted> (key \"PORT\")");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub const fn masked(&self) -> MaskedDeserializeError<'_> {
        MaskedDeserializeError(self)
    }
    /// Returns the one-based line of the declaration, or `0` when it is
    /// unknown.
    ///
    /// The line is `0` for an error about the whole map, such as a missing
    /// field, for a value consumed through `#[serde(flatten)]`, and for any
    /// error from a loaded environment loaded without origins.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }
    /// Returns the key of the declaration whose value did not convert.
    ///
    /// Returns `Some("")` when the key itself did not convert, and `None` for
    /// an error about the whole map or a value consumed through
    /// `#[serde(flatten)]`.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }
    /// Returns the error message, without the line and key.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line != 0 {
            write!(formatter, "line {}: ", self.line)?;
        }
        formatter.write_str(&self.message)?;
        match &self.key {
            Some(key) => write!(formatter, " (key {key:?})"),
            None => Ok(()),
        }
    }
}

impl core::error::Error for DeserializeError {}

/// A [`DeserializeError`] with its value-bearing message masked.
///
/// Returned by [`DeserializeError::masked`]. Its [`Debug`](fmt::Debug) and
/// [`Display`](fmt::Display) forms keep the line and key but replace the
/// message with `<redacted>`.
#[derive(Clone, Copy)]
pub struct MaskedDeserializeError<'a>(&'a DeserializeError);

impl fmt::Debug for MaskedDeserializeError<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt_masked(formatter)
    }
}

impl fmt::Display for MaskedDeserializeError<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.line != 0 {
            write!(formatter, "line {}: ", self.0.line)?;
        }
        formatter.write_str(REDACTED)?;
        match &self.0.key {
            Some(key) => write!(formatter, " (key {key:?})"),
            None => Ok(()),
        }
    }
}

impl Mask for DeserializeError {
    fn fmt_masked(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeserializeError")
            .field("line", &self.line)
            .field("key", &self.key)
            .field("message", &REDACTED)
            .finish()
    }
}

impl de::Error for DeserializeError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self::new(message.to_string())
    }
}

impl Document<'_> {
    /// Deserializes the effective declarations into a Serde type.
    ///
    /// The document is read as a map from each key to its effective value, the
    /// value that [`Document::get`] returns. Duplicate keys follow the
    /// dialect's rule. Declarations that do not take part in lookup and empty
    /// keys are left out, and a bare key appears, as an empty string, only
    /// under [`BareKeys::Empty`](crate::BareKeys::Empty). Values stay literal:
    /// `${HOST}` is the text `${HOST}`. Keys the target type does not name are
    /// ignored unless it uses `#[serde(deny_unknown_fields)]`.
    ///
    /// The top level must be a struct or a map. Each value converts like this:
    ///
    /// | Target type | Accepted values |
    /// |---|---|
    /// | `String` | Any value |
    /// | `bool` | Exactly `true` or `false` |
    /// | Integers and floats | Rust's `FromStr` syntax for the type |
    /// | `char` | Exactly one character |
    /// | `Option<T>` | `None` when the key is absent, otherwise `Some` of the converted value |
    /// | `()` and unit structs | An empty value |
    /// | Newtype structs | The rule of the inner type |
    /// | Bytes, as requested by `serde_bytes::ByteBuf` | The UTF-8 bytes of the value |
    ///
    /// An empty value is still present: `PORT=` read as `Option<u16>` is an
    /// error, not `None`. Sequences, nested maps and structs, tuples and enums
    /// are rejected as values, even enums with only unit variants and
    /// `Vec<u8>`, which Serde reads as a sequence; deserialize a `String` and
    /// convert it yourself when a field needs another shape.
    ///
    /// Map keys are passed as strings, so a map with non-string keys, such as
    /// `BTreeMap<u16, String>`, fails. Fields inside `#[serde(flatten)]` also
    /// receive every value as a string, so only fields that accept a string,
    /// such as `String`, work there.
    ///
    /// # Errors
    ///
    /// Returns a [`DeserializeError`] if a value or key does not convert, or if
    /// the target type rejects the map, for example because a required field
    /// is missing. A value error names the key and the line of its declaration,
    /// except for a value consumed through `#[serde(flatten)]`: serde decodes
    /// those after map access and reports line `0` and no key.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::parse_strict;
    ///
    /// #[derive(serde::Deserialize)]
    /// #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    /// struct Config {
    ///     host: String,
    ///     port: u16,
    ///     debug: Option<bool>,
    /// }
    ///
    /// let doc = parse_strict("# web server\nHOST=localhost\nPORT=8080\n")?;
    /// let config: Config = doc.deserialize()?;
    /// assert_eq!(config.host, "localhost");
    /// assert_eq!(config.port, 8080);
    /// assert_eq!(config.debug, None);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, DeserializeError> {
        T::deserialize(DocumentDeserializer { document: self })
    }
}

#[cfg(feature = "load")]
impl LoadedEnvironment {
    /// Deserializes the final values into a Serde type.
    ///
    /// Needs the `load` and `serde` features. The map holds the values that
    /// [`LoadedEnvironment::get`] returns, after the cascade order and the
    /// dialect's lookup rules. They are literal, and nothing is read from the
    /// process environment. Keys and values convert with the same rules as
    /// [`Document::deserialize`].
    ///
    /// A value that does not convert reports the one-based line of its winning
    /// declaration, but not the file; pass [`DeserializeError::key`] to
    /// [`LoadedEnvironment::origin`] to find it. When the load used
    /// [`LoadOptions::with_origins`](crate::load::LoadOptions::with_origins)
    /// with `false`, the line is `0`.
    ///
    /// # Errors
    ///
    /// Returns a [`DeserializeError`] if a value or key does not convert, or if
    /// the target type rejects the map, for example because a required field
    /// is missing. A value consumed through `#[serde(flatten)]` reports line
    /// `0` and no key.
    ///
    /// # Examples
    ///
    /// With `PORT=3000` and `DATABASE_URL=postgres://localhost/dev` in `.env`,
    /// and `PORT=8080` in `.env.production`:
    ///
    /// ```
    /// use adotenv::load::load_cascade;
    ///
    /// #[derive(serde::Deserialize)]
    /// #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    /// struct Config {
    ///     database_url: String,
    ///     port: u16,
    /// }
    ///
    /// # let dir = std::env::temp_dir().join(format!("adotenv-doc-LoadedEnvironment-deserialize-{}", std::process::id()));
    /// # std::fs::create_dir_all(&dir)?;
    /// # std::fs::write(dir.join(".env"), "PORT=3000\nDATABASE_URL=postgres://localhost/dev\n")?;
    /// # std::fs::write(dir.join(".env.production"), "PORT=8080\n")?;
    /// let loaded = load_cascade(&dir, "production")?;
    /// let config: Config = loaded.deserialize()?;
    /// assert_eq!(config.database_url, "postgres://localhost/dev");
    /// assert_eq!(config.port, 8080);
    /// # std::fs::remove_dir_all(&dir)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, DeserializeError> {
        T::deserialize(LoadedEnvironmentDeserializer { environment: self })
    }
}

#[cfg(feature = "load")]
struct LoadedEnvironmentDeserializer<'env> {
    environment: &'env LoadedEnvironment,
}

#[cfg(feature = "load")]
impl<'de> Deserializer<'de> for LoadedEnvironmentDeserializer<'_> {
    type Error = DeserializeError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_map(EntriesAccess {
            entries: loaded_entries(self.environment).into_iter(),
            current: None,
        })
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

#[cfg(feature = "load")]
fn loaded_entries(environment: &LoadedEnvironment) -> Vec<(String, String, usize)> {
    environment
        .values()
        .map(|(key, value)| {
            let line = environment.origin(key).map_or(0, |origin| origin.line());
            (key.to_string(), value.to_string(), line)
        })
        .collect()
}

struct DocumentDeserializer<'doc, 'src> {
    document: &'doc Document<'src>,
}

impl<'de> Deserializer<'de> for DocumentDeserializer<'_, '_> {
    type Error = DeserializeError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_map(EntriesAccess {
            entries: entries(self.document).into_iter(),
            current: None,
        })
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

fn entries(document: &Document<'_>) -> Vec<(String, String, usize)> {
    document
        .effective_entries()
        .map(|entry| {
            (
                entry.key().to_string(),
                entry.value().to_string(),
                entry.line(),
            )
        })
        .collect()
}

struct EntriesAccess {
    entries: alloc::vec::IntoIter<(String, String, usize)>,
    current: Option<(String, String, usize)>,
}

impl<'de> MapAccess<'de> for EntriesAccess {
    type Error = DeserializeError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some((key, value, line)) = self.entries.next() else {
            return Ok(None);
        };
        self.current = Some((key.clone(), value, line));
        seed.deserialize(KeyDeserializer { key, line }).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let Some((key, value, line)) = self.current.take() else {
            return Err(DeserializeError::new("value requested without a key"));
        };
        // Conversions that run after the value is read, such as
        // `#[serde(try_from = "String")]`, fail outside `ValueDeserializer`.
        let context = key.clone();
        seed.deserialize(ValueDeserializer { key, value, line })
            .map_err(|error| error.with_context(line, context))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}

struct KeyDeserializer {
    key: String,
    line: usize,
}

impl<'de> Deserializer<'de> for KeyDeserializer {
    type Error = DeserializeError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor
            .visit_str(&self.key)
            .map_err(|error: DeserializeError| error.with_context(self.line, String::new()))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

struct ValueDeserializer {
    key: String,
    value: String,
    line: usize,
}

impl ValueDeserializer {
    fn invalid(&self, expected: &str) -> DeserializeError {
        DeserializeError::at(
            self.line,
            self.key.clone(),
            format!("expected {expected}, found {:?}", self.value),
        )
    }

    fn context<T>(&self, result: Result<T, DeserializeError>) -> Result<T, DeserializeError> {
        result.map_err(|error: DeserializeError| error.with_context(self.line, self.key.clone()))
    }
}

macro_rules! deserialize_number {
    ($method:ident, $visit:ident, $ty:ty, $name:literal) => {
        fn $method<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            let value = self.value.parse::<$ty>().map_err(|_| self.invalid($name))?;
            let result = visitor.$visit(value);
            self.context(result)
        }
    };
}

impl<'de> Deserializer<'de> for ValueDeserializer {
    type Error = DeserializeError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor
            .visit_str(&self.value)
            .map_err(|error: DeserializeError| error.with_context(self.line, self.key.clone()))
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .parse::<bool>()
            .map_err(|_| self.invalid("a boolean"))?;
        let result = visitor.visit_bool(value);
        self.context(result)
    }

    deserialize_number!(deserialize_i8, visit_i8, i8, "an i8");
    deserialize_number!(deserialize_i16, visit_i16, i16, "an i16");
    deserialize_number!(deserialize_i32, visit_i32, i32, "an i32");
    deserialize_number!(deserialize_i64, visit_i64, i64, "an i64");
    deserialize_number!(deserialize_i128, visit_i128, i128, "an i128");
    deserialize_number!(deserialize_u8, visit_u8, u8, "a u8");
    deserialize_number!(deserialize_u16, visit_u16, u16, "a u16");
    deserialize_number!(deserialize_u32, visit_u32, u32, "a u32");
    deserialize_number!(deserialize_u64, visit_u64, u64, "a u64");
    deserialize_number!(deserialize_u128, visit_u128, u128, "a u128");
    deserialize_number!(deserialize_f32, visit_f32, f32, "an f32");
    deserialize_number!(deserialize_f64, visit_f64, f64, "an f64");

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut chars = self.value.chars();
        let (Some(character), None) = (chars.next(), chars.next()) else {
            return Err(self.invalid("one character"));
        };
        let result = visitor.visit_char(character);
        self.context(result)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let result = visitor.visit_str(&self.value);
        self.context(result)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let result = visitor.visit_bytes(self.value.as_bytes());
        self.context(result)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let result = visitor.visit_some(self);
        result
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if self.value.is_empty() {
            visitor.visit_unit()
        } else {
            Err(self.invalid("an empty unit"))
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(self.invalid("a scalar value, not a sequence"))
    }

    fn deserialize_tuple<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(self.invalid("a scalar value, not a tuple"))
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(self.invalid("a scalar value, not a tuple struct"))
    }

    fn deserialize_map<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(self.invalid("a scalar value, not a map"))
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(self.invalid("a scalar value, not a struct"))
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(self.invalid("a scalar value, not an enum"))
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}
