//! End-to-end tests for the report-only linter command.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "adotenv-lint-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, source).unwrap();
        path
    }
    fn file_at(&self, relative: &str, source: &str) -> PathBuf {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, source).unwrap();
        path
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_adotenv-lint"))
        .args(args)
        .output()
        .unwrap()
}

fn run_in(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_adotenv-lint"))
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn reports_findings_and_exit_code_one() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "A=1\nA=2\n");
    let output = run(&[file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("duplicate-key"), "{text}");
    assert!(text.contains('A'), "{text}");
}

#[test]
fn clean_file_exits_zero() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "A=1\nB=2\n");
    let output = run(&[file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout.len(), 0);
}

#[test]
fn key_order_and_json_output_work() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "B=2\nA=1\n");
    let output = run(&["--format=json", "--key-order", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with('['), "{text}");
    assert!(text.contains("\"rule\":\"key-order\""), "{text}");
    assert!(text.contains("\"path\":"), "{text}");
}

#[test]
fn recursive_discovery_finds_env_cascades() {
    let dir = TempDir::new();
    dir.file("nested/.env", "A=1\n");
    dir.file("nested/.env.production", "B=2\n");
    dir.file("nested/ignored.txt", "A=1\nA=2\n");
    let output = run(&["--recursive", dir.path().to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn parse_and_usage_failures_exit_two() {
    let dir = TempDir::new();
    let bad = dir.file("bad.env", "BAD KEY=value\n");
    let output = run(&[bad.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));

    let output = run(&["--unknown"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn double_dash_allows_a_dash_prefixed_path() {
    let dir = TempDir::new();
    let file = dir.file("--dash.env", "A=1\nA=2\n");
    let output = run(&["--", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn fix_applies_verified_key_order_and_exits_zero() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "B=2\nA=1\n");
    let output = run(&["--fix", "--key-order", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&file).unwrap(), "A=1\nB=2\n");
}

#[test]
fn fix_reports_unfixable_findings_without_rewriting() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "A=1\nA=2\n");
    let output = run(&["--fix", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&file).unwrap(), "A=1\nA=2\n");
}

#[test]
fn fix_applies_quote_style() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "A='one'\nC='two'\n");
    let output = run(&["--fix", "--quote-style=double", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&file).unwrap(), "A=\"one\"\nC=\"two\"\n");
}

#[test]
fn config_controls_rules_and_cli_overrides_format() {
    let dir = TempDir::new();
    let config = dir.file(
        ".adotenv-lint.toml",
        "key_order = true\nformat = \"json\"\n",
    );
    let file = dir.file("app.env", "B=2\nA=1\n");

    let output = run(&["--config", config.to_str().unwrap(), file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.trim_start().starts_with('['), "{text}");
    assert!(text.contains("key-order"), "{text}");

    let output = run(&[
        "--config",
        config.to_str().unwrap(),
        "--format=human",
        file.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.trim_start().starts_with('['), "{text}");
    assert!(text.contains("key-order"), "{text}");
}

#[test]
fn default_config_is_read_from_the_working_directory() {
    let dir = TempDir::new();
    dir.file(".adotenv-lint.toml", "key_order = true\n");
    dir.file("app.env", "B=2\nA=1\n");
    let output = run_in(dir.path(), &["app.env"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("key-order"));
}

#[test]
fn invalid_config_is_a_usage_failure() {
    let dir = TempDir::new();
    let config = dir.file(".adotenv-lint.toml", "unknown = true\n");
    let file = dir.file("app.env", "A=1\n");
    let output = run(&["--config", config.to_str().unwrap(), file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("unknown field"));
}

#[cfg(windows)]
#[test]
fn recursive_discovery_is_case_insensitive_on_windows() {
    let dir = TempDir::new();
    dir.file(".ENV", "A=1\nA=2\n");
    dir.file(".ENV.production", "B=2\nB=3\n");
    dir.file_at(".GIT/ignored.env", "C=1\nC=2\n");
    let output = run(&["--recursive", dir.path().to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(".ENV"), "{text}");
    assert!(text.contains(".ENV.production"), "{text}");
    assert!(!text.contains("ignored.env"), "{text}");
}

#[test]
fn fix_batches_multiple_key_case_renames() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "a=1\nb=2\n");
    let output = run(&["--fix", "--key-case=upper", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&file).unwrap(), "A=1\nB=2\n");
}

#[test]
fn rule_severity_and_ignore_patterns_are_honored() {
    let dir = TempDir::new();
    dir.file(
        ".adotenv-lint.toml",
        "ignore = [\".env.skip\"]\n[rules]\nduplicate-key = \"error\"\n",
    );
    dir.file(".env.skip", "A=1\nA=2\n");
    dir.file(".env", "B=1\nB=2\n");

    let output = run_in(dir.path(), &["--format=json", ".env"]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("\"severity\":\"error\""), "{text}");

    let output = run_in(dir.path(), &["--recursive", "."]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(".env"), "{text}");
    assert!(!text.contains(".env.skip"), "{text}");

    let output = run_in(
        dir.path(),
        &["--recursive", "--exclude=.env", "--exclude=.env.skip", "."],
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn config_is_discovered_from_an_ancestor_directory() {
    let dir = TempDir::new();
    dir.file(".adotenv-lint.toml", "key_order = true\n");
    dir.file_at("nested/app.env", "B=2\nA=1\n");
    let nested = dir.path().join("nested");
    let output = run_in(&nested, &["app.env"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("key-order"));
}
