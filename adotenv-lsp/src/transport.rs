//! Bounded synchronous JSON-RPC 2.0 `Content-Length` stdio transport.
//!
//! Enforces explicit limits on header line length, header count and payload
//! size before allocating message buffers so oversized or malformed frames
//! cannot exhaust memory ahead of document parse limits.

use std::{
    fmt,
    io::{BufRead, ErrorKind, Write},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Default maximum JSON-RPC payload size accepted per LSP frame (16 MiB).
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
/// Maximum byte length of a single LSP header line (including line ending).
pub const MAX_HEADER_LINE_BYTES: usize = 8 * 1024;
/// Maximum number of header lines accepted per LSP frame.
pub const MAX_HEADERS: usize = 32;

/// JSON-RPC 2.0 / LSP 3.17 standard error codes.
pub mod error_code {
    /// Invalid JSON was received by the server.
    pub const PARSE_ERROR: i32 = -32700;
    /// The JSON sent is not a valid JSON-RPC 2.0 Request object.
    pub const INVALID_REQUEST: i32 = -32600;
    /// The method does not exist or is not supported.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid method parameters.
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal server error.
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Request received before the server finished `initialize`.
    pub const SERVER_NOT_INITIALIZED: i32 = -32002;
    /// LSP request failed (for example rejected rename or format).
    pub const REQUEST_FAILED: i32 = -32803;
}

/// JSON-RPC 2.0 request identifier (integer or string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// Integer identifier.
    Int(i64),
    /// String identifier.
    Str(String),
}

/// A decoded JSON-RPC 2.0 message read from a bounded frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncomingMessage {
    /// A JSON-RPC request expecting a response.
    Request {
        /// Request identifier.
        id: RequestId,
        /// Method name.
        method: String,
        /// Parameters (`Value::Null` when omitted).
        params: Value,
    },
    /// A JSON-RPC notification (no response expected).
    Notification {
        /// Method name.
        method: String,
        /// Parameters (`Value::Null` when omitted).
        params: Value,
    },
    /// A JSON-RPC response (ignored by the server).
    Response,
    /// A syntactically or structurally invalid JSON-RPC payload.
    Invalid {
        /// Request identifier, if one could be extracted.
        id: Option<RequestId>,
        /// JSON-RPC error code.
        code: i32,
        /// Human-readable error message.
        message: String,
    },
}

/// Framing or I/O error from the bounded LSP transport.
#[derive(Debug)]
pub enum TransportError {
    /// Underlying I/O error.
    Io(std::io::Error),
    /// Stream ended mid-header or mid-payload.
    UnexpectedEof,
    /// A header line exceeded [`MAX_HEADER_LINE_BYTES`].
    HeaderLineTooLong {
        /// Configured byte limit.
        limit: usize,
    },
    /// More than [`MAX_HEADERS`] header lines appeared in one frame.
    TooManyHeaders {
        /// Configured header count limit.
        limit: usize,
    },
    /// A header line was malformed or not valid UTF-8.
    InvalidHeader(String),
    /// Frame ended headers without a `Content-Length` header.
    MissingContentLength,
    /// Frame contained more than one `Content-Length` header.
    DuplicateContentLength,
    /// Declared `Content-Length` exceeded the configured payload limit.
    MessageTooLarge {
        /// Declared payload byte length.
        length: u64,
        /// Configured maximum payload bytes.
        limit: usize,
    },
    /// Serialization error while encoding an outgoing message.
    Serialize(serde_json::Error),
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "transport I/O error: {error}"),
            Self::UnexpectedEof => formatter.write_str("unexpected EOF in LSP message"),
            Self::HeaderLineTooLong { limit } => {
                write!(formatter, "LSP header line exceeded {limit} bytes")
            }
            Self::TooManyHeaders { limit } => {
                write!(formatter, "LSP frame exceeded {limit} headers")
            }
            Self::InvalidHeader(detail) => write!(formatter, "invalid LSP header: {detail}"),
            Self::MissingContentLength => formatter.write_str("missing Content-Length header"),
            Self::DuplicateContentLength => formatter.write_str("duplicate Content-Length header"),
            Self::MessageTooLarge { length, limit } => write!(
                formatter,
                "LSP message length {length} exceeded limit of {limit} bytes"
            ),
            Self::Serialize(error) => write!(formatter, "failed to encode LSP message: {error}"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Serialize(error) => Some(error),
            _ => None,
        }
    }
}

/// Synchronous bounded JSON-RPC 2.0 `Content-Length` transport.
pub struct Transport<R, W> {
    reader: R,
    writer: W,
    max_message_bytes: usize,
    header_buf: Vec<u8>,
}

impl<R: BufRead, W: Write> Transport<R, W> {
    /// Creates a bounded transport over `reader` and `writer`.
    pub fn new(reader: R, writer: W, max_message_bytes: usize) -> Self {
        Self {
            reader,
            writer,
            max_message_bytes,
            header_buf: Vec::new(),
        }
    }

    /// Consumes the transport and returns the underlying writer.
    #[cfg(test)]
    pub fn into_writer(self) -> W {
        self.writer
    }

    /// Reads the next framed JSON-RPC message, returning `Ok(None)` on clean EOF
    /// between frames.
    pub fn read_message(&mut self) -> Result<Option<IncomingMessage>, TransportError> {
        let mut content_length: Option<u64> = None;
        let mut header_count = 0usize;
        loop {
            let saw_bytes = read_bounded_line(
                &mut self.reader,
                &mut self.header_buf,
                MAX_HEADER_LINE_BYTES,
            )?;
            if !saw_bytes {
                if header_count == 0 {
                    return Ok(None);
                }
                return Err(TransportError::UnexpectedEof);
            }
            if !self.header_buf.ends_with(b"\n") {
                return Err(TransportError::UnexpectedEof);
            }
            let line = std::str::from_utf8(&self.header_buf).map_err(|_| {
                TransportError::InvalidHeader("header is not valid UTF-8".to_owned())
            })?;
            let trimmed = line
                .strip_suffix("\r\n")
                .or_else(|| line.strip_suffix('\n'))
                .unwrap_or(line);
            if trimmed.contains('\r') {
                return Err(TransportError::InvalidHeader(
                    "unexpected carriage return in header".to_owned(),
                ));
            }
            if trimmed.is_empty() {
                break;
            }
            header_count += 1;
            if header_count > MAX_HEADERS {
                return Err(TransportError::TooManyHeaders { limit: MAX_HEADERS });
            }
            let Some((name, value)) = trimmed.split_once(':') else {
                return Err(TransportError::InvalidHeader(
                    "missing ':' separator".to_owned(),
                ));
            };
            if name.trim().eq_ignore_ascii_case("content-length") {
                if content_length.is_some() {
                    return Err(TransportError::DuplicateContentLength);
                }
                let raw = value.trim();
                if raw.starts_with('+') || raw.is_empty() {
                    return Err(TransportError::InvalidHeader(format!(
                        "invalid Content-Length: {raw}"
                    )));
                }
                let parsed = raw.parse::<u64>().map_err(|_| {
                    TransportError::InvalidHeader(format!("invalid Content-Length: {raw}"))
                })?;
                content_length = Some(parsed);
            }
        }
        let Some(length) = content_length else {
            return Err(TransportError::MissingContentLength);
        };
        let max_bytes = u64::try_from(self.max_message_bytes).unwrap_or(u64::MAX);
        if length > max_bytes {
            return Err(TransportError::MessageTooLarge {
                length,
                limit: self.max_message_bytes,
            });
        }
        let Ok(length_usize) = usize::try_from(length) else {
            return Err(TransportError::MessageTooLarge {
                length,
                limit: self.max_message_bytes,
            });
        };
        let mut body = vec![0u8; length_usize];
        if let Err(error) = self.reader.read_exact(&mut body) {
            if error.kind() == ErrorKind::UnexpectedEof {
                return Err(TransportError::UnexpectedEof);
            }
            return Err(TransportError::Io(error));
        }
        Ok(Some(decode_message(&body)))
    }

    /// Writes a JSON-RPC success response and flushes immediately.
    pub fn send_ok<T: Serialize>(
        &mut self,
        id: RequestId,
        result: &T,
    ) -> Result<(), TransportError> {
        #[derive(Serialize)]
        struct OkResponse<'a, T: Serialize> {
            jsonrpc: &'static str,
            id: RequestId,
            result: &'a T,
        }
        self.write_json(&OkResponse {
            jsonrpc: "2.0",
            id,
            result,
        })
    }

    /// Writes a JSON-RPC error response and flushes immediately.
    pub fn send_error(
        &mut self,
        id: Option<RequestId>,
        code: i32,
        message: &str,
    ) -> Result<(), TransportError> {
        #[derive(Serialize)]
        struct ErrorObject<'a> {
            code: i32,
            message: &'a str,
        }
        #[derive(Serialize)]
        struct ErrResponse<'a> {
            jsonrpc: &'static str,
            id: Option<RequestId>,
            error: ErrorObject<'a>,
        }
        self.write_json(&ErrResponse {
            jsonrpc: "2.0",
            id,
            error: ErrorObject { code, message },
        })
    }

    /// Writes a JSON-RPC notification and flushes immediately.
    pub fn send_notification<T: Serialize>(
        &mut self,
        method: &str,
        params: &T,
    ) -> Result<(), TransportError> {
        #[derive(Serialize)]
        struct OutNotification<'a, T: Serialize> {
            jsonrpc: &'static str,
            method: &'a str,
            params: &'a T,
        }
        self.write_json(&OutNotification {
            jsonrpc: "2.0",
            method,
            params,
        })
    }

    fn write_json<T: Serialize>(&mut self, payload: &T) -> Result<(), TransportError> {
        let bytes = serde_json::to_vec(payload).map_err(TransportError::Serialize)?;
        let header = format!("Content-Length: {}\r\n\r\n", bytes.len());
        write_all_retry(&mut self.writer, header.as_bytes()).map_err(TransportError::Io)?;
        write_all_retry(&mut self.writer, &bytes).map_err(TransportError::Io)?;
        flush_retry(&mut self.writer).map_err(TransportError::Io)?;
        Ok(())
    }
}

fn write_all_retry(writer: &mut impl Write, mut buf: &[u8]) -> std::io::Result<()> {
    while !buf.is_empty() {
        match writer.write(buf) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
            Ok(written) => buf = &buf[written..],
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn flush_retry(writer: &mut impl Write) -> std::io::Result<()> {
    loop {
        match writer.flush() {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

fn read_bounded_line(
    reader: &mut impl BufRead,
    out: &mut Vec<u8>,
    limit: usize,
) -> Result<bool, TransportError> {
    out.clear();
    loop {
        let chunk = match reader.fill_buf() {
            Ok(chunk) => chunk,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(TransportError::Io(error)),
        };
        if chunk.is_empty() {
            return Ok(!out.is_empty());
        }
        let remaining = limit.saturating_sub(out.len());
        let search_len = chunk.len().min(remaining);
        if let Some(newline) = chunk[..search_len].iter().position(|&byte| byte == b'\n') {
            let take = newline + 1;
            out.extend_from_slice(&chunk[..take]);
            reader.consume(take);
            return Ok(true);
        }
        if chunk.len() > remaining {
            return Err(TransportError::HeaderLineTooLong { limit });
        }
        let len = chunk.len();
        out.extend_from_slice(chunk);
        reader.consume(len);
    }
}

fn decode_message(body: &[u8]) -> IncomingMessage {
    let value: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(error) => {
            return IncomingMessage::Invalid {
                id: None,
                code: error_code::PARSE_ERROR,
                message: format!("malformed JSON payload: {error}"),
            };
        }
    };
    let Value::Object(mut object) = value else {
        return IncomingMessage::Invalid {
            id: None,
            code: error_code::INVALID_REQUEST,
            message: "JSON-RPC message must be an object".to_owned(),
        };
    };
    let has_method = object.contains_key("method");
    let has_result_or_error = object.contains_key("result") || object.contains_key("error");
    let id_value = object.remove("id");
    let id = match id_value {
        None => None,
        Some(Value::Null) if !has_method && has_result_or_error => None,
        Some(Value::Number(num)) => match num.as_i64() {
            Some(int) => Some(RequestId::Int(int)),
            None => {
                return IncomingMessage::Invalid {
                    id: None,
                    code: error_code::INVALID_REQUEST,
                    message: "request id must be an integer or string".to_owned(),
                };
            }
        },
        Some(Value::String(text)) => Some(RequestId::Str(text)),
        Some(_) => {
            return IncomingMessage::Invalid {
                id: None,
                code: error_code::INVALID_REQUEST,
                message: "request id must be an integer or string".to_owned(),
            };
        }
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return IncomingMessage::Invalid {
            id,
            code: error_code::INVALID_REQUEST,
            message: "jsonrpc version must be \"2.0\"".to_owned(),
        };
    }
    if has_method && has_result_or_error {
        return IncomingMessage::Invalid {
            id,
            code: error_code::INVALID_REQUEST,
            message: "JSON-RPC message cannot contain both method and result/error".to_owned(),
        };
    }
    if has_result_or_error {
        return IncomingMessage::Response;
    }
    let Some(method_value) = object.remove("method") else {
        return IncomingMessage::Invalid {
            id,
            code: error_code::INVALID_REQUEST,
            message: "JSON-RPC message must contain method, result, or error".to_owned(),
        };
    };
    let Some(method) = method_value.as_str().map(str::to_owned) else {
        return IncomingMessage::Invalid {
            id,
            code: error_code::INVALID_REQUEST,
            message: "method must be a string".to_owned(),
        };
    };
    let params = object.remove("params").unwrap_or(Value::Null);
    match id {
        Some(id) => IncomingMessage::Request { id, method, params },
        None => IncomingMessage::Notification { method, params },
    }
}
