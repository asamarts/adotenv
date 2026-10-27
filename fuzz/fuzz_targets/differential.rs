#![no_main]

use std::{collections::BTreeMap, str};

use adotenv::{parse_with, Dialect, Severity};
use libfuzzer_sys::fuzz_target;

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
        if key.is_empty() || value.is_empty() {
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
    let Ok(text) = str::from_utf8(data) else {
        return;
    };
    if !comparable(text) {
        return;
    }

    let (document, diagnostics) = parse_with(text, Dialect::PORTABLE);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity() == Severity::Error)
    {
        return;
    }

    let ours: BTreeMap<&str, &str> = document
        .effective_entries()
        .map(|entry| (entry.key(), entry.value()))
        .collect();
    let theirs = dotenv_compat::parse(text.as_bytes());
    let theirs: BTreeMap<&str, &str> = theirs
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();

    assert_eq!(ours, theirs, "source = {text:?}");
});
