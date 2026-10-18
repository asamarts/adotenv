//! Opt-in `serde` deserialization of literal documents.
//!
//! The deserializer exposes effective declarations as a string-valued map.
//! Literal values are parsed into Serde primitives when requested; nested
//! sequences/maps and enum representations are not invented. Errors carry the
//! source line for value conversion failures.

use alloc::{
    collections::BTreeSet,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

use serde::de::{self, DeserializeOwned, Deserializer, MapAccess, Visitor};

use crate::{BareKeys, Document};

/// A serde error tied to the literal document line where possible.
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
    /// One-based source line, or zero when Serde produced a structural error
    /// without a value location.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }
    /// Key associated with the error, when available.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }
    /// Human-readable error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            formatter.write_str(&self.message)
        } else {
            write!(
                formatter,
                "line {}: {}{}",
                self.line,
                self.message,
                self.key
                    .as_ref()
                    .map_or(String::new(), |key| format!(" (key {key:?})"))
            )
        }
    }
}

impl core::error::Error for DeserializeError {}

impl de::Error for DeserializeError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self::new(message.to_string())
    }
}

impl Document<'_> {
    /// Deserialize effective literal declarations into a Serde type.
    ///
    /// Duplicate keys follow the selected dialect; nonparticipating
    /// declarations and empty keys are ignored. Bare declarations are exposed
    /// only when the dialect’s `BareKeys::Empty` policy makes them effective
    /// empty strings. Values remain literal and are never expanded.
    ///
    /// # Errors
    /// Returns [`DeserializeError`] when a value cannot be parsed as the
    /// requested type. Value-conversion errors carry the declaration line.
    ///
    /// # Examples
    ///
    /// ```
    /// use adotenv::parse_strict;
    ///
    /// #[derive(serde::Deserialize)]
    /// struct Config {
    ///     host: String,
    ///     port: u16,
    /// }
    ///
    /// let doc = parse_strict("host=localhost\nport=8080\n").unwrap();
    /// let config: Config = doc.deserialize().unwrap();
    /// assert_eq!(config.host, "localhost");
    /// assert_eq!(config.port, 8080);
    /// ```
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, DeserializeError> {
        T::deserialize(DocumentDeserializer { document: self })
    }
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
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for entry in document.entries() {
        if !entry.participates_in_lookup() || entry.key().is_empty() {
            continue;
        }
        if !seen.insert(entry.key().to_string()) {
            continue;
        }
        let Some(effective) = document.get_entry(entry.key()) else {
            continue;
        };
        let value = if let Some(value) = effective.value() {
            value.to_string()
        } else if document.dialect().bare_keys == BareKeys::Empty {
            String::new()
        } else {
            continue;
        };
        entries.push((effective.key().to_string(), value, effective.line()));
    }
    entries
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
        seed.deserialize(ValueDeserializer { key, value, line })
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
