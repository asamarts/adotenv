//! Focused tests for the transport-free LSP operations.

use adotenv_lsp::{
    apply_change, completion, definition, diagnostics, hover, references, rename, EqualsSpacing,
    KeyCase, LintOptions, LspOptions, QuoteStyle, RenameError, Rule, Severity,
};
use lsp_types::{
    CompletionResponse, DiagnosticSeverity, GotoDefinitionResponse, HoverContents, NumberOrString,
    Position, Uri,
};

fn uri() -> Uri {
    "file:///tmp/.env".parse().unwrap()
}

fn position(line: u32, character: u32) -> Position {
    Position { line, character }
}

fn code(value: &lsp_types::Diagnostic) -> Option<&str> {
    match value.code.as_ref()? {
        NumberOrString::String(value) => Some(value),
        NumberOrString::Number(_) => None,
    }
}

#[test]
fn content_changes_use_utf16_ranges_and_accept_full_replacements() {
    let range = lsp_types::Range::new(position(0, 2), position(0, 3));
    assert_eq!(
        apply_change("A=1\n", Some(range), "2"),
        Some("A=2\n".to_owned())
    );
    assert_eq!(
        apply_change("A=1\n", None, "B=2\n"),
        Some("B=2\n".to_owned())
    );
    let out_of_bounds = lsp_types::Range::new(position(0, 9), position(0, 10));
    assert_eq!(apply_change("A=1\n", Some(out_of_bounds), "x"), None);

    let bom = apply_change(
        "\u{feff}A=1\n",
        Some(lsp_types::Range::new(position(0, 0), position(0, 1))),
        "B",
    );
    assert_eq!(bom, Some("\u{feff}B=1\n".to_owned()));

    let astral = apply_change(
        "A=😀\n",
        Some(lsp_types::Range::new(position(0, 2), position(0, 4))),
        "x",
    );
    assert_eq!(astral, Some("A=x\n".to_owned()));
}

#[test]
fn diagnostics_map_codes_ranges_and_related_locations() {
    let diagnostics = diagnostics("A=1\nA=2\nBAD KEY=3\n", &uri(), &LspOptions::default());
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(code(&diagnostics[0]), Some("dotenv.duplicate-key"));
    assert_eq!(diagnostics[0].range.start, position(1, 0));
    assert!(diagnostics[0].related_information.is_some());
    assert_eq!(code(&diagnostics[1]), Some("dotenv.invalid-key"));
    assert_eq!(diagnostics[1].range.start, position(2, 0));
}

#[test]
fn optional_linter_findings_are_published() {
    let options = LspOptions::default().with_lint_options(
        LintOptions::default()
            .with_duplicate_keys(false)
            .with_key_order(true),
    );
    let diagnostics = diagnostics("B=2\nA=1\n", &uri(), &options);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(code(&diagnostics[0]), Some("adotenv-lint.key-order"));
}

#[test]
fn linter_policy_types_are_reexported_and_keep_severity() {
    let options = LspOptions::default().with_lint_options(
        LintOptions::new()
            .with_duplicate_keys(false)
            .with_key_case(KeyCase::Upper)
            .with_quote_style(QuoteStyle::Double)
            .with_equals_spacing(EqualsSpacing::Spaces)
            .with_severity(Rule::KeyCase, Severity::Error),
    );
    let diagnostics = diagnostics("host=\"localhost\"\n", &uri(), &options);
    let finding = diagnostics
        .iter()
        .find(|diagnostic| code(diagnostic) == Some("adotenv-lint.key-case"))
        .expect("key-case finding");
    assert_eq!(finding.severity, Some(DiagnosticSeverity::ERROR));
}

#[test]
fn lsp_positions_hide_a_leading_byte_order_mark() {
    let source = "\u{feff}HOST=localhost\n";
    let hover = hover(source, position(0, 0), &LspOptions::default()).unwrap();
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markup hover");
    };
    assert!(markup.value.contains("HOST"));
    assert_eq!(
        hover.range.unwrap(),
        lsp_types::Range::new(position(0, 0), position(0, 4))
    );
}

#[test]
fn completion_lists_declared_keys() {
    let response = completion("HOST=localhost\nPORT=8080\n", position(0, 0));
    let CompletionResponse::Array(items) = response else {
        panic!("expected array completion");
    };
    let labels: Vec<_> = items.into_iter().map(|item| item.label).collect();
    assert_eq!(labels, ["HOST", "PORT"]);

    let partial_key = completion("HOST=localhost\nPORT=8080\n", position(0, 2));
    let CompletionResponse::Array(items) = partial_key else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let inside_value = completion("HOST=localhost\n", position(0, 6));
    let CompletionResponse::Array(items) = inside_value else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let reference = completion("HOST=localhost\nURL=${HOST}\n", position(1, 5));
    let CompletionResponse::Array(items) = reference else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let comment = completion("HOST=localhost\n# $HOST\n", position(1, 3));
    let CompletionResponse::Array(items) = comment else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);
}

#[test]
fn definition_follows_a_reference() {
    let source = "HOST=localhost\nURL=${HOST}\n";
    let target = definition(source, &uri(), position(1, 7)).unwrap();
    let GotoDefinitionResponse::Scalar(location) = target else {
        panic!("expected one definition location");
    };
    assert_eq!(location.range.start, position(0, 0));
    assert_eq!(location.range.end, position(0, 4));
}

#[test]
fn hover_shows_the_key_and_can_mask_the_value() {
    let source = "HOST=localhost\n";
    let visible = hover(source, position(0, 0), &LspOptions::default()).unwrap();
    let HoverContents::Markup(markup) = visible.contents else {
        panic!("expected markup hover");
    };
    assert!(markup.value.contains("localhost"));

    let masked = hover(
        source,
        position(0, 0),
        &LspOptions::default().with_mask_values(true),
    )
    .unwrap();
    let HoverContents::Markup(markup) = masked.contents else {
        panic!("expected markup hover");
    };
    assert!(markup.value.contains("<redacted>"));
    assert!(!markup.value.contains("localhost"));
}

#[test]
fn references_include_declarations_and_value_references() {
    let source = "HOST=localhost\nURL=${HOST}\n";
    let locations = references(source, &uri(), position(0, 0), true);
    assert_eq!(locations.len(), 2);
    assert_eq!(locations[0].range.start, position(0, 0));
    assert_eq!(locations[1].range.start, position(1, 4));

    let without_declaration = references(source, &uri(), position(0, 0), false);
    assert_eq!(without_declaration.len(), 1);
    assert_eq!(without_declaration[0].range.start, position(1, 4));
}

#[test]
#[allow(clippy::mutable_key_type)]
fn rename_uses_verified_edits_when_the_key_is_unreferenced() {
    let edit = rename("HOST=localhost\n", &uri(), position(0, 0), "SERVER").unwrap();
    let changes = edit.changes.unwrap();
    let edits = changes.get(&uri()).unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "SERVER");
    assert_eq!(edits[0].range.start, position(0, 0));
    assert_eq!(edits[0].range.end, position(0, 4));
}

#[test]
fn rename_refuses_a_referenced_key() {
    let error = rename(
        "HOST=localhost\nURL=${HOST}\n",
        &uri(),
        position(0, 0),
        "SERVER",
    )
    .unwrap_err();
    assert!(matches!(error, RenameError::References));
}
