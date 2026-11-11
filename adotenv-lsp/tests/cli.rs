//! Binary-level configuration checks.

use std::{fs, process::Command};

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "adotenv-lsp-cli-{name}-{}-{}.toml",
        std::process::id(),
        std::thread::current().name().unwrap_or("main")
    ))
}

#[test]
fn invalid_explicit_config_fails_before_the_server_starts() {
    let path = temp_path("invalid");
    fs::write(&path, "unknown = true\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_adotenv-lsp"))
        .arg("--config")
        .arg(&path)
        .output()
        .unwrap();
    let _ = fs::remove_file(&path);
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
