//! Stdio language server for adotenv documents.

mod transport;

use std::{
    env,
    error::Error,
    io::{BufRead, Write},
    path::PathBuf,
    process::ExitCode,
};

use adotenv_lint::{
    load_effective_config, parse_dialect, ConfigExplicit, IgnoredPaths, LintConfig, LintPolicy,
};
use adotenv_lsp::{Dialect, LspOptions, ParseOptions, PositionEncoding, Session};
use lsp_types::{
    CodeActionKind, CodeActionOptions, CodeActionParams, CodeActionProviderCapability,
    CompletionOptions, CompletionParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentFormattingParams, DocumentSymbolParams,
    GotoDefinitionParams, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult,
    OneOf, PublishDiagnosticsParams, ReferenceParams, RenameOptions, RenameParams,
    ServerCapabilities, ServerInfo, TextDocumentPositionParams, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri, WorkDoneProgressOptions, WorkspaceSymbolParams,
};
use serde_json::Value;

use crate::transport::{
    error_code, IncomingMessage, RequestId, Transport, TransportError, DEFAULT_MAX_MESSAGE_BYTES,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

enum RequestError {
    InvalidParams(Box<dyn Error + Send + Sync>),
    Transport(TransportError),
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
                     [--max-message-bytes <n>] \
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
    let explicit_config = match &cli.config {
        Some(path) => Some(LintConfig::load(path)?),
        None => None,
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut transport = Transport::new(stdin.lock(), stdout.lock(), cli.max_message_bytes);
    serve(&mut transport, &cli, explicit_config.as_ref())
}

#[allow(clippy::too_many_lines)]
fn serve<R: BufRead, W: Write>(
    transport: &mut Transport<R, W>,
    cli: &Cli,
    explicit_config: Option<&LintConfig>,
) -> Result<()> {
    // Wait for `initialize` per LSP 3.17 §3.1: reject other requests with
    // ServerNotInitialized (-32002) and ignore other notifications except `exit`.
    let (init_id, init_params) = loop {
        let Some(message) = transport.read_message()? else {
            return Err("client disconnected before initialize".into());
        };
        match message {
            IncomingMessage::Request { id, method, params } if method == "initialize" => {
                match serde_json::from_value::<InitializeParams>(params) {
                    Ok(parsed) => break (id, parsed),
                    Err(error) => {
                        let _ = transport.send_error(
                            Some(id),
                            error_code::INVALID_PARAMS,
                            &error.to_string(),
                        );
                        return Err(error.into());
                    }
                }
            }
            IncomingMessage::Request { id, .. } => {
                transport.send_error(
                    Some(id),
                    error_code::SERVER_NOT_INITIALIZED,
                    "server is not initialized",
                )?;
            }
            IncomingMessage::Notification { method, .. } if method == "exit" => {
                return Err("client exited before shutdown".into());
            }
            IncomingMessage::Notification { .. } | IncomingMessage::Response => {}
            IncomingMessage::Invalid { id, code, message } => {
                transport.send_error(id, code, &message)?;
            }
        }
    };

    let encoding = PositionEncoding::negotiate(
        init_params
            .capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_deref()),
    );
    let (options, ignore) = match resolve_session_policy(cli, explicit_config, &init_params) {
        Ok((opts, ignore)) => (opts.with_position_encoding(encoding), ignore),
        Err(error) => {
            let _ = transport.send_error(
                Some(init_id),
                error_code::INTERNAL_ERROR,
                &error.to_string(),
            );
            return Err(error);
        }
    };

    let capabilities = ServerCapabilities {
        position_encoding: Some(encoding.to_lsp_kind()),
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
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        document_formatting_provider: Some(OneOf::Left(true)),
        code_action_provider: options.lint.map(|_| {
            CodeActionProviderCapability::Options(CodeActionOptions {
                code_action_kinds: Some(vec![
                    CodeActionKind::QUICKFIX,
                    CodeActionKind::SOURCE_FIX_ALL,
                ]),
                ..CodeActionOptions::default()
            })
        }),
        ..ServerCapabilities::default()
    };
    let init_result = InitializeResult {
        capabilities,
        server_info: Some(ServerInfo {
            name: "adotenv-lsp".to_owned(),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }),
    };
    transport.send_ok(init_id, &init_result)?;

    let mut session = Session::new(options);
    let mut shutdown_received = false;
    while let Some(message) = transport.read_message()? {
        match message {
            IncomingMessage::Request { id, method, params } => {
                if shutdown_received {
                    transport.send_error(
                        Some(id),
                        error_code::INVALID_REQUEST,
                        "server is shutting down",
                    )?;
                    continue;
                }
                if method == "shutdown" {
                    shutdown_received = true;
                    transport.send_ok(id, &Value::Null)?;
                    continue;
                }
                if method == "initialize" {
                    transport.send_error(
                        Some(id),
                        error_code::INVALID_REQUEST,
                        "server is already initialized",
                    )?;
                    continue;
                }
                match handle_request(
                    transport,
                    id.clone(),
                    &method,
                    params,
                    &session,
                    ignore.as_ref(),
                ) {
                    Ok(()) => {}
                    Err(RequestError::InvalidParams(error)) => {
                        transport.send_error(
                            Some(id),
                            error_code::INVALID_PARAMS,
                            &error.to_string(),
                        )?;
                    }
                    Err(RequestError::Transport(error)) => return Err(error.into()),
                }
            }
            IncomingMessage::Notification { method, params } => {
                if method == "exit" {
                    break;
                }
                if shutdown_received {
                    continue;
                }
                match handle_notification(transport, &method, params, &mut session, ignore.as_ref())
                {
                    Ok(()) => {}
                    Err(NotificationError::Malformed(error)) => {
                        eprintln!("adotenv-lsp: ignored {method}: {error}");
                    }
                    Err(NotificationError::Transport(error)) => return Err(error),
                }
            }
            IncomingMessage::Response => {}
            IncomingMessage::Invalid { id, code, message } => {
                transport.send_error(id, code, &message)?;
            }
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
    max_message_bytes: usize,
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

fn parse_positive_limit(flag: &str, value: &str) -> Result<usize> {
    let parsed = parse_limit(flag, value)?;
    if parsed == 0 {
        return Err(format!("{flag} must be at least 1").into());
    }
    Ok(parsed)
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
        max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
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
            "--max-message-bytes" => {
                let value = arguments
                    .next()
                    .ok_or("--max-message-bytes requires a number")?;
                cli.max_message_bytes = parse_positive_limit("--max-message-bytes", &value)?;
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
            other if other.starts_with("--max-message-bytes=") => {
                cli.max_message_bytes = parse_positive_limit(
                    "--max-message-bytes",
                    &other["--max-message-bytes=".len()..],
                )?;
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

fn discovery_root(init_params: &InitializeParams) -> Result<PathBuf> {
    if let Some(folders) = &init_params.workspace_folders {
        for folder in folders {
            if let Some(path) = file_uri_path(&folder.uri) {
                return Ok(path);
            }
        }
    }
    #[allow(deprecated)]
    if let Some(root_uri) = &init_params.root_uri {
        if let Some(path) = file_uri_path(root_uri) {
            return Ok(path);
        }
    }
    Ok(env::current_dir()?)
}

fn resolve_session_policy(
    cli: &Cli,
    explicit_config: Option<&LintConfig>,
    init_params: &InitializeParams,
) -> Result<(LspOptions, Option<IgnoredPaths>)> {
    if let Some(config) = explicit_config {
        return build_policy(cli, Some(config));
    }
    if cli.discover_lint_config {
        let start = discovery_root(init_params)?;
        let discovered = load_effective_config(None, &start)?;
        return build_policy(cli, discovered.as_ref());
    }
    build_policy(cli, None)
}

#[cfg(test)]
fn load_policy_from_dir(
    cli: &Cli,
    start_dir: &std::path::Path,
) -> Result<(LspOptions, Option<IgnoredPaths>)> {
    let config = if let Some(path) = &cli.config {
        Some(LintConfig::load(path)?)
    } else if cli.discover_lint_config {
        load_effective_config(None, start_dir)?
    } else {
        None
    };
    build_policy(cli, config.as_ref())
}

fn build_policy(
    cli: &Cli,
    config: Option<&LintConfig>,
) -> Result<(LspOptions, Option<IgnoredPaths>)> {
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

fn handle_notification<R: BufRead, W: Write>(
    transport: &mut Transport<R, W>,
    method: &str,
    params: Value,
    session: &mut Session,
    ignore: Option<&IgnoredPaths>,
) -> std::result::Result<(), NotificationError> {
    match method {
        "textDocument/didOpen" => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(params)
                .map_err(|error| NotificationError::Malformed(error.into()))?;
            let uri = params.text_document.uri;
            session.open(
                uri.clone(),
                Some(params.text_document.version),
                params.text_document.text,
            );
            publish_diagnostics(transport, &uri, session, ignore)
                .map_err(NotificationError::Transport)?;
        }
        "textDocument/didChange" => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(params)
                .map_err(|error| NotificationError::Malformed(error.into()))?;
            let uri = params.text_document.uri;
            session
                .change(
                    &uri,
                    Some(params.text_document.version),
                    &params.content_changes,
                )
                .map_err(|error| NotificationError::Malformed(error.into()))?;
            publish_diagnostics(transport, &uri, session, ignore)
                .map_err(NotificationError::Transport)?;
        }
        "textDocument/didClose" => {
            let params: DidCloseTextDocumentParams = serde_json::from_value(params)
                .map_err(|error| NotificationError::Malformed(error.into()))?;
            let uri = params.text_document.uri;
            session.close(&uri);
            publish_empty_diagnostics(transport, &uri).map_err(NotificationError::Transport)?;
        }
        _ => {}
    }
    Ok(())
}

fn publish_diagnostics<R: BufRead, W: Write>(
    transport: &mut Transport<R, W>,
    uri: &Uri,
    session: &Session,
    ignore: Option<&IgnoredPaths>,
) -> Result<()> {
    let doc = session
        .document(uri)
        .ok_or("diagnostics requested for an unopened document")?;
    let version = session.version(uri);
    if ignored_uri(uri, ignore) {
        return send_diagnostics(transport, uri, Vec::new(), version);
    }
    send_diagnostics(transport, uri, doc.diagnostics(uri), version)
}

fn file_uri_path(uri: &Uri) -> Option<PathBuf> {
    if !uri
        .scheme()
        .is_some_and(|scheme| scheme.as_str().eq_ignore_ascii_case("file"))
    {
        return None;
    }
    if let Some(authority) = uri.authority() {
        let raw = authority.as_str();
        if !raw.is_empty() && !raw.eq_ignore_ascii_case("localhost") {
            return None;
        }
    }
    Some(decoded_uri_path(uri.path().as_str()))
}

fn ignored_uri(uri: &Uri, ignore: Option<&IgnoredPaths>) -> bool {
    let Some(ignore) = ignore else {
        return false;
    };
    let Some(path) = file_uri_path(uri) else {
        return false;
    };
    ignore.is_ignored(&path)
}

/// Decodes the percent-escaped path component of a file URI for glob matching
/// and workspace-folder config discovery.
///
/// `uri.path()` keeps escapes such as `%20`, while ignore patterns and
/// filesystem paths use the decoded spelling. Windows drive-letter paths
/// (`/C:/...`) have their leading slash stripped. Invalid escapes or invalid
/// UTF-8 fall back to the original path.
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
    if bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && (bytes.len() == 3 || bytes[3] == b'/' || bytes[3] == b'\\')
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

fn publish_empty_diagnostics<R: BufRead, W: Write>(
    transport: &mut Transport<R, W>,
    uri: &Uri,
) -> Result<()> {
    send_diagnostics(transport, uri, Vec::new(), None)
}

fn send_diagnostics<R: BufRead, W: Write>(
    transport: &mut Transport<R, W>,
    uri: &Uri,
    diagnostics: Vec<lsp_types::Diagnostic>,
    version: Option<i32>,
) -> Result<()> {
    let params = PublishDiagnosticsParams::new(uri.clone(), diagnostics, version);
    transport.send_notification("textDocument/publishDiagnostics", &params)?;
    Ok(())
}

fn parse_request_params<T: serde::de::DeserializeOwned>(
    params: Value,
) -> std::result::Result<T, RequestError> {
    serde_json::from_value(params).map_err(|error| RequestError::InvalidParams(error.into()))
}

fn require_open_document<'a>(
    session: &'a Session,
    uri: &Uri,
    operation: &str,
) -> std::result::Result<&'a adotenv_lsp::ParsedDocument, RequestError> {
    session.document(uri).ok_or_else(|| {
        RequestError::InvalidParams(
            format!("{operation} requested for an unopened document").into(),
        )
    })
}

#[allow(clippy::too_many_lines)]
fn handle_request<R: BufRead, W: Write>(
    transport: &mut Transport<R, W>,
    id: RequestId,
    method: &str,
    params: Value,
    session: &Session,
    ignore: Option<&IgnoredPaths>,
) -> std::result::Result<(), RequestError> {
    match method {
        "textDocument/completion" => {
            let params: CompletionParams = parse_request_params(params)?;
            let uri = &params.text_document_position.text_document.uri;
            let doc = require_open_document(session, uri, "completion")?;
            transport
                .send_ok(id, &doc.completion(params.text_document_position.position))
                .map_err(RequestError::Transport)?;
        }
        "workspace/symbol" => {
            let params: WorkspaceSymbolParams = parse_request_params(params)?;
            transport
                .send_ok(id, &session.workspace_symbols(&params.query))
                .map_err(RequestError::Transport)?;
        }
        "textDocument/documentSymbol" => {
            let params: DocumentSymbolParams = parse_request_params(params)?;
            let uri = &params.text_document.uri;
            let doc = require_open_document(session, uri, "document symbols")?;
            transport
                .send_ok(id, &doc.document_symbols())
                .map_err(RequestError::Transport)?;
        }
        "textDocument/definition" => {
            let params: GotoDefinitionParams = parse_request_params(params)?;
            let uri = &params.text_document_position_params.text_document.uri;
            let doc = require_open_document(session, uri, "definition")?;
            transport
                .send_ok(
                    id,
                    &doc.definition(uri, params.text_document_position_params.position),
                )
                .map_err(RequestError::Transport)?;
        }
        "textDocument/hover" => {
            let params: HoverParams = parse_request_params(params)?;
            let uri = &params.text_document_position_params.text_document.uri;
            let doc = require_open_document(session, uri, "hover")?;
            transport
                .send_ok(
                    id,
                    &doc.hover(params.text_document_position_params.position),
                )
                .map_err(RequestError::Transport)?;
        }
        "textDocument/references" => {
            let params: ReferenceParams = parse_request_params(params)?;
            let position = params.text_document_position;
            let uri = &position.text_document.uri;
            let doc = require_open_document(session, uri, "references")?;
            transport
                .send_ok(
                    id,
                    &doc.references(uri, position.position, params.context.include_declaration),
                )
                .map_err(RequestError::Transport)?;
        }
        "textDocument/prepareRename" => {
            let params: TextDocumentPositionParams = parse_request_params(params)?;
            let uri = &params.text_document.uri;
            let doc = require_open_document(session, uri, "prepareRename")?;
            transport
                .send_ok(id, &doc.prepare_rename(params.position))
                .map_err(RequestError::Transport)?;
        }
        "textDocument/rename" => {
            let params: RenameParams = parse_request_params(params)?;
            let position = params.text_document_position;
            let uri = &position.text_document.uri;
            let doc = require_open_document(session, uri, "rename")?;
            match doc.rename(uri, position.position, &params.new_name) {
                Ok(edit) => transport
                    .send_ok(id, &edit)
                    .map_err(RequestError::Transport)?,
                Err(error) => transport
                    .send_error(Some(id), error_code::REQUEST_FAILED, &error.to_string())
                    .map_err(RequestError::Transport)?,
            }
        }
        "textDocument/formatting" => {
            let params: DocumentFormattingParams = parse_request_params(params)?;
            let uri = &params.text_document.uri;
            let doc = require_open_document(session, uri, "formatting")?;
            match doc.formatting(&params.options) {
                Ok(edits) => transport
                    .send_ok(id, &edits)
                    .map_err(RequestError::Transport)?,
                Err(error) => transport
                    .send_error(Some(id), error_code::REQUEST_FAILED, &error.to_string())
                    .map_err(RequestError::Transport)?,
            }
        }
        "textDocument/codeAction" => {
            let params: CodeActionParams = parse_request_params(params)?;
            let uri = &params.text_document.uri;
            let doc = require_open_document(session, uri, "code actions")?;
            let actions = if ignored_uri(uri, ignore) {
                Vec::new()
            } else {
                doc.code_actions(uri, params.range, &params.context)
            };
            transport
                .send_ok(id, &actions)
                .map_err(RequestError::Transport)?;
        }
        _ => {
            transport
                .send_error(
                    Some(id),
                    error_code::METHOD_NOT_FOUND,
                    "unsupported request",
                )
                .map_err(RequestError::Transport)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{TransportError, MAX_HEADERS, MAX_HEADER_LINE_BYTES};
    use adotenv_lsp::Dialect;
    use std::io::Cursor;
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
            max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
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

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "adotenv-lsp-dir-{name}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn frame(value: &Value) -> Vec<u8> {
        let body = serde_json::to_vec(value).unwrap();
        let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        out.extend_from_slice(&body);
        out
    }

    fn decode_frames(mut bytes: &[u8]) -> Vec<Value> {
        let mut out = Vec::new();
        while !bytes.is_empty() {
            let header_end = bytes
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .expect("header terminator");
            let header = std::str::from_utf8(&bytes[..header_end]).unwrap();
            let len: usize = header
                .strip_prefix("Content-Length: ")
                .unwrap()
                .parse()
                .unwrap();
            let start = header_end + 4;
            let body = &bytes[start..start + len];
            out.push(serde_json::from_slice(body).unwrap());
            bytes = &bytes[start + len..];
        }
        out
    }

    #[test]
    fn defaults_to_no_lint_configuration() {
        let cli = parse_args_from(args(&[])).unwrap();
        assert!(!cli.mask_values);
        assert!(cli.max_input_bytes.is_none());
        assert!(cli.max_lines.is_none());
        assert_eq!(cli.max_message_bytes, DEFAULT_MAX_MESSAGE_BYTES);
        assert!(cli.config.is_none());
        assert!(!cli.discover_lint_config);
    }

    #[test]
    fn config_dialect_is_applied_to_lsp_options() {
        let file = TempFile::new("dialect", "dialect = \"python\"\n");
        let cli = cli_with_config(Some(file.0.clone()));
        let (options, _) = load_policy_from_dir(&cli, Path::new(".")).unwrap();
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
            "--max-message-bytes",
            "32768",
        ]))
        .unwrap();
        assert_eq!(cli.dialect, Some(Dialect::COMPOSE));
        assert_eq!(cli.max_input_bytes, Some(4096));
        assert_eq!(cli.max_lines, Some(128));
        assert_eq!(cli.max_items, Some(64));
        assert_eq!(cli.max_diagnostics, Some(16));
        assert_eq!(cli.max_message_bytes, 32768);
        let (options, _) = load_policy_from_dir(&cli, Path::new(".")).unwrap();
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
            "--max-message-bytes=1024",
        ]))
        .unwrap();
        assert_eq!(cli_eq.dialect, Some(Dialect::DOCKER_RAW));
        assert_eq!(cli_eq.max_input_bytes, Some(64));
        assert_eq!(cli_eq.max_lines, Some(4));
        assert_eq!(cli_eq.max_items, Some(8));
        assert_eq!(cli_eq.max_diagnostics, Some(2));
        assert_eq!(cli_eq.max_message_bytes, 1024);

        let file = TempFile::new("dialect-override", "dialect = \"python\"\n");
        let mut override_cli = cli_with_config(Some(file.0.clone()));
        override_cli.dialect = Some(Dialect::NODE_DOTENV);
        let (override_options, _) = load_policy_from_dir(&override_cli, Path::new(".")).unwrap();
        assert_eq!(override_options.dialect, Dialect::NODE_DOTENV);

        assert!(parse_args_from(args(&["--dialect"])).is_err());
        assert!(parse_args_from(args(&["--dialect=unknown"])).is_err());
        assert!(parse_args_from(args(&["--max-input-bytes"])).is_err());
        assert!(parse_args_from(args(&["--max-lines"])).is_err());
        assert!(parse_args_from(args(&["--max-items"])).is_err());
        assert!(parse_args_from(args(&["--max-diagnostics"])).is_err());
        assert!(parse_args_from(args(&["--max-message-bytes"])).is_err());
        assert!(parse_args_from(args(&["--max-input-bytes=abc"])).is_err());
        assert!(parse_args_from(args(&["--max-input-bytes=+10"])).is_err());
        assert!(parse_args_from(args(&["--max-lines=-1"])).is_err());
        assert!(parse_args_from(args(&["--max-message-bytes", "0"])).is_err());
        assert!(parse_args_from(args(&["--max-message-bytes=0"])).is_err());
        assert!(parse_args_from(args(&["--max-message-bytes=+10"])).is_err());
        assert!(parse_args_from(args(&["--max-message-bytes=-1"])).is_err());
        assert!(parse_args_from(args(&["--max-message-bytes=abc"])).is_err());
    }

    #[test]
    fn parses_explicit_config_and_discovery() {
        let cli = parse_args_from(args(&["--mask-values", "--config", "policy.toml"])).unwrap();
        assert!(cli.mask_values);
        assert_eq!(cli.config, Some(PathBuf::from("policy.toml")));
        assert!(!cli.discover_lint_config);

        let cli = parse_args_from(args(&["--config=policy.toml"])).unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("policy.toml")));

        assert!(
            parse_args_from(args(&["--config=policy.toml", "--discover-lint-config"])).is_err()
        );

        let cli = parse_args_from(args(&["--discover-lint-config"])).unwrap();
        assert!(cli.discover_lint_config);

        assert_eq!(decoded_uri_path("/a%2"), PathBuf::from("/a%2"));
        assert_eq!(decoded_uri_path("/%20"), PathBuf::from("/ "));
    }

    #[test]
    fn parses_help_and_version_flags() {
        assert!(parse_args_from(args(&["--help"])).unwrap().help);
        assert!(parse_args_from(args(&["--version"])).unwrap().version);
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
        let (options, ignore) = load_policy_from_dir(&cli, Path::new(".")).unwrap();
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
        let (options, ignore) = load_policy_from_dir(&cli, Path::new(".")).unwrap();
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
        let (options, _) = load_policy_from_dir(&cli, Path::new(".")).unwrap();
        let uri: Uri = "file:///tmp/.env".parse().unwrap();
        let diagnostics = adotenv_lsp::diagnostics("B=2\nA=1\nA=2\n", &uri, &options);
        assert!(!diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code.as_ref(),
                Some(lsp_types::NumberOrString::String(code))
                    if code == "adotenv-lint.duplicate-key"
            )
        }));
    }

    #[test]
    fn ignored_uri_uses_configured_patterns() {
        let ignore = IgnoredPaths::new(&[".env.skip".to_owned()]).unwrap();
        let uri: Uri = "file:///tmp/.env.skip".parse().unwrap();
        assert!(ignored_uri(&uri, Some(&ignore)));
        let nested: Uri = "file:///project/target/.env".parse().unwrap();
        let target_ignore = IgnoredPaths::new(&["target/**".to_owned()]).unwrap();
        assert!(ignored_uri(&nested, Some(&target_ignore)));
        let uri: Uri = "file:///tmp/.env".parse().unwrap();
        assert!(!ignored_uri(&uri, Some(&ignore)));
        assert!(!ignored_uri(&uri, None));
    }

    #[test]
    fn ignored_uri_decodes_percent_escaped_paths() {
        let ignore = IgnoredPaths::new(&["my dir/**".to_owned()]).unwrap();
        let uri: Uri = "file:///tmp/my%20dir/.env".parse().unwrap();
        assert!(ignored_uri(&uri, Some(&ignore)));
        let untitled: Uri = "untitled:my%20dir/.env".parse().unwrap();
        assert!(!ignored_uri(&untitled, Some(&ignore)));
        let win_uri: Uri = "file:///c%3A/my%20dir/.env".parse().unwrap();
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
        assert_eq!(decoded_uri_path("/C:"), PathBuf::from("C:"));
        assert_eq!(
            decoded_uri_path("/c:notdrive"),
            PathBuf::from("/c:notdrive")
        );
        let local: Uri = "file://localhost/tmp/.env".parse().unwrap();
        assert_eq!(file_uri_path(&local), Some(PathBuf::from("/tmp/.env")));
        let local_upper: Uri = "file://LOCALHOST/tmp/.env".parse().unwrap();
        assert_eq!(
            file_uri_path(&local_upper),
            Some(PathBuf::from("/tmp/.env"))
        );
        let no_auth: Uri = "file:/tmp/.env".parse().unwrap();
        assert_eq!(file_uri_path(&no_auth), Some(PathBuf::from("/tmp/.env")));
        let remote: Uri = "file://remote/share/.env".parse().unwrap();
        assert_eq!(file_uri_path(&remote), None);
        let userinfo: Uri = "file://user@localhost/tmp/.env".parse().unwrap();
        assert_eq!(file_uri_path(&userinfo), None);
        let drive_only: Uri = "file:///C:".parse().unwrap();
        assert_eq!(file_uri_path(&drive_only), Some(PathBuf::from("C:")));
    }

    #[test]
    fn invalid_explicit_config_is_an_error() {
        let file = TempFile::new("invalid", "unknown = true\n");
        let cli = cli_with_config(Some(file.0.clone()));
        assert!(load_policy_from_dir(&cli, Path::new(".")).is_err());
    }

    #[test]
    fn transport_bounds_oversized_headers_counts_and_payloads() {
        // Oversized Content-Length is rejected before allocating payload.
        let mut t = Transport::new(
            Cursor::new(b"Content-Length: 999999999999\r\n\r\n".to_vec()),
            Vec::new(),
            1024,
        );
        assert!(matches!(
            t.read_message(),
            Err(TransportError::MessageTooLarge {
                length: 999_999_999_999,
                limit: 1024
            })
        ));

        // Oversized header line without newline is rejected at MAX_HEADER_LINE_BYTES.
        let huge_header = vec![b'A'; MAX_HEADER_LINE_BYTES + 16];
        let mut t = Transport::new(Cursor::new(huge_header), Vec::new(), 1024);
        assert!(matches!(
            t.read_message(),
            Err(TransportError::HeaderLineTooLong {
                limit: MAX_HEADER_LINE_BYTES
            })
        ));

        // Too many headers are rejected at MAX_HEADERS.
        let mut many_headers = String::new();
        for i in 0..=MAX_HEADERS {
            use std::fmt::Write as _;
            let _ = writeln!(many_headers, "X-Pad-{i}: 1\r");
        }
        many_headers.push_str("Content-Length: 2\r\n\r\n{}");
        let mut t = Transport::new(Cursor::new(many_headers.into_bytes()), Vec::new(), 1024);
        assert!(matches!(
            t.read_message(),
            Err(TransportError::TooManyHeaders { limit: MAX_HEADERS })
        ));

        // Missing or duplicate Content-Length and truncated payload fail cleanly.
        let mut t = Transport::new(
            Cursor::new(b"Content-Type: application/json\r\n\r\n{}".to_vec()),
            Vec::new(),
            1024,
        );
        assert!(matches!(
            t.read_message(),
            Err(TransportError::MissingContentLength)
        ));

        let mut t = Transport::new(
            Cursor::new(b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}".to_vec()),
            Vec::new(),
            1024,
        );
        assert!(matches!(
            t.read_message(),
            Err(TransportError::DuplicateContentLength)
        ));

        let mut t = Transport::new(
            Cursor::new(b"Content-Length: 10\r\n\r\n{}".to_vec()),
            Vec::new(),
            1024,
        );
        assert!(matches!(
            t.read_message(),
            Err(TransportError::UnexpectedEof)
        ));

        // Malformed JSON yields ParseError without tearing down framing.
        let mut t = Transport::new(
            Cursor::new(b"Content-Length: 3\r\n\r\n{{{".to_vec()),
            Vec::new(),
            1024,
        );
        assert!(matches!(
            t.read_message(),
            Ok(Some(IncomingMessage::Invalid {
                code: error_code::PARSE_ERROR,
                ..
            }))
        ));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn serve_handles_full_lsp_lifecycle_over_bounded_transport() {
        let cfg = TempFile::new(
            "serve-lint",
            "key_case = \"upper\"\nignore = [\".env.skip\"]\n",
        );
        let loaded = LintConfig::load(&cfg.0).unwrap();
        let mut cli = cli_with_config(Some(cfg.0.clone()));
        cli.mask_values = true;

        let uri = "file:///tmp/.env";
        let skip_uri = "file:///tmp/.env.skip";
        let mut input = Vec::new();
        // Pre-init request gets ServerNotInitialized (-32002).
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 0, "method": "textDocument/hover", "params": {}
        })));
        // Initialize negotiating utf-8.
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "capabilities": {
                    "general": { "positionEncodings": ["utf-8", "utf-16"] }
                }
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "initialized", "params": {}
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri, "languageId": "dotenv", "version": 1,
                    "text": "host=secret  \nURL=${host}\nBAD KEY=1\n"
                }
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": skip_uri, "languageId": "dotenv", "version": 1,
                    "text": "bad=1\n"
                }
            }
        })));
        // didChange removes invalid line.
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{
                    "range": {
                        "start": { "line": 2, "character": 0 },
                        "end": { "line": 3, "character": 0 }
                    },
                    "text": ""
                }]
            }
        })));
        // Unopened didChange & out-of-range line didChange are ignored cleanly.
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": "file:///tmp/ghost.env", "version": 1 },
                "contentChanges": [{ "text": "GHOST=1\n" }]
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 3 },
                "contentChanges": [{
                    "range": {
                        "start": { "line": 99, "character": 0 },
                        "end": { "line": 99, "character": 1 }
                    },
                    "text": "x"
                }]
            }
        })));
        // Requests: completion, documentSymbol, workspace/symbol, definition, hover,
        // references, prepareRename, rename, formatting, codeAction (normal & ignored),
        // error cases, didClose, shutdown, post-shutdown request, exit.
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "textDocument/completion",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 0, "character": 0 } }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "textDocument/documentSymbol",
            "params": { "textDocument": { "uri": uri } }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 4, "method": "workspace/symbol",
            "params": { "query": "url" }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 5, "method": "textDocument/definition",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 1, "character": 7 } }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 6, "method": "textDocument/hover",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 0, "character": 0 } }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 7, "method": "textDocument/references",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 0 },
                "context": { "includeDeclaration": true }
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 8, "method": "textDocument/prepareRename",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 1, "character": 7 } }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 9, "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 0 },
                "newName": "SERVER"
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 10, "method": "textDocument/formatting",
            "params": {
                "textDocument": { "uri": uri },
                "options": { "tabSize": 2, "insertSpaces": true, "trimTrailingWhitespace": true }
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 11, "method": "textDocument/codeAction",
            "params": {
                "textDocument": { "uri": uri },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 4 }
                },
                "context": { "diagnostics": [] }
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 12, "method": "textDocument/codeAction",
            "params": {
                "textDocument": { "uri": skip_uri },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 3 }
                },
                "context": { "diagnostics": [] }
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 13, "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 0 },
                "newName": "BAD-REF"
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 14, "method": "textDocument/doesNotExist",
            "params": {}
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/didClose",
            "params": { "textDocument": { "uri": uri } }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 15, "method": "shutdown", "params": null
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 16, "method": "workspace/symbol", "params": { "query": "" }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "exit", "params": null
        })));

        let mut transport =
            Transport::new(Cursor::new(input), Vec::new(), DEFAULT_MAX_MESSAGE_BYTES);
        serve(&mut transport, &cli, Some(&loaded)).unwrap();
        let messages = decode_frames(&transport.into_writer());

        let by_id = |target: i64| -> &Value {
            messages
                .iter()
                .find(|msg| msg.get("id").and_then(Value::as_i64) == Some(target))
                .expect("response id")
        };

        assert_eq!(
            by_id(0)["error"]["code"],
            error_code::SERVER_NOT_INITIALIZED
        );
        let init = &by_id(1)["result"];
        assert_eq!(init["serverInfo"]["name"], "adotenv-lsp");
        assert_eq!(init["capabilities"]["positionEncoding"], "utf-8");
        assert_eq!(
            init["capabilities"]["renameProvider"]["prepareProvider"],
            true
        );
        assert_eq!(init["capabilities"]["documentFormattingProvider"], true);
        assert_eq!(
            init["capabilities"]["codeActionProvider"]["codeActionKinds"],
            serde_json::json!(["quickfix", "source.fixAll"])
        );

        assert_eq!(by_id(2)["result"].as_array().unwrap().len(), 2);
        assert_eq!(by_id(3)["result"].as_array().unwrap().len(), 2);
        assert_eq!(by_id(4)["result"][0]["name"], "URL");
        assert_eq!(by_id(5)["result"]["range"]["start"]["line"], 0);
        assert!(by_id(6)["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("<redacted>"));
        // References return identifier span `(1, 6)..(1, 10)` for `${host}`.
        assert_eq!(by_id(7)["result"][1]["range"]["start"]["character"], 6);
        assert_eq!(by_id(7)["result"][1]["range"]["end"]["character"], 10);
        // prepareRename returns range + placeholder.
        assert_eq!(by_id(8)["result"]["placeholder"], "host");
        assert_eq!(by_id(8)["result"]["range"]["start"]["character"], 6);
        // rename returns 2 edits.
        assert_eq!(
            by_id(9)["result"]["changes"][uri].as_array().unwrap().len(),
            2
        );
        // formatting trims trailing whitespace on line 0.
        assert_eq!(by_id(10)["result"].as_array().unwrap().len(), 1);
        // codeAction offers quickfix + source.fixAll for `host` -> `HOST`.
        assert!(!by_id(11)["result"].as_array().unwrap().is_empty());
        // Ignored URI returns no code actions.
        assert!(by_id(12)["result"].as_array().unwrap().is_empty());
        assert_eq!(by_id(13)["error"]["code"], error_code::REQUEST_FAILED);
        assert_eq!(by_id(14)["error"]["code"], error_code::METHOD_NOT_FOUND);
        assert_eq!(by_id(15)["result"], Value::Null);
        assert_eq!(by_id(16)["error"]["code"], error_code::INVALID_REQUEST);
    }

    #[test]
    fn discover_lint_config_uses_workspace_folders_and_root_uri() {
        let dir = TempDir::new("ws-discover");
        std::fs::write(
            dir.0.join(".adotenv-lint.toml"),
            "dialect = \"python\"\nkey_case = \"upper\"\n",
        )
        .unwrap();
        let sub = dir.0.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let raw = sub.to_string_lossy().replace('\\', "/").replace(' ', "%20");
        let folder_uri = if raw.starts_with('/') {
            format!("file://{raw}")
        } else {
            format!("file:///{raw}")
        };
        let mut cli = cli_with_config(None);
        cli.discover_lint_config = true;

        let mut input = Vec::new();
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "capabilities": {},
                "workspaceFolders": [{ "uri": folder_uri, "name": "sub" }]
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "initialized", "params": {}
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///tmp/.env", "languageId": "dotenv", "version": 1,
                    "text": "'key one'=1\n"
                }
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "exit", "params": null
        })));

        let mut transport =
            Transport::new(Cursor::new(input), Vec::new(), DEFAULT_MAX_MESSAGE_BYTES);
        serve(&mut transport, &cli, None).unwrap();
        let messages = decode_frames(&transport.into_writer());
        let diag = messages
            .iter()
            .find(|msg| {
                msg.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            })
            .unwrap();
        // Python dialect parsed quoted key `'key one'` cleanly and key_case = "upper" flagged it!
        let diags = diag["params"]["diagnostics"].as_array().unwrap();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0]["code"], "adotenv-lint.key-case");
    }

    #[test]
    fn serve_returns_error_on_exit_without_shutdown() {
        let cli = cli_with_config(None);
        let mut input = Vec::new();
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "capabilities": {} }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "initialized", "params": {}
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "exit", "params": null
        })));
        let mut transport =
            Transport::new(Cursor::new(input), Vec::new(), DEFAULT_MAX_MESSAGE_BYTES);
        assert!(serve(&mut transport, &cli, None).is_err());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn transport_handles_retries_framing_boundaries_and_jsonrpc_validation() {
        use std::io::{self, ErrorKind, Read};

        struct InterruptOnceReader {
            data: Vec<u8>,
            pos: usize,
            interrupted: bool,
        }
        impl Read for InterruptOnceReader {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if !self.interrupted {
                    self.interrupted = true;
                    return Err(io::Error::new(ErrorKind::Interrupted, "signal"));
                }
                let remaining = &self.data[self.pos..];
                let count = remaining.len().min(buf.len());
                buf[..count].copy_from_slice(&remaining[..count]);
                self.pos += count;
                Ok(count)
            }
        }
        impl BufRead for InterruptOnceReader {
            fn fill_buf(&mut self) -> io::Result<&[u8]> {
                if !self.interrupted {
                    self.interrupted = true;
                    return Err(io::Error::new(ErrorKind::Interrupted, "signal"));
                }
                Ok(&self.data[self.pos..])
            }
            fn consume(&mut self, amt: usize) {
                self.pos += amt;
            }
        }

        struct InterruptOnceWriter {
            out: Vec<u8>,
            interrupted_write: bool,
            interrupted_flush: bool,
        }
        impl Write for InterruptOnceWriter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                if !self.interrupted_write {
                    self.interrupted_write = true;
                    return Err(io::Error::new(ErrorKind::Interrupted, "signal"));
                }
                let take = buf.len().min(4);
                self.out.extend_from_slice(&buf[..take]);
                Ok(take)
            }
            fn flush(&mut self) -> io::Result<()> {
                if !self.interrupted_flush {
                    self.interrupted_flush = true;
                    return Err(io::Error::new(ErrorKind::Interrupted, "signal"));
                }
                Ok(())
            }
        }

        // Interrupted reads & writes succeed transparently.
        let req = frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": "req-1", "method": "shutdown"
        }));
        let mut t = Transport::new(
            InterruptOnceReader {
                data: req,
                pos: 0,
                interrupted: false,
            },
            InterruptOnceWriter {
                out: Vec::new(),
                interrupted_write: false,
                interrupted_flush: false,
            },
            1024,
        );
        let msg = t.read_message().unwrap().unwrap();
        assert_eq!(
            msg,
            IncomingMessage::Request {
                id: RequestId::Str("req-1".to_owned()),
                method: "shutdown".to_owned(),
                params: Value::Null,
            }
        );
        // Clean EOF after complete frame returns Ok(None).
        assert_eq!(t.read_message().unwrap(), None);
        t.send_ok(RequestId::Str("req-1".to_owned()), &Value::Null)
            .unwrap();
        let written = decode_frames(&t.into_writer().out);
        assert_eq!(written[0]["id"], "req-1");
        assert_eq!(written[0]["result"], Value::Null);

        // Exact MAX_HEADER_LINE_BYTES header succeeds; MAX_HEADER_LINE_BYTES + 1 fails.
        let prefix = b"X-Pad: ";
        let mut exact = Vec::new();
        exact.extend_from_slice(prefix);
        exact.resize(MAX_HEADER_LINE_BYTES - 2, b'a');
        exact.extend_from_slice(b"\r\nContent-Length: 42\r\n\r\n");
        let body = br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let mut exact_frame = Vec::new();
        exact_frame.extend_from_slice(&exact[..MAX_HEADER_LINE_BYTES]);
        exact_frame.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        exact_frame.extend_from_slice(body);
        let mut t = Transport::new(Cursor::new(exact_frame), Vec::new(), 1024);
        assert!(matches!(
            t.read_message(),
            Ok(Some(IncomingMessage::Request { .. }))
        ));

        let mut over = Vec::new();
        over.extend_from_slice(prefix);
        over.resize(MAX_HEADER_LINE_BYTES - 1, b'a');
        over.extend_from_slice(b"\r\nContent-Length: 2\r\n\r\n{}");
        let mut t = Transport::new(Cursor::new(over), Vec::new(), 1024);
        assert!(matches!(
            t.read_message(),
            Err(TransportError::HeaderLineTooLong {
                limit: MAX_HEADER_LINE_BYTES
            })
        ));

        // Malformed header variants.
        for bad in [
            b"Content-Length: 2\rbad\r\n\r\n{}".as_slice(),
            b"X-Bin: \xff\r\nContent-Length: 2\r\n\r\n{}".as_slice(),
            b"MalformedHeaderWithoutColon\r\n\r\n".as_slice(),
            b"Content-Length: +2\r\n\r\n{}".as_slice(),
            b"Content-Length: \r\n\r\n{}".as_slice(),
            b"Content-Length: abc\r\n\r\n{}".as_slice(),
        ] {
            let mut t = Transport::new(Cursor::new(bad.to_vec()), Vec::new(), 1024);
            assert!(
                matches!(t.read_message(), Err(TransportError::InvalidHeader(_))),
                "{bad:?}"
            );
        }

        // Mid-header EOF variants.
        for eof in [
            b"Content-Length: 2\r\n".as_slice(),
            b"Content-Length: 2".as_slice(),
        ] {
            let mut t = Transport::new(Cursor::new(eof.to_vec()), Vec::new(), 1024);
            assert!(
                matches!(t.read_message(), Err(TransportError::UnexpectedEof)),
                "{eof:?}"
            );
        }

        // JSON-RPC validation edge cases.
        let read_one = |raw: &[u8]| -> IncomingMessage {
            let mut buf = format!("Content-Length: {}\r\n\r\n", raw.len()).into_bytes();
            buf.extend_from_slice(raw);
            let mut t = Transport::new(Cursor::new(buf), Vec::new(), 4096);
            t.read_message().unwrap().unwrap()
        };
        assert!(matches!(
            read_one(br#"[{"jsonrpc":"2.0","id":1,"method":"x"}]"#),
            IncomingMessage::Invalid {
                code: error_code::INVALID_REQUEST,
                ..
            }
        ));
        assert!(matches!(
            read_one(br#"{"jsonrpc":"2.0","id":null,"method":"x"}"#),
            IncomingMessage::Invalid {
                code: error_code::INVALID_REQUEST,
                ..
            }
        ));
        assert_eq!(
            read_one(br#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"x"}}"#),
            IncomingMessage::Response
        );
        assert_eq!(
            read_one(br#"{"jsonrpc":"2.0","id":7,"result":null}"#),
            IncomingMessage::Response
        );
        assert!(matches!(
            read_one(br#"{"jsonrpc":"1.0","id":7,"result":null}"#),
            IncomingMessage::Invalid {
                id: Some(RequestId::Int(7)),
                code: error_code::INVALID_REQUEST,
                ..
            }
        ));
        assert!(matches!(
            read_one(br#"{"jsonrpc":"2.0","id":1.5,"method":"x"}"#),
            IncomingMessage::Invalid {
                id: None,
                code: error_code::INVALID_REQUEST,
                ..
            }
        ));
        assert!(matches!(
            read_one(br#"{"jsonrpc":"2.0","id":1,"method":"x","result":null}"#),
            IncomingMessage::Invalid {
                id: Some(RequestId::Int(1)),
                code: error_code::INVALID_REQUEST,
                ..
            }
        ));
        assert!(matches!(
            read_one(br#"{"jsonrpc":"2.0","id":1,"method":123}"#),
            IncomingMessage::Invalid {
                id: Some(RequestId::Int(1)),
                code: error_code::INVALID_REQUEST,
                ..
            }
        ));
        assert!(matches!(
            read_one(br#"{"jsonrpc":"2.0","id":1}"#),
            IncomingMessage::Invalid {
                id: Some(RequestId::Int(1)),
                code: error_code::INVALID_REQUEST,
                ..
            }
        ));
        let deeply_nested = format!("{}{}", "[".repeat(256), "]".repeat(256));
        assert!(matches!(
            read_one(deeply_nested.as_bytes()),
            IncomingMessage::Invalid {
                code: error_code::PARSE_ERROR,
                ..
            }
        ));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn serve_covers_root_uri_duplicate_init_invalid_config_errors_and_broken_pipe() {
        struct FailAfterOneFrame {
            writes_allowed: usize,
            failed_attempts: usize,
        }
        impl Write for FailAfterOneFrame {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                if self.writes_allowed > 0 {
                    self.writes_allowed -= 1;
                    Ok(buf.len())
                } else {
                    self.failed_attempts += 1;
                    Err(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "broken pipe",
                    ))
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        // rootUri fallback + --dialect override over discovered config.
        let dir = TempDir::new("root-uri");
        std::fs::write(
            dir.0.join(".adotenv-lint.toml"),
            "dialect = \"python\"\nkey_case = \"upper\"\n",
        )
        .unwrap();
        let raw = dir
            .0
            .to_string_lossy()
            .replace('\\', "/")
            .replace(' ', "%20");
        let root_uri = if raw.starts_with('/') {
            format!("file://{raw}")
        } else {
            format!("file:///{raw}")
        };
        let mut cli = cli_with_config(None);
        cli.discover_lint_config = true;
        cli.dialect = Some(Dialect::PORTABLE);
        cli.max_lines = Some(1);

        let mut input = Vec::new();
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "capabilities": {},
                "workspaceFolders": [{ "uri": "file://remote/share", "name": "remote" }],
                "rootUri": root_uri
            }
        })));
        // Duplicate initialize after startup returns INVALID_REQUEST (-32600).
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "initialize",
            "params": { "capabilities": {} }
        })));
        // Unopened document request returns INVALID_PARAMS (-32602).
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": "file:///tmp/unopened.env" },
                "position": { "line": 0, "character": 0 }
            }
        })));
        // Open document exceeding max_lines=1 and request formatting -> REQUEST_FAILED (-32803).
        let doc_uri = "file:///tmp/limited.env";
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": doc_uri, "languageId": "dotenv", "version": 1,
                    "text": "a=1\nb=2\n"
                }
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 4, "method": "textDocument/formatting",
            "params": {
                "textDocument": { "uri": doc_uri },
                "options": { "tabSize": 2, "insertSpaces": true }
            }
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 5, "method": "shutdown", "params": null
        })));
        input.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "exit", "params": null
        })));
        let mut transport =
            Transport::new(Cursor::new(input), Vec::new(), DEFAULT_MAX_MESSAGE_BYTES);
        serve(&mut transport, &cli, None).unwrap();
        let messages = decode_frames(&transport.into_writer());
        let by_id = |target: i64| -> &Value {
            messages
                .iter()
                .find(|msg| msg.get("id").and_then(Value::as_i64) == Some(target))
                .expect("response id")
        };
        assert_eq!(by_id(2)["error"]["code"], error_code::INVALID_REQUEST);
        assert_eq!(by_id(3)["error"]["code"], error_code::INVALID_PARAMS);
        assert_eq!(by_id(4)["error"]["code"], error_code::REQUEST_FAILED);

        // Invalid discovered config returns INTERNAL_ERROR (-32603) and Err.
        let bad_dir = TempDir::new("bad-cfg");
        std::fs::write(bad_dir.0.join(".adotenv-lint.toml"), "unknown = true\n").unwrap();
        let bad_raw = bad_dir
            .0
            .to_string_lossy()
            .replace('\\', "/")
            .replace(' ', "%20");
        let bad_uri = if bad_raw.starts_with('/') {
            format!("file://{bad_raw}")
        } else {
            format!("file:///{bad_raw}")
        };
        let mut bad_cli = cli_with_config(None);
        bad_cli.discover_lint_config = true;
        let bad_input = frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "capabilities": {}, "rootUri": bad_uri }
        }));
        let mut transport = Transport::new(
            Cursor::new(bad_input),
            Vec::new(),
            DEFAULT_MAX_MESSAGE_BYTES,
        );
        assert!(serve(&mut transport, &bad_cli, None).is_err());
        let responses = decode_frames(&transport.into_writer());
        assert_eq!(responses[0]["error"]["code"], error_code::INTERNAL_ERROR);

        // Pre-initialize exit and clean EOF both fail cleanly.
        let exit_early = frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "exit", "params": null
        }));
        let mut t = Transport::new(
            Cursor::new(exit_early),
            Vec::new(),
            DEFAULT_MAX_MESSAGE_BYTES,
        );
        assert!(serve(&mut t, &cli_with_config(None), None).is_err());

        let mut t = Transport::new(
            Cursor::new(Vec::<u8>::new()),
            Vec::new(),
            DEFAULT_MAX_MESSAGE_BYTES,
        );
        assert!(serve(&mut t, &cli_with_config(None), None).is_err());

        // Without lint policy, codeActionProvider is omitted from capabilities.
        let mut no_lint = Vec::new();
        no_lint.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "capabilities": {} }
        })));
        no_lint.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null
        })));
        no_lint.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "method": "exit", "params": null
        })));
        let mut t = Transport::new(Cursor::new(no_lint), Vec::new(), DEFAULT_MAX_MESSAGE_BYTES);
        serve(&mut t, &cli_with_config(None), None).unwrap();
        let out = decode_frames(&t.into_writer());
        assert!(out[0]["result"]["capabilities"]["codeActionProvider"].is_null());

        // Transport write failure inside handle_request returns Err without attempting send_error.
        let mut pipe_in = Vec::new();
        pipe_in.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "capabilities": {} }
        })));
        pipe_in.extend(frame(&serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "workspace/symbol", "params": { "query": "" }
        })));
        // initialize writes 2 buffers (header + body); allow those 2, fail on workspace/symbol.
        let mut t = Transport::new(
            Cursor::new(pipe_in),
            FailAfterOneFrame {
                writes_allowed: 2,
                failed_attempts: 0,
            },
            DEFAULT_MAX_MESSAGE_BYTES,
        );
        assert!(serve(&mut t, &cli_with_config(None), None).is_err());
        // Must have failed exactly once (no second write attempt via send_error).
        assert_eq!(t.into_writer().failed_attempts, 1);
    }
}
