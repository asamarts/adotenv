//! Stdio language server for adotenv documents.

use std::{
    collections::HashMap,
    env,
    error::Error,
    path::{Path, PathBuf},
    process::ExitCode,
};

use adotenv_lint::{load_effective_config, ConfigExplicit, IgnoredPaths, LintConfig, LintPolicy};
use adotenv_lsp::{
    apply_change, completion, definition, diagnostics, document_symbols, hover, references, rename,
    workspace_symbols, LspOptions,
};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CompletionOptions, CompletionParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbolParams, GotoDefinitionParams, HoverParams,
    HoverProviderCapability, InitializeParams, OneOf, PublishDiagnosticsParams, ReferenceParams,
    RenameParams, ServerCapabilities, SymbolInformation, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri, WorkspaceSymbolParams,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("adotenv-lsp: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = parse_args()?;
    let (options, ignore) = load_policy(&cli)?;
    let (connection, io_threads) = Connection::stdio();
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL,
        )),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!["$".to_owned(), "{".to_owned()]),
            ..CompletionOptions::default()
        }),
        definition_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Left(true)),
        ..ServerCapabilities::default()
    };
    let initialization = connection.initialize(serde_json::to_value(capabilities)?)?;
    let _: InitializeParams = serde_json::from_value(initialization)?;

    let mut documents = HashMap::<String, String>::new();
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    break;
                }
                if let Err(error) = handle_request(&connection, &request, &documents, options) {
                    send_error(
                        &connection,
                        request.id.clone(),
                        lsp_server::ErrorCode::InvalidParams,
                        &error.to_string(),
                    )?;
                }
            }
            Message::Notification(notification) => {
                if let Err(error) = handle_notification(
                    &connection,
                    &notification,
                    &mut documents,
                    options,
                    ignore.as_ref(),
                ) {
                    let _ = error;
                }
            }
            Message::Response(_) => {}
        }
    }
    drop(connection);
    io_threads.join()?;
    Ok(())
}

struct Cli {
    mask_values: bool,
    config: Option<PathBuf>,
    discover_lint_config: bool,
}

fn parse_args() -> Result<Cli> {
    parse_args_from(env::args().skip(1))
}

fn parse_args_from(arguments: impl IntoIterator<Item = String>) -> Result<Cli> {
    let mut cli = Cli {
        mask_values: false,
        config: None,
        discover_lint_config: false,
    };
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--mask-values" => cli.mask_values = true,
            "--discover-lint-config" => cli.discover_lint_config = true,
            "--config" => {
                let value = arguments.next().ok_or("--config requires a path")?;
                cli.config = Some(PathBuf::from(value));
            }
            "--" => {
                if let Some(extra) = arguments.next() {
                    return Err(format!("unexpected argument: {extra}").into());
                }
            }
            other if other.starts_with("--config=") => {
                cli.config = Some(PathBuf::from(&other["--config=".len()..]));
            }
            other => return Err(format!("unknown option: {other}").into()),
        }
    }
    if cli.config.is_some() && cli.discover_lint_config {
        return Err("--config and --discover-lint-config cannot be combined".into());
    }
    Ok(cli)
}

fn load_policy(cli: &Cli) -> Result<(LspOptions, Option<IgnoredPaths>)> {
    let start = env::current_dir()?;
    let config = if let Some(path) = &cli.config {
        Some(LintConfig::load(path)?)
    } else if cli.discover_lint_config {
        load_effective_config(None, &start)?
    } else {
        None
    };
    let mut options = LspOptions::default().with_mask_values(cli.mask_values);
    let Some(config) = config else {
        return Ok((options, None));
    };
    let mut policy = LintPolicy::default();
    config.apply(&mut policy, ConfigExplicit::default());
    options = options.with_lint_options(policy.lint);
    let ignore = policy
        .ignored_paths()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    Ok((options, Some(ignore)))
}

fn handle_notification(
    connection: &Connection,
    notification: &Notification,
    documents: &mut HashMap<String, String>,
    options: LspOptions,
    ignore: Option<&IgnoredPaths>,
) -> Result<()> {
    match notification.method.as_str() {
        "textDocument/didOpen" => {
            let params: DidOpenTextDocumentParams =
                serde_json::from_value(notification.params.clone())?;
            let uri = params.text_document.uri;
            documents.insert(uri.as_str().to_owned(), params.text_document.text);
            publish_diagnostics(connection, &uri, documents, options, ignore)?;
        }
        "textDocument/didChange" => {
            let params: DidChangeTextDocumentParams =
                serde_json::from_value(notification.params.clone())?;
            let uri = params.text_document.uri;
            let mut text = documents.get(uri.as_str()).cloned().unwrap_or_default();
            for change in params.content_changes {
                text = apply_change(&text, change.range, &change.text)
                    .ok_or("invalid incremental document change")?;
            }
            documents.insert(uri.as_str().to_owned(), text);
            publish_diagnostics(connection, &uri, documents, options, ignore)?;
        }
        "textDocument/didClose" => {
            let params: DidCloseTextDocumentParams =
                serde_json::from_value(notification.params.clone())?;
            let uri = params.text_document.uri;
            documents.remove(uri.as_str());
            publish_empty_diagnostics(connection, &uri)?;
        }
        _ => {}
    }
    Ok(())
}

fn publish_diagnostics(
    connection: &Connection,
    uri: &Uri,
    documents: &HashMap<String, String>,
    options: LspOptions,
    ignore: Option<&IgnoredPaths>,
) -> Result<()> {
    if ignored_uri(uri, ignore) {
        return publish_empty_diagnostics(connection, uri);
    }
    let text = documents
        .get(uri.as_str())
        .ok_or("diagnostics requested for an unopened document")?;
    send_diagnostics(connection, uri, diagnostics(text, uri, &options))
}

fn ignored_uri(uri: &Uri, ignore: Option<&IgnoredPaths>) -> bool {
    ignore.is_some_and(|ignore| ignore.is_ignored(Path::new(uri.path().as_str())))
}

fn publish_empty_diagnostics(connection: &Connection, uri: &Uri) -> Result<()> {
    send_diagnostics(connection, uri, Vec::new())
}

fn send_diagnostics(
    connection: &Connection,
    uri: &Uri,
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> Result<()> {
    let params = PublishDiagnosticsParams::new(uri.clone(), diagnostics, None);
    connection
        .sender
        .send(Message::Notification(Notification::new(
            "textDocument/publishDiagnostics".to_owned(),
            params,
        )))
        .map_err(|_| "LSP connection closed")?;
    Ok(())
}

fn handle_request(
    connection: &Connection,
    request: &Request,
    documents: &HashMap<String, String>,
    options: LspOptions,
) -> Result<()> {
    match request.method.as_str() {
        "textDocument/completion" => {
            let params: CompletionParams = serde_json::from_value(request.params.clone())?;
            let uri = &params.text_document_position.text_document.uri;
            let text = documents
                .get(uri.as_str())
                .ok_or("completion requested for an unopened document")?;
            send_ok(
                connection,
                request.id.clone(),
                &completion(text, params.text_document_position.position),
            )
        }
        "workspace/symbol" => {
            let params: WorkspaceSymbolParams = serde_json::from_value(request.params.clone())?;
            send_ok(
                connection,
                request.id.clone(),
                &workspace_symbol_response(documents, &params.query),
            )
        }
        "textDocument/documentSymbol" => {
            let params: DocumentSymbolParams = serde_json::from_value(request.params.clone())?;
            let uri = &params.text_document.uri;
            let text = documents
                .get(uri.as_str())
                .ok_or("document symbols requested for an unopened document")?;
            send_ok(connection, request.id.clone(), &document_symbols(text))
        }
        "textDocument/definition" => {
            let params: GotoDefinitionParams = serde_json::from_value(request.params.clone())?;
            let uri = &params.text_document_position_params.text_document.uri;
            let text = documents
                .get(uri.as_str())
                .ok_or("definition requested for an unopened document")?;
            send_ok(
                connection,
                request.id.clone(),
                &definition(text, uri, params.text_document_position_params.position),
            )
        }
        "textDocument/hover" => {
            let params: HoverParams = serde_json::from_value(request.params.clone())?;
            let uri = &params.text_document_position_params.text_document.uri;
            let text = documents
                .get(uri.as_str())
                .ok_or("hover requested for an unopened document")?;
            send_ok(
                connection,
                request.id.clone(),
                &hover(
                    text,
                    params.text_document_position_params.position,
                    &options,
                ),
            )
        }
        "textDocument/references" => {
            let params: ReferenceParams = serde_json::from_value(request.params.clone())?;
            let position = params.text_document_position;
            let uri = &position.text_document.uri;
            let text = documents
                .get(uri.as_str())
                .ok_or("references requested for an unopened document")?;
            send_ok(
                connection,
                request.id.clone(),
                &references(
                    text,
                    uri,
                    position.position,
                    params.context.include_declaration,
                ),
            )
        }
        "textDocument/rename" => {
            let params: RenameParams = serde_json::from_value(request.params.clone())?;
            let position = params.text_document_position;
            let uri = &position.text_document.uri;
            let text = documents
                .get(uri.as_str())
                .ok_or("rename requested for an unopened document")?;
            match rename(text, uri, position.position, &params.new_name) {
                Ok(edit) => send_ok(connection, request.id.clone(), &edit),
                Err(error) => send_error(
                    connection,
                    request.id.clone(),
                    lsp_server::ErrorCode::RequestFailed,
                    &error.to_string(),
                ),
            }
        }
        _ => send_error(
            connection,
            request.id.clone(),
            lsp_server::ErrorCode::MethodNotFound,
            "unsupported request",
        ),
    }
}

fn workspace_symbol_response(
    documents: &HashMap<String, String>,
    query: &str,
) -> Vec<SymbolInformation> {
    let mut symbols = Vec::new();
    for (uri_text, text) in documents {
        if let Ok(uri) = uri_text.parse::<Uri>() {
            symbols.extend(workspace_symbols(text, &uri, query));
        }
    }
    symbols.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.location.uri.as_str().cmp(right.location.uri.as_str()))
    });
    symbols
}

fn send_ok<T: serde::Serialize>(
    connection: &Connection,
    id: lsp_server::RequestId,
    value: &T,
) -> Result<()> {
    connection
        .sender
        .send(Message::Response(Response::new_ok(
            id,
            serde_json::to_value(value)?,
        )))
        .map_err(|_| "LSP connection closed")?;
    Ok(())
}

fn send_error(
    connection: &Connection,
    id: lsp_server::RequestId,
    code: lsp_server::ErrorCode,
    message: &str,
) -> Result<()> {
    connection
        .sender
        .send(Message::Response(Response::new_err(
            id,
            code as i32,
            message.to_owned(),
        )))
        .map_err(|_| "LSP connection closed")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn defaults_to_no_lint_configuration() {
        let cli = parse_args_from(args(&[])).unwrap();
        assert!(!cli.mask_values);
        assert!(cli.config.is_none());
        assert!(!cli.discover_lint_config);
    }

    #[test]
    fn parses_explicit_config_and_discovery() {
        let cli = parse_args_from(args(&["--mask-values", "--config", "policy.toml"])).unwrap();
        assert!(cli.mask_values);
        assert_eq!(cli.config, Some(PathBuf::from("policy.toml")));
        assert!(!cli.discover_lint_config);

        let cli = parse_args_from(args(&["--config=policy.toml", "--discover-lint-config"]));
        assert!(cli.is_err());

        let cli = parse_args_from(args(&["--discover-lint-config"])).unwrap();
        assert!(cli.discover_lint_config);
    }

    #[test]
    fn rejects_unknown_or_incomplete_arguments() {
        assert!(parse_args_from(args(&["--config"])).is_err());
        assert!(parse_args_from(args(&["--unknown"])).is_err());
        assert!(parse_args_from(args(&["--", "extra"])).is_err());
    }

    #[test]
    fn default_policy_has_no_linter_or_ignores() {
        let cli = Cli {
            mask_values: false,
            config: None,
            discover_lint_config: false,
        };
        let (options, ignore) = load_policy(&cli).unwrap();
        assert!(options.lint.is_none());
        assert!(ignore.is_none());
    }

    #[test]
    fn explicit_config_resolves_lint_policy_and_ignores() {
        let path =
            std::env::temp_dir().join(format!("adotenv-lsp-policy-{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "key_order = true\nignore = [\".env.skip\"]\n[rules]\nduplicate-key = \"error\"\n",
        )
        .unwrap();
        let cli = Cli {
            mask_values: false,
            config: Some(path.clone()),
            discover_lint_config: false,
        };
        let (options, ignore) = load_policy(&cli).unwrap();
        let lint = options.lint.expect("lint policy");
        assert!(lint.key_order);
        assert_eq!(
            lint.severity_for_rule(adotenv_lint::Rule::DuplicateKey),
            adotenv::Severity::Error
        );
        assert!(ignore.unwrap().is_ignored(Path::new(".env.skip")));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn ignored_uri_uses_configured_patterns() {
        let ignore = IgnoredPaths::new(&[".env.skip".to_owned()]).unwrap();
        let uri: lsp_types::Uri = "file:///tmp/.env.skip".parse().unwrap();
        assert!(ignored_uri(&uri, Some(&ignore)));
        let uri: lsp_types::Uri = "file:///tmp/.env".parse().unwrap();
        assert!(!ignored_uri(&uri, Some(&ignore)));
        assert!(!ignored_uri(&uri, None));
    }

    #[test]
    fn invalid_explicit_config_is_an_error() {
        let path =
            std::env::temp_dir().join(format!("adotenv-lsp-invalid-{}.toml", std::process::id()));
        std::fs::write(&path, "unknown = true\n").unwrap();
        let cli = Cli {
            mask_values: false,
            config: Some(path.clone()),
            discover_lint_config: false,
        };
        assert!(load_policy(&cli).is_err());
        let _ = std::fs::remove_file(path);
    }
}
