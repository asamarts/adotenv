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

fn json_usize(json: &str, key: &str) -> usize {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .unwrap_or_else(|| panic!("missing {needle} in {json}"))
        + needle.len();
    let end = json[start..]
        .find([',', '}'])
        .map_or(json.len(), |offset| start + offset);
    json[start..end]
        .parse::<usize>()
        .unwrap_or_else(|error| panic!("invalid {key} in {json}: {error}"))
}

#[test]
fn compact_and_compact_entries_measure_nonzero_heap_and_clean_drop() {
    for (mode, implementation) in [
        ("values", "adotenv-compact"),
        ("load", "adotenv-compact"),
        ("values", "adotenv-compact-entries"),
    ] {
        let output = run(&[
            "--mode",
            mode,
            "--implementation",
            implementation,
            "--size",
            "128",
            "--shape",
            "plain",
        ]);
        assert!(
            output.status.success(),
            "{mode}/{implementation}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
        assert!(stdout.contains(&format!("\"implementation\":\"{implementation}\"")));
        let peak = json_usize(&stdout, "peak_heap_bytes");
        let retained = json_usize(&stdout, "retained_heap_bytes");
        let after_drop = json_usize(&stdout, "heap_after_drop_bytes");
        let allocations = json_usize(&stdout, "allocations");
        assert!(retained > 0, "{stdout}");
        assert!(peak >= retained, "{stdout}");
        assert_eq!(after_drop, 0, "{stdout}");
        assert!(allocations > 0, "{stdout}");
    }
}

#[test]
fn rejects_unknown_cli_flags() {
    let output = run(&[
        "--mode",
        "values",
        "--implementation",
        "adotenv",
        "--size",
        "8",
        "--shape",
        "plain",
        "--bogus",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 output");
    assert!(stderr.contains("unknown argument"), "{stderr}");
}
