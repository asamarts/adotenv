use crate::{parse, DiagnosticCode, Severity};
use alloc::string::ToString;

const CODES: [DiagnosticCode; 16] = [
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
