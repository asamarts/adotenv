use crate::{parse, parse_strict, DiagnosticCode, Severity};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::Write as _;

const CODES: [DiagnosticCode; 17] = [
    DiagnosticCode::InvalidKey,
    DiagnosticCode::UnterminatedQuote,
    DiagnosticCode::TrailingText,
    DiagnosticCode::UnknownEscape,
    DiagnosticCode::DuplicateKey,
    DiagnosticCode::BareKey,
    DiagnosticCode::NonPosixKey,
    DiagnosticCode::TightHash,
    DiagnosticCode::Backtick,
    DiagnosticCode::ByteOrderMark,
    DiagnosticCode::LoneCarriageReturn,
    DiagnosticCode::ForbiddenBareKey,
    DiagnosticCode::ForbiddenByteOrderMark,
    DiagnosticCode::InvalidEscape,
    DiagnosticCode::IgnoredStatement,
    DiagnosticCode::LineTooLong,
    DiagnosticCode::TooManyDiagnostics,
];

#[test]
fn every_code_has_a_stable_identity_severity_and_message() {
    for code in CODES {
        let identity = code.as_str();
        assert!(identity.starts_with("dotenv."), "{code:?}");
        assert!(!code.message().is_empty(), "{code:?}");
        assert_eq!(
            code.severity(),
            match code {
                DiagnosticCode::InvalidKey
                | DiagnosticCode::UnterminatedQuote
                | DiagnosticCode::TrailingText
                | DiagnosticCode::ForbiddenBareKey
                | DiagnosticCode::ForbiddenByteOrderMark
                | DiagnosticCode::InvalidEscape
                | DiagnosticCode::LineTooLong => Severity::Error,
                DiagnosticCode::ByteOrderMark => Severity::Info,
                _ => Severity::Warning,
            }
        );
    }
}

#[test]
fn parse_caps_repeated_value_diagnostics() {
    let source = format!("A={}\n", "#".repeat(10_001));
    let (_, diagnostics) = parse(&source);
    assert_eq!(diagnostics.len(), 10_001);
    assert!(diagnostics[..10_000]
        .iter()
        .all(|diagnostic| diagnostic.code() == DiagnosticCode::TightHash));
    let summary = diagnostics.last().unwrap();
    assert_eq!(summary.code(), DiagnosticCode::TooManyDiagnostics);
    assert_eq!(summary.span(), source.len()..source.len());
}

#[test]
fn parse_caps_lone_carriage_return_diagnostics() {
    let source = "\r".repeat(10_001);
    let (_, diagnostics) = parse(&source);
    assert_eq!(diagnostics.len(), 10_001);
    assert!(diagnostics[..10_000]
        .iter()
        .all(|diagnostic| diagnostic.code() == DiagnosticCode::LoneCarriageReturn));
    assert_eq!(
        diagnostics.last().unwrap().code(),
        DiagnosticCode::TooManyDiagnostics
    );
}

#[test]
fn parse_keeps_the_first_error_after_the_diagnostic_budget() {
    let source = format!("A={}\nBAD KEY=1\n", "#".repeat(10_001));
    let (_, diagnostics) = parse(&source);
    assert!(diagnostics
        .windows(2)
        .all(|pair| pair[0].span().start <= pair[1].span().start));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code() == DiagnosticCode::InvalidKey));
    assert_eq!(
        diagnostics.last().unwrap().code(),
        DiagnosticCode::TooManyDiagnostics
    );
    assert_eq!(
        parse_strict(&source).unwrap_err().code(),
        DiagnosticCode::InvalidKey
    );
}

#[test]
fn parse_keeps_an_error_dropped_while_merging_lone_crs() {
    let source = format!("{}BAD KEY=1\n", "\r".repeat(10_001));
    let (_, diagnostics) = parse(&source);
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code() == DiagnosticCode::InvalidKey));
    assert_eq!(
        diagnostics.last().unwrap().code(),
        DiagnosticCode::TooManyDiagnostics
    );
    assert_eq!(
        parse_strict(&source).unwrap_err().code(),
        DiagnosticCode::InvalidKey
    );
}

#[test]
fn duplicate_diagnostic_accessors_address_primary_and_secondary_spans() {
    let source = "A=one\nA=two\n";
    let (_, diagnostics) = parse(source);
    let duplicate = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code() == DiagnosticCode::DuplicateKey)
        .unwrap();
    assert_eq!(duplicate.line(), 2);
    assert_eq!(duplicate.severity(), Severity::Warning);
    assert_eq!(duplicate.message(), DiagnosticCode::DuplicateKey.message());
    assert_eq!(&source[duplicate.span()], "A");
    let secondary = duplicate.secondary_span().unwrap();
    assert_eq!(&source[secondary], "A");
    assert_eq!(
        duplicate.to_string(),
        "line 2: duplicate key; lookup follows the selected dialect policy (dotenv.duplicate-key)"
    );
}

#[test]
fn parse_caps_bare_key_diagnostics() {
    let mut source = String::new();
    for index in 0..12_000 {
        writeln!(source, "KEY{index}").unwrap();
    }
    let (_, diagnostics) = parse(&source);
    assert_eq!(diagnostics.len(), 10_001);
    assert!(diagnostics[..10_000]
        .iter()
        .all(|diagnostic| diagnostic.code() == DiagnosticCode::BareKey));
    assert_eq!(
        diagnostics.last().unwrap().code(),
        DiagnosticCode::TooManyDiagnostics
    );
}

#[test]
fn parse_reports_the_exact_budget_when_an_error_follows() {
    let source = format!("A={}\nBAD KEY=1\n", "#".repeat(10_001));
    let (_, diagnostics) = parse(&source);
    assert_eq!(diagnostics.len(), 10_002);
    assert_eq!(
        diagnostics.last().unwrap().code(),
        DiagnosticCode::TooManyDiagnostics
    );
}

#[test]
fn lone_cr_merging_honors_limits_and_omitted_error_selection() {
    use crate::{parse_with_options, Dialect, ParseOptions};

    // These cases exercise the exact boundary where the lone-CR stream and
    // the ordinary diagnostic stream share the configured budget.
    let cases: &[(&str, usize, &[DiagnosticCode])] = &[
        (
            "\r\r",
            1,
            &[
                DiagnosticCode::LoneCarriageReturn,
                DiagnosticCode::LimitExceeded,
            ],
        ),
        (
            "\r\r",
            2,
            &[
                DiagnosticCode::LoneCarriageReturn,
                DiagnosticCode::LoneCarriageReturn,
            ],
        ),
        (
            "\rA=#\r",
            1,
            &[
                DiagnosticCode::LoneCarriageReturn,
                DiagnosticCode::LimitExceeded,
            ],
        ),
        (
            "\rA=#\r",
            2,
            &[
                DiagnosticCode::LoneCarriageReturn,
                DiagnosticCode::TightHash,
                DiagnosticCode::LimitExceeded,
            ],
        ),
        (
            "A=#\r\r",
            1,
            &[DiagnosticCode::TightHash, DiagnosticCode::LimitExceeded],
        ),
        (
            "A=#\r\r",
            2,
            &[
                DiagnosticCode::TightHash,
                DiagnosticCode::LoneCarriageReturn,
                DiagnosticCode::LimitExceeded,
            ],
        ),
        (
            "\rBAD KEY=1\r",
            2,
            &[
                DiagnosticCode::LoneCarriageReturn,
                DiagnosticCode::InvalidKey,
                DiagnosticCode::LimitExceeded,
            ],
        ),
    ];
    for (source, limit, expected) in cases {
        let (_, diagnostics) = parse_with_options(
            source,
            Dialect::PORTABLE,
            ParseOptions::new().with_max_diagnostics(*limit),
        );
        assert_eq!(
            diagnostics
                .iter()
                .map(crate::Diagnostic::code)
                .collect::<Vec<_>>(),
            *expected,
            "source={source:?}, limit={limit}"
        );
    }
}

#[test]
fn lone_cr_budget_keeps_the_first_omitted_error_after_warnings() {
    let source = format!("{}WARN\rBAD KEY=1\n", "\r".repeat(10_001));
    let (_, diagnostics) = parse(&source);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == DiagnosticCode::InvalidKey),
        "{:?}",
        diagnostics
            .iter()
            .map(crate::Diagnostic::code)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        diagnostics.last().unwrap().code(),
        DiagnosticCode::TooManyDiagnostics
    );
}
