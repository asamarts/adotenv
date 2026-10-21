//! CLI integration tests.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "adotenv-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
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
    Command::new(env!("CARGO_BIN_EXE_adotenv-cli"))
        .args(args)
        .output()
        .unwrap()
}

fn run_with_stdin(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_adotenv-cli"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

#[test]
fn get_prints_effective_values_and_reports_missing_keys() {
    let dir = TempDir::new();
    let file = dir.file(".env", "A=1\nB=\n");
    let output = run(&["get", file.to_str().unwrap(), "A"]);
    assert!(output.status.success());
    assert_eq!(stdout(&output), "1\n");

    let output = run(&["get", file.to_str().unwrap(), "B"]);
    assert!(output.status.success());
    assert_eq!(stdout(&output), "\n");

    let output = run(&["get", file.to_str().unwrap(), "MISSING"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("missing key"));
}

#[test]
fn check_reports_errors_and_success() {
    let dir = TempDir::new();
    let good = dir.file("good.env", "A=1\n");
    let output = run(&["check", good.to_str().unwrap()]);
    assert!(output.status.success());

    let bad = dir.file("bad.env", "BAD KEY=value\n");
    let output = run(&["check", bad.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("dotenv.invalid-key"));
}

#[test]
fn fmt_writes_sorted_normalized_output() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "B=2\nA=1\n\n\n");
    let output = run(&["fmt", "--write", "--sort", file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read_to_string(&file).unwrap(), "A = 1\nB = 2\n\n");
}

#[test]
fn fmt_without_write_prints_output() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "B=2\nA=1\n");
    let output = run(&["fmt", "--sort", file.to_str().unwrap()]);
    assert!(output.status.success());
    assert_eq!(stdout(&output), "A = 1\nB = 2\n");
}

#[test]
fn diff_reports_value_changes_and_exit_status() {
    let dir = TempDir::new();
    let left = dir.file("left.env", "A=1\nB=2\n");
    let right = dir.file("right.env", "A=3\nC=4\n");
    let output = run(&["diff", left.to_str().unwrap(), right.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let output = stdout(&output);
    assert!(output.contains("~ A: 1 -> 3"));
    assert!(output.contains("- B=2"));
    assert!(output.contains("+ C=4"));
}

#[test]
fn example_and_load_commands_work() {
    let dir = TempDir::new();
    let file = dir.file(".env", "# app\nA=1\nBAD KEY=secret\n");
    let output = run(&["example", file.to_str().unwrap()]);
    assert!(output.status.success());
    assert_eq!(stdout(&output), "# app\nA=\n#\n");

    let load_dir = TempDir::new();
    fs::write(load_dir.path().join(".env"), "A=1\n").unwrap();
    fs::write(load_dir.path().join(".env.local"), "B=2\n").unwrap();
    let output = run(&["load", load_dir.path().to_str().unwrap()]);
    assert!(output.status.success());
    assert_eq!(stdout(&output), "A=1\nB=2\n");
}

#[test]
fn set_updates_a_unique_key() {
    let dir = TempDir::new();
    let file = dir.file(".env", "A=1\n");
    let output = run(&["set", file.to_str().unwrap(), "A", "2"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read_to_string(&file).unwrap(), "A=2\n");
}

#[test]
fn help_and_version_are_available() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("Commands:"));

    let output = run(&["--version"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("adotenv-cli"));
}

#[test]
fn run_requires_a_separator_and_program() {
    let dir = TempDir::new();
    let output = run(&["run", dir.path().to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
}

#[cfg(unix)]
#[test]
fn run_executes_a_child_with_loaded_values() {
    let dir = TempDir::new();
    fs::write(dir.path().join(".env"), "ADOTENV_CLI_RUN=value\n").unwrap();
    let output = run(&[
        "run",
        dir.path().to_str().unwrap(),
        "--",
        "sh",
        "-c",
        "printf %s \"$ADOTENV_CLI_RUN\"",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "value");
}

#[cfg(unix)]
#[test]
fn run_propagates_the_child_exit_code() {
    let dir = TempDir::new();
    fs::write(dir.path().join(".env"), "A=1\n").unwrap();
    let output = run(&[
        "run",
        dir.path().to_str().unwrap(),
        "--",
        "sh",
        "-c",
        "exit 7",
    ]);
    assert_eq!(output.status.code(), Some(7));
}

#[test]
fn run_rejects_nul_environment_values_before_spawning() {
    let dir = TempDir::new();
    fs::write(dir.path().join(".env"), "A=bad\0value\n").unwrap();
    let output = run(&[
        "run",
        dir.path().to_str().unwrap(),
        "--",
        "adotenv-command-that-must-not-run",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("NUL"));
}

#[test]
fn dialect_selection_applies_to_commands() {
    let dir = TempDir::new();
    let file = dir.file("node.env", "A=a#b\n");
    let portable = run(&["get", file.to_str().unwrap(), "A"]);
    assert_eq!(stdout(&portable), "a#b\n");

    let node = run(&["get", "--dialect", "node", file.to_str().unwrap(), "A"]);
    assert_eq!(stdout(&node), "a\n");

    let node = run(&["--dialect", "node", "get", file.to_str().unwrap(), "A"]);
    assert_eq!(stdout(&node), "a\n");

    let unknown = run(&["get", "--dialect", "nope", file.to_str().unwrap(), "A"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(stderr(&unknown).contains("unknown dialect"));
}

#[test]
fn fmt_check_reports_changes_without_writing() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "A=1\n");
    let output = run(&["fmt", "--check", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("would reformat"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "A=1\n");

    let formatted = dir.file("formatted.env", "A = 1\n");
    let output = run(&["fmt", "--check", formatted.to_str().unwrap()]);
    assert!(output.status.success());

    let output = run(&["fmt", "--write", "--check", file.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn stdin_is_accepted_for_read_only_commands() {
    let dir = TempDir::new();
    let other = dir.file("other.env", "A=2\n");

    let output = run_with_stdin(&["get", "-", "A"], "A=1\n");
    assert!(output.status.success());
    assert_eq!(stdout(&output), "1\n");

    let output = run_with_stdin(&["get", "--stdin", "A"], "A=1\n");
    assert!(output.status.success());
    assert_eq!(stdout(&output), "1\n");

    let output = run_with_stdin(&["check", "--stdin"], "BAD KEY=value\n");
    assert_eq!(output.status.code(), Some(1));

    let output = run_with_stdin(&["fmt", "-"], "B=2\nA=1\n");
    assert!(output.status.success());
    assert_eq!(stdout(&output), "B = 2\nA = 1\n");

    let output = run_with_stdin(&["diff", "-", other.to_str().unwrap()], "A=1\n");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout(&output), "~ A: 1 -> 2\n");

    let output = run(&["fmt", "--write", "-"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("cannot write standard input"));

    let output = run(&["set", "-", "A", "1"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("cannot write standard input"));

    let output = run_with_stdin(&["check", "-", "--stdin"], "A=1\n");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("at most once"));
}

#[test]
fn check_json_uses_a_versioned_schema() {
    let dir = TempDir::new();
    let good = dir.file("good.env", "A=1\n");
    let bad = dir.file("bad.env", "BAD KEY=value\n");

    let output = run(&["check", "--json", good.to_str().unwrap()]);
    assert!(output.status.success());
    let json = stdout(&output);
    assert!(json.starts_with("{\"version\":1,\"files\":["));
    assert!(json.contains("\"ok\":true"));
    assert!(json.contains("\"diagnostics\":[]"));

    let output = run(&["check", "--json", bad.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let json = stdout(&output);
    assert!(json.contains("\"ok\":false"));
    assert!(json.contains("\"code\":\"dotenv.invalid-key\""));
    assert!(json.contains("\"line\":1"));
    assert!(json.contains("\"span\":{\"start\":0,"));

    let output = run(&["check", "--json", "definitely-missing.env"]);
    assert_eq!(output.status.code(), Some(1));
    let json = stdout(&output);
    assert!(json.contains("\"ok\":false"));
    assert!(json.contains("\"error\":"));

    let output = run_with_stdin(&["check", "--json", "-"], "A=1\n");
    assert!(output.status.success());
    assert!(stdout(&output).contains("\"ok\":true"));
}
