//! Stdio language server for adotenv documents.

use std::{collections::HashMap, error::Error};

use adotenv_lsp::{completion, definition, diagnostics, hover, references, rename, LspOptions};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CompletionOptions, CompletionParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, GotoDefinitionParams, HoverParams, HoverProviderCapability,
    InitializeParams, OneOf, PublishDiagnosticsParams, ReferenceParams, RenameParams,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn main() -> Result<()> {
    let options = LspOptions::default()
        .with_mask_values(std::env::args().any(|argument| argument == "--mask-values"));
    let (connection, io_threads) = Connection::stdio();
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!["$".to_owned(), "{".to_owned()]),
            ..CompletionOptions::default()
        }),
        definition_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
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
                if let Err(error) = handle_notification(&connection, &notification, &mut documents)
                {
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

fn handle_notification(
    connection: &Connection,
    notification: &Notification,
    documents: &mut HashMap<String, String>,
) -> Result<()> {
    match notification.method.as_str() {
        "textDocument/didOpen" => {
            let params: DidOpenTextDocumentParams =
                serde_json::from_value(notification.params.clone())?;
            let uri = params.text_document.uri;
            documents.insert(uri.as_str().to_owned(), params.text_document.text);
            publish_diagnostics(connection, &uri, documents)?;
        }
        "textDocument/didChange" => {
            let params: DidChangeTextDocumentParams =
                serde_json::from_value(notification.params.clone())?;
            let uri = params.text_document.uri;
            if let Some(change) = params.content_changes.into_iter().last() {
                documents.insert(uri.as_str().to_owned(), change.text);
                publish_diagnostics(connection, &uri, documents)?;
            }
        }
        "textDocument/didClose" => {
            let params: DidCloseTextDocumentParams =
                serde_json::from_value(notification.params.clone())?;
            documents.remove(params.text_document.uri.as_str());
        }
        _ => {}
    }
    Ok(())
}

fn publish_diagnostics(
    connection: &Connection,
    uri: &Uri,
    documents: &HashMap<String, String>,
) -> Result<()> {
    let text = documents
        .get(uri.as_str())
        .ok_or("diagnostics requested for an unopened document")?;
    let params = PublishDiagnosticsParams::new(uri.clone(), diagnostics(text, uri), None);
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
