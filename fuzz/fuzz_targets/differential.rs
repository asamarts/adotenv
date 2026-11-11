#![no_main]
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use adotenv::{parse_values, parse_with, Dialect, Severity};
use adotenv_bench::competitors::{parse_values as parse_competitor, Competitor};
use libfuzzer_sys::fuzz_target;

/// Keep the differential oracle to the Portable grammar where the pinned
/// in-memory parsers have defined, equivalent semantics.
fn comparable(text: &str) -> bool {
    if text.contains('\r') {
        return false;
    }
    for line in text.split('\n') {
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        if key.is_empty() || value.is_empty() || key.starts_with("export") || key == "__proto__" {
            // The pinned parsers disagree on `export`-prefixed keys and
            // Node-specific object keys; those are outside the literal
            // Portable comparison.
            return false;
        }
        if !key.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic() || byte == b'_' || (index > 0 && byte.is_ascii_digit())
        }) {
            return false;
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/'))
        {
            return false;
        }
    }
    true
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    // Exercise valid, malformed and recovery input without turning parser
    // diagnostics into an upstream-equivalence assertion.
    let (document, diagnostics) = parse_with(text, Dialect::PORTABLE);
    assert_eq!(document.source(), text);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity() == Severity::Error)
    {
        return;
    }

    if !comparable(text) {
        return;
    }

    let ours: BTreeMap<String, String> = parse_values(text).expect("comparable Portable input");
    for competitor in Competitor::ALL
        .into_iter()
        .filter(|competitor| competitor.supports_values())
    {
        let theirs = parse_competitor(competitor, text)
            .unwrap_or_else(|error| panic!("{}: {error}; source={text:?}", competitor.label()));
        assert_eq!(
            ours,
            theirs,
            "{} disagrees; source = {text:?}",
            competitor.label()
        );
    }
});
