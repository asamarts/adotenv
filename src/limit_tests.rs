use alloc::{borrow::ToOwned, string::String, string::ToString};
use core::fmt::Write as _;

use crate::{
    parse_owned_with_options, parse_strict_with_options, parse_values_compact_with_options,
    parse_values_with_options, parse_with_options, Diagnostic, DiagnosticCode, Dialect, ItemKind,
    LimitKind, LineBreaks, ParseOptions, Severity,
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
fn line_limit_is_exact_across_endings_dialects_and_entry_points() {
    let cases = [
        ("", Dialect::PORTABLE, 0),
        ("A=1", Dialect::PORTABLE, 1),
        ("A=1\n", Dialect::PORTABLE, 1),
        ("A=1\r\n", Dialect::PORTABLE, 1),
        ("A=1\r", Dialect::PORTABLE, 1),
        ("A=1\r", Dialect::COMPOSE, 1),
        ("\n", Dialect::PORTABLE, 1),
        ("\n\n", Dialect::PORTABLE, 2),
        ("A=1\nB=2\n", Dialect::PORTABLE, 2),
        ("A=1\r\nB=2\r\n", Dialect::PORTABLE, 2),
        ("A=1\rB=2", Dialect::PORTABLE, 2),
        ("A=1\rB=2", Dialect::COMPOSE, 1),
        ("A=1\r\r\n", Dialect::PORTABLE, 2),
        ("A=1\r\r\n", Dialect::COMPOSE, 1),
        ("A=\"one\ntwo\nthree\"\n", Dialect::PORTABLE, 3),
        ("# c1\n# c2\n# c3\n# c4\n", Dialect::PORTABLE, 4),
        ("\n\n\n\n", Dialect::PORTABLE, 4),
    ];

    for (source, dialect, expected_lines) in cases {
        // Exact boundary passes across all entry points.
        let ok_options = ParseOptions::new().with_max_lines(expected_lines);
        let (doc, diags) = parse_with_options(source, dialect, ok_options);
        assert_eq!(doc.source(), source);
        assert!(
            !diags.iter().any(|diag| diag.limit().is_some()),
            "expected no limit for {source:?} ({dialect:?}) at {expected_lines}: {diags:?}"
        );
        assert!(parse_strict_with_options(source, dialect, ok_options).is_ok());
        assert!(parse_values_with_options(source, dialect, ok_options).is_ok());
        assert!(parse_values_compact_with_options(source, dialect, ok_options).is_ok());
        let (owned, owned_diags) = parse_owned_with_options(source.to_owned(), dialect, ok_options);
        assert_eq!(owned.source(), source);
        assert!(!owned_diags.iter().any(|diag| diag.limit().is_some()));

        if expected_lines > 0 {
            let cap = expected_lines - 1;
            let fail_options = ParseOptions::new().with_max_lines(cap);
            let (doc, diags) = parse_with_options(source, dialect, fail_options);
            assert_eq!(doc.source(), source);
            let diag = diags
                .iter()
                .find(|diag| diag.limit().is_some())
                .expect("line limit diagnostic");
            let observed = limit(diag);
            assert_eq!(diag.code(), DiagnosticCode::LimitExceeded);
            assert_eq!(diag.severity(), Severity::Error);
            assert_eq!(observed.kind(), LimitKind::Lines);
            assert_eq!(observed.limit(), cap);
            assert_eq!(observed.observed(), expected_lines);
            assert_eq!(doc.items().len(), 1);
            assert_eq!(doc.items().next().unwrap().kind(), ItemKind::Invalid);

            let strict_err = parse_strict_with_options(source, dialect, fail_options).unwrap_err();
            assert_eq!(limit(&strict_err), observed);
            let values_err = parse_values_with_options(source, dialect, fail_options).unwrap_err();
            assert_eq!(limit(&values_err), observed);
            let compact_err =
                parse_values_compact_with_options(source, dialect, fail_options).unwrap_err();
            assert_eq!(limit(&compact_err), observed);
            let (_, owned_diags) =
                parse_owned_with_options(source.to_owned(), dialect, fail_options);
            assert_eq!(limit(&owned_diags[0]), observed);
        }
    }
}

#[test]
fn line_limit_covers_fast_paths_and_universal_diagnostic_lines() {
    // Exercise the >512-byte simple-portable fast path in values and compact parsers.
    let mut big = String::new();
    for index in 0..80 {
        writeln!(big, "KEY_{index:03}=VALUE_{index:03}").unwrap();
    }
    assert!(big.len() > 512);
    assert!(parse_values_with_options(
        &big,
        Dialect::PORTABLE,
        ParseOptions::new().with_max_lines(80),
    )
    .is_ok());
    assert!(parse_values_compact_with_options(
        &big,
        Dialect::PORTABLE,
        ParseOptions::new().with_max_lines(80),
    )
    .is_ok());
    let values_err = parse_values_with_options(
        &big,
        Dialect::PORTABLE,
        ParseOptions::new().with_max_lines(10),
    )
    .unwrap_err();
    assert_eq!(limit(&values_err).kind(), LimitKind::Lines);
    assert_eq!(limit(&values_err).limit(), 10);
    assert_eq!(limit(&values_err).observed(), 11);
    let compact_err = parse_values_compact_with_options(
        &big,
        Dialect::PORTABLE,
        ParseOptions::new().with_max_lines(10),
    )
    .unwrap_err();
    assert_eq!(limit(&compact_err), limit(&values_err));

    // Line-break mode: embedded lone CR under LineBreaks::Lf stays on one physical line,
    // while Diagnostic::line() still follows universal 1-based line numbers.
    let lf_dialect = Dialect::PORTABLE.with_line_breaks(LineBreaks::Lf);
    let err = parse_strict_with_options(
        "A=1\rB=2\nC=3\n",
        lf_dialect,
        ParseOptions::new().with_max_lines(1),
    )
    .unwrap_err();
    assert_eq!(limit(&err).kind(), LimitKind::Lines);
    assert_eq!(limit(&err).observed(), 2);
    assert_eq!(err.line(), 3);
    assert_eq!(err.span(), 8..12);
}

#[test]
fn zero_limits_report_a_failure() {
    let line_options = ParseOptions::default().with_max_lines(0);
    assert!(parse_strict_with_options("", Dialect::PORTABLE, line_options).is_ok());
    let (_, bom_diags) = parse_with_options("\u{feff}", Dialect::PORTABLE, line_options);
    assert!(bom_diags.iter().any(|diagnostic| {
        diagnostic
            .limit()
            .is_some_and(|limit| limit.kind() == LimitKind::Lines && limit.observed() == 1)
    }));

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
    let (_, bom_diag_limit) =
        parse_with_options("\u{feff}A=1\n", Dialect::PORTABLE, diagnostic_options);
    assert!(bom_diag_limit.iter().any(|diagnostic| {
        diagnostic
            .limit()
            .is_some_and(|limit| limit.kind() == LimitKind::Diagnostics && limit.observed() == 1)
    }));
}

#[test]
fn limit_kind_and_exceeded_format_clearly() {
    assert_eq!(LimitKind::InputBytes.as_str(), "input-bytes");
    assert_eq!(LimitKind::OutputBytes.as_str(), "output-bytes");
    assert_eq!(LimitKind::Lines.as_str(), "lines");
    assert_eq!(LimitKind::Items.as_str(), "items");
    assert_eq!(LimitKind::Diagnostics.as_str(), "diagnostics");
    assert_eq!(LimitKind::References.as_str(), "references");
    assert_eq!(LimitKind::InputBytes.to_string(), "input-bytes");
    assert_eq!(LimitKind::OutputBytes.to_string(), "output-bytes");
    assert_eq!(LimitKind::Lines.to_string(), "lines");
    assert_eq!(LimitKind::Items.to_string(), "items");
    assert_eq!(LimitKind::Diagnostics.to_string(), "diagnostics");
    assert_eq!(LimitKind::References.to_string(), "references");
    assert_eq!(
        crate::LimitExceeded::new(LimitKind::References, 2, 3).to_string(),
        "references limit 2 exceeded with observed 3"
    );

    let error = parse_strict_with_options(
        "A=1\n",
        Dialect::PORTABLE,
        ParseOptions::default().with_max_input_bytes(3),
    )
    .unwrap_err();
    assert_eq!(
        limit(&error).to_string(),
        "input-bytes limit 3 exceeded with observed 4"
    );

    let line_error = parse_strict_with_options(
        "A=1\nB=2\n",
        Dialect::PORTABLE,
        ParseOptions::default().with_max_lines(1),
    )
    .unwrap_err();
    assert_eq!(
        limit(&line_error).to_string(),
        "lines limit 1 exceeded with observed 2"
    );

    let (_, diagnostics) = parse_with_options(
        "A=1\nB=2\n",
        Dialect::PORTABLE,
        ParseOptions::default().with_max_items(1),
    );
    let item_limit = diagnostics
        .iter()
        .find_map(Diagnostic::limit)
        .expect("item limit diagnostic");
    assert_eq!(
        item_limit.to_string(),
        "items limit 1 exceeded with observed 2"
    );

    let (_, diagnostics) = parse_with_options(
        "A=1\nA=2\nA=3\n",
        Dialect::PORTABLE,
        ParseOptions::default().with_max_diagnostics(1),
    );
    let diag_limit = diagnostics
        .iter()
        .find_map(Diagnostic::limit)
        .expect("diagnostic limit diagnostic");
    assert_eq!(
        diag_limit.to_string(),
        "diagnostics limit 1 exceeded with observed 2"
    );
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
        load_cascade_entries_compact_with_options, load_cascade_values_compact_with_options,
        load_cascade_values_with_options, load_cascade_with_options,
        load_entries_compact_with_options, LoadError, LoadOptions,
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

    let line_limits =
        LoadOptions::new(Dialect::PORTABLE).with_limits(ParseOptions::default().with_max_lines(1));
    assert!(matches!(
        load_cascade_with_options(&directory, "", line_limits),
        Err(LoadError::Limit { limit, .. }) if limit.kind() == LimitKind::Lines && limit.observed() == 2
    ));
    assert!(matches!(
        load_cascade_values_with_options(&directory, "", line_limits),
        Err(LoadError::Limit { limit, .. }) if limit.kind() == LimitKind::Lines && limit.observed() == 2
    ));
    assert!(matches!(
        load_cascade_values_compact_with_options(&directory, "", line_limits),
        Err(LoadError::Limit { limit, .. }) if limit.kind() == LimitKind::Lines && limit.observed() == 2
    ));
    assert!(matches!(
        load_cascade_entries_compact_with_options(&directory, "", read),
        Err(LoadError::Limit { limit, .. }) if limit.kind() == LimitKind::InputBytes && limit.limit() == 3
    ));
    assert!(matches!(
        load_cascade_entries_compact_with_options(&directory, "", items),
        Err(LoadError::Limit { limit, .. }) if limit.kind() == LimitKind::Items
    ));
    assert!(matches!(
        load_cascade_entries_compact_with_options(&directory, "", line_limits),
        Err(LoadError::Limit { limit, .. }) if limit.kind() == LimitKind::Lines && limit.observed() == 2
    ));
    assert!(matches!(
        load_entries_compact_with_options([directory.join(".env")], line_limits),
        Err(LoadError::Limit { limit, .. }) if limit.kind() == LimitKind::Lines && limit.observed() == 2
    ));
    fs::write(directory.join(".env"), "A=1\nA=2\n").unwrap();
    let diag_limits = LoadOptions::new(Dialect::PORTABLE)
        .with_limits(ParseOptions::default().with_max_diagnostics(0));
    // max_diagnostics applies to the recovering parser (load_cascade_with_options),
    // not strict/compact entry loading.
    assert!(matches!(
        load_cascade_with_options(&directory, "", diag_limits),
        Err(LoadError::Limit { limit, .. }) if limit.kind() == LimitKind::Diagnostics
    ));
    let compact_dup =
        load_cascade_entries_compact_with_options(&directory, "", diag_limits).unwrap();
    assert_eq!(compact_dup.get("A"), Some(Some("2")));

    fs::remove_dir_all(&directory).unwrap();
}
