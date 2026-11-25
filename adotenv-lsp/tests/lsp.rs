//! Focused tests for the transport-free LSP operations.

use adotenv::EditError;
use adotenv_lsp::{
    apply_change, completion, completion_with_options, definition, definition_with_options,
    diagnostics, document_symbols, document_symbols_with_options, hover, references, rename,
    workspace_symbols, Dialect, EqualsSpacing, KeyCase, LintOptions, LspOptions, QuoteStyle,
    RenameError, Rule, Severity,
};
use lsp_types::{
    CompletionItemKind, CompletionResponse, DiagnosticSeverity, DocumentSymbolResponse,
    GotoDefinitionResponse, HoverContents, NumberOrString, Position, SymbolKind, Uri,
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
    assert_eq!(
        diagnostics[0].range,
        lsp_types::Range::new(position(1, 0), position(1, 1))
    );
    assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert_eq!(diagnostics[0].source.as_deref(), Some("adotenv"));
    assert_ne!(diagnostics[0].message, "");
    assert!(diagnostics[0].related_information.is_some());
    assert_eq!(code(&diagnostics[1]), Some("dotenv.invalid-key"));
    assert_eq!(diagnostics[1].range.start, position(2, 0));
}

#[test]
fn options_dialect_controls_every_operation() {
    let source = "'A B'=1\nUSE=${A B}\n";
    let options = LspOptions::new().with_dialect(Dialect::PYTHON_DOTENV);

    assert_eq!(diagnostics(source, &uri(), &options).len(), 0);
    assert_ne!(diagnostics(source, &uri(), &LspOptions::new()).len(), 0);

    let DocumentSymbolResponse::Nested(symbols) = document_symbols_with_options(source, &options)
    else {
        panic!("expected nested document symbols");
    };
    assert_eq!(symbols[0].name, "A B");

    let target = definition_with_options(source, &uri(), position(1, 7), &options).unwrap();
    let GotoDefinitionResponse::Scalar(location) = target else {
        panic!("expected one definition location");
    };
    assert_eq!(location.range.start, position(0, 1));

    let completion = completion_with_options(source, position(0, 1), &options);
    let CompletionResponse::Array(items) = completion else {
        panic!("expected array completion");
    };
    assert_eq!(items[0].label, "A B");
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
    assert_eq!(
        diagnostics[0].range,
        lsp_types::Range::new(position(0, 0), position(2, 0))
    );
    assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
    assert_eq!(diagnostics[0].source.as_deref(), Some("adotenv-lint"));
    assert_ne!(diagnostics[0].message, "");
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
fn bom_diagnostic_has_a_visible_range() {
    let diagnostics = diagnostics("\u{feff}A=1\n", &uri(), &LspOptions::new());
    let bom = diagnostics
        .iter()
        .find(|diagnostic| code(diagnostic) == Some("dotenv.bom"))
        .expect("bom diagnostic");
    assert_eq!(
        bom.range,
        lsp_types::Range::new(position(0, 0), position(0, 1))
    );
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
#[allow(clippy::too_many_lines)]
fn completion_lists_declared_keys() {
    let response = completion("HOST=localhost\nPORT=8080\n", position(0, 0));
    let CompletionResponse::Array(items) = response else {
        panic!("expected array completion");
    };
    let labels: Vec<_> = items.iter().map(|item| item.label.clone()).collect();
    assert_eq!(labels, ["HOST", "PORT"]);
    assert_eq!(items[0].kind, Some(CompletionItemKind::VARIABLE));
    assert_eq!(items[0].detail.as_deref(), Some("declared .env key"));
    assert_eq!(items[0].insert_text.as_deref(), Some("HOST"));

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

    let underscore_reference = completion("HOST=localhost\nURL=$H_\n", position(1, 7));
    let CompletionResponse::Array(items) = underscore_reference else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let escaped_quote = completion("HOST=localhost\nURL=\"a\\\"b # $H\"\n", position(1, 14));
    let CompletionResponse::Array(items) = escaped_quote else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let escaped_quote_then_comment =
        completion("HOST=localhost\nURL=\"a\\\"\" # $H\n", position(1, 14));
    let CompletionResponse::Array(items) = escaped_quote_then_comment else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let comment = completion("HOST=localhost\n# $HOST\n", position(1, 3));
    let CompletionResponse::Array(items) = comment else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let inline_comment = completion("HOST=localhost # $H\n", position(0, 17));
    let CompletionResponse::Array(items) = inline_comment else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let single_quoted = completion("HOST=localhost\nURL='$H'\n", position(1, 7));
    let CompletionResponse::Array(items) = single_quoted else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    // An apostrophe or `#` inside a double-quoted value is literal text and
    // must not suppress reference completion.
    let double_quoted_apostrophe = completion("HOST=localhost\nURL=\"it's $H\"\n", position(1, 12));
    let CompletionResponse::Array(items) = double_quoted_apostrophe else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let hash_in_double_quotes = completion("HOST=localhost\nURL=\"a#b $H\"\n", position(1, 11));
    let CompletionResponse::Array(items) = hash_in_double_quotes else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let tight_hash = completion("HOST=localhost\nURL=a#b $H\n", position(1, 10));
    let CompletionResponse::Array(items) = tight_hash else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let tight_hash_node = completion_with_options(
        "HOST=localhost\nURL=a#b $H\n",
        position(1, 10),
        &LspOptions::new().with_dialect(Dialect::NODE_DOTENV),
    );
    let CompletionResponse::Array(items) = tight_hash_node else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let tight_hash_docker = completion_with_options(
        "HOST=localhost\nURL=a#b $H\n",
        position(1, 10),
        &LspOptions::new().with_dialect(Dialect::DOCKER_RAW),
    );
    let CompletionResponse::Array(items) = tight_hash_docker else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let tight_hash_python = completion_with_options(
        "HOST=localhost\nURL=a#b $H\n",
        position(1, 10),
        &LspOptions::new().with_dialect(Dialect::PYTHON_DOTENV),
    );
    let CompletionResponse::Array(items) = tight_hash_python else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let tab_hash_portable = completion("HOST=localhost\nURL=a\t#b $H\n", position(1, 11));
    let CompletionResponse::Array(items) = tab_hash_portable else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let tab_hash_compose = completion_with_options(
        "HOST=localhost\nURL=a\t#b $H\n",
        position(1, 11),
        &LspOptions::new().with_dialect(Dialect::COMPOSE),
    );
    let CompletionResponse::Array(items) = tab_hash_compose else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let exported_key = completion("export   HOST=localhost\n", position(0, 13));
    let CompletionResponse::Array(items) = exported_key else {
        panic!("expected array completion");
    };
    assert_eq!(items[0].label, "HOST");
}

#[test]
fn document_symbols_outline_declared_keys_without_values() {
    let source = "\u{feff}# app\nHOST=localhost\nPORT=8080\nBARE\nBAD KEY=1\n";
    let DocumentSymbolResponse::Nested(symbols) = document_symbols(source) else {
        panic!("expected nested document symbols");
    };
    let names: Vec<_> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();
    assert_eq!(names, ["HOST", "PORT", "BARE"]);
    assert!(symbols.iter().all(|symbol| symbol.kind == SymbolKind::KEY));
    assert!(symbols.iter().all(|symbol| symbol.detail.is_none()));
    assert_eq!(symbols[0].range.start, position(1, 0));
    assert_eq!(symbols[0].range.end, position(1, 14));
    assert_eq!(
        symbols[0].selection_range,
        lsp_types::Range::new(position(1, 0), position(1, 4))
    );
    assert_eq!(
        symbols[2].selection_range,
        lsp_types::Range::new(position(3, 0), position(3, 4))
    );
    assert_eq!(symbols[2].range.end, position(3, 4));

    let DocumentSymbolResponse::Nested(symbols) = document_symbols("\u{feff}HOST=localhost\n")
    else {
        panic!("expected nested document symbols");
    };
    assert_eq!(symbols[0].range.start, position(0, 0));
    assert_eq!(symbols[0].range.end, position(0, 14));
    assert_eq!(
        symbols[0].selection_range,
        lsp_types::Range::new(position(0, 0), position(0, 4))
    );
}

#[test]
fn document_symbols_keep_duplicate_declarations_and_empty_documents() {
    let DocumentSymbolResponse::Nested(symbols) = document_symbols("A=1\nA=2\n") else {
        panic!("expected nested document symbols");
    };
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "A");
    assert_eq!(symbols[1].name, "A");
    assert_eq!(symbols[0].selection_range.start, position(0, 0));
    assert_eq!(symbols[1].selection_range.start, position(1, 0));

    let DocumentSymbolResponse::Nested(symbols) = document_symbols("# only comments\n") else {
        panic!("expected nested document symbols");
    };
    assert_eq!(symbols.len(), 0);
}

#[test]
fn workspace_symbols_filter_and_locate_declared_keys() {
    let source = "\u{feff}HOST=localhost\nPORT=8080\nBARE\n";
    let filtered = workspace_symbols(source, &uri(), "po");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "PORT");
    assert_eq!(filtered[0].kind, SymbolKind::KEY);
    assert_eq!(filtered[0].location.uri, uri());
    assert_eq!(filtered[0].location.range.start, position(1, 0));
    assert_eq!(filtered[0].location.range.end, position(1, 9));

    let all = workspace_symbols(source, &uri(), "");
    let names: Vec<_> = all.iter().map(|symbol| symbol.name.as_str()).collect();
    assert_eq!(names, ["HOST", "PORT", "BARE"]);
    assert!(all.iter().all(|symbol| symbol.container_name.is_none()));
    assert_eq!(workspace_symbols(source, &uri(), "missing").len(), 0);
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
fn definition_accepts_a_cursor_at_the_end_of_a_key() {
    let source = "HOST=localhost\n";
    let target = definition(source, &uri(), position(0, 4)).unwrap();
    let GotoDefinitionResponse::Scalar(location) = target else {
        panic!("expected one definition location");
    };
    assert_eq!(location.range.start, position(0, 0));
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
fn hover_does_not_claim_an_inline_comment() {
    let source = "HOST=localhost # note\n";
    assert!(hover(source, position(0, 17), &LspOptions::new()).is_none());
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
fn nested_references_resolve_to_the_innermost_name() {
    let source = "HOST=localhost\nOTHER=other\nURL=${OTHER:-$HOST}\n";
    let target = definition(source, &uri(), position(2, 15)).unwrap();
    let GotoDefinitionResponse::Scalar(location) = target else {
        panic!("expected one definition location");
    };
    assert_eq!(location.range.start, position(0, 0));
    assert_eq!(location.range.end, position(0, 4));

    let locations = references(source, &uri(), position(2, 15), true);
    assert_eq!(locations.len(), 2);
    assert_eq!(locations[0].range.start, position(0, 0));
    assert_eq!(locations[1].range.start, position(2, 13));

    let edit = rename(source, &uri(), position(2, 15), "SERVER").unwrap();
    let edits = edit.changes.unwrap().get(&uri()).unwrap().clone();
    assert_eq!(edits.len(), 2);
    assert_eq!(edits[0].new_text, "SERVER");
    assert_eq!(edits[1].new_text, "SERVER");
}

#[test]
#[allow(clippy::mutable_key_type)]
fn rename_updates_literal_value_references() {
    let source = "HOST=localhost\nURL=${HOST}\nPORT=$HOST\n";
    let edit = rename(source, &uri(), position(0, 0), "SERVER").unwrap();
    let changes = edit.changes.unwrap();
    let edits = changes.get(&uri()).unwrap();
    assert_eq!(edits.len(), 3);
    assert!(edits.iter().all(|edit| edit.new_text == "SERVER"));
    assert_eq!(
        edits[1].range,
        lsp_types::Range::new(position(1, 6), position(1, 10))
    );
    assert_eq!(
        edits[2].range,
        lsp_types::Range::new(position(2, 6), position(2, 10))
    );
}

#[test]
#[allow(clippy::mutable_key_type)]
fn rename_from_a_reference_targets_the_referenced_key() {
    let source = "HOST=localhost\nURL=${HOST}\n";
    let edit = rename(source, &uri(), position(1, 7), "SERVER").unwrap();
    let edits = edit.changes.unwrap().get(&uri()).unwrap().clone();
    assert_eq!(edits.len(), 2);
    assert!(edits.iter().all(|edit| edit.new_text == "SERVER"));
}

#[test]
#[allow(clippy::mutable_key_type)]
fn rename_braces_a_dotted_reference() {
    let source = "HOST=localhost\nURL=$HOST\n";
    let edit = rename(source, &uri(), position(1, 5), "APP.HOST").unwrap();
    let edits = edit.changes.unwrap().get(&uri()).unwrap().clone();
    assert_eq!(edits.len(), 2);
    assert_eq!(edits[1].new_text, "${APP.HOST}");
    assert_eq!(
        edits[1].range,
        lsp_types::Range::new(position(1, 4), position(1, 9))
    );
}

#[test]
#[allow(clippy::mutable_key_type)]
fn rename_keeps_bom_adjusted_positions() {
    let source = "\u{feff}HOST=localhost\nURL=${HOST}\n";
    let edit = rename(source, &uri(), position(0, 0), "SERVER").unwrap();
    let edits = edit.changes.unwrap().get(&uri()).unwrap().clone();
    assert_eq!(
        edits[0].range,
        lsp_types::Range::new(position(0, 0), position(0, 4))
    );
    assert_eq!(
        edits[1].range,
        lsp_types::Range::new(position(1, 6), position(1, 10))
    );
}

#[test]
fn rename_requires_a_key_or_reference_position() {
    let error = rename("HOST=localhost\n", &uri(), position(0, 6), "SERVER").unwrap_err();
    assert!(matches!(error, RenameError::NoSymbol));
}

#[test]
fn rename_rejects_a_key_that_cannot_be_a_reference() {
    let error = rename(
        "HOST=localhost\nURL=${HOST}\n",
        &uri(),
        position(0, 0),
        "APP-HOST",
    )
    .unwrap_err();
    assert!(matches!(
        error,
        RenameError::Edit(EditError::InvalidReferenceKey)
    ));
}

#[test]
fn hover_covers_references_bare_keys_and_edge_cases() {
    use std::error::Error as _;

    // Reference hover (resolved and unresolved) + bare key hover.
    let source = "HOST=localhost\nURL=${HOST}/${MISSING}\nBARE\n";
    let resolved = hover(source, position(1, 7), &LspOptions::new()).unwrap();
    let HoverContents::Markup(markup) = resolved.contents else {
        panic!("expected markup hover");
    };
    assert!(markup.value.contains("`${HOST}`; declared on line 1"));

    let unresolved = hover(source, position(1, 16), &LspOptions::new()).unwrap();
    let HoverContents::Markup(markup) = unresolved.contents else {
        panic!("expected markup hover");
    };
    assert!(markup.value.contains("`${MISSING}`; unresolved"));

    let bare = hover(source, position(2, 0), &LspOptions::new()).unwrap();
    let HoverContents::Markup(markup) = bare.contents else {
        panic!("expected markup hover");
    };
    assert!(markup.value.contains("`<bare>`"));

    // RenameError Display and Error::source coverage.
    assert!(RenameError::NoSymbol.to_string().contains("no .env key"));
    assert!(RenameError::NoSymbol.source().is_none());
    let edit_err = RenameError::from(EditError::InvalidKey);
    assert!(edit_err.to_string().contains("rename refused:"));
    assert!(edit_err.source().is_some());

    // Completion out-of-range and after closing single quote.
    let CompletionResponse::Array(oob) = completion("A=1\n", position(9, 0)) else {
        panic!("expected array");
    };
    assert_eq!(oob, Vec::new());
    let CompletionResponse::Array(after_sq) = completion("HOST=1\nURL='a' $H\n", position(1, 10))
    else {
        panic!("expected array");
    };
    assert_eq!(after_sq.len(), 1);
    assert_eq!(after_sq[0].label, "HOST");

    // apply_change inverted range and references out-of-range / non-symbol.
    assert_eq!(
        apply_change(
            "A=1\n",
            Some(lsp_types::Range::new(position(0, 2), position(0, 1))),
            "x"
        ),
        None
    );
    assert_eq!(
        references("A=1\n", &uri(), position(9, 0), true),
        Vec::new()
    );
    assert_eq!(
        references("# comment\n", &uri(), position(0, 2), true),
        Vec::new()
    );
}

#[test]
fn parse_options_and_strip_all_bom_apply_across_lsp_operations() {
    use adotenv::BomPolicy;
    use adotenv_lsp::{LimitKind, ParseOptions};

    // ParseOptions::with_max_lines enforced across diagnostics and rename.
    let bounded = LspOptions::new().with_parse_options(ParseOptions::new().with_max_lines(2));
    let diags = diagnostics("A=1\nB=2\nC=3\n", &uri(), &bounded);
    assert_eq!(code(&diags[0]), Some("dotenv.limit-exceeded"));
    let rename_err =
        adotenv_lsp::rename_with_options("A=1\nB=2\nC=3\n", &uri(), position(0, 0), "X", &bounded)
            .unwrap_err();
    assert!(matches!(
        rename_err,
        RenameError::Edit(EditError::Limit(limit)) if limit.kind() == LimitKind::Lines
    ));

    // BomPolicy::StripAll hides full multi-BOM prefix on line 0 and anchors diagnostic.
    let strip_all = LspOptions::new().with_dialect(Dialect::PORTABLE.with_bom(BomPolicy::StripAll));
    let multi = "\u{feff}\u{feff}HOST=localhost\n";
    let bom_diags = diagnostics(multi, &uri(), &strip_all);
    let bom = bom_diags
        .iter()
        .find(|d| code(d) == Some("dotenv.bom"))
        .expect("bom diagnostic");
    assert_eq!(
        bom.range,
        lsp_types::Range::new(position(0, 0), position(0, 1))
    );
    let hov = hover(multi, position(0, 0), &strip_all).unwrap();
    assert_eq!(
        hov.range.unwrap(),
        lsp_types::Range::new(position(0, 0), position(0, 4))
    );
    assert_eq!(
        adotenv_lsp::apply_change_with_options(
            multi,
            Some(lsp_types::Range::new(position(0, 0), position(0, 4))),
            "NAME",
            &strip_all,
        ),
        Some("\u{feff}\u{feff}NAME=localhost\n".to_owned())
    );

    // BomPolicy::Keep does not hide the leading BOM in position mapping.
    let keep_bom = LspOptions::new().with_dialect(Dialect::NODE_DOTENV.with_bom(BomPolicy::Keep));
    let single = "\u{feff}HOST=localhost\n";
    let keep_hov = hover(single, position(0, 1), &keep_bom).unwrap();
    assert_eq!(
        keep_hov.range.unwrap(),
        lsp_types::Range::new(position(0, 1), position(0, 5))
    );

    // Mutation-driven regression assertions:
    // 1. Completion on line > 0 at key start does not bleed into the previous line.
    let CompletionResponse::Array(line1_key) = completion("A=1\nK\n", position(1, 1)) else {
        panic!("expected array");
    };
    assert_eq!(line1_key.len(), 2);
    assert_eq!(line1_key[0].label, "A");

    // 2. Pure insertion (zero-width range, start == end) succeeds in apply_change.
    assert_eq!(
        apply_change(
            "A=1\n",
            Some(lsp_types::Range::new(position(0, 0), position(0, 0))),
            "export "
        ),
        Some("export A=1\n".to_owned())
    );

    // 3. BOM on line 0 does not shift positions or edits on line 1.
    let line1_hov = hover("\u{feff}A=1\nHOST=2\n", position(1, 0), &LspOptions::new()).unwrap();
    assert_eq!(
        line1_hov.range.unwrap(),
        lsp_types::Range::new(position(1, 0), position(1, 4))
    );
    assert_eq!(
        apply_change(
            "\u{feff}A=1\nHOST=2\n",
            Some(lsp_types::Range::new(position(1, 0), position(1, 4))),
            "NAME"
        ),
        Some("\u{feff}A=1\nNAME=2\n".to_owned())
    );

    // 4. Non-BOM zero-width diagnostic at offset 0 stays zero-width (not 0..1),
    //    and a BOM-prefixed limit diagnostic spanning past the BOM is not clamped to 0..1.
    let missing_key = diagnostics("=1\n", &uri(), &LspOptions::new());
    assert_eq!(
        missing_key[0].range,
        lsp_types::Range::new(position(0, 0), position(0, 0))
    );
    let bom_limit = diagnostics(
        "\u{feff}HOST=localhost\n",
        &uri(),
        &LspOptions::new().with_parse_options(ParseOptions::new().with_max_input_bytes(4)),
    );
    assert_eq!(code(&bom_limit[0]), Some("dotenv.limit-exceeded"));
    assert_eq!(
        bom_limit[0].range,
        lsp_types::Range::new(position(0, 0), position(1, 0))
    );
}

#[test]
fn bom_comment_suppresses_completion_and_hover_escapes_backticks_and_newlines() {
    // Leading BOM before a comment line must not trigger completion.
    let bom_comment = "\u{feff}# $HOST\nHOST=localhost\n";
    let items = completion_with_options(bom_comment, position(0, 7), &LspOptions::new());
    match items {
        lsp_types::CompletionResponse::Array(list) => assert_eq!(list.len(), 0),
        lsp_types::CompletionResponse::List(list) => {
            panic!("expected empty completion array, got {list:?}")
        }
    }

    // Hover escapes backticks and renders multiline values inside fenced blocks.
    let backtick_doc = "CMD=\"echo `hi`\"\nMULTI=\"line1\\nline2\"\n";
    let tick_hover = adotenv_lsp::hover(backtick_doc, position(0, 1), &LspOptions::new()).unwrap();
    let lsp_types::HoverContents::Markup(markup) = tick_hover.contents else {
        panic!("expected markdown hover");
    };
    assert!(markup.value.contains("`` echo `hi` ``"), "{}", markup.value);

    let multi_hover = adotenv_lsp::hover(backtick_doc, position(1, 1), &LspOptions::new()).unwrap();
    let lsp_types::HoverContents::Markup(markup) = multi_hover.contents else {
        panic!("expected markdown hover");
    };
    assert!(
        markup.value.contains("```text\nline1\nline2\n```"),
        "{}",
        markup.value
    );

    // Reference hover also escapes backticks in the referenced key name.
    let ref_doc = "'K`1'=ok\nUSE=${K`1}\n";
    let ref_opts = LspOptions::new().with_dialect(adotenv::Dialect::PYTHON_DOTENV);
    let ref_hover = adotenv_lsp::hover(ref_doc, position(1, 7), &ref_opts).unwrap();
    let lsp_types::HoverContents::Markup(markup) = ref_hover.contents else {
        panic!("expected markdown hover");
    };
    assert!(
        markup.value.contains("``${K`1}``; declared on line 1"),
        "{}",
        markup.value
    );

    // Inline code padding follows CommonMark: pad only when both ends are spaces
    // and at least one non-space byte exists.
    let pad_doc = "PAD=\" x \"\nLEAD=\" x\"\nTRAIL=\"x \"\nSPACES=\"  \"\n";
    let hover_md = |line: u32| -> String {
        let h = adotenv_lsp::hover(pad_doc, position(line, 1), &LspOptions::new()).unwrap();
        let lsp_types::HoverContents::Markup(m) = h.contents else {
            panic!("expected markdown hover");
        };
        m.value
    };
    assert!(hover_md(0).contains("Value: `  x  `"), "{}", hover_md(0));
    assert!(hover_md(1).contains("Value: ` x`"), "{}", hover_md(1));
    assert!(hover_md(2).contains("Value: `x `"), "{}", hover_md(2));
    assert!(hover_md(3).contains("Value: `  `"), "{}", hover_md(3));
}

#[test]
fn rename_with_options_enforces_parse_limits_on_planned_output() {
    use adotenv_lsp::{LimitKind, ParseOptions};

    // Input fits within 24 bytes ("A=1\nX=$A\nY=$A\n" = 14 bytes),
    // but growing rename ("VERY_LONG_HOST_NAME=1\nX=$VERY_LONG_HOST_NAME\nY=$VERY_LONG_HOST_NAME\n")
    // exceeds max_input_bytes and fails with LimitKind::InputBytes.
    let bounded =
        LspOptions::new().with_parse_options(ParseOptions::new().with_max_input_bytes(24));
    let grow_err = adotenv_lsp::rename_with_options(
        "A=1\nX=$A\nY=$A\n",
        &uri(),
        position(0, 0),
        "VERY_LONG_HOST_NAME",
        &bounded,
    )
    .unwrap_err();
    assert!(
        matches!(grow_err, RenameError::Edit(EditError::Limit(limit)) if limit.kind() == LimitKind::InputBytes),
        "{grow_err:?}"
    );

    // Growing rename that fits exactly within max_input_bytes succeeds.
    let fit = LspOptions::new().with_parse_options(ParseOptions::new().with_max_input_bytes(23));
    let ok =
        adotenv_lsp::rename_with_options("A=1\nX=$A\nY=$A\n", &uri(), position(0, 0), "HOST", &fit)
            .unwrap();
    assert_eq!(ok.changes.unwrap().into_values().next().unwrap().len(), 3);

    // Shrinking rename fits cleanly within max_input_bytes.
    let shrink_opts =
        LspOptions::new().with_parse_options(ParseOptions::new().with_max_input_bytes(24));
    let shrunk = adotenv_lsp::rename_with_options(
        "LONG_KEY=1\nX=$LONG_KEY\n",
        &uri(),
        position(0, 0),
        "A",
        &shrink_opts,
    )
    .unwrap();
    assert_eq!(
        shrunk.changes.unwrap().into_values().next().unwrap().len(),
        2
    );

    // max_items is enforced cleanly when exceeded on input or rename verification.
    let item_opts = LspOptions::new().with_parse_options(ParseOptions::new().with_max_items(2));
    let item_err = adotenv_lsp::rename_with_options(
        "A=1\nX=$A\nY=$A\n",
        &uri(),
        position(0, 0),
        "B",
        &item_opts,
    )
    .unwrap_err();
    assert!(
        matches!(item_err, RenameError::Edit(EditError::Limit(limit)) if limit.kind() == LimitKind::Items),
        "{item_err:?}"
    );
}
