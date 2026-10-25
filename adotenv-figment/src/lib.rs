//! Figment provider for literal adotenv documents and loaded environments.
//!
//! Values are exposed as a flat `Profile::Default` dictionary with their
//! original key spelling. Figment does not perform scalar coercion from
//! strings; deserialize string fields or convert values explicitly. Duplicate
//! policy and lookup participation follow the wrapped source. With the `load`
//! feature the provider also accepts a `LoadedEnvironment`.

use std::collections::BTreeMap;

#[cfg(feature = "load")]
use adotenv::load::LoadedEnvironment;
use adotenv::Document;
use figment::{
    value::{Dict, Map, Value},
    Metadata, Profile, Provider,
};

/// A Figment provider containing the adapter's effective string values.
#[derive(Debug, Clone)]
pub struct AdotenvProvider {
    values: BTreeMap<String, String>,
    source: &'static str,
}

impl Default for AdotenvProvider {
    fn default() -> Self {
        Self {
            values: BTreeMap::new(),
            source: "empty",
        }
    }
}

impl AdotenvProvider {
    /// Build a provider from an explicit key/value map.
    #[must_use]
    pub fn from_values(values: BTreeMap<String, String>) -> Self {
        Self {
            values,
            source: "explicit values",
        }
    }

    /// Build a provider from final values in a loaded environment.
    ///
    /// Available only with the `load` feature.
    #[cfg(feature = "load")]
    #[must_use]
    pub fn from_environment(environment: &LoadedEnvironment) -> Self {
        Self {
            values: environment
                .values()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            source: "loaded environment",
        }
    }

    /// Build a provider from a document's effective literal declarations.
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
        Self {
            values,
            source: "literal document",
        }
    }

    /// The effective values exposed by this provider.
    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

impl Provider for AdotenvProvider {
    fn metadata(&self) -> Metadata {
        Metadata::from("adotenv", self.source)
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        let mut dict = Dict::new();
        for (key, value) in &self.values {
            dict.insert(key.clone(), Value::from(value.as_str()));
        }
        let mut profiles = Map::new();
        profiles.insert(Profile::Default, dict);
        Ok(profiles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adotenv::{parse_with, BareKeys, Dialect, Duplicates};
    use figment::Figment;
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Config {
        host: String,
        port: String,
    }

    #[test]
    fn figment_provider_exposes_flat_values() {
        let figment = Figment::new().merge(AdotenvProvider::from_values(BTreeMap::from([
            ("host".to_string(), "localhost".to_string()),
            ("port".to_string(), "8080".to_string()),
        ])));
        let config: Config = figment.extract().unwrap();
        assert_eq!(
            config,
            Config {
                host: "localhost".to_string(),
                port: "8080".to_string(),
            }
        );
    }

    #[test]
    fn figment_preserves_literal_key_spelling() {
        let values: HashMap<String, String> = Figment::new()
            .merge(AdotenvProvider::from_values(BTreeMap::from([
                ("A.B".to_string(), "dotted".to_string()),
                ("UPPER".to_string(), "upper".to_string()),
            ])))
            .extract()
            .unwrap();
        assert_eq!(values.get("A.B").map(String::as_str), Some("dotted"));
        assert_eq!(values.get("UPPER").map(String::as_str), Some("upper"));
    }

    #[test]
    fn document_provider_honors_duplicate_and_quote_rules() {
        let (document, _) = parse_with(
            "A='first'\nA=second\nB=\"two words\"\nC='$HOME'\nD=\"line\\nbreak\"\n",
            Dialect::PORTABLE,
        );
        let provider = AdotenvProvider::from_document(&document);
        assert_eq!(
            provider.values().get("A").map(String::as_str),
            Some("second")
        );
        assert_eq!(
            provider.values().get("B").map(String::as_str),
            Some("two words")
        );
        assert_eq!(
            provider.values().get("C").map(String::as_str),
            Some("$HOME")
        );
        assert_eq!(
            provider.values().get("D").map(String::as_str),
            Some("line\nbreak")
        );

        let first = Dialect::PORTABLE.with_duplicates(Duplicates::First);
        let (document, _) = parse_with("A='first'\nA=second\n", first);
        let provider = AdotenvProvider::from_document(&document);
        assert_eq!(
            provider.values().get("A").map(String::as_str),
            Some("first")
        );
    }

    #[test]
    fn document_provider_honors_bare_and_nonparticipating_rules() {
        let empty = Dialect::PORTABLE.with_bare_keys(BareKeys::Empty);
        let (document, _) = parse_with("A=first\nA\nB\n", empty);
        let provider = AdotenvProvider::from_document(&document);
        assert_eq!(provider.values().get("A").map(String::as_str), Some(""));
        assert_eq!(provider.values().get("B").map(String::as_str), Some(""));

        let ignore = Dialect::PORTABLE.with_bare_keys(BareKeys::Ignore);
        let (document, _) = parse_with("A=first\nA\nB\n", ignore);
        let provider = AdotenvProvider::from_document(&document);
        assert_eq!(
            provider.values().get("A").map(String::as_str),
            Some("first")
        );
        assert!(!provider.values().contains_key("B"));
    }

    #[cfg(feature = "load")]
    #[test]
    fn environment_provider_copies_final_values() {
        let path = std::env::temp_dir().join(format!(
            "adotenv-figment-environment-{}.env",
            std::process::id()
        ));
        std::fs::write(&path, "A=from-file\nA=final\nB=two\n").unwrap();
        let environment = adotenv::load::load_files([&path], Dialect::PORTABLE).unwrap();
        let provider = AdotenvProvider::from_environment(&environment);
        assert_eq!(
            provider.values().get("A").map(String::as_str),
            Some("final")
        );
        assert_eq!(provider.values().get("B").map(String::as_str), Some("two"));
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod typed_tests {
    use super::*;
    use figment::Figment;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct NumericConfig {
        port: u16,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct StringConfig {
        port: String,
    }

    #[test]
    fn numeric_values_require_explicit_conversion() {
        let figment = Figment::new().merge(AdotenvProvider::from_values(BTreeMap::from([(
            "port".to_string(),
            "8080".to_string(),
        )])));

        let direct: Result<NumericConfig, _> = figment.extract();
        assert!(direct.is_err());

        let raw: StringConfig = figment.extract().unwrap();
        assert_eq!(raw.port.parse::<u16>().unwrap(), 8080);
    }
}
