//! End-to-end checks for the isolated memory benchmark binary.

use std::process::Command;

fn run(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_memory"))
        .args(arguments)
        .output()
        .expect("run memory benchmark")
}

#[test]
fn values_and_document_emit_json() {
    for mode in ["values", "document"] {
        let output = run(&[
            "--mode",
            mode,
            "--implementation",
            "adotenv",
            "--size",
            "8",
            "--shape",
            "plain",
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
        assert!(stdout.contains("\"mode\":\""));
        assert!(stdout.contains("\"peak_heap_bytes\":"));
        assert!(stdout.contains("\"heap_after_drop_bytes\":"));
    }
}

#[test]
fn load_competitor_and_markdown_work() {
    let output = run(&[
        "--mode",
        "load",
        "--implementation",
        "dotenvy",
        "--size",
        "8",
        "--shape",
        "plain",
        "--format",
        "markdown",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
    assert!(stdout.starts_with('|'));
    assert!(stdout.contains("dotenvy"));
}

#[test]
fn unsupported_competitor_document_mode_fails_clearly() {
    let output = run(&[
        "--mode",
        "document",
        "--implementation",
        "dotenv",
        "--size",
        "8",
        "--shape",
        "plain",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 output");
    assert!(stderr.contains("does not support"));
}
