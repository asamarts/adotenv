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

#[test]
fn recursive_discovery_applies_env_names_ignores_and_excludes() {
    let dir = TempDir::new();
    dir.file_at(".env", "A = 1\n");
    dir.file_at(".env.local", "B = 2\n");
    dir.file_at("sub/.env.production", "C = 3\n");
    dir.file_at("sub/.env.bad", "BAD KEY=value\n");
    dir.file_at(".envrc", "BAD KEY=value\n");
    dir.file_at(".git/.env", "BAD KEY=value\n");
    dir.file_at("target/.env", "BAD KEY=value\n");
    dir.file_at("sub/not-env", "BAD KEY=value\n");

    let output = run(&["check", "--json", "-r", dir.path().to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let json = stdout(&output);
    assert!(json.contains(".env.production"));
    assert!(json.contains(".env.bad"));
    assert!(!json.contains(".git/.env"));
    assert!(!json.contains("target/.env"));
    assert!(!json.contains("not-env"));
    assert!(!json.contains(".envrc"));

    let output = run(&[
        "check",
        "--json",
        "--recursive",
        "--exclude",
        "sub",
        dir.path().to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let json = stdout(&output);
    assert!(json.contains(".env"));
    assert!(json.contains(".env.local"));
    assert!(!json.contains("sub/"));

    let output = run(&["check", "--exclude", "sub", dir.path().to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--recursive"));

    let output = run(&[
        "fmt",
        "--recursive",
        "--check",
        "--exclude",
        "sub",
        dir.path().to_str().unwrap(),
    ]);
    assert!(output.status.success());
}

#[cfg(unix)]
#[test]
fn recursive_discovery_does_not_follow_symlinks() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new();
    let outside = TempDir::new();
    dir.file_at(".env", "A = 1\n");
    outside.file_at(".env", "BAD KEY=value\n");
    symlink(outside.path(), dir.path().join("linked-dir")).unwrap();
    symlink(outside.path().join(".env"), dir.path().join(".env.link")).unwrap();

    let output = run(&[
        "check",
        "--json",
        "--recursive",
        dir.path().to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let json = stdout(&output);
    assert!(json.contains(".env\""));
    assert!(!json.contains("linked-dir"));
    assert!(!json.contains(".env.link"));
}

#[test]
fn check_renders_human_json_and_github_formats() {
    let dir = TempDir::new();
    let bad = dir.file("bad,name.env", "A=1\nBAD KEY=secret\n");

    let human = run(&["check", bad.to_str().unwrap()]);
    assert_eq!(human.status.code(), Some(1));
    let text = stdout(&human);
    assert!(text.contains(":2:1: error[dotenv.invalid-key]:"));
    assert!(text.contains("1 error, 0 warnings, 0 info in 1 file"));
    assert!(!text.contains("BAD KEY"));
    assert!(!text.contains("secret"));

    let explicit = run(&["check", "--format", "human", bad.to_str().unwrap()]);
    assert_eq!(stdout(&explicit), text);

    let json = run(&["check", "--format=json", bad.to_str().unwrap()]);
    assert_eq!(json.status.code(), Some(1));
    assert!(stdout(&json).starts_with("{\"version\":1,\"files\":["));
    assert!(stdout(&json).contains("\"code\":\"dotenv.invalid-key\""));

    let github = run(&["check", "--format", "github", bad.to_str().unwrap()]);
    assert_eq!(github.status.code(), Some(1));
    let annotations = stdout(&github);
    assert!(annotations.contains("::error file="));
    assert!(annotations.contains("line=2,col=1,title=dotenv.invalid-key"));
    assert!(annotations.contains("bad%2Cname.env"));
    assert!(!annotations.contains("secret"));
}

#[test]
fn check_source_is_an_explicit_human_only_opt_in() {
    let dir = TempDir::new();
    let bad = dir.file("bad.env", "A=1\nBAD KEY=secret\n");

    let output = run(&["check", "--source", bad.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("2 | BAD KEY=secret"));
    assert!(text.contains("| ^^^^^^^"));
    assert!(text.contains(":2:1: error[dotenv.invalid-key]:"));

    for args in [
        vec!["check", "--source", "--json", bad.to_str().unwrap()],
        vec![
            "check",
            "--source",
            "--format",
            "github",
            bad.to_str().unwrap(),
        ],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(stderr(&output).contains("--source requires human output"));
    }

    let output = run(&["check", "--format", "sarif", bad.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--format expects human, json or github"));

    let output = run(&[
        "check",
        "--json",
        "--format",
        "github",
        bad.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("conflicting output formats"));
}

#[test]
fn github_format_annotates_read_failures() {
    let output = run(&["check", "--format", "github", "definitely-missing.env"]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("::error file=definitely-missing.env,title=adotenv::"));
    assert!(text.contains("definitely-missing.env"));
}

#[test]
fn diff_compares_multiple_files_against_a_baseline_and_renders_a_matrix() {
    let dir = TempDir::new();
    let base = dir.file("base.env", "A=1\nB=2\n");
    let one = dir.file("one.env", "A=3\nC=4\n");
    let two = dir.file("two.env", "A=1\nB=2\n");

    let output = run(&[
        "diff",
        base.to_str().unwrap(),
        one.to_str().unwrap(),
        two.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("base.env ->"));
    assert!(text.contains("one.env"));
    assert!(text.contains("~ A: 1 -> 3"));
    assert!(text.contains("- B=2"));
    assert!(text.contains("+ C=4"));
    assert!(text.contains("two.env"));
    assert!(text.contains("(no differences)"));

    let output = run(&[
        "diff",
        "--format",
        "matrix",
        base.to_str().unwrap(),
        one.to_str().unwrap(),
        two.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("key"));
    assert!(text.contains("base.env"));
    assert!(text.contains("one.env"));
    assert!(text.contains("two.env"));
    assert!(text.contains("C"));
    assert!(text.contains("<unset>"));
}

#[test]
fn diff_json_is_versioned_and_reports_each_candidate() {
    let dir = TempDir::new();
    let base = dir.file("base.env", "A=1\n");
    let changed = dir.file("changed.env", "A=2\n");
    let same = dir.file("same.env", "A=1\n");

    let output = run(&[
        "diff",
        "--format=json",
        base.to_str().unwrap(),
        changed.to_str().unwrap(),
        same.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let json = stdout(&output);
    assert!(json.starts_with("{\"version\":1,\"baseline\":"));
    assert!(json.contains("\"path\":"));
    assert!(json.contains("\"kind\":\"value\""));
    assert!(json.contains("\"left\":{\"state\":\"value\",\"value\":\"1\",\"exported\":false}"));
    assert!(json.contains("\"right\":{\"state\":\"value\",\"value\":\"2\",\"exported\":false}"));

    let output = run(&[
        "diff",
        "--json",
        base.to_str().unwrap(),
        same.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("\"differences\":[]"));

    let output = run(&[
        "diff",
        "--json",
        "--format",
        "matrix",
        base.to_str().unwrap(),
        changed.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("conflicting output formats"));

    let output = run(&[
        "diff",
        "--format",
        "sarif",
        base.to_str().unwrap(),
        changed.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--format expects human, matrix or json"));

    let output = run(&["diff", base.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn writes_replace_existing_files_without_leftovers() {
    let dir = TempDir::new();
    let file = dir.file("app.env", "A=1\n");

    let output = run(&["set", file.to_str().unwrap(), "A", "2"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read_to_string(&file).unwrap(), "A=2\n");

    let output = run(&["fmt", "--write", "--sort", file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read_to_string(&file).unwrap(), "A = 2\n");

    let mut entries: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    assert_eq!(entries, vec!["app.env"]);
}
