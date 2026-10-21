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

use std::collections::BTreeMap;

#[cfg(feature = "load")]
use adotenv::load::LoadedEnvironment;
use adotenv::Document;
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
        let values = document
            .effective_entries()
            .map(|entry| (entry.key().to_string(), entry.value().to_string()))
            .collect();
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
    use adotenv::{parse_with, BareKeys, Dialect, Duplicates};
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

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct TypedConfig {
        host: String,
        port: u16,
    }

    #[test]
    fn typed_deserialization_uses_config_scalar_coercion() {
        let config = Config::builder()
            .add_source(AdotenvSource::from_values(BTreeMap::from([
                ("host".to_string(), "localhost".to_string()),
                ("port".to_string(), "8080".to_string()),
            ])))
            .build()
            .unwrap();
        let typed: TypedConfig = config.try_deserialize().unwrap();
        assert_eq!(
            typed,
            TypedConfig {
                host: "localhost".to_string(),
                port: 8080,
            }
        );
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

        let first = Dialect::PORTABLE.with_duplicates(Duplicates::First);
        let (document, _) = parse_with("A='first'\nA=second\n", first);
        let source = AdotenvSource::from_document(&document);
        assert_eq!(source.values().get("A").map(String::as_str), Some("first"));
    }

    #[test]
    fn document_source_honors_bare_and_nonparticipating_rules() {
        let empty = Dialect::PORTABLE.with_bare_keys(BareKeys::Empty);
        let (document, _) = parse_with("A=first\nA\nB\n", empty);
        let source = AdotenvSource::from_document(&document);
        assert_eq!(source.values().get("A").map(String::as_str), Some(""));
        assert_eq!(source.values().get("B").map(String::as_str), Some(""));

        let ignore = Dialect::PORTABLE.with_bare_keys(BareKeys::Ignore);
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
