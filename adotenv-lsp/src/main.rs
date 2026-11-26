//! Stdio language server for adotenv documents.

use std::{collections::HashMap, env, error::Error, path::PathBuf, process::ExitCode};

use adotenv_lint::{
    load_effective_config, parse_dialect, ConfigExplicit, IgnoredPaths, LintConfig, LintPolicy,
};
use adotenv_lsp::{
    apply_change_with_options, completion_with_options, definition_with_options, diagnostics,
    document_symbols_with_options, hover, references_with_options, rename_with_options,
    workspace_symbols_with_options, Dialect, LspOptions, ParseOptions,
};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CompletionOptions, CompletionParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbolParams, GotoDefinitionParams, HoverParams,
    HoverProviderCapability, InitializeParams, InitializeResult, OneOf, PublishDiagnosticsParams,
    ReferenceParams, RenameParams, ServerCapabilities, ServerInfo, SymbolInformation,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri, WorkspaceSymbolParams,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

struct DocumentState {
    text: String,
    version: Option<i32>,
}

enum NotificationError {
    Malformed(Box<dyn Error + Send + Sync>),
    Transport(Box<dyn Error + Send + Sync>),
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("adotenv-lsp: {error}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "adotenv-lsp [-h|--help] [--version] [--mask-values] \
                     [--dialect <portable|node|python|compose|docker>] \
                     [--max-input-bytes <n>] [--max-lines <n>] \
                     [--max-items <n>] [--max-diagnostics <n>] \
                     [--config <path> | --discover-lint-config]\n";

fn run() -> Result<()> {
    let cli = parse_args()?;
    if cli.help {
        print!("{USAGE}");
        return Ok(());
    }
    if cli.version {
        println!("adotenv-lsp {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let (options, ignore) = load_policy(&cli)?;
    let (connection, io_threads) = Connection::stdio();
    let result = serve(&connection, options, ignore.as_ref());
    drop(connection);
    let join_result = io_threads.join();
    result?;
    join_result?;
    Ok(())
}

fn serve(
    connection: &Connection,
    options: LspOptions,
    ignore: Option<&IgnoredPaths>,
) -> Result<()> {
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
    let (init_id, initialization) = connection.initialize_start()?;
    let _: InitializeParams = serde_json::from_value(initialization)?;
    let init_result = InitializeResult {
        capabilities,
        server_info: Some(ServerInfo {
            name: "adotenv-lsp".to_owned(),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }),
    };
    connection.initialize_finish(init_id, serde_json::to_value(init_result)?)?;

    let mut documents = HashMap::<String, DocumentState>::new();
    let mut shutdown_received = false;
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    shutdown_received = true;
                    break;
                }
                if let Err(error) = handle_request(connection, &request, &documents, options) {
                    send_error(
                        connection,
                        request.id.clone(),
                        lsp_server::ErrorCode::InvalidParams,
                        &error.to_string(),
                    )?;
                }
            }
            Message::Notification(notification) => {
                if notification.method == "exit" {
                    break;
                }
                match handle_notification(
                    connection,
                    &notification,
                    &mut documents,
                    options,
                    ignore,
                ) {
                    Ok(()) => {}
                    Err(NotificationError::Malformed(error)) => {
                        eprintln!("adotenv-lsp: ignored {}: {error}", notification.method);
                    }
                    Err(NotificationError::Transport(error)) => return Err(error),
                }
            }
            Message::Response(_) => {}
        }
    }
    if !shutdown_received {
        return Err("client exited before shutdown".into());
    }
    Ok(())
}

#[allow(clippy::struct_excessive_bools)]
struct Cli {
    mask_values: bool,
    dialect: Option<Dialect>,
    max_input_bytes: Option<usize>,
    max_lines: Option<usize>,
    max_items: Option<usize>,
    max_diagnostics: Option<usize>,
    config: Option<PathBuf>,
    discover_lint_config: bool,
    help: bool,
    version: bool,
}

fn parse_args() -> Result<Cli> {
    let arguments = env::args_os()
        .skip(1)
        .map(|argument| {
            argument
                .into_string()
                .map_err(|_| "arguments must be valid UTF-8".to_owned())
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    parse_args_from(arguments)
}

fn parse_limit(flag: &str, value: &str) -> Result<usize> {
    if value.starts_with('+') {
        return Err(format!("{flag} expects a non-negative integer: {value}").into());
    }
    value
        .parse::<usize>()
        .map_err(|_| format!("{flag} expects a non-negative integer: {value}").into())
}

#[allow(clippy::too_many_lines)]
fn parse_args_from(arguments: impl IntoIterator<Item = String>) -> Result<Cli> {
    let mut cli = Cli {
        mask_values: false,
        dialect: None,
        max_input_bytes: None,
        max_lines: None,
        max_items: None,
        max_diagnostics: None,
        config: None,
        discover_lint_config: false,
        help: false,
        version: false,
    };
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-h" | "--help" => cli.help = true,
            "--version" => cli.version = true,
            "--mask-values" => cli.mask_values = true,
            "--discover-lint-config" => cli.discover_lint_config = true,
            "--dialect" => {
                let value = arguments.next().ok_or("--dialect requires a name")?;
                cli.dialect = Some(parse_dialect(&value)?);
            }
            "--max-input-bytes" => {
                let value = arguments
                    .next()
                    .ok_or("--max-input-bytes requires a number")?;
                cli.max_input_bytes = Some(parse_limit("--max-input-bytes", &value)?);
            }
            "--max-lines" => {
                let value = arguments.next().ok_or("--max-lines requires a number")?;
                cli.max_lines = Some(parse_limit("--max-lines", &value)?);
            }
            "--max-items" => {
                let value = arguments.next().ok_or("--max-items requires a number")?;
                cli.max_items = Some(parse_limit("--max-items", &value)?);
            }
            "--max-diagnostics" => {
                let value = arguments
                    .next()
                    .ok_or("--max-diagnostics requires a number")?;
                cli.max_diagnostics = Some(parse_limit("--max-diagnostics", &value)?);
            }
            "--config" => {
                let value = arguments.next().ok_or("--config requires a path")?;
                cli.config = Some(PathBuf::from(value));
            }
            "--" => {
                if let Some(extra) = arguments.next() {
                    return Err(format!("unexpected argument: {extra}").into());
                }
            }
            other if other.starts_with("--dialect=") => {
                cli.dialect = Some(parse_dialect(&other["--dialect=".len()..])?);
            }
            other if other.starts_with("--max-input-bytes=") => {
                cli.max_input_bytes = Some(parse_limit(
                    "--max-input-bytes",
                    &other["--max-input-bytes=".len()..],
                )?);
            }
            other if other.starts_with("--max-lines=") => {
                cli.max_lines = Some(parse_limit("--max-lines", &other["--max-lines=".len()..])?);
            }
            other if other.starts_with("--max-items=") => {
                cli.max_items = Some(parse_limit("--max-items", &other["--max-items=".len()..])?);
            }
            other if other.starts_with("--max-diagnostics=") => {
                cli.max_diagnostics = Some(parse_limit(
                    "--max-diagnostics",
                    &other["--max-diagnostics=".len()..],
                )?);
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
    let mut parse_options = ParseOptions::new();
    if let Some(limit) = cli.max_input_bytes {
        parse_options = parse_options.with_max_input_bytes(limit);
    }
    if let Some(limit) = cli.max_lines {
        parse_options = parse_options.with_max_lines(limit);
    }
    if let Some(limit) = cli.max_items {
        parse_options = parse_options.with_max_items(limit);
    }
    if let Some(limit) = cli.max_diagnostics {
        parse_options = parse_options.with_max_diagnostics(limit);
    }
    let mut options = LspOptions::default()
        .with_mask_values(cli.mask_values)
        .with_parse_options(parse_options);
    if let Some(dialect) = cli.dialect {
        options = options.with_dialect(dialect);
    }
    let Some(config) = config else {
        return Ok((options, None));
    };
    // Parser diagnostics already publish duplicate-key findings. A config
    // file should not duplicate them unless it explicitly selects the rule.
    let mut policy = LintPolicy::default();
    if let Some(dialect) = cli.dialect {
        policy.dialect = dialect;
    }
    policy.lint = policy.lint.with_duplicate_keys(false);
    let explicit = ConfigExplicit {
        dialect: cli.dialect.is_some(),
        ..ConfigExplicit::default()
    };
    config.apply(&mut policy, explicit);
    options = options
        .with_dialect(policy.dialect)
        .with_lint_options(policy.lint);
    let ignore = policy
        .ignored_paths()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    Ok((options, Some(ignore)))
}

fn handle_notification(
    connection: &Connection,
    notification: &Notification,
    documents: &mut HashMap<String, DocumentState>,
    options: LspOptions,
    ignore: Option<&IgnoredPaths>,
) -> std::result::Result<(), NotificationError> {
    match notification.method.as_str() {
        "textDocument/didOpen" => {
            let params: DidOpenTextDocumentParams =
                serde_json::from_value(notification.params.clone())
                    .map_err(|error| NotificationError::Malformed(error.into()))?;
            let uri = params.text_document.uri;
            documents.insert(
                uri.as_str().to_owned(),
                DocumentState {
                    text: params.text_document.text,
                    version: Some(params.text_document.version),
                },
            );
            publish_diagnostics(connection, &uri, documents, options, ignore)
                .map_err(NotificationError::Transport)?;
        }
        "textDocument/didChange" => {
            let params: DidChangeTextDocumentParams =
                serde_json::from_value(notification.params.clone())
                    .map_err(|error| NotificationError::Malformed(error.into()))?;
            let uri = params.text_document.uri;
            let Some(existing) = documents.get(uri.as_str()) else {
                return Err(NotificationError::Malformed(
                    "change received for an unopened document".into(),
                ));
            };
            let mut text = existing.text.clone();
            for change in params.content_changes {
                text = apply_change_with_options(&text, change.range, &change.text, &options)
                    .ok_or_else(|| {
                        NotificationError::Malformed("invalid incremental document change".into())
                    })?;
            }
            if let Some(doc) = documents.get_mut(uri.as_str()) {
                doc.text = text;
                doc.version = Some(params.text_document.version);
            }
            publish_diagnostics(connection, &uri, documents, options, ignore)
                .map_err(NotificationError::Transport)?;
        }
        "textDocument/didClose" => {
            let params: DidCloseTextDocumentParams =
                serde_json::from_value(notification.params.clone())
                    .map_err(|error| NotificationError::Malformed(error.into()))?;
            let uri = params.text_document.uri;
            documents.remove(uri.as_str());
            publish_empty_diagnostics(connection, &uri).map_err(NotificationError::Transport)?;
        }
        _ => {}
    }
    Ok(())
}

fn publish_diagnostics(
    connection: &Connection,
    uri: &Uri,
    documents: &HashMap<String, DocumentState>,
    options: LspOptions,
    ignore: Option<&IgnoredPaths>,
) -> Result<()> {
    let doc = documents
        .get(uri.as_str())
        .ok_or("diagnostics requested for an unopened document")?;
    if ignored_uri(uri, ignore) {
        return send_diagnostics(connection, uri, Vec::new(), doc.version);
    }
    send_diagnostics(
        connection,
        uri,
        diagnostics(&doc.text, uri, &options),
        doc.version,
    )
}

fn ignored_uri(uri: &Uri, ignore: Option<&IgnoredPaths>) -> bool {
    let Some(ignore) = ignore else {
        return false;
    };
    if !uri
        .scheme()
        .is_some_and(|scheme| scheme.as_str().eq_ignore_ascii_case("file"))
    {
        return false;
    }
    ignore.is_ignored(&decoded_uri_path(uri.path().as_str()))
}

/// Decodes the percent-escaped path component of a file URI for glob matching.
///
/// `uri.path()` keeps escapes such as `%20`, while ignore patterns are written
/// with the real path spelling. Windows drive-letter paths (`/C:/...`) have
/// their leading slash stripped. Invalid escapes or invalid UTF-8 fall back to
/// the original path rather than suppressing diagnostics unexpectedly.
fn decoded_uri_path(raw: &str) -> PathBuf {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                decoded.push(high << 4 | low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(decoded).map_or_else(
        |_| PathBuf::from(raw),
        |text| {
            let trimmed = strip_windows_drive_slash(&text);
            PathBuf::from(trimmed)
        },
    )
}

fn strip_windows_drive_slash(path: &str) -> &str {
    let bytes = path.as_bytes();
    if bytes.len() >= 4
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && (bytes[3] == b'/' || bytes[3] == b'\\')
    {
        &path[1..]
    } else {
        path
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn publish_empty_diagnostics(connection: &Connection, uri: &Uri) -> Result<()> {
    send_diagnostics(connection, uri, Vec::new(), None)
}

fn send_diagnostics(
    connection: &Connection,
    uri: &Uri,
    diagnostics: Vec<lsp_types::Diagnostic>,
    version: Option<i32>,
) -> Result<()> {
    let params = PublishDiagnosticsParams::new(uri.clone(), diagnostics, version);
    connection
        .sender
        .send(Message::Notification(Notification::new(
            "textDocument/publishDiagnostics".to_owned(),
            params,
        )))
        .map_err(|_| "LSP connection closed")?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn handle_request(
    connection: &Connection,
    request: &Request,
    documents: &HashMap<String, DocumentState>,
    options: LspOptions,
) -> Result<()> {
    match request.method.as_str() {
        "textDocument/completion" => {
            let params: CompletionParams = serde_json::from_value(request.params.clone())?;
            let uri = &params.text_document_position.text_document.uri;
            let text = &documents
                .get(uri.as_str())
                .ok_or("completion requested for an unopened document")?
                .text;
            send_ok(
                connection,
                request.id.clone(),
                &completion_with_options(text, params.text_document_position.position, &options),
            )
        }
        "workspace/symbol" => {
            let params: WorkspaceSymbolParams = serde_json::from_value(request.params.clone())?;
            send_ok(
                connection,
                request.id.clone(),
                &workspace_symbol_response(documents, &params.query, &options),
            )
        }
        "textDocument/documentSymbol" => {
            let params: DocumentSymbolParams = serde_json::from_value(request.params.clone())?;
            let uri = &params.text_document.uri;
            let text = &documents
                .get(uri.as_str())
                .ok_or("document symbols requested for an unopened document")?
                .text;
            send_ok(
                connection,
                request.id.clone(),
                &document_symbols_with_options(text, &options),
            )
        }
        "textDocument/definition" => {
            let params: GotoDefinitionParams = serde_json::from_value(request.params.clone())?;
            let uri = &params.text_document_position_params.text_document.uri;
            let text = &documents
                .get(uri.as_str())
                .ok_or("definition requested for an unopened document")?
                .text;
            send_ok(
                connection,
                request.id.clone(),
                &definition_with_options(
                    text,
                    uri,
                    params.text_document_position_params.position,
                    &options,
                ),
            )
        }
        "textDocument/hover" => {
            let params: HoverParams = serde_json::from_value(request.params.clone())?;
            let uri = &params.text_document_position_params.text_document.uri;
            let text = &documents
                .get(uri.as_str())
                .ok_or("hover requested for an unopened document")?
                .text;
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
            let text = &documents
                .get(uri.as_str())
                .ok_or("references requested for an unopened document")?
                .text;
            send_ok(
                connection,
                request.id.clone(),
                &references_with_options(
                    text,
                    uri,
                    position.position,
                    params.context.include_declaration,
                    &options,
                ),
            )
        }
        "textDocument/rename" => {
            let params: RenameParams = serde_json::from_value(request.params.clone())?;
            let position = params.text_document_position;
            let uri = &position.text_document.uri;
            let text = &documents
                .get(uri.as_str())
                .ok_or("rename requested for an unopened document")?
                .text;
            match rename_with_options(text, uri, position.position, &params.new_name, &options) {
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
    documents: &HashMap<String, DocumentState>,
    query: &str,
    options: &LspOptions,
) -> Vec<SymbolInformation> {
    let mut symbols = Vec::new();
    for (uri_text, doc) in documents {
        if let Ok(uri) = uri_text.parse::<Uri>() {
            symbols.extend(workspace_symbols_with_options(
                &doc.text, &uri, query, options,
            ));
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
    use adotenv_lsp::Dialect;
    use std::path::Path;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn cli_with_config(config: Option<PathBuf>) -> Cli {
        Cli {
            mask_values: false,
            dialect: None,
            max_input_bytes: None,
            max_lines: None,
            max_items: None,
            max_diagnostics: None,
            config,
            discover_lint_config: false,
            help: false,
            version: false,
        }
    }

    struct TempFile(PathBuf);

    impl TempFile {
        fn new(name: &str, contents: &str) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "adotenv-lsp-{name}-{}-{}.toml",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let file = Self(path);
            std::fs::write(&file.0, contents).unwrap();
            file
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn defaults_to_no_lint_configuration() {
        let cli = parse_args_from(args(&[])).unwrap();
        assert!(!cli.mask_values);
        assert!(cli.max_input_bytes.is_none());
        assert!(cli.max_lines.is_none());
        assert!(cli.config.is_none());
        assert!(!cli.discover_lint_config);
    }

    #[test]
    fn config_dialect_is_applied_to_lsp_options() {
        let file = TempFile::new("dialect", "dialect = \"python\"\n");
        let cli = cli_with_config(Some(file.0.clone()));
        let (options, _) = load_policy(&cli).unwrap();
        assert_eq!(options.dialect, Dialect::PYTHON_DOTENV);
    }

    #[test]
    fn parses_limit_and_dialect_flags_and_forwards_to_options() {
        let cli = parse_args_from(args(&[
            "--dialect",
            "compose",
            "--max-input-bytes",
            "4096",
            "--max-lines=128",
            "--max-items",
            "64",
            "--max-diagnostics=16",
        ]))
        .unwrap();
        assert_eq!(cli.dialect, Some(Dialect::COMPOSE));
        assert_eq!(cli.max_input_bytes, Some(4096));
        assert_eq!(cli.max_lines, Some(128));
        assert_eq!(cli.max_items, Some(64));
        assert_eq!(cli.max_diagnostics, Some(16));
        let (options, _) = load_policy(&cli).unwrap();
        assert_eq!(options.dialect, Dialect::COMPOSE);
        assert_eq!(options.parse_options.max_input_bytes, Some(4096));
        assert_eq!(options.parse_options.max_lines, Some(128));
        assert_eq!(options.parse_options.max_items, Some(64));
        assert_eq!(options.parse_options.max_diagnostics, Some(16));

        let cli_eq = parse_args_from(args(&[
            "--dialect=docker",
            "--max-input-bytes=64",
            "--max-lines",
            "4",
            "--max-items=8",
            "--max-diagnostics",
            "2",
        ]))
        .unwrap();
        assert_eq!(cli_eq.dialect, Some(Dialect::DOCKER_RAW));
        assert_eq!(cli_eq.max_input_bytes, Some(64));
        assert_eq!(cli_eq.max_lines, Some(4));
        assert_eq!(cli_eq.max_items, Some(8));
        assert_eq!(cli_eq.max_diagnostics, Some(2));

        // Explicit --dialect overrides config dialect.
        let file = TempFile::new("dialect-override", "dialect = \"python\"\n");
        let mut override_cli = cli_with_config(Some(file.0.clone()));
        override_cli.dialect = Some(Dialect::NODE_DOTENV);
        let (override_options, _) = load_policy(&override_cli).unwrap();
        assert_eq!(override_options.dialect, Dialect::NODE_DOTENV);

        assert!(parse_args_from(args(&["--dialect"])).is_err());
        assert!(parse_args_from(args(&["--dialect=unknown"])).is_err());
        assert!(parse_args_from(args(&["--max-input-bytes"])).is_err());
        assert!(parse_args_from(args(&["--max-lines"])).is_err());
        assert!(parse_args_from(args(&["--max-items"])).is_err());
        assert!(parse_args_from(args(&["--max-diagnostics"])).is_err());
        assert!(parse_args_from(args(&["--max-input-bytes=abc"])).is_err());
        assert!(parse_args_from(args(&["--max-input-bytes=+10"])).is_err());
        assert!(parse_args_from(args(&["--max-lines=-1"])).is_err());
    }

    #[test]
    fn parses_explicit_config_and_discovery() {
        let cli = parse_args_from(args(&["--mask-values", "--config", "policy.toml"])).unwrap();
        assert!(cli.mask_values);
        assert_eq!(cli.config, Some(PathBuf::from("policy.toml")));
        assert!(!cli.discover_lint_config);

        let cli = parse_args_from(args(&["--config=policy.toml"])).unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("policy.toml")));

        let cli = parse_args_from(args(&["--config=policy.toml", "--discover-lint-config"]));
        assert!(cli.is_err());

        let cli = parse_args_from(args(&["--discover-lint-config"])).unwrap();
        assert!(cli.discover_lint_config);

        assert_eq!(decoded_uri_path("/a%2"), PathBuf::from("/a%2"));
        assert_eq!(decoded_uri_path("/%20"), PathBuf::from("/ "));
    }

    #[test]
    fn parses_help_and_version_flags() {
        let cli = parse_args_from(args(&["--help"])).unwrap();
        assert!(cli.help);
        let cli = parse_args_from(args(&["--version"])).unwrap();
        assert!(cli.version);
    }

    #[test]
    fn rejects_unknown_or_incomplete_arguments() {
        assert!(parse_args_from(args(&["--config"])).is_err());
        assert!(parse_args_from(args(&["--unknown"])).is_err());
        assert!(parse_args_from(args(&["--", "extra"])).is_err());
    }

    #[test]
    fn default_policy_has_no_linter_or_ignores() {
        let cli = cli_with_config(None);
        let (options, ignore) = load_policy(&cli).unwrap();
        assert!(options.lint.is_none());
        assert!(ignore.is_none());
    }

    #[test]
    fn explicit_config_resolves_lint_policy_and_ignores() {
        let file = TempFile::new(
            "policy",
            "key_order = true\nignore = [\".env.skip\"]\n[rules]\nduplicate-key = \"error\"\n",
        );
        let cli = cli_with_config(Some(file.0.clone()));
        let (options, ignore) = load_policy(&cli).unwrap();
        let lint = options.lint.expect("lint policy");
        assert!(lint.key_order);
        assert_eq!(
            lint.severity_for_rule(adotenv_lint::Rule::DuplicateKey),
            adotenv::Severity::Error
        );
        assert!(ignore.unwrap().is_ignored(Path::new(".env.skip")));
    }

    #[test]
    fn config_policy_does_not_duplicate_parser_diagnostics() {
        let file = TempFile::new("duplicate", "key_order = true\n");
        let cli = cli_with_config(Some(file.0.clone()));
        let (options, _) = load_policy(&cli).unwrap();
        let uri: lsp_types::Uri = "file:///tmp/.env".parse().unwrap();
        let diagnostics = adotenv_lsp::diagnostics("B=2\nA=1\nA=2\n", &uri, &options);
        assert!(
            !diagnostics.iter().any(|diagnostic| {
                matches!(
                    diagnostic.code.as_ref(),
                    Some(lsp_types::NumberOrString::String(code))
                        if code == "adotenv-lint.duplicate-key"
                )
            }),
            "parser duplicate diagnostics were duplicated by the linter"
        );
    }

    #[test]
    fn ignored_uri_uses_configured_patterns() {
        let ignore = IgnoredPaths::new(&[".env.skip".to_owned()]).unwrap();
        let uri: lsp_types::Uri = "file:///tmp/.env.skip".parse().unwrap();
        assert!(ignored_uri(&uri, Some(&ignore)));
        let nested: lsp_types::Uri = "file:///project/target/.env".parse().unwrap();
        let target_ignore = IgnoredPaths::new(&["target/**".to_owned()]).unwrap();
        assert!(ignored_uri(&nested, Some(&target_ignore)));
        let uri: lsp_types::Uri = "file:///tmp/.env".parse().unwrap();
        assert!(!ignored_uri(&uri, Some(&ignore)));
        assert!(!ignored_uri(&uri, None));
    }

    #[test]
    fn ignored_uri_decodes_percent_escaped_paths() {
        let ignore = IgnoredPaths::new(&["my dir/**".to_owned()]).unwrap();
        let uri: lsp_types::Uri = "file:///tmp/my%20dir/.env".parse().unwrap();
        assert!(ignored_uri(&uri, Some(&ignore)));
        // Non-file URIs are never suppressed by filesystem ignore globs.
        let untitled: lsp_types::Uri = "untitled:my%20dir/.env".parse().unwrap();
        assert!(!ignored_uri(&untitled, Some(&ignore)));
        // Windows drive-letter URIs strip the leading slash.
        let win_uri: lsp_types::Uri = "file:///c%3A/my%20dir/.env".parse().unwrap();
        assert!(ignored_uri(&win_uri, Some(&ignore)));
        assert_eq!(
            decoded_uri_path("/c%3A/proj/.env"),
            PathBuf::from("c:/proj/.env")
        );
        assert_eq!(
            decoded_uri_path("/tmp/my%20dir%2fsub%2F.env"),
            PathBuf::from("/tmp/my dir/sub/.env")
        );
        assert_eq!(
            decoded_uri_path("/tmp/bad%ZZpath"),
            PathBuf::from("/tmp/bad%ZZpath")
        );
        assert_eq!(
            decoded_uri_path("/tmp/%FFbad"),
            PathBuf::from("/tmp/%FFbad")
        );
    }

    #[test]
    fn invalid_explicit_config_is_an_error() {
        let file = TempFile::new("invalid", "unknown = true\n");
        let cli = cli_with_config(Some(file.0.clone()));
        assert!(load_policy(&cli).is_err());
    }

    fn send_req(client: &Connection, id: i32, method: &str, params: serde_json::Value) -> Response {
        client
            .sender
            .send(Message::Request(Request::new(
                id.into(),
                method.to_owned(),
                params,
            )))
            .unwrap();
        match client
            .receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap()
        {
            Message::Response(response) => response,
            other => panic!("expected response for {method}, got {other:?}"),
        }
    }

    fn recv_diagnostics(client: &Connection) -> PublishDiagnosticsParams {
        match client
            .receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap()
        {
            Message::Notification(notification)
                if notification.method == "textDocument/publishDiagnostics" =>
            {
                serde_json::from_value(notification.params).unwrap()
            }
            other => panic!("expected publishDiagnostics, got {other:?}"),
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn serve_handles_full_lsp_lifecycle_over_memory_transport() {
        let (server, client) = Connection::memory();
        let ignore = IgnoredPaths::new(&[".env.skip".to_owned()]).unwrap();
        let options = LspOptions::new().with_mask_values(true);
        let thread = std::thread::spawn(move || serve(&server, options, Some(&ignore)));

        // Initialize handshake
        let init = send_req(
            &client,
            1,
            "initialize",
            serde_json::json!({ "capabilities": {} }),
        );
        let result = init.response_result.unwrap();
        assert_eq!(result["serverInfo"]["name"], "adotenv-lsp");
        assert_eq!(result["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
        let caps = &result["capabilities"];
        assert_eq!(caps["textDocumentSync"], 2);
        assert_eq!(
            caps["completionProvider"]["triggerCharacters"],
            serde_json::json!(["$", "{"])
        );
        assert_eq!(caps["definitionProvider"], true);
        assert_eq!(caps["documentSymbolProvider"], true);
        assert_eq!(caps["hoverProvider"], true);
        assert_eq!(caps["workspaceSymbolProvider"], true);
        assert_eq!(caps["referencesProvider"], true);
        assert_eq!(caps["renameProvider"], true);
        client
            .sender
            .send(Message::Notification(Notification::new(
                "initialized".to_owned(),
                serde_json::json!({}),
            )))
            .unwrap();

        // didOpen -> publishDiagnostics
        let uri = "file:///tmp/.env";
        client
            .sender
            .send(Message::Notification(Notification::new(
                "textDocument/didOpen".to_owned(),
                serde_json::json!({
                    "textDocument": {
                        "uri": uri,
                        "languageId": "dotenv",
                        "version": 1,
                        "text": "HOST=secret\nURL=${HOST}\nBAD KEY=1\n"
                    }
                }),
            )))
            .unwrap();
        let diags = recv_diagnostics(&client);
        assert_eq!(diags.uri.as_str(), uri);
        assert_eq!(diags.version, Some(1));
        assert_eq!(diags.diagnostics.len(), 1);

        // Ignored URI publishes empty diagnostics
        let skip_uri = "file:///tmp/.env.skip";
        client
            .sender
            .send(Message::Notification(Notification::new(
                "textDocument/didOpen".to_owned(),
                serde_json::json!({
                    "textDocument": {
                        "uri": skip_uri,
                        "languageId": "dotenv",
                        "version": 1,
                        "text": "BAD KEY=1\n"
                    }
                }),
            )))
            .unwrap();
        let skipped_diags = recv_diagnostics(&client);
        assert_eq!(skipped_diags.uri.as_str(), skip_uri);
        assert_eq!(skipped_diags.version, Some(1));
        assert_eq!(skipped_diags.diagnostics, Vec::new());

        // didChange (incremental removal of invalid line) -> clean diagnostics
        client
            .sender
            .send(Message::Notification(Notification::new(
                "textDocument/didChange".to_owned(),
                serde_json::json!({
                    "textDocument": { "uri": uri, "version": 2 },
                    "contentChanges": [{
                        "range": {
                            "start": { "line": 2, "character": 0 },
                            "end": { "line": 3, "character": 0 }
                        },
                        "text": ""
                    }]
                }),
            )))
            .unwrap();
        let updated = recv_diagnostics(&client);
        assert_eq!(updated.version, Some(2));
        assert_eq!(updated.diagnostics, Vec::new());

        // didChange for an unopened document is ignored without fabricating a buffer.
        client
            .sender
            .send(Message::Notification(Notification::new(
                "textDocument/didChange".to_owned(),
                serde_json::json!({
                    "textDocument": { "uri": "file:///tmp/ghost.env", "version": 1 },
                    "contentChanges": [{ "text": "GHOST=1\n" }]
                }),
            )))
            .unwrap();

        // Invalid incremental change & unknown notification & unexpected response are ignored cleanly
        client
            .sender
            .send(Message::Notification(Notification::new(
                "textDocument/didChange".to_owned(),
                serde_json::json!({
                    "textDocument": { "uri": uri, "version": 3 },
                    "contentChanges": [{
                        "range": {
                            "start": { "line": 99, "character": 0 },
                            "end": { "line": 99, "character": 1 }
                        },
                        "text": "x"
                    }]
                }),
            )))
            .unwrap();
        client
            .sender
            .send(Message::Notification(Notification::new(
                "custom/ignored".to_owned(),
                serde_json::json!({}),
            )))
            .unwrap();
        client
            .sender
            .send(Message::Response(Response::new_ok(
                999.into(),
                serde_json::Value::Null,
            )))
            .unwrap();

        // completion, documentSymbol, workspace/symbol, definition, hover, references, rename
        let comp = send_req(
            &client,
            2,
            "textDocument/completion",
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 0 }
            }),
        );
        assert_eq!(comp.response_result.unwrap().as_array().unwrap().len(), 2);

        let doc_syms = send_req(
            &client,
            3,
            "textDocument/documentSymbol",
            serde_json::json!({ "textDocument": { "uri": uri } }),
        );
        assert_eq!(
            doc_syms.response_result.unwrap().as_array().unwrap().len(),
            2
        );

        let ws_syms = send_req(
            &client,
            4,
            "workspace/symbol",
            serde_json::json!({ "query": "" }),
        );
        let ws_arr = ws_syms.response_result.unwrap();
        let ws_names: Vec<&str> = ws_arr
            .as_array()
            .unwrap()
            .iter()
            .map(|sym| sym["name"].as_str().unwrap())
            .collect();
        assert_eq!(ws_names, vec!["HOST", "URL"]);

        let def = send_req(
            &client,
            5,
            "textDocument/definition",
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 7 }
            }),
        );
        assert_eq!(def.response_result.unwrap()["range"]["start"]["line"], 0);

        let hov = send_req(
            &client,
            6,
            "textDocument/hover",
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 0 }
            }),
        );
        let hov_val = hov.response_result.unwrap()["contents"]["value"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(hov_val.contains("<redacted>"));
        assert!(!hov_val.contains("secret"));

        let refs = send_req(
            &client,
            7,
            "textDocument/references",
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 0 },
                "context": { "includeDeclaration": true }
            }),
        );
        assert_eq!(refs.response_result.unwrap().as_array().unwrap().len(), 2);

        let ren_ok = send_req(
            &client,
            8,
            "textDocument/rename",
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 0 },
                "newName": "SERVER"
            }),
        );
        assert_eq!(
            ren_ok.response_result.unwrap()["changes"][uri]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        // Error paths: bad rename (RequestFailed), unopened doc (InvalidParams), unknown method (MethodNotFound)
        let ren_err = send_req(
            &client,
            9,
            "textDocument/rename",
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 0 },
                "newName": "BAD-REF"
            }),
        );
        assert_eq!(
            ren_err.response_result.unwrap_err().code,
            lsp_server::ErrorCode::RequestFailed as i32
        );

        let unopened = send_req(
            &client,
            10,
            "textDocument/hover",
            serde_json::json!({
                "textDocument": { "uri": "file:///tmp/missing.env" },
                "position": { "line": 0, "character": 0 }
            }),
        );
        assert_eq!(
            unopened.response_result.unwrap_err().code,
            lsp_server::ErrorCode::InvalidParams as i32
        );

        let unknown = send_req(
            &client,
            11,
            "textDocument/doesNotExist",
            serde_json::json!({}),
        );
        assert_eq!(
            unknown.response_result.unwrap_err().code,
            lsp_server::ErrorCode::MethodNotFound as i32
        );

        // didClose clears diagnostics
        client
            .sender
            .send(Message::Notification(Notification::new(
                "textDocument/didClose".to_owned(),
                serde_json::json!({ "textDocument": { "uri": uri } }),
            )))
            .unwrap();
        let closed = recv_diagnostics(&client);
        assert_eq!(closed.version, None);
        assert_eq!(closed.diagnostics, Vec::new());

        // Shutdown + exit
        let shut = send_req(&client, 12, "shutdown", serde_json::Value::Null);
        assert!(shut.response_result.is_ok());
        client
            .sender
            .send(Message::Notification(Notification::new(
                "exit".to_owned(),
                serde_json::Value::Null,
            )))
            .unwrap();

        thread.join().unwrap().unwrap();
    }

    #[test]
    fn serve_returns_error_on_exit_without_shutdown() {
        let (server, client) = Connection::memory();
        let thread = std::thread::spawn(move || serve(&server, LspOptions::new(), None));

        let init = send_req(
            &client,
            1,
            "initialize",
            serde_json::json!({ "capabilities": {} }),
        );
        assert!(init.response_result.is_ok());
        client
            .sender
            .send(Message::Notification(Notification::new(
                "initialized".to_owned(),
                serde_json::json!({}),
            )))
            .unwrap();
        client
            .sender
            .send(Message::Notification(Notification::new(
                "exit".to_owned(),
                serde_json::Value::Null,
            )))
            .unwrap();

        assert!(thread.join().unwrap().is_err());
    }

    #[test]
    fn serve_rejects_rename_exceeding_max_input_bytes() {
        let (server, client) = Connection::memory();
        let options = LspOptions::new()
            .with_parse_options(adotenv_lsp::ParseOptions::new().with_max_input_bytes(20));
        let thread = std::thread::spawn(move || serve(&server, options, None));

        let init = send_req(
            &client,
            1,
            "initialize",
            serde_json::json!({ "capabilities": {} }),
        );
        assert!(init.response_result.is_ok());
        client
            .sender
            .send(Message::Notification(Notification::new(
                "initialized".to_owned(),
                serde_json::json!({}),
            )))
            .unwrap();

        let uri = "file:///tmp/bounded.env";
        client
            .sender
            .send(Message::Notification(Notification::new(
                "textDocument/didOpen".to_owned(),
                serde_json::json!({
                    "textDocument": {
                        "uri": uri,
                        "languageId": "dotenv",
                        "version": 1,
                        "text": "A=1\nX=$A\n"
                    }
                }),
            )))
            .unwrap();
        let _ = recv_diagnostics(&client);

        let ren_err = send_req(
            &client,
            2,
            "textDocument/rename",
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 0 },
                "newName": "VERY_LONG_HOST_NAME"
            }),
        );
        assert_eq!(
            ren_err.response_result.unwrap_err().code,
            lsp_server::ErrorCode::RequestFailed as i32
        );

        let shut = send_req(&client, 3, "shutdown", serde_json::Value::Null);
        assert!(shut.response_result.is_ok());
        client
            .sender
            .send(Message::Notification(Notification::new(
                "exit".to_owned(),
                serde_json::Value::Null,
            )))
            .unwrap();
        thread.join().unwrap().unwrap();
    }
}
