use alloc::borrow::ToOwned;

use crate::{
    parse_owned_with_options, parse_strict_with_options, parse_values_with_options,
    parse_with_options, Diagnostic, DiagnosticCode, Dialect, ItemKind, LimitKind, ParseOptions,
    Severity,
};

fn limit(diagnostic: &Diagnostic) -> crate::LimitExceeded {
    diagnostic.limit().expect("limit diagnostic")
}

#[test]
fn input_byte_limit_is_exact_and_structured() {
    let source = "A=1\n";
    for (limit_bytes, limited) in [
        (source.len() - 1, true),
        (source.len(), false),
        (source.len() + 1, false),
    ] {
        let options = ParseOptions::default().with_max_input_bytes(limit_bytes);
        let (document, diagnostics) = parse_with_options(source, Dialect::PORTABLE, options);
        assert_eq!(document.source(), source);
        assert_eq!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.limit().is_some()),
            limited
        );
        let strict = parse_strict_with_options(source, Dialect::PORTABLE, options);
        let values = parse_values_with_options(source, Dialect::PORTABLE, options);
        if limited {
            let error = strict.unwrap_err();
            let observed = limit(&error);
            assert_eq!(error.code(), DiagnosticCode::LimitExceeded);
            assert_eq!(error.severity(), Severity::Error);
            assert_eq!(observed.kind(), LimitKind::InputBytes);
            assert_eq!(observed.limit(), limit_bytes);
            assert_eq!(observed.observed(), source.len());
            assert_eq!(observed.kind().as_str(), "input-bytes");
            assert!(matches!(values, Err(error) if error.limit().is_some()));
            assert_eq!(document.items().count(), 1);
            assert_eq!(document.items().next().unwrap().kind(), ItemKind::Invalid);
            let (owned, owned_diagnostics) =
                parse_owned_with_options(source.to_owned(), Dialect::PORTABLE, options);
            assert_eq!(owned.source(), source);
            assert!(owned_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.limit().is_some()));
        } else {
            assert!(strict.is_ok());
            assert!(values.is_ok());
            assert!(!diagnostics
                .iter()
                .any(|diagnostic| diagnostic.limit().is_some()));
        }
    }
}

#[test]
fn item_limit_is_exact_and_structured() {
    let source = "A=1\nB=2\n";
    for (limit_items, limited) in [(1, true), (2, false), (3, false)] {
        let options = ParseOptions::default().with_max_items(limit_items);
        let (document, diagnostics) = parse_with_options(source, Dialect::PORTABLE, options);
        assert_eq!(document.source(), source);
        let found = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.limit().is_some());
        assert_eq!(found.is_some(), limited);
        let strict = parse_strict_with_options(source, Dialect::PORTABLE, options);
        let values = parse_values_with_options(source, Dialect::PORTABLE, options);
        if limited {
            let observed = limit(found.unwrap());
            assert_eq!(observed.kind(), LimitKind::Items);
            assert_eq!(observed.limit(), limit_items);
            assert_eq!(observed.observed(), 2);
            assert_eq!(
                strict.unwrap_err().limit().unwrap().kind(),
                LimitKind::Items
            );
            assert!(matches!(values, Err(error) if error.limit().is_some()));
            assert_eq!(document.items().last().unwrap().kind(), ItemKind::Invalid);
        } else {
            assert!(strict.is_ok());
            assert!(values.is_ok());
        }
    }
}

#[test]
fn diagnostic_limit_is_exact_and_structured() {
    let source = "A=1\nA=2\nA=3\n";
    for (limit_count, limited, expected_len) in [(1, true, 2), (2, false, 2), (3, false, 2)] {
        let options = ParseOptions::default().with_max_diagnostics(limit_count);
        let (_, diagnostics) = parse_with_options(source, Dialect::PORTABLE, options);
        assert_eq!(diagnostics.len(), expected_len);
        let found = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.limit().is_some());
        assert_eq!(found.is_some(), limited);
        if limited {
            let observed = limit(found.unwrap());
            assert_eq!(observed.kind(), LimitKind::Diagnostics);
            assert_eq!(observed.limit(), 1);
            assert_eq!(observed.observed(), 2);
            assert_eq!(found.unwrap().severity(), Severity::Error);
        }
    }
}

#[test]
fn zero_limits_report_a_failure() {
    let item_options = ParseOptions::default().with_max_items(0);
    let (_, diagnostics) = parse_with_options("A=1\n", Dialect::PORTABLE, item_options);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .limit()
            .is_some_and(|limit| limit.kind() == LimitKind::Items)
    }));

    let diagnostic_options = ParseOptions::default().with_max_diagnostics(0);
    let (_, diagnostics) = parse_with_options("A=1\nA=2\n", Dialect::PORTABLE, diagnostic_options);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .limit()
            .is_some_and(|limit| limit.kind() == LimitKind::Diagnostics)
    }));
}

#[test]
fn limits_are_a_no_op_when_not_configured() {
    let (document, diagnostics) =
        parse_with_options("A=1\n", Dialect::PORTABLE, ParseOptions::default());
    assert_eq!(diagnostics.len(), 0);
    assert_eq!(document.get("A"), Some("1"));
}

#[cfg(feature = "load")]
#[test]
fn load_read_and_parse_limits_are_structured() {
    use crate::load::{
        load_cascade_values_with_options, load_cascade_with_options, LoadError, LoadOptions,
    };
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "adotenv-limit-load-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join(".env"), "A=1\nB=2\n").unwrap();

    let read = LoadOptions::new(Dialect::PORTABLE)
        .with_limits(ParseOptions::default().with_max_input_bytes(3));
    match load_cascade_with_options(&directory, "", read) {
        Err(LoadError::Limit { path, limit }) => {
            assert_eq!(path, directory.join(".env"));
            assert_eq!(limit.kind(), LimitKind::InputBytes);
            assert_eq!(limit.limit(), 3);
        }
        other => panic!("expected a read limit, got {other:?}"),
    }

    let items =
        LoadOptions::new(Dialect::PORTABLE).with_limits(ParseOptions::default().with_max_items(1));
    match load_cascade_with_options(&directory, "", items) {
        Err(LoadError::Limit { limit, .. }) => {
            assert_eq!(limit.kind(), LimitKind::Items);
        }
        other => panic!("expected an item limit, got {other:?}"),
    }

    let values = load_cascade_values_with_options(
        &directory,
        "",
        LoadOptions::new(Dialect::PORTABLE)
            .with_limits(ParseOptions::default().with_max_input_bytes(3)),
    );
    assert!(matches!(
        values,
        Err(LoadError::Limit { limit, .. }) if limit.kind() == LimitKind::InputBytes
    ));

    fs::remove_dir_all(&directory).unwrap();
}
