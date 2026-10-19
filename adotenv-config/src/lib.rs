//! `config` crate source adapter for literal adotenv documents and loaded
//! environments.
//!
//! The source exposes only effective literal `String` values. Duplicate policy
//! and lookup participation follow the document or environment being wrapped.
//! With the `load` feature it also accepts a [`LoadedEnvironment`].
//!
//! Key names are handed to `config` unchanged; `config` applies its own
//! case-insensitive and dotted-path semantics. See the crate documentation for
//! those native key rules.

use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "load")]
use adotenv::load::LoadedEnvironment;
use adotenv::{BareKeys, Document};
use config::{Map, Source, Value};

/// A `config` source containing the adapter's effective string values.
#[derive(Debug, Clone, Default)]
pub struct AdotenvSource {
    values: BTreeMap<String, String>,
}

impl AdotenvSource {
    /// Build a source from an explicit key/value map.
    #[must_use]
    pub fn from_values(values: BTreeMap<String, String>) -> Self {
        Self { values }
    }

    /// Build a source from final values in a loaded environment.
    ///
    /// Available only with the `load` feature.
    #[cfg(feature = "load")]
    #[must_use]
    pub fn from_environment(environment: &LoadedEnvironment) -> Self {
        Self {
            values: environment.values().clone(),
        }
    }

    /// Build a source from a document's effective literal declarations.
    ///
    /// Duplicate keys follow the document's dialect. Nonparticipating and empty
    /// keys are omitted; bare declarations contribute `""` only under
    /// `BareKeys::Empty`.
    #[must_use]
    pub fn from_document(document: &Document<'_>) -> Self {
        let mut values = BTreeMap::new();
        let mut seen = BTreeSet::new();
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
            values.insert(effective.key().to_string(), value);
        }
        Self { values }
    }

    /// The effective values exposed by this source.
    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

impl Source for AdotenvSource {
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new(self.clone())
    }

    fn collect(&self) -> Result<Map<String, Value>, config::ConfigError> {
        let mut map = Map::new();
        for (key, value) in &self.values {
            map.insert(key.clone(), Value::new(None, value.as_str()));
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adotenv::{parse_with, Dialect, Duplicates};
    use config::Config;

    #[test]
    fn config_source_exposes_flat_strings() {
        let config = Config::builder()
            .add_source(AdotenvSource::from_values(BTreeMap::from([
                ("host".to_string(), "localhost".to_string()),
                ("port".to_string(), "8080".to_string()),
            ])))
            .build()
            .unwrap();
        assert_eq!(config.get_string("host").unwrap(), "localhost");
        assert_eq!(config.get_int("port").unwrap(), 8080);
    }

    #[test]
    fn config_uses_native_path_and_case_semantics() {
        let config = Config::builder()
            .add_source(AdotenvSource::from_values(BTreeMap::from([
                ("A.B".to_string(), "dotted".to_string()),
                ("UPPER".to_string(), "upper".to_string()),
            ])))
            .build()
            .unwrap();
        assert_eq!(config.get_string("a.b").unwrap(), "dotted");
        assert_eq!(config.get_string("A.B").unwrap(), "dotted");
        assert_eq!(config.get_string("upper").unwrap(), "upper");
        assert_eq!(config.get_string("UPPER").unwrap(), "upper");
    }

    #[test]
    fn document_source_honors_duplicate_and_quote_rules() {
        let (document, _) = parse_with(
            "A='first'\nA=second\nB=\"two words\"\nC='$HOME'\nD=\"line\\nbreak\"\n",
            Dialect::PORTABLE,
        );
        let source = AdotenvSource::from_document(&document);
        assert_eq!(source.values().get("A").map(String::as_str), Some("second"));
        assert_eq!(
            source.values().get("B").map(String::as_str),
            Some("two words")
        );
        assert_eq!(source.values().get("C").map(String::as_str), Some("$HOME"));
        assert_eq!(
            source.values().get("D").map(String::as_str),
            Some("line\nbreak")
        );

        let first = Dialect {
            duplicates: Duplicates::First,
            ..Dialect::PORTABLE
        };
        let (document, _) = parse_with("A='first'\nA=second\n", first);
        let source = AdotenvSource::from_document(&document);
        assert_eq!(source.values().get("A").map(String::as_str), Some("first"));
    }

    #[test]
    fn document_source_honors_bare_and_nonparticipating_rules() {
        let empty = Dialect {
            bare_keys: BareKeys::Empty,
            ..Dialect::PORTABLE
        };
        let (document, _) = parse_with("A=first\nA\nB\n", empty);
        let source = AdotenvSource::from_document(&document);
        assert_eq!(source.values().get("A").map(String::as_str), Some(""));
        assert_eq!(source.values().get("B").map(String::as_str), Some(""));

        let ignore = Dialect {
            bare_keys: BareKeys::Ignore,
            ..Dialect::PORTABLE
        };
        let (document, _) = parse_with("A=first\nA\nB\n", ignore);
        let source = AdotenvSource::from_document(&document);
        assert_eq!(source.values().get("A").map(String::as_str), Some("first"));
        assert!(!source.values().contains_key("B"));
    }

    #[cfg(feature = "load")]
    #[test]
    fn environment_source_copies_final_values() {
        let path = std::env::temp_dir().join(format!(
            "adotenv-config-environment-{}.env",
            std::process::id()
        ));
        std::fs::write(&path, "A=from-file\nA=final\nB=two\n").unwrap();
        let environment = adotenv::load::load_files([&path], Dialect::PORTABLE).unwrap();
        let source = AdotenvSource::from_environment(&environment);
        assert_eq!(source.values().get("A").map(String::as_str), Some("final"));
        assert_eq!(source.values().get("B").map(String::as_str), Some("two"));
        let _ = std::fs::remove_file(path);
    }
}
