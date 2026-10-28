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

#[derive(Debug, Deserialize, PartialEq)]
struct Primitives {
    i8: i8,
    i16: i16,
    i32: i32,
    i64: i64,
    i128: i128,
    u8: u8,
    u16: u16,
    u32: u32,
    u64: u64,
    u128: u128,
    f32: f32,
    f64: f64,
    boolean: bool,
    character: char,
    unit: (),
    option: Option<String>,
    newtype: Newtype,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Newtype(u16);

#[test]
fn every_supported_scalar_shape_deserializes() {
    let source = "i8=-8\ni16=-16\ni32=-32\ni64=-64\ni128=-128\nu8=8\nu16=16\nu32=32\nu64=64\nu128=128\nf32=1.5\nf64=-2.5\nboolean=true\ncharacter=é\nunit=\noption=present\nnewtype=7\n";
    let doc = parse_strict(source).unwrap();
    let values: Primitives = doc.deserialize().unwrap();
    assert_eq!(
        values,
        Primitives {
            i8: -8,
            i16: -16,
            i32: -32,
            i64: -64,
            i128: -128,
            u8: 8,
            u16: 16,
            u32: 32,
            u64: 64,
            u128: 128,
            f32: 1.5,
            f64: -2.5,
            boolean: true,
            character: 'é',
            unit: (),
            option: Some("present".to_string()),
            newtype: Newtype(7),
        }
    );
}

#[test]
fn invalid_scalars_and_non_scalar_shapes_keep_value_context() {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct InvalidBool {
        value: bool,
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct InvalidChar {
        value: char,
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct InvalidUnit {
        value: (),
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct NestedMap {
        value: alloc::collections::BTreeMap<String, String>,
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct NestedStruct {
        value: Config,
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct NestedTuple {
        value: (String, String),
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct NestedTupleStruct {
        value: Pair,
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct Pair(String, String);
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    enum Kind {
        One,
    }
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct NestedEnum {
        value: Kind,
    }

    let cases = [
        ("value=not-a-bool\n", "a boolean"),
        ("value=too-long\n", "one character"),
        ("value=not-empty\n", "an empty unit"),
        ("value=x\n", "not a map"),
        ("value=x\n", "not a struct"),
        ("value=x\n", "not a tuple"),
        ("value=x\n", "not a tuple struct"),
        ("value=One\n", "not an enum"),
    ];
    for (source, expected) in cases {
        let doc = parse_strict(source).unwrap();
        let error = match expected {
            "a boolean" => doc.deserialize::<InvalidBool>().unwrap_err(),
            "one character" => doc.deserialize::<InvalidChar>().unwrap_err(),
            "an empty unit" => doc.deserialize::<InvalidUnit>().unwrap_err(),
            "not a map" => doc.deserialize::<NestedMap>().unwrap_err(),
            "not a struct" => doc.deserialize::<NestedStruct>().unwrap_err(),
            "not a tuple" => doc.deserialize::<NestedTuple>().unwrap_err(),
            "not a tuple struct" => doc.deserialize::<NestedTupleStruct>().unwrap_err(),
            "not an enum" => doc.deserialize::<NestedEnum>().unwrap_err(),
            other => panic!("unexpected case {other}"),
        };
        assert_eq!(error.line(), 1);
        assert_eq!(error.key(), Some("value"));
        assert!(error.message().contains(expected), "{error}");
    }
}

#[test]
fn structural_errors_have_no_line_and_display_without_context() {
    let doc = parse_strict("host=localhost\nport=8080\nempty=\n").unwrap();
    let error = doc.deserialize::<Config>().unwrap_err();
    assert_eq!(error.line(), 0);
    assert_eq!(error.key(), None);
    assert_eq!(error.to_string(), error.message());
}

#[test]
fn value_error_display_includes_line_and_key() {
    let doc = parse_strict("host=localhost\nport=not-a-number\ndebug=true\nempty=\n").unwrap();
    let error = doc.deserialize::<Config>().unwrap_err();
    let display = error.to_string();
    assert!(display.starts_with("line 2: "));
    assert!(display.ends_with(" (key \"port\")"));
}

#[test]
fn duplicate_and_bare_policies_control_serde_membership() {
    use crate::Duplicates;

    let first = Dialect::PORTABLE.with_duplicates(Duplicates::First);
    let (doc, _) = parse_with("present=first\npresent=last\n", first);
    let optional: Optional = doc.deserialize().unwrap();
    assert_eq!(optional.present.as_deref(), Some("first"));

    for bare in [BareKeys::Ignore, BareKeys::Preserve] {
        let dialect = Dialect::PORTABLE.with_bare_keys(bare);
        let (doc, _) = parse_with("host\nport=8080\ndebug=true\nempty=\n", dialect);
        assert!(doc.deserialize::<Config>().is_err());
    }
}

#[test]
fn non_string_keys_report_value_context() {
    let doc = parse_strict("not-a-number=value\n").unwrap();
    let error = doc
        .deserialize::<alloc::collections::BTreeMap<u16, String>>()
        .unwrap_err();
    assert_eq!(error.line(), 1);
    assert_eq!(error.key(), Some(""));
    assert!(error.message().contains("u16"));
}
