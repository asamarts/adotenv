//! Binary-level configuration and stdio transport checks.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

struct TempFile(std::path::PathBuf);

impl TempFile {
    fn new(name: &str, contents: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "adotenv-lsp-cli-{name}-{}-{}.toml",
            std::process::id(),
            std::thread::current().name().unwrap_or("main")
        ));
        fs::write(&path, contents).unwrap();
        Self(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "adotenv-lsp-cli-{name}-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("main")
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn invalid_explicit_config_fails_before_the_server_starts() {
    let file = TempFile::new("invalid", "unknown = true\n");
    let output = Command::new(env!("CARGO_BIN_EXE_adotenv-lsp"))
        .arg("--config")
        .arg(&file.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown field"), "{stderr}");
}

#[test]
fn conflicting_config_modes_fail_before_the_server_starts() {
    let output = Command::new(env!("CARGO_BIN_EXE_adotenv-lsp"))
        .args(["--config", "policy.toml", "--discover-lint-config"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot be combined"), "{stderr}");
}

#[test]
fn help_and_version_flags_exit_successfully() {
    let help = Command::new(env!("CARGO_BIN_EXE_adotenv-lsp"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    let stdout = String::from_utf8_lossy(&help.stdout);
    assert!(stdout.contains("--dialect"));
    assert!(stdout.contains("--max-input-bytes"));
    assert!(stdout.contains("--max-lines"));
    assert!(stdout.contains("--max-items"));
    assert!(stdout.contains("--max-diagnostics"));
    assert!(stdout.contains("--max-message-bytes"));

    let version = Command::new(env!("CARGO_BIN_EXE_adotenv-lsp"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("adotenv-lsp {}", env!("CARGO_PKG_VERSION"))
    );
}

fn write_lsp_message(writer: &mut impl Write, payload: &serde_json::Value) {
    let body = serde_json::to_vec(payload).unwrap();
    write!(writer, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
    writer.write_all(&body).unwrap();
    writer.flush().unwrap();
}

fn read_lsp_message(reader: &mut impl BufRead) -> serde_json::Value {
    let mut length = None;
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            length = Some(value.trim().parse::<usize>().unwrap());
        }
    }
    let mut body = vec![0u8; length.expect("Content-Length header")];
    reader.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[test]
fn stdio_server_handles_initialize_diagnostics_hover_and_shutdown() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_adotenv-lsp"))
        .args(["--mask-values", "--max-lines", "2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "capabilities": {} }
        }),
    );
    let init = read_lsp_message(&mut stdout);
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["serverInfo"]["name"], "adotenv-lsp");
    assert_eq!(
        init["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        init["result"]["capabilities"]["hoverProvider"].as_bool(),
        Some(true)
    );
    assert!(init["result"]["capabilities"]["codeActionProvider"].is_null());

    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }),
    );
    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///tmp/.env",
                    "languageId": "dotenv",
                    "version": 1,
                    "text": "A=1\nB=2\nC=3\n"
                }
            }
        }),
    );
    let diag = read_lsp_message(&mut stdout);
    assert_eq!(diag["method"], "textDocument/publishDiagnostics");
    assert_eq!(diag["params"]["version"], 1);
    assert_eq!(
        diag["params"]["diagnostics"][0]["code"],
        "dotenv.limit-exceeded"
    );

    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "shutdown",
            "params": null
        }),
    );
    let shut = read_lsp_message(&mut stdout);
    assert_eq!(shut["id"], 2);
    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": null
        }),
    );
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success());
}

#[test]
fn stdio_server_exits_nonzero_on_exit_without_shutdown() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_adotenv-lsp"))
        .args(["--dialect", "python"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "capabilities": {} }
        }),
    );
    let _ = read_lsp_message(&mut stdout);
    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }),
    );
    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": null
        }),
    );
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(!status.success());
}

#[test]
fn stdio_server_replies_invalid_params_and_exits_without_hanging_on_bad_initialize() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_adotenv-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Keep stdin open to verify the server does not deadlock in io_threads.join().
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": "not-an-object"
        }),
    );
    let response = read_lsp_message(&mut stdout);
    assert_eq!(response["id"], 1);
    assert_eq!(response["error"]["code"], -32602);

    let status = child.wait().unwrap();
    assert!(!status.success());
    drop(stdin);
}

#[test]
fn stdio_server_rejects_oversized_content_length_before_allocating() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_adotenv-lsp"))
        .args(["--max-message-bytes", "64"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    write!(stdin, "Content-Length: 999999999999\r\n\r\n").unwrap();
    stdin.flush().unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("exceeded limit of 64 bytes"), "{stderr}");
}

#[test]
#[allow(clippy::too_many_lines)]
fn stdio_server_discovers_config_from_workspace_folder_and_serves_code_actions_and_formatting() {
    let dir = TempDir::new("ws");
    let sub = dir.0.join("nested");
    fs::create_dir_all(&sub).unwrap();
    fs::write(
        dir.0.join(".adotenv-lint.toml"),
        "key_case = \"upper\"\ntrim_trailing_whitespace = true\n",
    )
    .unwrap();
    let raw = sub.to_string_lossy().replace('\\', "/").replace(' ', "%20");
    let folder_uri = if raw.starts_with('/') {
        format!("file://{raw}")
    } else {
        format!("file:///{raw}")
    };

    let mut child = Command::new(env!("CARGO_BIN_EXE_adotenv-lsp"))
        .arg("--discover-lint-config")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "capabilities": {
                    "general": { "positionEncodings": ["utf-8", "utf-16"] }
                },
                "workspaceFolders": [{ "uri": folder_uri, "name": "nested" }]
            }
        }),
    );
    let init = read_lsp_message(&mut stdout);
    assert_eq!(init["result"]["capabilities"]["positionEncoding"], "utf-8");
    assert_eq!(
        init["result"]["capabilities"]["renameProvider"]["prepareProvider"],
        true
    );
    assert_eq!(
        init["result"]["capabilities"]["documentFormattingProvider"],
        true
    );
    assert_eq!(
        init["result"]["capabilities"]["codeActionProvider"]["codeActionKinds"],
        serde_json::json!(["quickfix", "source.fixAll"])
    );

    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }),
    );
    let doc_uri = "file:///tmp/.env";
    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": doc_uri,
                    "languageId": "dotenv",
                    "version": 1,
                    "text": "host=localhost  \n"
                }
            }
        }),
    );
    let diag = read_lsp_message(&mut stdout);
    assert_eq!(diag["params"]["diagnostics"].as_array().unwrap().len(), 2);

    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/prepareRename",
            "params": {
                "textDocument": { "uri": doc_uri },
                "position": { "line": 0, "character": 1 }
            }
        }),
    );
    let prep = read_lsp_message(&mut stdout);
    assert_eq!(prep["result"]["placeholder"], "host");

    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/formatting",
            "params": {
                "textDocument": { "uri": doc_uri },
                "options": { "tabSize": 2, "insertSpaces": true }
            }
        }),
    );
    let fmt_resp = read_lsp_message(&mut stdout);
    assert_eq!(fmt_resp["result"].as_array().unwrap().len(), 1);

    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/codeAction",
            "params": {
                "textDocument": { "uri": doc_uri },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 4 }
                },
                "context": { "diagnostics": [] }
            }
        }),
    );
    let actions = read_lsp_message(&mut stdout);
    assert!(actions["result"].as_array().unwrap().len() >= 2);

    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0", "id": 5, "method": "shutdown", "params": null
        }),
    );
    let _ = read_lsp_message(&mut stdout);
    write_lsp_message(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0", "method": "exit", "params": null
        }),
    );
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success());
}
