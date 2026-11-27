//! Focused tests for the transport-free LSP operations, `ParsedDocument` and `Session`.

use adotenv_lsp::{
    apply_change, code_actions, completion, definition, diagnostics, document_symbols, formatting,
    hover, prepare_rename, references, rename, workspace_symbols, BomPolicy, Dialect, EditError,
    EqualsSpacing, FinalNewline, FormatOptions, FormattingError, KeyCase, LimitKind, LintOptions,
    LspOptions, ParseOptions, ParsedDocument, PositionEncoding, QuoteStyle, RenameError, Rule,
    Session, SessionError, Severity,
};
use lsp_types::{
    CodeActionContext, CodeActionKind, CodeActionOrCommand, CompletionItemKind, CompletionResponse,
    DiagnosticSeverity, DocumentSymbolResponse, FormattingOptions, GotoDefinitionResponse,
    HoverContents, NumberOrString, Position, PositionEncodingKind, PrepareRenameResponse, Range,
    SymbolKind, TextDocumentContentChangeEvent, Uri,
};

fn uri() -> Uri {
    "file:///tmp/.env".parse().unwrap()
}

fn position(line: u32, character: u32) -> Position {
    Position { line, character }
}

fn opts() -> LspOptions {
    LspOptions::new()
}

fn code(value: &lsp_types::Diagnostic) -> Option<&str> {
    match value.code.as_ref()? {
        NumberOrString::String(value) => Some(value),
        NumberOrString::Number(_) => None,
    }
}

#[test]
fn content_changes_use_utf16_ranges_clamp_eol_and_accept_full_replacements() {
    let range = Range::new(position(0, 2), position(0, 3));
    assert_eq!(
        apply_change("A=1\n", Some(range), "2", &opts()),
        Some("A=2\n".to_owned())
    );
    assert_eq!(
        apply_change("A=1\n", None, "B=2\n", &opts()),
        Some("B=2\n".to_owned())
    );
    // LSP 3.17 §3.1.1: `character` beyond line length clamps to line end.
    let clamped = Range::new(position(0, 2), position(0, u32::MAX));
    assert_eq!(
        apply_change("A=1\n", Some(clamped), "2", &opts()),
        Some("A=2\n".to_owned())
    );
    let clamped_insert = Range::new(position(0, 9), position(0, 10));
    assert_eq!(
        apply_change("A=1\n", Some(clamped_insert), "x", &opts()),
        Some("A=1x\n".to_owned())
    );
    // Out-of-bounds line or split UTF-16 surrogate still returns None.
    let oob_line = Range::new(position(9, 0), position(9, 1));
    assert_eq!(apply_change("A=1\n", Some(oob_line), "x", &opts()), None);
    let split_surrogate = Range::new(position(0, 3), position(0, 4));
    assert_eq!(
        apply_change("A=😀\n", Some(split_surrogate), "x", &opts()),
        None
    );

    let bom = apply_change(
        "\u{feff}A=1\n",
        Some(Range::new(position(0, 0), position(0, 1))),
        "B",
        &opts(),
    );
    assert_eq!(bom, Some("\u{feff}B=1\n".to_owned()));

    let astral = apply_change(
        "A=😀\n",
        Some(Range::new(position(0, 2), position(0, 4))),
        "x",
        &opts(),
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
        Range::new(position(1, 0), position(1, 1))
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

    let DocumentSymbolResponse::Nested(symbols) = document_symbols(source, &options) else {
        panic!("expected nested document symbols");
    };
    assert_eq!(symbols[0].name, "A B");

    let target = definition(source, &uri(), position(1, 7), &options).unwrap();
    let GotoDefinitionResponse::Scalar(location) = target else {
        panic!("expected one definition location");
    };
    assert_eq!(location.range.start, position(0, 1));

    let completion = completion(source, position(0, 1), &options);
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
        Range::new(position(0, 0), position(2, 0))
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
    assert_eq!(bom.range, Range::new(position(0, 0), position(0, 1)));
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
        Range::new(position(0, 0), position(0, 4))
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn completion_lists_declared_keys() {
    let response = completion("HOST=localhost\nPORT=8080\n", position(0, 0), &opts());
    let CompletionResponse::Array(items) = response else {
        panic!("expected array completion");
    };
    let labels: Vec<_> = items.iter().map(|item| item.label.clone()).collect();
    assert_eq!(labels, ["HOST", "PORT"]);
    assert_eq!(items[0].kind, Some(CompletionItemKind::VARIABLE));
    assert_eq!(items[0].detail.as_deref(), Some("declared .env key"));
    assert_eq!(items[0].insert_text.as_deref(), Some("HOST"));

    let CompletionResponse::Array(items) =
        completion("HOST=localhost\nPORT=8080\n", position(0, 2), &opts())
    else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let CompletionResponse::Array(items) = completion("HOST=localhost\n", position(0, 6), &opts())
    else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let CompletionResponse::Array(items) =
        completion("HOST=localhost\nURL=${HOST}\n", position(1, 5), &opts())
    else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let CompletionResponse::Array(items) =
        completion("HOST=localhost\nURL=$H_\n", position(1, 7), &opts())
    else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let CompletionResponse::Array(items) = completion(
        "HOST=localhost\nURL=\"a\\\"b # $H\"\n",
        position(1, 14),
        &opts(),
    ) else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let CompletionResponse::Array(items) = completion(
        "HOST=localhost\nURL=\"a\\\"\" # $H\n",
        position(1, 14),
        &opts(),
    ) else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let CompletionResponse::Array(items) =
        completion("HOST=localhost\n# $HOST\n", position(1, 3), &opts())
    else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let CompletionResponse::Array(items) =
        completion("HOST=localhost # $H\n", position(0, 17), &opts())
    else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let CompletionResponse::Array(items) =
        completion("HOST=localhost\nURL='$H'\n", position(1, 7), &opts())
    else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let CompletionResponse::Array(items) = completion(
        "HOST=localhost\nURL=\"it's $H\"\n",
        position(1, 12),
        &opts(),
    ) else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let CompletionResponse::Array(items) =
        completion("HOST=localhost\nURL=\"a#b $H\"\n", position(1, 11), &opts())
    else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let CompletionResponse::Array(items) =
        completion("HOST=localhost\nURL=a#b $H\n", position(1, 10), &opts())
    else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let CompletionResponse::Array(items) = completion(
        "HOST=localhost\nURL=a#b $H\n",
        position(1, 10),
        &LspOptions::new().with_dialect(Dialect::NODE_DOTENV),
    ) else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let CompletionResponse::Array(items) = completion(
        "HOST=localhost\nURL=a#b $H\n",
        position(1, 10),
        &LspOptions::new().with_dialect(Dialect::DOCKER_RAW),
    ) else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let CompletionResponse::Array(items) = completion(
        "HOST=localhost\nURL=a#b $H\n",
        position(1, 10),
        &LspOptions::new().with_dialect(Dialect::PYTHON_DOTENV),
    ) else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let CompletionResponse::Array(items) =
        completion("HOST=localhost\nURL=a\t#b $H\n", position(1, 11), &opts())
    else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 0);

    let CompletionResponse::Array(items) = completion(
        "HOST=localhost\nURL=a\t#b $H\n",
        position(1, 11),
        &LspOptions::new().with_dialect(Dialect::COMPOSE),
    ) else {
        panic!("expected array completion");
    };
    assert_eq!(items.len(), 2);

    let CompletionResponse::Array(items) =
        completion("export   HOST=localhost\n", position(0, 13), &opts())
    else {
        panic!("expected array completion");
    };
    assert_eq!(items[0].label, "HOST");
}

#[test]
fn document_symbols_outline_declared_keys_without_values() {
    let source = "\u{feff}# app\nHOST=localhost\nPORT=8080\nBARE\nBAD KEY=1\n";
    let DocumentSymbolResponse::Nested(symbols) = document_symbols(source, &opts()) else {
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
        Range::new(position(1, 0), position(1, 4))
    );
    assert_eq!(
        symbols[2].selection_range,
        Range::new(position(3, 0), position(3, 4))
    );
    assert_eq!(symbols[2].range.end, position(3, 4));

    let DocumentSymbolResponse::Nested(symbols) =
        document_symbols("\u{feff}HOST=localhost\n", &opts())
    else {
        panic!("expected nested document symbols");
    };
    assert_eq!(symbols[0].range.start, position(0, 0));
    assert_eq!(symbols[0].range.end, position(0, 14));
    assert_eq!(
        symbols[0].selection_range,
        Range::new(position(0, 0), position(0, 4))
    );
}

#[test]
fn document_symbols_keep_duplicate_declarations_and_empty_documents() {
    let DocumentSymbolResponse::Nested(symbols) = document_symbols("A=1\nA=2\n", &opts()) else {
        panic!("expected nested document symbols");
    };
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "A");
    assert_eq!(symbols[1].name, "A");
    assert_eq!(symbols[0].selection_range.start, position(0, 0));
    assert_eq!(symbols[1].selection_range.start, position(1, 0));

    let DocumentSymbolResponse::Nested(symbols) = document_symbols("# only comments\n", &opts())
    else {
        panic!("expected nested document symbols");
    };
    assert_eq!(symbols.len(), 0);
}

#[test]
fn workspace_symbols_filter_and_locate_declared_keys() {
    let source = "\u{feff}HOST=localhost\nPORT=8080\nBARE\n";
    let filtered = workspace_symbols(source, &uri(), "po", &opts());
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "PORT");
    assert_eq!(filtered[0].kind, SymbolKind::KEY);
    assert_eq!(filtered[0].location.uri, uri());
    assert_eq!(filtered[0].location.range.start, position(1, 0));
    assert_eq!(filtered[0].location.range.end, position(1, 9));

    let all = workspace_symbols(source, &uri(), "", &opts());
    let names: Vec<_> = all.iter().map(|symbol| symbol.name.as_str()).collect();
    assert_eq!(names, ["HOST", "PORT", "BARE"]);
    assert!(all.iter().all(|symbol| symbol.container_name.is_none()));
    assert_eq!(
        workspace_symbols(source, &uri(), "missing", &opts()).len(),
        0
    );
}

#[test]
fn definition_follows_a_reference() {
    let source = "HOST=localhost\nURL=${HOST}\n";
    let target = definition(source, &uri(), position(1, 7), &opts()).unwrap();
    let GotoDefinitionResponse::Scalar(location) = target else {
        panic!("expected one definition location");
    };
    assert_eq!(location.range.start, position(0, 0));
    assert_eq!(location.range.end, position(0, 4));
}

#[test]
fn definition_accepts_a_cursor_at_the_end_of_a_key() {
    let source = "HOST=localhost\n";
    let target = definition(source, &uri(), position(0, 4), &opts()).unwrap();
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
fn references_return_exact_identifier_spans_for_declarations_and_values() {
    let source = "HOST=localhost\nURL=${HOST:-fallback}\n";
    let locations = references(source, &uri(), position(0, 0), true, &opts());
    assert_eq!(locations.len(), 2);
    assert_eq!(
        locations[0].range,
        Range::new(position(0, 0), position(0, 4))
    );
    // Exact `reference.name` span (`HOST` inside `${HOST:-fallback}`), not the whole `${...}` expression.
    assert_eq!(
        locations[1].range,
        Range::new(position(1, 6), position(1, 10))
    );

    let without_declaration = references(source, &uri(), position(0, 0), false, &opts());
    assert_eq!(without_declaration.len(), 1);
    assert_eq!(
        without_declaration[0].range,
        Range::new(position(1, 6), position(1, 10))
    );
}

#[test]
#[allow(clippy::mutable_key_type)]
fn rename_uses_verified_edits_when_the_key_is_unreferenced() {
    let edit = rename(
        "HOST=localhost\n",
        &uri(),
        position(0, 0),
        "SERVER",
        &opts(),
    )
    .unwrap();
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
    let target = definition(source, &uri(), position(2, 15), &opts()).unwrap();
    let GotoDefinitionResponse::Scalar(location) = target else {
        panic!("expected one definition location");
    };
    assert_eq!(location.range.start, position(0, 0));
    assert_eq!(location.range.end, position(0, 4));

    let locations = references(source, &uri(), position(2, 15), true, &opts());
    assert_eq!(locations.len(), 2);
    assert_eq!(
        locations[0].range,
        Range::new(position(0, 0), position(0, 4))
    );
    assert_eq!(
        locations[1].range,
        Range::new(position(2, 14), position(2, 18))
    );

    let edit = rename(source, &uri(), position(2, 15), "SERVER", &opts()).unwrap();
    let edits = edit.changes.unwrap().get(&uri()).unwrap().clone();
    assert_eq!(edits.len(), 2);
    assert_eq!(edits[0].new_text, "SERVER");
    assert_eq!(edits[1].new_text, "SERVER");
}

#[test]
#[allow(clippy::mutable_key_type)]
fn rename_updates_literal_value_references() {
    let source = "HOST=localhost\nURL=${HOST}\nPORT=$HOST\n";
    let edit = rename(source, &uri(), position(0, 0), "SERVER", &opts()).unwrap();
    let changes = edit.changes.unwrap();
    let edits = changes.get(&uri()).unwrap();
    assert_eq!(edits.len(), 3);
    assert!(edits.iter().all(|edit| edit.new_text == "SERVER"));
    assert_eq!(edits[1].range, Range::new(position(1, 6), position(1, 10)));
    assert_eq!(edits[2].range, Range::new(position(2, 6), position(2, 10)));
}

#[test]
#[allow(clippy::mutable_key_type)]
fn rename_from_a_reference_targets_the_referenced_key() {
    let source = "HOST=localhost\nURL=${HOST}\n";
    let edit = rename(source, &uri(), position(1, 7), "SERVER", &opts()).unwrap();
    let edits = edit.changes.unwrap().get(&uri()).unwrap().clone();
    assert_eq!(edits.len(), 2);
    assert!(edits.iter().all(|edit| edit.new_text == "SERVER"));
}

#[test]
#[allow(clippy::mutable_key_type)]
fn rename_braces_a_dotted_reference() {
    let source = "HOST=localhost\nURL=$HOST\n";
    let edit = rename(source, &uri(), position(1, 5), "APP.HOST", &opts()).unwrap();
    let edits = edit.changes.unwrap().get(&uri()).unwrap().clone();
    assert_eq!(edits.len(), 2);
    assert_eq!(edits[1].new_text, "${APP.HOST}");
    assert_eq!(edits[1].range, Range::new(position(1, 4), position(1, 9)));
}

#[test]
#[allow(clippy::mutable_key_type)]
fn rename_keeps_bom_adjusted_positions() {
    let source = "\u{feff}HOST=localhost\nURL=${HOST}\n";
    let edit = rename(source, &uri(), position(0, 0), "SERVER", &opts()).unwrap();
    let edits = edit.changes.unwrap().get(&uri()).unwrap().clone();
    assert_eq!(edits[0].range, Range::new(position(0, 0), position(0, 4)));
    assert_eq!(edits[1].range, Range::new(position(1, 6), position(1, 10)));
}

#[test]
fn rename_requires_a_key_or_reference_position() {
    let error = rename(
        "HOST=localhost\n",
        &uri(),
        position(0, 6),
        "SERVER",
        &opts(),
    )
    .unwrap_err();
    assert!(matches!(error, RenameError::NoSymbol));
}

#[test]
fn rename_rejects_a_key_that_cannot_be_a_reference() {
    let error = rename(
        "HOST=localhost\nURL=${HOST}\n",
        &uri(),
        position(0, 0),
        "APP-HOST",
        &opts(),
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

    assert!(RenameError::NoSymbol.to_string().contains("no .env key"));
    assert!(RenameError::NoSymbol.source().is_none());
    let edit_err = RenameError::from(EditError::InvalidKey);
    assert!(edit_err.to_string().contains("rename refused:"));
    assert!(edit_err.source().is_some());

    let CompletionResponse::Array(oob) = completion("A=1\n", position(9, 0), &opts()) else {
        panic!("expected array");
    };
    assert_eq!(oob, Vec::new());
    let CompletionResponse::Array(after_sq) =
        completion("HOST=1\nURL='a' $H\n", position(1, 10), &opts())
    else {
        panic!("expected array");
    };
    assert_eq!(after_sq.len(), 1);
    assert_eq!(after_sq[0].label, "HOST");

    assert_eq!(
        apply_change(
            "A=1\n",
            Some(Range::new(position(0, 2), position(0, 1))),
            "x",
            &opts()
        ),
        None
    );
    assert_eq!(
        references("A=1\n", &uri(), position(9, 0), true, &opts()),
        Vec::new()
    );
    assert_eq!(
        references("# comment\n", &uri(), position(0, 2), true, &opts()),
        Vec::new()
    );
}

#[test]
fn parse_options_and_strip_all_bom_apply_across_lsp_operations() {
    let bounded = LspOptions::new().with_parse_options(ParseOptions::new().with_max_lines(2));
    let diags = diagnostics("A=1\nB=2\nC=3\n", &uri(), &bounded);
    assert_eq!(code(&diags[0]), Some("dotenv.limit-exceeded"));
    let rename_err = rename("A=1\nB=2\nC=3\n", &uri(), position(0, 0), "X", &bounded).unwrap_err();
    assert!(matches!(
        rename_err,
        RenameError::Edit(EditError::Limit(limit)) if limit.kind() == LimitKind::Lines
    ));

    let strip_all = LspOptions::new().with_dialect(Dialect::PORTABLE.with_bom(BomPolicy::StripAll));
    let multi = "\u{feff}\u{feff}HOST=localhost\n";
    let bom_diags = diagnostics(multi, &uri(), &strip_all);
    let bom = bom_diags
        .iter()
        .find(|d| code(d) == Some("dotenv.bom"))
        .expect("bom diagnostic");
    assert_eq!(bom.range, Range::new(position(0, 0), position(0, 1)));
    let hov = hover(multi, position(0, 0), &strip_all).unwrap();
    assert_eq!(
        hov.range.unwrap(),
        Range::new(position(0, 0), position(0, 4))
    );
    assert_eq!(
        apply_change(
            multi,
            Some(Range::new(position(0, 0), position(0, 4))),
            "NAME",
            &strip_all,
        ),
        Some("\u{feff}\u{feff}NAME=localhost\n".to_owned())
    );

    let keep_bom = LspOptions::new().with_dialect(Dialect::NODE_DOTENV.with_bom(BomPolicy::Keep));
    let single = "\u{feff}HOST=localhost\n";
    let keep_hov = hover(single, position(0, 1), &keep_bom).unwrap();
    assert_eq!(
        keep_hov.range.unwrap(),
        Range::new(position(0, 1), position(0, 5))
    );

    let CompletionResponse::Array(line1_key) = completion("A=1\nK\n", position(1, 1), &opts())
    else {
        panic!("expected array");
    };
    assert_eq!(line1_key.len(), 2);
    assert_eq!(line1_key[0].label, "A");

    assert_eq!(
        apply_change(
            "A=1\n",
            Some(Range::new(position(0, 0), position(0, 0))),
            "export ",
            &opts()
        ),
        Some("export A=1\n".to_owned())
    );

    let line1_hov = hover("\u{feff}A=1\nHOST=2\n", position(1, 0), &LspOptions::new()).unwrap();
    assert_eq!(
        line1_hov.range.unwrap(),
        Range::new(position(1, 0), position(1, 4))
    );
    assert_eq!(
        apply_change(
            "\u{feff}A=1\nHOST=2\n",
            Some(Range::new(position(1, 0), position(1, 4))),
            "NAME",
            &opts()
        ),
        Some("\u{feff}A=1\nNAME=2\n".to_owned())
    );

    let missing_key = diagnostics("=1\n", &uri(), &LspOptions::new());
    assert_eq!(
        missing_key[0].range,
        Range::new(position(0, 0), position(0, 0))
    );
    let bom_limit = diagnostics(
        "\u{feff}HOST=localhost\n",
        &uri(),
        &LspOptions::new().with_parse_options(ParseOptions::new().with_max_input_bytes(4)),
    );
    assert_eq!(code(&bom_limit[0]), Some("dotenv.limit-exceeded"));
    assert_eq!(
        bom_limit[0].range,
        Range::new(position(0, 0), position(1, 0))
    );
}

#[test]
fn bom_comment_suppresses_completion_and_hover_escapes_backticks_and_newlines() {
    let bom_comment = "\u{feff}# $HOST\nHOST=localhost\n";
    let CompletionResponse::Array(list) =
        completion(bom_comment, position(0, 7), &LspOptions::new())
    else {
        panic!("expected completion array");
    };
    assert_eq!(list.len(), 0);

    let backtick_doc = "CMD=\"echo `hi`\"\nMULTI=\"line1\\nline2\"\n";
    let tick_hover = hover(backtick_doc, position(0, 1), &LspOptions::new()).unwrap();
    let HoverContents::Markup(markup) = tick_hover.contents else {
        panic!("expected markdown hover");
    };
    assert!(markup.value.contains("`` echo `hi` ``"), "{}", markup.value);

    let multi_hover = hover(backtick_doc, position(1, 1), &LspOptions::new()).unwrap();
    let HoverContents::Markup(markup) = multi_hover.contents else {
        panic!("expected markdown hover");
    };
    assert!(
        markup.value.contains("```text\nline1\nline2\n```"),
        "{}",
        markup.value
    );

    let ref_doc = "'K`1'=ok\nUSE=${K`1}\n";
    let ref_opts = LspOptions::new().with_dialect(Dialect::PYTHON_DOTENV);
    let ref_hover = hover(ref_doc, position(1, 7), &ref_opts).unwrap();
    let HoverContents::Markup(markup) = ref_hover.contents else {
        panic!("expected markdown hover");
    };
    assert!(
        markup.value.contains("``${K`1}``; declared on line 1"),
        "{}",
        markup.value
    );

    let pad_doc = "PAD=\" x \"\nLEAD=\" x\"\nTRAIL=\"x \"\nSPACES=\"  \"\n";
    let hover_md = |line: u32| -> String {
        let h = hover(pad_doc, position(line, 1), &LspOptions::new()).unwrap();
        let HoverContents::Markup(m) = h.contents else {
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
fn rename_enforces_parse_limits_on_planned_output() {
    let bounded =
        LspOptions::new().with_parse_options(ParseOptions::new().with_max_input_bytes(24));
    let grow_err = rename(
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

    let fit = LspOptions::new().with_parse_options(ParseOptions::new().with_max_input_bytes(23));
    let ok = rename("A=1\nX=$A\nY=$A\n", &uri(), position(0, 0), "HOST", &fit).unwrap();
    assert_eq!(ok.changes.unwrap().into_values().next().unwrap().len(), 3);

    let shrink_opts =
        LspOptions::new().with_parse_options(ParseOptions::new().with_max_input_bytes(24));
    let shrunk = rename(
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

    let zero_ref_opts =
        LspOptions::new().with_parse_options(ParseOptions::new().with_max_input_bytes(8));
    let zero_ref_err = rename(
        "A=1\n",
        &uri(),
        position(0, 0),
        "VERY_LONG_NAME",
        &zero_ref_opts,
    )
    .unwrap_err();
    assert!(
        matches!(zero_ref_err, RenameError::Edit(EditError::Limit(limit)) if limit.kind() == LimitKind::InputBytes),
        "{zero_ref_err:?}"
    );
    let zero_ref_ok = rename("A=1\n", &uri(), position(0, 0), "LONG", &zero_ref_opts).unwrap();
    assert_eq!(
        zero_ref_ok
            .changes
            .unwrap()
            .into_values()
            .next()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn rename_enforces_lines_diagnostics_and_items_limits() {
    let line_opts = LspOptions::new().with_parse_options(ParseOptions::new().with_max_lines(1));
    let line_err = rename("A=1\nX=$A\n", &uri(), position(0, 0), "B", &line_opts).unwrap_err();
    assert!(
        matches!(line_err, RenameError::Edit(EditError::Limit(limit)) if limit.kind() == LimitKind::Lines),
        "{line_err:?}"
    );
    let line_ok_opts = LspOptions::new().with_parse_options(ParseOptions::new().with_max_lines(2));
    assert!(rename("A=1\nX=$A\n", &uri(), position(0, 0), "B", &line_ok_opts).is_ok());

    let diag_zero =
        LspOptions::new().with_parse_options(ParseOptions::new().with_max_diagnostics(0));
    let diag_err = rename(
        "\u{feff}A=1\nX=$A\n",
        &uri(),
        position(0, 0),
        "B",
        &diag_zero,
    )
    .unwrap_err();
    assert!(
        matches!(diag_err, RenameError::Edit(EditError::Limit(limit)) if limit.kind() == LimitKind::Diagnostics),
        "{diag_err:?}"
    );
    let verify_diag_err =
        rename("A=1\nX=$A\n", &uri(), position(0, 0), "APP.A", &diag_zero).unwrap_err();
    assert!(
        matches!(verify_diag_err, RenameError::Edit(EditError::Limit(limit)) if limit.kind() == LimitKind::Diagnostics),
        "{verify_diag_err:?}"
    );
    let diag_one =
        LspOptions::new().with_parse_options(ParseOptions::new().with_max_diagnostics(1));
    assert!(rename(
        "\u{feff}A=1\nX=$A\n",
        &uri(),
        position(0, 0),
        "B",
        &diag_one
    )
    .is_ok());
    assert!(rename("A=1\nX=$A\n", &uri(), position(0, 0), "APP.A", &diag_one).is_ok());

    let item_opts = LspOptions::new().with_parse_options(ParseOptions::new().with_max_items(2));
    let item_err =
        rename("A=1\nX=$A\nY=$A\n", &uri(), position(0, 0), "B", &item_opts).unwrap_err();
    assert!(
        matches!(item_err, RenameError::Edit(EditError::Limit(limit)) if limit.kind() == LimitKind::Items),
        "{item_err:?}"
    );
}

#[test]
fn crlf_lf_only_dialects_and_bom_edge_cases_produce_valid_ranges() {
    for dialect in [Dialect::DOCKER_RAW, Dialect::COMPOSE] {
        let options = LspOptions::new().with_dialect(dialect);
        let source = "HOST=localhost\r\nPORT=8080\r\n";
        let DocumentSymbolResponse::Nested(symbols) = document_symbols(source, &options) else {
            panic!("expected nested document symbols");
        };
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].range.start, position(0, 0));
        assert_eq!(symbols[0].range.end, position(0, 14));
        assert_eq!(symbols[1].range.start, position(1, 0));
        assert_eq!(symbols[1].range.end, position(1, 9));
        for sym in &symbols {
            assert!(sym.range.start <= sym.selection_range.start);
            assert!(sym.selection_range.end <= sym.range.end);
        }
        let ws = workspace_symbols(source, &uri(), "", &options);
        assert_eq!(ws[1].location.range.start, position(1, 0));
        assert_eq!(ws[1].location.range.end, position(1, 9));
    }

    let bom_err = LspOptions::new().with_dialect(Dialect::PORTABLE.with_bom(BomPolicy::Error));
    let diags = diagnostics("\u{feff}HOST=1\n", &uri(), &bom_err);
    assert_eq!(code(&diags[0]), Some("dotenv.forbidden-bom"));
    assert_eq!(diags[0].range, Range::new(position(0, 0), position(0, 1)));
    let hov = hover("\u{feff}HOST=1\n", position(0, 1), &bom_err).unwrap();
    assert_eq!(
        hov.range.unwrap(),
        Range::new(position(0, 1), position(0, 5))
    );

    for empty_bom in ["\u{feff}", "\u{feff}\n"] {
        let diags = diagnostics(empty_bom, &uri(), &LspOptions::new());
        let bom = diags
            .iter()
            .find(|d| code(d) == Some("dotenv.bom"))
            .expect("bom diagnostic");
        assert_eq!(bom.range, Range::new(position(0, 0), position(0, 0)));
    }

    let astral_diags = diagnostics("\u{feff}😀=1\n", &uri(), &LspOptions::new());
    let astral_bom = astral_diags
        .iter()
        .find(|d| code(d) == Some("dotenv.bom"))
        .expect("bom diagnostic");
    assert_eq!(astral_bom.range, Range::new(position(0, 0), position(0, 2)));

    let crlf_escape = diagnostics("A=\"\\\r\n\"\n", &uri(), &LspOptions::new());
    let esc = crlf_escape
        .iter()
        .find(|d| code(d) == Some("dotenv.unknown-escape"))
        .expect("unknown-escape diagnostic");
    assert_eq!(esc.range, Range::new(position(0, 3), position(0, 4)));

    assert!(definition("HOST", &uri(), position(0, 4), &opts()).is_some());
}

#[test]
fn rename_recovers_unrelated_invalid_lines_and_enforces_command_allowlist() {
    let source = "BAD KEY=1\nHOST=localhost\nURL=${HOST}\n";
    let edit = rename(source, &uri(), position(1, 0), "SERVER", &opts()).unwrap();
    let edits = &edit.changes.unwrap()[&uri()];
    assert_eq!(edits.len(), 2);
    assert_eq!(edits[0].range.start, position(1, 0));
    assert_eq!(edits[1].range.start, position(2, 6));

    let cmd_err = rename(
        "HOST=localhost\nCMD=$(echo $HOST)\n",
        &uri(),
        position(0, 0),
        "1BAD",
        &LspOptions::new(),
    )
    .unwrap_err();
    assert_eq!(cmd_err, RenameError::from(EditError::WouldChangeMeaning));
    assert_eq!(cmd_err.clone(), cmd_err);

    let with_lint = LspOptions::new().with_lint_options(LintOptions::new().with_key_order(true));
    assert!(with_lint.lint.is_some());
    assert!(with_lint.without_lint_options().lint.is_none());
}

#[test]
fn completion_handles_multiline_dialects_escapes_and_dotted_braced_refs() {
    let items_len = |source: &str, pos: Position, dialect: Dialect| -> usize {
        let options = LspOptions::new().with_dialect(dialect);
        let CompletionResponse::Array(items) = completion(source, pos, &options) else {
            panic!("expected completion array");
        };
        items.len()
    };

    assert_eq!(
        items_len(
            "HOST=1\nURL='line1\n$H'\n",
            position(2, 2),
            Dialect::PORTABLE
        ),
        0
    );
    assert_eq!(
        items_len("HOST=1\nURL='line1\n$H", position(2, 2), Dialect::PORTABLE),
        0
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL=\"line1\nfoo\"\n",
            position(2, 3),
            Dialect::PORTABLE
        ),
        0
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL=\"line1\n$H\"\n",
            position(2, 2),
            Dialect::PORTABLE
        ),
        2
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL=\"line1\nfoo",
            position(2, 3),
            Dialect::PORTABLE
        ),
        0
    );
    assert_eq!(
        items_len("HOST=1\nURL=\"line1\n$H", position(2, 2), Dialect::PORTABLE),
        1
    );
    assert_eq!(
        items_len("HOST=1\nURL=`$H`\n", position(1, 7), Dialect::NODE_DOTENV),
        0
    );
    assert_eq!(
        items_len("HOST=1\nURL: '$H'\n", position(1, 8), Dialect::COMPOSE),
        0
    );
    assert_eq!(
        items_len("HOST=1\nURL: $H\n", position(1, 7), Dialect::COMPOSE),
        2
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL='it\\'s $H'\n",
            position(1, 13),
            Dialect::PYTHON_DOTENV
        ),
        0
    );
    assert_eq!(
        items_len("HOST=1\nURL='$H\n", position(1, 7), Dialect::DOCKER_RAW),
        2
    );
}

#[test]
fn completion_handles_dotted_braced_refs_and_escaped_dollars() {
    let items_len = |source: &str, pos: Position, dialect: Dialect| -> usize {
        let options = LspOptions::new().with_dialect(dialect);
        let CompletionResponse::Array(items) = completion(source, pos, &options) else {
            panic!("expected completion array");
        };
        items.len()
    };

    assert_eq!(
        items_len(
            "APP.HOST=1\nURL=${APP.H\n",
            position(1, 11),
            Dialect::PORTABLE
        ),
        2
    );
    assert_eq!(
        items_len(
            "APP.HOST=1\nURL=$APP.H\n",
            position(1, 10),
            Dialect::PORTABLE
        ),
        0
    );
    assert_eq!(
        items_len("HOST=1\nURL=\"\\$H\"\n", position(1, 8), Dialect::PORTABLE),
        0
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL=\"\\\\$H\"\n",
            position(1, 9),
            Dialect::PORTABLE
        ),
        2
    );
    assert_eq!(
        items_len("HOST=1\nURL=$$H\n", position(1, 7), Dialect::COMPOSE),
        0
    );
    assert_eq!(
        items_len("HOST=1\nURL=$$$H\n", position(1, 8), Dialect::COMPOSE),
        2
    );
}

#[test]
fn hover_escapes_multiline_quoted_keys() {
    let py = LspOptions::new().with_dialect(Dialect::PYTHON_DOTENV);
    let hov = hover("'A\n\nB'=1\n", position(0, 1), &py).unwrap();
    let HoverContents::Markup(markup) = hov.contents else {
        panic!("expected markup hover");
    };
    assert!(markup.value.starts_with("`A\\n\\nB`"), "{}", markup.value);
}

#[test]
fn completion_matches_parser_quotes_comments_and_escape_rules() {
    let items_len = |source: &str, pos: Position, dialect: Dialect| -> usize {
        let options = LspOptions::new().with_dialect(dialect);
        let CompletionResponse::Array(items) = completion(source, pos, &options) else {
            panic!("expected completion array");
        };
        items.len()
    };

    assert_eq!(
        items_len("HOST=1\nMSG=it's $H\n", position(1, 11), Dialect::PORTABLE),
        2
    );
    assert_eq!(
        items_len(
            "HOST=1\nMSG=a\"b # $H\n",
            position(1, 12),
            Dialect::PORTABLE
        ),
        0
    );
    assert_eq!(
        items_len(
            "HOST=1\nMSG=a`b $H\n",
            position(1, 10),
            Dialect::NODE_DOTENV
        ),
        2
    );
    assert_eq!(
        items_len("HOST=1\nA\"B=foo\n", position(1, 6), Dialect::PYTHON_DOTENV),
        0
    );
    assert_eq!(
        items_len("HOST=1\nURL=\\$H\n", position(1, 7), Dialect::PORTABLE),
        2
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL=\"\\$H\"\n",
            position(1, 8),
            Dialect::NODE_DOTENV
        ),
        2
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL=\"\\$H\"\n",
            position(1, 8),
            Dialect::PYTHON_DOTENV
        ),
        2
    );
    assert_eq!(
        items_len("HOST=1\nURL=\"\\$H\"\n", position(1, 8), Dialect::COMPOSE),
        0
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL=\"${A:-\\$H\"\n",
            position(1, 13),
            Dialect::PORTABLE
        ),
        2
    );
    assert_eq!(
        items_len("HOST=1\nBAD $H\n", position(1, 6), Dialect::PORTABLE),
        0
    );
    assert_eq!(
        items_len("HOST=1\nURL=$100\n", position(1, 8), Dialect::PORTABLE),
        0
    );
    assert_eq!(
        items_len("HOST=1\nURL=$é\n", position(1, 6), Dialect::PORTABLE),
        0
    );
}

#[test]
fn completion_matches_dialect_comments_node_quotes_and_unclosed_recovery() {
    let items_len = |source: &str, pos: Position, dialect: Dialect| -> usize {
        let options = LspOptions::new().with_dialect(dialect);
        let CompletionResponse::Array(items) = completion(source, pos, &options) else {
            panic!("expected completion array");
        };
        items.len()
    };

    assert_eq!(
        items_len("HOST=1\nexport H\n", position(1, 8), Dialect::DOCKER_RAW),
        0
    );
    assert_eq!(
        items_len("HOST=1\nURL= #$H\n", position(1, 8), Dialect::COMPOSE),
        2
    );
    assert_eq!(
        items_len("HOST=1\nURL=\"a\"#$H\n", position(1, 10), Dialect::COMPOSE),
        0
    );
    assert_eq!(
        items_len("HOST=1\nURL='a\\\\'$H\n", position(1, 11), Dialect::COMPOSE),
        2
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL=\"a\\\" # $H\n",
            position(1, 13),
            Dialect::NODE_DOTENV
        ),
        0
    );
    assert_eq!(
        items_len(
            "HOST=1\nA=\"unclosed\nB='unclosed\n$H",
            position(3, 2),
            Dialect::PORTABLE
        ),
        0
    );
    assert_eq!(
        items_len(
            "BROKEN='unclosed\nHOST=1\nURL=$H\n",
            position(2, 6),
            Dialect::PORTABLE
        ),
        2
    );
    let ignore_malformed = Dialect::PORTABLE.with_malformed_lines(adotenv::MalformedLines::Ignore);
    assert_eq!(
        items_len("HOST=1\nURL='line1\n$H", position(2, 2), ignore_malformed),
        0
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL=$(echo ')' \\$H)\n",
            position(1, 20),
            Dialect::PORTABLE
        ),
        0
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL=$(echo \\${a)} \\$H)\n",
            position(1, 21),
            Dialect::PORTABLE
        ),
        0
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL=$(echo \\${a)} $H)\n",
            position(1, 20),
            Dialect::PORTABLE
        ),
        2
    );
    assert_eq!(
        items_len(
            "HOST=1\nURL=$(echo \"${A:-\\$H}\")\n",
            position(1, 20),
            Dialect::PORTABLE
        ),
        2
    );
    let py_error_tail = Dialect::PYTHON_DOTENV.with_quoted_tail(adotenv::QuotedTail::Error);
    assert_eq!(
        items_len("HOST=1\nA='\\\\' $H", position(1, 9), py_error_tail),
        1
    );
    let node_custom_sq = Dialect::NODE_DOTENV.with_single_escapes(adotenv::SingleEscapes::Quote);
    assert_eq!(
        items_len("HOST=1\nA='\\' $H\n", position(1, 8), node_custom_sq),
        2
    );
    let custom_backtick = Dialect::PORTABLE.with_backticks(true);
    assert_eq!(
        items_len("HOST=1\nURL=`a\\` $H`\n", position(1, 11), custom_backtick),
        0
    );
}

#[test]
fn position_encoding_negotiation_and_multibyte_coordinates() {
    assert_eq!(PositionEncoding::negotiate(None), PositionEncoding::Utf16);
    assert_eq!(
        PositionEncoding::negotiate(Some(&[])),
        PositionEncoding::Utf16
    );
    assert_eq!(
        PositionEncoding::negotiate(Some(&[PositionEncodingKind::new("unknown")])),
        PositionEncoding::Utf16
    );
    assert_eq!(
        PositionEncoding::negotiate(Some(&[
            PositionEncodingKind::UTF8,
            PositionEncodingKind::UTF16
        ])),
        PositionEncoding::Utf8
    );
    assert_eq!(
        PositionEncoding::negotiate(Some(&[PositionEncodingKind::UTF32])),
        PositionEncoding::Utf32
    );
    assert_eq!(PositionEncoding::Utf8.as_str(), "utf-8");
    assert_eq!(PositionEncoding::Utf16.as_str(), "utf-16");
    assert_eq!(PositionEncoding::Utf32.as_str(), "utf-32");
    assert_eq!(
        PositionEncoding::from_lsp_kind(&PositionEncoding::Utf8.to_lsp_kind()),
        Some(PositionEncoding::Utf8)
    );

    // `😀` takes 4 UTF-8 bytes, 2 UTF-16 code units, and 1 UTF-32 scalar.
    let source = "HOST=😀\nURL=😀${HOST}\n";
    for (encoding, start_col, end_col, bom_width) in [
        (PositionEncoding::Utf8, 10, 14, 4),
        (PositionEncoding::Utf16, 8, 12, 2),
        (PositionEncoding::Utf32, 7, 11, 1),
    ] {
        let options = LspOptions::new().with_position_encoding(encoding);
        let locs = references(source, &uri(), position(0, 0), false, &options);
        assert_eq!(
            locs[0].range,
            Range::new(position(1, start_col), position(1, end_col)),
            "{encoding:?}"
        );
        // BOM diagnostic anchors to full encoded width of first visible char `😀`.
        let bom_diags = diagnostics("\u{feff}😀=1\n", &uri(), &options);
        let bom = bom_diags
            .iter()
            .find(|d| code(d) == Some("dotenv.bom"))
            .unwrap();
        assert_eq!(
            bom.range,
            Range::new(position(0, 0), position(0, bom_width)),
            "{encoding:?}"
        );
    }

    // UTF-8 rejects mid-character byte columns instead of panicking in replace_range.
    let utf8 = LspOptions::new().with_position_encoding(PositionEncoding::Utf8);
    assert_eq!(
        apply_change(
            "A=😀\n",
            Some(Range::new(position(0, 3), position(0, 6))),
            "x",
            &utf8
        ),
        None
    );
    assert_eq!(
        apply_change(
            "A=😀\n",
            Some(Range::new(position(0, 2), position(0, 6))),
            "x",
            &utf8
        ),
        Some("A=x\n".to_owned())
    );
    // UTF-32 replaces emoji at 2..3.
    let utf32 = LspOptions::new().with_position_encoding(PositionEncoding::Utf32);
    assert_eq!(
        apply_change(
            "A=😀\n",
            Some(Range::new(position(0, 2), position(0, 3))),
            "x",
            &utf32
        ),
        Some("A=x\n".to_owned())
    );
}

#[test]
fn prepare_rename_validates_keys_references_and_limits() {
    let source = "HOST=localhost\nURL=${HOST:-default}/${MISSING}\n";
    assert_eq!(
        prepare_rename(source, position(0, 2), &opts()),
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(position(0, 0), position(0, 4)),
            placeholder: "HOST".to_owned(),
        })
    );
    assert_eq!(
        prepare_rename(source, position(1, 7), &opts()),
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(position(1, 6), position(1, 10)),
            placeholder: "HOST".to_owned(),
        })
    );
    // Unresolved reference, value text, and comment are not renameable.
    assert_eq!(prepare_rename(source, position(1, 25), &opts()), None);
    assert_eq!(prepare_rename(source, position(0, 8), &opts()), None);
    assert_eq!(prepare_rename("# HOST=1\n", position(0, 3), &opts()), None);

    // Limit-exceeded document rejects prepareRename.
    let limited = LspOptions::new().with_parse_options(ParseOptions::new().with_max_lines(1));
    assert_eq!(prepare_rename("A=1\nB=2\n", position(0, 0), &limited), None);

    // Python quoted key returns inner key span and decoded placeholder.
    let py = LspOptions::new().with_dialect(Dialect::PYTHON_DOTENV);
    assert_eq!(
        prepare_rename("'A B'=1\n", position(0, 2), &py),
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(position(0, 1), position(0, 4)),
            placeholder: "A B".to_owned(),
        })
    );
}

#[test]
fn formatting_honors_lsp_options_lint_policy_and_parse_limits() {
    use std::error::Error as _;

    // Default formatting trims trailing whitespace and ensures a final newline.
    let default_fmt = FormattingOptions::default();
    let edits = formatting("A=1   \nB=2", &default_fmt, &opts()).unwrap();
    assert_eq!(edits.len(), 2);

    // Explicit FormattingOptions false overrides defaults.
    let no_changes = FormattingOptions {
        trim_trailing_whitespace: Some(false),
        insert_final_newline: Some(false),
        ..FormattingOptions::default()
    };
    assert_eq!(
        formatting("A=1   \nB=2", &no_changes, &opts()).unwrap(),
        Vec::new()
    );

    // Lint policy quote_style = Unquoted works without forced extra flags.
    let unquoted_lint = LspOptions::new()
        .with_lint_options(LintOptions::new().with_quote_style(QuoteStyle::Unquoted));
    let unquoted_edits = formatting("A=\"plain\"\n", &default_fmt, &unquoted_lint).unwrap();
    assert_eq!(unquoted_edits.len(), 1);
    assert_eq!(unquoted_edits[0].new_text, "plain");

    // Explicit LspOptions::with_format_options overrides default/lint policy.
    let explicit = LspOptions::new().with_format_options(
        FormatOptions::default()
            .with_equals_spacing(EqualsSpacing::Spaces)
            .with_final_newline(FinalNewline::Ensure),
    );
    assert!(explicit.format.is_some());
    assert!(explicit.without_format_options().format.is_none());
    let spaced = formatting("A=1\n", &default_fmt, &explicit).unwrap();
    assert_eq!(spaced.len(), 2);
    let mut formatted_text = "A=1\n".to_owned();
    for edit in spaced.iter().rev() {
        formatted_text =
            apply_change(&formatted_text, Some(edit.range), &edit.new_text, &explicit).unwrap();
    }
    assert_eq!(formatted_text, "A = 1\n");

    // Parse limits reject both already-truncated documents and expanding output.
    let input_limited = LspOptions::new().with_parse_options(ParseOptions::new().with_max_lines(1));
    let err = formatting("A=1\nB=2\n", &default_fmt, &input_limited).unwrap_err();
    assert!(matches!(err, FormattingError::Limit(l) if l.kind() == LimitKind::Lines));
    assert!(err.to_string().contains("formatting refused:"));
    assert!(err.source().is_none());
    let fmt_err = FormattingError::from(adotenv_lsp::FormatError::WouldChangeMeaning);
    assert!(fmt_err.to_string().contains("formatting refused:"));
    assert!(fmt_err.source().is_some());

    let output_limited = LspOptions::new()
        .with_format_options(FormatOptions::default().with_equals_spacing(EqualsSpacing::Spaces))
        .with_parse_options(ParseOptions::new().with_max_input_bytes(4));
    let grow_err = formatting("A=1\n", &default_fmt, &output_limited).unwrap_err();
    assert!(matches!(grow_err, FormattingError::Limit(l) if l.kind() == LimitKind::InputBytes));
}

#[test]
fn code_actions_provide_quickfixes_source_fix_all_and_bom_preservation() {
    let lint = LintOptions::new()
        .with_duplicate_keys(false)
        .with_key_case(KeyCase::Upper)
        .with_key_order(true)
        .with_trim_trailing_whitespace(true);
    let options = LspOptions::new().with_lint_options(lint);

    // Without lint policy, code actions are empty.
    let all_range = Range::new(position(0, 0), position(5, 0));
    let empty_ctx = CodeActionContext::default();
    assert_eq!(
        code_actions("b=2  \na=1  \n", &uri(), all_range, &empty_ctx, &opts()),
        Vec::new()
    );

    // Multi-issue BOM document produces deduplicated quickfixes + BOM-safe source.fixAll.
    let source = "\u{feff}b=2  \na=1  \n";
    let actions = code_actions(source, &uri(), all_range, &empty_ctx, &options);
    let quickfixes: Vec<_> = actions
        .iter()
        .filter_map(|item| match item {
            CodeActionOrCommand::CodeAction(a) if a.kind == Some(CodeActionKind::QUICKFIX) => {
                Some(a)
            }
            _ => None,
        })
        .collect();
    // 2 KeyCase renames + 1 Sort + 1 deduplicated Format (combining both Whitespace diagnostics).
    assert_eq!(quickfixes.len(), 4);
    let format_fix = quickfixes
        .iter()
        .find(|a| a.title.contains("Format document"))
        .unwrap();
    assert_eq!(format_fix.diagnostics.as_ref().unwrap().len(), 2);

    // `only: [source.fixAll]` returns only the batch fixAll action and preserves single BOM.
    let fix_all_ctx = CodeActionContext {
        only: Some(vec![CodeActionKind::SOURCE_FIX_ALL]),
        ..CodeActionContext::default()
    };
    let fix_all = code_actions(source, &uri(), all_range, &fix_all_ctx, &options);
    assert_eq!(fix_all.len(), 1);
    let CodeActionOrCommand::CodeAction(action) = &fix_all[0] else {
        panic!("expected code action");
    };
    assert_eq!(action.kind, Some(CodeActionKind::SOURCE_FIX_ALL));
    let edits = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri()];
    assert_eq!(edits.len(), 1);
    let applied = apply_change(source, Some(edits[0].range), &edits[0].new_text, &options).unwrap();
    assert_eq!(applied, "\u{feff}A=1\nB=2\n");

    // Parent kind prefix `"source"` matches `"source.fixAll"`.
    let parent_ctx = CodeActionContext {
        only: Some(vec![CodeActionKind::SOURCE]),
        ..CodeActionContext::default()
    };
    assert_eq!(
        code_actions(source, &uri(), all_range, &parent_ctx, &options).len(),
        1
    );

    // Unrelated invalid line still allows quickfix + fallback single-stage fixAll on valid entries.
    let broken = "BAD KEY=1\nhost=1\n";
    let broken_opts = LspOptions::new().with_lint_options(
        LintOptions::new()
            .with_duplicate_keys(false)
            .with_key_case(KeyCase::Upper),
    );
    let broken_actions = code_actions(broken, &uri(), all_range, &empty_ctx, &broken_opts);
    assert_eq!(broken_actions.len(), 2);
}

#[test]
fn parsed_document_and_session_cache_state_and_apply_changes_atomically() {
    let options = LspOptions::new().with_mask_values(true);
    let mut doc = ParsedDocument::parse("HOST=secret\nURL=${HOST}\n", options);
    assert_eq!(doc.options(), options);
    assert_eq!(doc.document().entries().count(), 2);
    assert!(doc.apply_changes(&[]));

    // Atomic rollback: change #2 is invalid (line 99), so change #1 must not persist.
    assert!(!doc.apply_changes(&[
        TextDocumentContentChangeEvent {
            range: Some(Range::new(position(0, 0), position(0, 4))),
            range_length: None,
            text: "NAME".to_owned(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range::new(position(99, 0), position(99, 1))),
            range_length: None,
            text: "x".to_owned(),
        },
    ]));
    assert_eq!(doc.source(), "HOST=secret\nURL=${HOST}\n");

    let updated = doc
        .apply_change(Some(Range::new(position(0, 5), position(0, 11))), "public")
        .unwrap();
    assert_eq!(updated.source(), "HOST=public\nURL=${HOST}\n");

    // Multi-document Session.
    let mut session = Session::new(options);
    assert!(session.is_empty());
    let u1: Uri = "file:///tmp/a.env".parse().unwrap();
    let u2: Uri = "file:///tmp/b.env".parse().unwrap();
    assert_eq!(
        session.change(&u1, Some(1), &[]),
        Err(SessionError::NotOpen)
    );
    assert!(SessionError::NotOpen.to_string().contains("not open"));
    assert!(SessionError::InvalidChange.to_string().contains("invalid"));

    session.open(u1.clone(), Some(1), "ZEBRA=1\n");
    session.open(u2.clone(), Some(1), "ALPHA=2\n");
    assert_eq!(session.len(), 2);
    assert_eq!(session.version(&u1), Some(1));

    assert_eq!(
        session.change(
            &u1,
            Some(2),
            &[TextDocumentContentChangeEvent {
                range: Some(Range::new(position(5, 0), position(5, 1))),
                range_length: None,
                text: "x".to_owned(),
            }]
        ),
        Err(SessionError::InvalidChange)
    );
    assert_eq!(session.version(&u1), Some(1));

    session
        .change(
            &u1,
            Some(2),
            &[TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "BETA=3\n".to_owned(),
            }],
        )
        .unwrap();
    assert_eq!(session.version(&u1), Some(2));
    assert_eq!(session.document(&u1).unwrap().source(), "BETA=3\n");

    let syms = session.workspace_symbols("");
    let names: Vec<_> = syms.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["ALPHA", "BETA"]);

    assert!(session.close(&u1));
    assert!(!session.close(&u1));
    assert_eq!(session.len(), 1);
}

#[test]
#[allow(clippy::too_many_lines)]
fn prepare_rename_rejects_duplicates_disjoint_operator_text_and_handles_edge_cases() {
    // Duplicate keys cannot be renamed by `Document::rename_with_references_with_options`.
    let dup = "HOST=1\nHOST=2\nURL=${HOST}\n";
    assert_eq!(prepare_rename(dup, position(0, 1), &opts()), None);
    assert_eq!(prepare_rename(dup, position(1, 1), &opts()), None);
    assert_eq!(prepare_rename(dup, position(2, 7), &opts()), None);

    // Non-lookup unique declarations (`__proto__`, Node `BareKeys::Ignore`) are accepted by `unique()`.
    let node = LspOptions::new().with_dialect(Dialect::NODE_DOTENV);
    assert_eq!(
        prepare_rename("__proto__=1\nURL=${__proto__}\n", position(1, 8), &node),
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(position(1, 6), position(1, 15)),
            placeholder: "__proto__".to_owned(),
        })
    );
    let ignore_bare =
        LspOptions::new().with_dialect(Dialect::PORTABLE.with_bare_keys(adotenv::BareKeys::Ignore));
    assert_eq!(
        prepare_rename("BARE\nURL=$BARE\n", position(1, 6), &ignore_bare),
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(position(1, 5), position(1, 9)),
            placeholder: "BARE".to_owned(),
        })
    );

    // Disjoint cursor on operator/fallback text or `$`/`{`/`}` returns None;
    // cursor inside or at end of identifier `HOST` (columns 6..=10) returns Some.
    let op_doc = "HOST=localhost\nURL=${HOST:-fallback}\n";
    for col in [4, 5, 11, 12, 13, 18, 20] {
        assert_eq!(
            prepare_rename(op_doc, position(1, col), &opts()),
            None,
            "col {col}"
        );
    }
    for col in [6, 8, 10] {
        assert_eq!(
            prepare_rename(op_doc, position(1, col), &opts()),
            Some(PrepareRenameResponse::RangeWithPlaceholder {
                range: Range::new(position(1, 6), position(1, 10)),
                placeholder: "HOST".to_owned(),
            }),
            "col {col}"
        );
    }

    // Nested `${OUTER:-$INNER}` resolves innermost name only when cursor is on that name.
    let nested = "OUTER=1\nINNER=2\nURL=${OUTER:-$INNER}\n";
    assert_eq!(
        prepare_rename(nested, position(2, 7), &opts()),
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(position(2, 6), position(2, 11)),
            placeholder: "OUTER".to_owned(),
        })
    );
    assert_eq!(
        prepare_rename(nested, position(2, 15), &opts()),
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(position(2, 14), position(2, 19)),
            placeholder: "INNER".to_owned(),
        })
    );
    assert_eq!(prepare_rename(nested, position(2, 12), &opts()), None);

    // Bare key at EOF (without trailing newline) is accepted; empty Compose key is rejected.
    assert_eq!(
        prepare_rename("HOST", position(0, 4), &opts()),
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(position(0, 0), position(0, 4)),
            placeholder: "HOST".to_owned(),
        })
    );
    let compose = LspOptions::new().with_dialect(Dialect::COMPOSE);
    assert_eq!(prepare_rename("=1\n", position(0, 0), &compose), None);
}

#[test]
fn formatting_supports_two_pass_unquoted_and_preserves_remove_final_newline() {
    let apply_all = |src: &str, edits: &[lsp_types::TextEdit], options: &LspOptions| -> String {
        let mut out = src.to_owned();
        for edit in edits.iter().rev() {
            out = apply_change(&out, Some(edit.range), &edit.new_text, options).unwrap();
        }
        out
    };

    // QuoteStyle::Unquoted combined with editor insertFinalNewline + trimTrailingWhitespace.
    let unquoted = LspOptions::new()
        .with_lint_options(LintOptions::new().with_quote_style(QuoteStyle::Unquoted));
    let editor_opts = FormattingOptions {
        trim_trailing_whitespace: Some(true),
        insert_final_newline: Some(true),
        ..FormattingOptions::default()
    };
    let src = "A=\"plain\"  \nB=\"two\"";
    let edits = formatting(src, &editor_opts, &unquoted).unwrap();
    assert_eq!(apply_all(src, &edits, &unquoted), "A=plain\nB=two\n");

    // Two-pass unquoted preserves leading BOM.
    let bom_src = "\u{feff}A=\"plain\"  \nB=\"two\"";
    let bom_edits = formatting(bom_src, &editor_opts, &unquoted).unwrap();
    assert_eq!(
        apply_all(bom_src, &bom_edits, &unquoted),
        "\u{feff}A=plain\nB=two\n"
    );

    // When only pass 1 or only pass 2 changes anything, edits stay granular.
    let combo = LspOptions::new().with_format_options(
        FormatOptions::default()
            .with_quote_style(QuoteStyle::Unquoted)
            .with_equals_spacing(EqualsSpacing::Spaces),
    );
    let only_pass1 = formatting("A = \"plain\"\n", &FormattingOptions::default(), &combo).unwrap();
    assert_eq!(only_pass1.len(), 1);
    assert_eq!(only_pass1[0].new_text, "plain");
    let only_pass2 = formatting("A=plain\n", &FormattingOptions::default(), &combo).unwrap();
    assert_eq!(only_pass2.len(), 2);
    assert_eq!(apply_all("A=plain\n", &only_pass2, &combo), "A = plain\n");

    // Explicit FinalNewline::Remove is preserved when editor passes insertFinalNewline = false.
    let remove_nl = LspOptions::new()
        .with_format_options(FormatOptions::default().with_final_newline(FinalNewline::Remove));
    let false_nl = FormattingOptions {
        insert_final_newline: Some(false),
        ..FormattingOptions::default()
    };
    let removed = formatting("A=1\n", &false_nl, &remove_nl).unwrap();
    assert_eq!(apply_all("A=1\n", &removed, &remove_nl), "A=1");

    // Explicit FinalNewline::Ensure is downgraded to Preserve when insertFinalNewline = false.
    let ensure_nl = LspOptions::new()
        .with_format_options(FormatOptions::default().with_final_newline(FinalNewline::Ensure));
    assert_eq!(
        formatting("A=1", &false_nl, &ensure_nl).unwrap(),
        Vec::new()
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn code_actions_cap_quickfixes_and_cover_bom_limits_and_filter_edge_cases() {
    use std::fmt::Write as _;

    // 80 lowercase keys: quickfix capped at 64, titles never leak values, source.fixAll fixes all 80.
    let mut many = String::new();
    for i in 0..80 {
        let _ = writeln!(many, "k{i:02}=super_secret_value");
    }
    let lint_upper = LspOptions::new().with_lint_options(
        LintOptions::new()
            .with_duplicate_keys(false)
            .with_key_case(KeyCase::Upper),
    );
    let full = Range::new(position(0, 0), position(100, 0));
    let actions = code_actions(
        &many,
        &uri(),
        full,
        &CodeActionContext::default(),
        &lint_upper,
    );
    let quickfixes: Vec<_> = actions
        .iter()
        .filter_map(|item| match item {
            CodeActionOrCommand::CodeAction(a) if a.kind == Some(CodeActionKind::QUICKFIX) => {
                Some(a)
            }
            _ => None,
        })
        .collect();
    assert_eq!(quickfixes.len(), 64);
    assert!(quickfixes
        .iter()
        .all(|a| !a.title.contains("super_secret_value")));
    let fix_all = actions
        .iter()
        .find_map(|item| match item {
            CodeActionOrCommand::CodeAction(a)
                if a.kind == Some(CodeActionKind::SOURCE_FIX_ALL) =>
            {
                Some(a)
            }
            _ => None,
        })
        .expect("source.fixAll action");
    let edits = &fix_all.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri()];
    let mut fixed = many.clone();
    for edit in edits.iter().rev() {
        fixed = apply_change(&fixed, Some(edit.range), &edit.new_text, &lint_upper).unwrap();
    }
    assert!(fixed.contains("K00=super_secret_value"));
    assert!(fixed.contains("K79=super_secret_value"));
    assert!(!fixed.contains("k79="));

    // Inverted range returns empty.
    let inverted = Range::new(position(2, 0), position(1, 0));
    assert_eq!(
        code_actions(
            "a=1\n",
            &uri(),
            inverted,
            &CodeActionContext::default(),
            &lint_upper
        ),
        Vec::new()
    );

    // `only` filtering edge cases.
    let filter = |kinds: Vec<CodeActionKind>| {
        code_actions(
            "a=1\n",
            &uri(),
            full,
            &CodeActionContext {
                only: Some(kinds),
                ..CodeActionContext::default()
            },
            &lint_upper,
        )
        .len()
    };
    assert_eq!(filter(Vec::new()), 0);
    assert_eq!(filter(vec![CodeActionKind::REFACTOR]), 0);
    assert_eq!(filter(vec![CodeActionKind::new("source.fixAll.extra")]), 0);
    assert_eq!(filter(vec![CodeActionKind::new("")]), 2);

    // BOM policies (`StripAll`, `Keep`, `Error`) preserve source structure on multi-stage `source.fixAll`.
    let multi_lint = LintOptions::new()
        .with_duplicate_keys(false)
        .with_key_case(KeyCase::Upper)
        .with_key_order(true)
        .with_trim_trailing_whitespace(true);
    let fix_all_ctx = CodeActionContext {
        only: Some(vec![CodeActionKind::SOURCE_FIX_ALL]),
        ..CodeActionContext::default()
    };
    let strip_all_opts = LspOptions::new()
        .with_dialect(Dialect::PORTABLE.with_bom(BomPolicy::StripAll))
        .with_lint_options(multi_lint);
    let strip_all_src = "\u{feff}\u{feff}b=2  \na=1  \n";
    let strip_all_actions =
        code_actions(strip_all_src, &uri(), full, &fix_all_ctx, &strip_all_opts);
    let CodeActionOrCommand::CodeAction(sa) = &strip_all_actions[0] else {
        panic!("expected action");
    };
    let sa_edit = &sa.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri()][0];
    assert_eq!(
        apply_change(
            strip_all_src,
            Some(sa_edit.range),
            &sa_edit.new_text,
            &strip_all_opts
        )
        .unwrap(),
        "\u{feff}\u{feff}A=1\nB=2\n"
    );

    let keep_opts = LspOptions::new()
        .with_dialect(Dialect::NODE_DOTENV.with_bom(BomPolicy::Keep))
        .with_lint_options(
            LintOptions::new()
                .with_duplicate_keys(false)
                .with_key_case(KeyCase::Upper)
                .with_trim_trailing_whitespace(true),
        );
    let keep_src = "\u{feff}A=1  \nb=2  \n";
    let keep_actions = code_actions(keep_src, &uri(), full, &fix_all_ctx, &keep_opts);
    assert_eq!(keep_actions.len(), 1);
    let CodeActionOrCommand::CodeAction(ka) = &keep_actions[0] else {
        panic!("expected action");
    };
    let mut keep_out = keep_src.to_owned();
    for edit in ka.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri()]
        .iter()
        .rev()
    {
        keep_out = apply_change(&keep_out, Some(edit.range), &edit.new_text, &keep_opts).unwrap();
    }
    assert_eq!(keep_out, "\u{feff}A=1\nB=2\n");

    let bom_err_opts = LspOptions::new()
        .with_dialect(Dialect::PORTABLE.with_bom(BomPolicy::Error))
        .with_lint_options(
            LintOptions::new()
                .with_duplicate_keys(false)
                .with_key_case(KeyCase::Upper),
        );
    let bom_err_src = "\u{feff}A=1\nb=2\n";
    let bom_err_actions = code_actions(bom_err_src, &uri(), full, &fix_all_ctx, &bom_err_opts);
    assert_eq!(bom_err_actions.len(), 1);
    let CodeActionOrCommand::CodeAction(ea) = &bom_err_actions[0] else {
        panic!("expected action");
    };
    let mut err_out = bom_err_src.to_owned();
    for edit in ea.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri()]
        .iter()
        .rev()
    {
        err_out = apply_change(&err_out, Some(edit.range), &edit.new_text, &bom_err_opts).unwrap();
    }
    assert_eq!(err_out, "\u{feff}A=1\nB=2\n");

    // Parse limit exceeded on input or expanding output suppresses code actions.
    let input_limited = lint_upper.with_parse_options(ParseOptions::new().with_max_lines(1));
    assert_eq!(
        code_actions(
            "a=1\nb=2\n",
            &uri(),
            full,
            &CodeActionContext::default(),
            &input_limited
        ),
        Vec::new()
    );

    let output_limited = LspOptions::new()
        .with_lint_options(
            LintOptions::new()
                .with_duplicate_keys(false)
                .with_equals_spacing(EqualsSpacing::Spaces),
        )
        .with_parse_options(ParseOptions::new().with_max_input_bytes(4));
    assert_eq!(
        code_actions(
            "A=1\n",
            &uri(),
            full,
            &CodeActionContext::default(),
            &output_limited
        ),
        Vec::new()
    );

    // Colliding quickfix (`a=1` -> `A` when `A=2` exists) is cleanly omitted without panicking.
    let colliding = code_actions(
        "a=1\nA=2\n",
        &uri(),
        full,
        &CodeActionContext::default(),
        &lint_upper,
    );
    assert_eq!(colliding, Vec::new());
}

#[test]
fn hover_inline_comment_boundary_and_linemap_edge_cases() {
    // Hover at exact end (col 21) and clamped past EOL (col 50) of inline comment returns None,
    // and hovering the empty trailing line `(1, 0)` after any line ending also returns None.
    let source = "HOST=localhost # note\n";
    assert!(hover(source, position(0, 21), &opts()).is_none());
    assert!(hover(source, position(0, 50), &opts()).is_none());
    assert!(hover(source, position(1, 0), &opts()).is_none());
    assert!(hover("HOST=secret\n", position(1, 0), &opts()).is_none());
    assert!(hover("HOST=secret\r\n", position(1, 0), &opts()).is_none());
    assert!(hover("HOST=secret\r", position(1, 0), &opts()).is_none());
    assert!(hover("HOST=secret", position(0, 11), &opts()).is_some());

    // Inverted range past EOL returns None instead of clamping both endpoints to EOL.
    assert_eq!(
        apply_change(
            "A=1\n",
            Some(Range::new(position(0, 10), position(0, 5))),
            "x",
            &opts()
        ),
        None
    );

    // Lone CR and empty document LineMap behavior.
    assert_eq!(
        apply_change(
            "A=1\rB=2\r",
            Some(Range::new(position(1, 0), position(1, 1))),
            "C",
            &opts()
        ),
        Some("A=1\rC=2\r".to_owned())
    );
    assert_eq!(
        apply_change(
            "",
            Some(Range::new(position(0, 0), position(0, 0))),
            "A=1\n",
            &opts()
        ),
        Some("A=1\n".to_owned())
    );

    // Valid multi-change `apply_changes` sequence (incremental -> full replacement -> incremental).
    let mut doc = ParsedDocument::parse("A=1\nB=2\n", opts());
    assert!(doc.apply_changes(&[
        TextDocumentContentChangeEvent {
            range: Some(Range::new(position(0, 0), position(0, 1))),
            range_length: None,
            text: "X".to_owned(),
        },
        TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "HOST=1\nURL=$HOST\n".to_owned(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range::new(position(0, 5), position(0, 6))),
            range_length: None,
            text: "localhost".to_owned(),
        },
    ]));
    assert_eq!(doc.source(), "HOST=localhost\nURL=$HOST\n");

    // References with BOM across UTF-8, UTF-16, and UTF-32.
    let bom_source = "\u{feff}HOST=😀\nURL=😀${HOST}\n";
    for (encoding, ref_start, ref_end) in [
        (PositionEncoding::Utf8, 10, 14),
        (PositionEncoding::Utf16, 8, 12),
        (PositionEncoding::Utf32, 7, 11),
    ] {
        let options = LspOptions::new().with_position_encoding(encoding);
        let locs = references(bom_source, &uri(), position(0, 0), true, &options);
        assert_eq!(locs.len(), 2, "{encoding:?}");
        assert_eq!(
            locs[0].range,
            Range::new(position(0, 0), position(0, 4)),
            "{encoding:?}"
        );
        assert_eq!(
            locs[1].range,
            Range::new(position(1, ref_start), position(1, ref_end)),
            "{encoding:?}"
        );
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn crlf_in_lf_only_dialects_emits_exact_verified_edits_without_corruption() {
    let apply_all = |src: &str, edits: &[lsp_types::TextEdit], options: &LspOptions| -> String {
        let mut out = src.to_owned();
        for edit in edits.iter().rev() {
            out = apply_change(&out, Some(edit.range), &edit.new_text, options).unwrap();
        }
        out
    };

    // 1. Built-in `Dialect::DOCKER_RAW` (`LineBreaks::Lf`) + rename on `"a\r\n"` (CRLF line
    // ending) and `"a\r\r\n"` (key `"a\r"` with lone CR inside key before CRLF ending).
    let docker = LspOptions::new().with_dialect(Dialect::DOCKER_RAW);
    let edit_crlf = rename("a\r\n", &uri(), position(0, 0), "B", &docker).unwrap();
    assert_eq!(
        apply_all("a\r\n", &edit_crlf.changes.unwrap()[&uri()], &docker),
        "B\r\n"
    );
    let edit_cr_in_key = rename("a\r\r\n", &uri(), position(0, 0), "B", &docker).unwrap();
    assert_eq!(
        apply_all("a\r\r\n", &edit_cr_in_key.changes.unwrap()[&uri()], &docker),
        "B\r\n"
    );

    // 2. Built-in `Dialect::DOCKER_RAW` + code_actions (`KeyCase::Upper`) on `"a\r\r\n"`.
    // Must produce `"A\r\r\n"` without corrupting or duplicating `\r`.
    let docker_lint = docker.with_lint_options(
        LintOptions::new()
            .with_duplicate_keys(false)
            .with_key_case(KeyCase::Upper),
    );
    let full = Range::new(position(0, 0), position(4, 0));
    let actions = code_actions(
        "a\r\r\n",
        &uri(),
        full,
        &CodeActionContext::default(),
        &docker_lint,
    );
    assert_eq!(actions.len(), 2);
    for action in &actions {
        let CodeActionOrCommand::CodeAction(ca) = action else {
            panic!("expected code action");
        };
        let edits = &ca.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri()];
        assert_eq!(apply_all("a\r\r\n", edits, &docker_lint), "A\r\r\n");
    }

    // 3. Custom `LineBreaks::Lf` dialect + formatting (`QuoteStyle::Double`) on `"A=1\r\r\n"`.
    // Value is `"1\r"` followed by CRLF line ending; applied LSP edit must match verified output.
    let lf_only = LspOptions::new()
        .with_dialect(Dialect::PORTABLE.with_line_breaks(adotenv::LineBreaks::Lf))
        .with_format_options(FormatOptions::default().with_quote_style(QuoteStyle::Double));
    let fmt_edits = formatting("A=1\r\r\n", &FormattingOptions::default(), &lf_only).unwrap();
    assert_eq!(
        apply_all("A=1\r\r\n", &fmt_edits, &lf_only),
        "A=\"1\\r\"\r\n"
    );

    // 4. Same under a leading BOM preserves the BOM and CRLF line ending.
    let bom_src = "\u{feff}A=1\r\r\n";
    let bom_edits = formatting(bom_src, &FormattingOptions::default(), &lf_only).unwrap();
    assert_eq!(
        apply_all(bom_src, &bom_edits, &lf_only),
        "\u{feff}A=\"1\\r\"\r\n"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn reference_boundary_alignment_unquoted_code_actions_and_incremental_linemap() {
    let apply_all = |src: &str, edits: &[lsp_types::TextEdit], options: &LspOptions| -> String {
        let mut out = src.to_owned();
        for edit in edits.iter().rev() {
            out = apply_change(&out, Some(edit.range), &edit.new_text, options).unwrap();
        }
        out
    };

    // Unbraced `$HOST` end-of-identifier cursor works both mid-file and at EOF.
    for src in ["HOST=1\nURL=$HOST\n", "HOST=1\nURL=$HOST"] {
        assert!(definition(src, &uri(), position(1, 9), &opts()).is_some());
        let hov = hover(src, position(1, 9), &opts()).unwrap();
        let HoverContents::Markup(m) = hov.contents else {
            panic!("expected markup");
        };
        assert!(m.value.contains("`${HOST}`"), "{}", m.value);
        assert_eq!(
            references(src, &uri(), position(1, 9), true, &opts()).len(),
            2
        );
        assert!(prepare_rename(src, position(1, 9), &opts()).is_some());
        assert!(rename(src, &uri(), position(1, 9), "SERVER", &opts()).is_ok());
    }

    // Adjacent `$A$B` resolves `$A` at `A|` (col 6) and `$B` at `B` (col 7) and `B|` (col 8).
    let adj = "A=1\nB=2\nX=$A$B\n";
    assert_eq!(
        prepare_rename(adj, position(2, 4), &opts()),
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(position(2, 3), position(2, 4)),
            placeholder: "A".to_owned(),
        })
    );
    assert_eq!(
        prepare_rename(adj, position(2, 5), &opts()),
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(position(2, 5), position(2, 6)),
            placeholder: "B".to_owned(),
        })
    );
    assert_eq!(
        prepare_rename(adj, position(2, 6), &opts()),
        Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Range::new(position(2, 5), position(2, 6)),
            placeholder: "B".to_owned(),
        })
    );

    // Operator fallback text in `${HOST:-fallback}` is never misattributed to `HOST`.
    let op = "HOST=localhost\nURL=${HOST:-fallback}\n";
    assert_eq!(definition(op, &uri(), position(1, 15), &opts()), None);
    assert_eq!(
        references(op, &uri(), position(1, 15), true, &opts()),
        Vec::new()
    );
    assert_eq!(prepare_rename(op, position(1, 15), &opts()), None);
    assert_eq!(
        rename(op, &uri(), position(1, 15), "SERVER", &opts()),
        Err(RenameError::NoSymbol)
    );
    let fallback_hover = hover(op, position(1, 15), &opts()).unwrap();
    let HoverContents::Markup(m) = fallback_hover.contents else {
        panic!("expected markup");
    };
    assert!(m.value.contains("`URL`"), "{}", m.value);

    // Empty Compose key (`=1\n`) is rejected consistently across all symbol ops.
    let compose = LspOptions::new().with_dialect(Dialect::COMPOSE);
    assert_eq!(definition("=1\n", &uri(), position(0, 0), &compose), None);
    assert_eq!(
        references("=1\n", &uri(), position(0, 0), true, &compose),
        Vec::new()
    );
    assert_eq!(prepare_rename("=1\n", position(0, 0), &compose), None);
    assert_eq!(
        rename("=1\n", &uri(), position(0, 0), "A", &compose),
        Err(RenameError::NoSymbol)
    );

    // `code_actions` applies `QuoteStyle::Unquoted` even when combined with trivia/rename/sort rules.
    let unquote_combo = LspOptions::new().with_lint_options(
        LintOptions::new()
            .with_duplicate_keys(false)
            .with_key_case(KeyCase::Upper)
            .with_key_order(true)
            .with_quote_style(QuoteStyle::Unquoted)
            .with_trim_trailing_whitespace(true),
    );
    let full = Range::new(position(0, 0), position(10, 0));
    let combo_src = "\u{feff}b=\"two\"  \na=\"one\"  \n";
    let actions = code_actions(
        combo_src,
        &uri(),
        full,
        &CodeActionContext::default(),
        &unquote_combo,
    );
    let format_qf = actions
        .iter()
        .find_map(|item| match item {
            CodeActionOrCommand::CodeAction(a)
                if a.kind == Some(CodeActionKind::QUICKFIX)
                    && a.title.contains("Format document") =>
            {
                Some(a)
            }
            _ => None,
        })
        .expect("format quickfix for unquoted + trim");
    let qf_edits = &format_qf.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri()];
    assert_eq!(
        apply_all(combo_src, qf_edits, &unquote_combo),
        "\u{feff}b=two\na=one\n"
    );

    let fix_all = actions
        .iter()
        .find_map(|item| match item {
            CodeActionOrCommand::CodeAction(a)
                if a.kind == Some(CodeActionKind::SOURCE_FIX_ALL) =>
            {
                Some(a)
            }
            _ => None,
        })
        .expect("source.fixAll for unquoted + rename + sort + trim");
    let fa_edits = &fix_all.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri()];
    assert_eq!(
        apply_all(combo_src, fa_edits, &unquote_combo),
        "\u{feff}A=one\nB=two\n"
    );

    // Format-only `source.fixAll` with `QuoteStyle::Unquoted` + `trim_trailing_whitespace`
    // also works when an unrelated invalid line is present.
    let fmt_only_unquote = LspOptions::new().with_lint_options(
        LintOptions::new()
            .with_duplicate_keys(false)
            .with_quote_style(QuoteStyle::Unquoted)
            .with_trim_trailing_whitespace(true),
    );
    let broken_unquote = "BAD KEY=1\nA=\"plain\"  \n";
    let broken_actions = code_actions(
        broken_unquote,
        &uri(),
        full,
        &CodeActionContext {
            only: Some(vec![CodeActionKind::SOURCE_FIX_ALL]),
            ..CodeActionContext::default()
        },
        &fmt_only_unquote,
    );
    assert_eq!(broken_actions.len(), 1);
    let CodeActionOrCommand::CodeAction(ba) = &broken_actions[0] else {
        panic!("expected action");
    };
    let ba_edits = &ba.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri()];
    assert_eq!(
        apply_all(broken_unquote, ba_edits, &fmt_only_unquote),
        "BAD KEY=1\nA=plain\n"
    );

    // Incremental LineMap batch updates match sequential `apply_change` across BOM, Unicode, and newlines.
    let strip_all = LspOptions::new().with_dialect(Dialect::PORTABLE.with_bom(BomPolicy::StripAll));
    let initial = "\u{feff}A=1\nB=😀\nC=3\n";
    let batch = vec![
        // Shrink line 0
        TextDocumentContentChangeEvent {
            range: Some(Range::new(position(0, 2), position(0, 3))),
            range_length: None,
            text: String::new(),
        },
        // Grow line 0 (insert value + extra BOM at col 0 under StripAll)
        TextDocumentContentChangeEvent {
            range: Some(Range::new(position(0, 0), position(0, 0))),
            range_length: None,
            text: "\u{feff}PREFIX_".to_owned(),
        },
        // Edit line 1 after emoji (UTF-16 col 4)
        TextDocumentContentChangeEvent {
            range: Some(Range::new(position(1, 4), position(1, 4))),
            range_length: None,
            text: "_END".to_owned(),
        },
        // Insert newline on line 1
        TextDocumentContentChangeEvent {
            range: Some(Range::new(position(1, 2), position(1, 2))),
            range_length: None,
            text: "x\nD=".to_owned(),
        },
        // Edit shifted line 3 (`C=3`)
        TextDocumentContentChangeEvent {
            range: Some(Range::new(position(3, 2), position(3, 3))),
            range_length: None,
            text: "99".to_owned(),
        },
    ];
    let mut seq = initial.to_owned();
    for change in &batch {
        seq = apply_change(&seq, change.range, &change.text, &strip_all).unwrap();
    }
    let mut doc = ParsedDocument::parse(initial, strip_all);
    assert!(doc.apply_changes(&batch));
    assert_eq!(doc.source(), seq);

    // PositionEncoding Display & From<PositionEncoding> for PositionEncodingKind.
    assert_eq!(PositionEncoding::Utf8.to_string(), "utf-8");
    assert_eq!(PositionEncoding::Utf16.to_string(), "utf-16");
    assert_eq!(PositionEncoding::Utf32.to_string(), "utf-32");
    assert_eq!(
        PositionEncodingKind::from(PositionEncoding::Utf32),
        PositionEncodingKind::UTF32
    );
}
