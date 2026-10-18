use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use serde::Deserialize;

use crate::{parse_strict, parse_with, BareKeys, Dialect};

#[derive(Debug, Deserialize, PartialEq)]
struct Config {
    host: String,
    port: u16,
    debug: bool,
    empty: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Optional {
    present: Option<String>,
    missing: Option<String>,
}

#[test]
fn deserializes_literal_structs_and_primitives() {
    let doc =
        parse_strict("host=localhost\nport=8080\ndebug=true\nempty=\nunknown=ignored\n").unwrap();
    let config: Config = doc.deserialize().unwrap();
    assert_eq!(
        config,
        Config {
            host: "localhost".to_string(),
            port: 8080,
            debug: true,
            empty: String::new(),
        }
    );
}

#[test]
fn duplicate_policy_and_optional_fields_are_applied() {
    let doc = parse_strict("present=value\npresent=later\n").unwrap();
    let optional: Optional = doc.deserialize().unwrap();
    assert_eq!(optional.present.as_deref(), Some("later"));
    assert_eq!(optional.missing, None);
}

#[test]
fn bare_empty_policy_is_visible_to_serde() {
    let dialect = Dialect {
        bare_keys: BareKeys::Empty,
        ..Dialect::PORTABLE
    };
    let (doc, _) = parse_with("host\nport=8080\ndebug=false\nempty=\n", dialect);
    let config: Config = doc.deserialize().unwrap();
    assert_eq!(config.host, "");
}

#[test]
fn value_errors_carry_the_source_line() {
    let doc = parse_strict("host=localhost\nport=not-a-number\ndebug=true\nempty=\n").unwrap();
    let error = doc.deserialize::<Config>().unwrap_err();
    assert_eq!(error.line(), 2);
    assert_eq!(error.key(), Some("port"));
    assert!(error.message().contains("u16"));
}

#[test]
fn missing_fields_and_bare_declarations_are_errors() {
    let doc = parse_strict("host=localhost\nport=8080\nempty=\n").unwrap();
    assert!(doc.deserialize::<Config>().is_err());

    let doc = parse_strict("host\nport=8080\ndebug=true\nempty=\n").unwrap();
    assert!(doc.deserialize::<Config>().is_err());
}

#[test]
fn quoted_values_are_decoded_before_serde() {
    let doc = parse_strict("host=\"local host\"\nport=8080\ndebug=true\nempty=''\n").unwrap();
    let config: Config = doc.deserialize().unwrap();
    assert_eq!(config.host, "local host");
    assert_eq!(config.empty, "");
}

#[test]
fn unsupported_nested_shapes_report_a_value_error() {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct Nested {
        values: Vec<String>,
    }
    let doc = parse_strict("values=one\n").unwrap();
    let error = doc.deserialize::<Nested>().unwrap_err();
    assert_eq!(error.key(), Some("values"));
    assert!(error.message().contains("sequence"));
}
