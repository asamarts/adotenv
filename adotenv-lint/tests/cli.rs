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

    let output = run_in(dir.path(), &["--format=github", ".env"]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with("::error "), "{text}");

    let output = run_in(dir.path(), &[".env"]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("error[duplicate-key]"), "{text}");

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

#[test]
fn fix_batches_renames_sorting_and_quote_style() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "b='two'\na='one'\n");
    let output = run(&[
        "--fix",
        "--key-case=upper",
        "--key-order",
        "--quote-style=double",
        file.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&file).unwrap(), "A=\"one\"\nB=\"two\"\n");
}

#[test]
fn exclude_matches_an_explicit_file_by_basename() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "A=1\nA=2\n");
    let output = run(&["--exclude=app.env", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout.len(), 0);
}

#[test]
fn fix_handles_a_long_target_file_name() {
    let dir = TempDir::new();
    let name = format!("{}.env", "x".repeat(180));
    let file = dir.file(&name, "B=2\nA=1\n");
    let output = run(&["--fix", "--key-order", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&file).unwrap(), "A=1\nB=2\n");
}

#[test]
fn rules_table_can_enable_and_disable_rules() {
    let dir = TempDir::new();
    dir.file(".adotenv-lint.toml", "[rules]\nkey-order = \"error\"\n");
    dir.file(".env", "B=2\nA=1\n");
    let output = run_in(dir.path(), &[".env"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("key-order"));

    dir.file(".adotenv-lint.toml", "[rules]\nduplicate-key = \"off\"\n");
    dir.file(".env", "A=1\nA=2\n");
    let output = run_in(dir.path(), &[".env"]);
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn parse_limit_flags_bound_input_bytes_and_lines() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "A=1\nB=2\nC=3\n");

    let too_many_lines = run(&["--max-lines", "2", file.to_str().unwrap()]);
    assert_eq!(too_many_lines.status.code(), Some(2));
    assert!(String::from_utf8(too_many_lines.stderr)
        .unwrap()
        .contains("line"));

    let too_many_bytes = run(&["--max-input-bytes=6", file.to_str().unwrap()]);
    assert_eq!(too_many_bytes.status.code(), Some(2));
    assert!(String::from_utf8(too_many_bytes.stderr)
        .unwrap()
        .contains("limit"));

    let within_limits = run(&[
        "--max-input-bytes",
        "64",
        "--max-lines=3",
        file.to_str().unwrap(),
    ]);
    assert_eq!(within_limits.status.code(), Some(0));

    let bad_val = run(&["--max-lines=abc", file.to_str().unwrap()]);
    assert_eq!(bad_val.status.code(), Some(2));
    let missing_val = run(&["--max-input-bytes"]);
    assert_eq!(missing_val.status.code(), Some(2));
}

#[test]
fn help_version_default_scan_multi_json_and_dir_without_recursive() {
    let help = run(&["--help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8(help.stdout)
        .unwrap()
        .contains("--max-lines"));

    let version = run(&["--version"]);
    assert_eq!(version.status.code(), Some(0));
    assert!(String::from_utf8(version.stdout)
        .unwrap()
        .starts_with("adotenv-lint "));

    let dir = TempDir::new();
    let file = dir.file(".env", "A=1\nA=2\nB=3\nB=4\n");

    // Passing a directory without --recursive fails with exit code 2.
    let dir_err = run(&[dir.path().to_str().unwrap()]);
    assert_eq!(dir_err.status.code(), Some(2));
    assert!(String::from_utf8(dir_err.stderr)
        .unwrap()
        .contains("--recursive"));

    // No path argument searches the current directory recursively.
    let default_scan = run_in(dir.path(), &["--format=json"]);
    assert_eq!(default_scan.status.code(), Some(1));
    let json = String::from_utf8(default_scan.stdout).unwrap();
    assert!(json.starts_with("[{") && json.trim_end().ends_with("}]"));
    assert!(json.contains("},{"));

    // Recursive walk enters normal directories and skips ignored directories.
    let walk_dir = TempDir::new();
    walk_dir.file_at("nested/.env", "A=1\nA=2\n");
    walk_dir.file_at(".git/.env", "X=1\nX=2\n");
    walk_dir.file_at("target/.env", "Y=1\nY=2\n");
    let walk_out = run(&["--recursive", walk_dir.path().to_str().unwrap()]);
    assert_eq!(walk_out.status.code(), Some(1));
    let walk_text = String::from_utf8(walk_out.stdout).unwrap();
    assert!(walk_text.contains("nested"));
    assert!(!walk_text.contains(".git"));
    assert!(!walk_text.contains("target"));

    // --fix handles a >160-byte filename whose byte 160 falls inside a UTF-8 char.
    let utf8_name = format!("{}é.env", "a".repeat(159));
    let utf8_file = dir.file(&utf8_name, "B=2\nA=1\n");
    let utf8_fix = run(&["--fix", "--key-order", utf8_file.to_str().unwrap()]);
    assert_eq!(utf8_fix.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&utf8_file).unwrap(), "A=1\nB=2\n");

    // --fix respects --max-lines.
    let fix_limited = run(&["--fix", "--max-lines=2", file.to_str().unwrap()]);
    assert_eq!(fix_limited.status.code(), Some(2));

    // Multiple failing files report every error instead of overwriting earlier ones.
    let bad1 = dir.file("bad1.env", "BAD KEY=1\n");
    let bad2 = dir.file("bad2.env", "ALSO BAD=2\n");
    let multi = run(&[bad1.to_str().unwrap(), bad2.to_str().unwrap()]);
    assert_eq!(multi.status.code(), Some(2));
    let multi_err = String::from_utf8(multi.stderr).unwrap();
    assert!(multi_err.contains("bad1.env"), "{multi_err}");
    assert!(multi_err.contains("bad2.env"), "{multi_err}");

    // --config --help passes "--help" as the config path instead of printing help.
    let opt_help = run(&["--config", "--help", file.to_str().unwrap()]);
    assert_eq!(opt_help.status.code(), Some(2));
    let opt_err = String::from_utf8(opt_help.stderr).unwrap();
    assert!(opt_err.contains("--help"), "{opt_err}");

    // Duplicate path spellings (.env and ./subdir/../.env) are deduplicated.
    let dedup_dir = TempDir::new();
    let target = dedup_dir.file(".env", "A=1\nA=2\n");
    let alt = dedup_dir.path().join(".").join(".env");
    let dedup = run(&[target.to_str().unwrap(), alt.to_str().unwrap()]);
    assert_eq!(dedup.status.code(), Some(1));
    let dedup_out = String::from_utf8(dedup.stdout).unwrap();
    assert_eq!(dedup_out.lines().count(), 1, "{dedup_out}");

    // --max-input-bytes rejects oversized files on both lint and --fix.
    let bytes_lint = run(&["--max-input-bytes=4", file.to_str().unwrap()]);
    assert_eq!(bytes_lint.status.code(), Some(2));
    assert!(String::from_utf8(bytes_lint.stderr)
        .unwrap()
        .contains("limit"));
    let bytes_fix = run(&["--fix", "--max-input-bytes=4", file.to_str().unwrap()]);
    assert_eq!(bytes_fix.status.code(), Some(2));

    // Explicit symlinked directory roots (with and without trailing slash) are refused.
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let sym_root = TempDir::new();
        let outside = TempDir::new();
        outside.file(".env", "A=1\nA=2\n");
        let link = sym_root.path().join("linked");
        symlink(outside.path(), &link).unwrap();

        let no_slash = run(&["--recursive", link.to_str().unwrap()]);
        assert_eq!(no_slash.status.code(), Some(2));
        assert!(String::from_utf8(no_slash.stderr)
            .unwrap()
            .contains("symbolic link directory"));

        let with_slash = run(&["--recursive", &format!("{}/", link.display())]);
        assert_eq!(with_slash.status.code(), Some(2));
        assert!(String::from_utf8(with_slash.stderr)
            .unwrap()
            .contains("symbolic link directory"));
    }
}

#[cfg(unix)]
#[test]
fn closed_stdout_pipe_and_stdout_write_errors_report_cleanly() {
    use std::process::Stdio;

    let dir = TempDir::new();
    let many = "K=v\n".repeat(10_000);
    let big = dir.file("big.env", &many);

    // Closed stdout pipe (BrokenPipe) must not report a spurious stdout error or exit 2.
    let mut child = Command::new(env!("CARGO_BIN_EXE_adotenv-lint"))
        .arg(&big)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let pipe_out = child.wait_with_output().unwrap();
    assert_eq!(
        pipe_out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&pipe_out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&pipe_out.stderr), "");

    // Real stdout write/flush error (/dev/full on Linux) must report stdout error once and exit 2.
    #[cfg(target_os = "linux")]
    if let Ok(full) = fs::OpenOptions::new().write(true).open("/dev/full") {
        let help_out = Command::new(env!("CARGO_BIN_EXE_adotenv-lint"))
            .arg("--help")
            .stdout(full)
            .output()
            .unwrap();
        assert_eq!(help_out.status.code(), Some(2));
        let err = String::from_utf8(help_out.stderr).unwrap();
        assert!(err.contains("stdout error"), "{err}");
        assert_eq!(err.matches("stdout error").count(), 1, "{err}");

        let full2 = fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap();
        let small = dir.file("small.env", "A=1\nA=2\n");
        let lint_out = Command::new(env!("CARGO_BIN_EXE_adotenv-lint"))
            .arg(&small)
            .stdout(full2)
            .output()
            .unwrap();
        assert_eq!(lint_out.status.code(), Some(2));
        let err2 = String::from_utf8(lint_out.stderr).unwrap();
        assert!(err2.contains("stdout error"), "{err2}");
        assert_eq!(err2.matches("stdout error").count(), 1, "{err2}");
    }
}
