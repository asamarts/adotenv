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

fn run_in(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_adotenv-cli"))
        .current_dir(directory)
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
    let mut stdin = child.stdin.take().unwrap();
    if let Err(error) = stdin.write_all(input.as_bytes()) {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe,
            "unexpected stdin write error: {error}"
        );
    }
    drop(stdin);
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
    assert!(!json.contains(".git"));
    assert!(!json.contains("target"));
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
    assert!(!json.contains(".env.bad"));

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
fn diff_does_not_emit_partial_output_when_a_later_candidate_fails() {
    let dir = TempDir::new();
    let base = dir.file("base.env", "A=1\n");
    let changed = dir.file("changed.env", "A=2\n");
    let bad = dir.file("bad.env", "BAD KEY=value\n");

    let output = run(&[
        "diff",
        base.to_str().unwrap(),
        changed.to_str().unwrap(),
        bad.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("bad.env"));

    let output = run(&[
        "diff",
        "--format=json",
        base.to_str().unwrap(),
        changed.to_str().unwrap(),
        bad.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");

    let output = run(&[
        "diff",
        "--format=matrix",
        base.to_str().unwrap(),
        changed.to_str().unwrap(),
        bad.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");

    let output = run(&[
        "diff",
        base.to_str().unwrap(),
        changed.to_str().unwrap(),
        dir.path().join("missing.env").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
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

#[cfg(unix)]
#[test]
fn writes_keep_the_existing_file_mode() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new();
    for mode in [0o600, 0o640, 0o644, 0o400] {
        let file = dir.file("app.env", "B=2\nA=1\n");
        fs::set_permissions(&file, fs::Permissions::from_mode(mode)).unwrap();
        let output = run(&["set", file.to_str().unwrap(), "A", "3"]);
        assert!(output.status.success(), "{}", stderr(&output));
        let metadata = fs::metadata(&file).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o7777, mode, "set {mode:o}");

        let output = run(&["fmt", "--write", "--sort", file.to_str().unwrap()]);
        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(fs::read_to_string(&file).unwrap(), "A = 3\nB = 2\n");
        let metadata = fs::metadata(&file).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o7777, mode, "fmt {mode:o}");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        fs::remove_file(&file).unwrap();
    }
}

#[cfg(unix)]
fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).unwrap().file_type().is_symlink()
}

#[cfg(unix)]
fn directory_names(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[cfg(unix)]
#[test]
fn writes_replace_the_symlink_target_and_keep_the_link() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new();
    let real = dir.file_at("real/app.env", "B=2\nA=1\n");
    let link = dir.path().join("link.env");
    let chain = dir.path().join("chain.env");
    symlink("real/app.env", &link).unwrap();
    symlink("link.env", &chain).unwrap();

    let output = run(&["set", link.to_str().unwrap(), "A", "3"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(is_symlink(&link));
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("real/app.env"));
    assert_eq!(fs::read_to_string(&real).unwrap(), "B=2\nA=3\n");

    let output = run(&["fmt", "--write", "--sort", chain.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(is_symlink(&chain));
    assert!(is_symlink(&link));
    assert_eq!(fs::read_to_string(&real).unwrap(), "A = 3\nB = 2\n");

    assert_eq!(
        directory_names(dir.path()),
        vec!["chain.env", "link.env", "real"]
    );
    assert_eq!(directory_names(&dir.path().join("real")), vec!["app.env"]);
}

/// A group the current user belongs to other than the primary group, if any.
#[cfg(unix)]
fn supplementary_group() -> Option<u32> {
    let id = |flag: &str| {
        Command::new("id")
            .arg(flag)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let primary: u32 = id("-g")?.trim().parse().ok()?;
    id("-G")?
        .split_whitespace()
        .filter_map(|group| group.parse().ok())
        .find(|group| *group != primary)
}

#[cfg(unix)]
#[test]
fn writes_keep_the_file_group() {
    use std::os::unix::fs::{chown, MetadataExt, PermissionsExt};

    let Some(group) = supplementary_group() else {
        eprintln!("skipping group test: no supplementary group");
        return;
    };
    let dir = TempDir::new();
    let file = dir.file("app.env", "B=2\nA=1\n");
    chown(&file, None, Some(group)).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();

    let output = run(&["set", file.to_str().unwrap(), "A", "3"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let metadata = fs::metadata(&file).unwrap();
    assert_eq!(metadata.gid(), group, "set kept the group");
    assert_eq!(metadata.mode() & 0o7777, 0o640);

    let output = run(&["fmt", "--write", file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    let metadata = fs::metadata(&file).unwrap();
    assert_eq!(metadata.gid(), group, "fmt kept the group");
    assert_eq!(metadata.mode() & 0o7777, 0o640);
}

#[test]
fn recursive_defaults_to_current_directory_and_normalizes_dot_excludes() {
    let dir = TempDir::new();
    dir.file_at(".env", "A = 1\n");
    dir.file_at("sub/.env.bad", "BAD KEY=secret\n");

    let output = run_in(dir.path(), &["check", "--json", "--recursive"]);
    assert_eq!(output.status.code(), Some(1));
    let json = stdout(&output);
    assert!(json.contains(".env"));
    assert!(json.contains(".env.bad"));

    let output = run_in(
        dir.path(),
        &["check", "--json", "--recursive", "--exclude", "./sub"],
    );
    assert!(output.status.success());
    let json = stdout(&output);
    assert!(json.contains(".env"));
    assert!(!json.contains(".env.bad"));
}

#[test]
fn recursive_multiple_roots_are_sorted_and_deduplicated() {
    let first = TempDir::new();
    let second = TempDir::new();
    first.file_at(".env", "A = 1\n");
    second.file_at(".env", "B = 2\n");

    let output = run(&[
        "check",
        "--json",
        "--recursive",
        first.path().to_str().unwrap(),
        second.path().to_str().unwrap(),
        first.path().to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let json = stdout(&output);
    assert_eq!(json.matches("\"ok\":true").count(), 2);
}

#[test]
fn recursive_empty_search_reports_and_succeeds() {
    let dir = TempDir::new();
    fs::create_dir(dir.path().join("empty")).unwrap();
    let empty = dir.path().join("empty");
    let output = run(&["check", "--json", "--recursive", empty.to_str().unwrap()]);
    assert!(output.status.success());
    assert_eq!(stdout(&output), "{\"version\":1,\"files\":[]}\n");
    assert!(stderr(&output).contains("no .env files found"));
}

#[cfg(unix)]
#[test]
fn recursive_discovery_reads_non_utf8_env_paths() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let dir = TempDir::new();
    dir.file_at(".env", "A = 1\n");
    let name = OsString::from_vec(b".env.\xff".to_vec());
    let non_utf8 = dir.path().join(name);
    if let Err(error) = fs::write(&non_utf8, "B=2\n") {
        eprintln!("skipping non-UTF-8 filename test: {error}");
        return;
    }

    let output = run(&[
        "check",
        "--json",
        "--recursive",
        dir.path().to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let json = stdout(&output);
    assert_eq!(json.matches("\"ok\":true").count(), 2);

    let output = run(&[
        "fmt",
        "--recursive",
        "--write",
        dir.path().to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read_to_string(non_utf8).unwrap(), "B = 2\n");
}

#[test]
fn check_source_displays_tab_expanded_carets() {
    let dir = TempDir::new();
    let file = dir.file("tabs.env", "\tA=1\n\tA=2\n");

    let output = run(&["check", "--source", file.to_str().unwrap()]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(":2:2: warning[dotenv.duplicate-key]:"));
    assert!(text.contains("2 |     A=2"));
    assert!(text.contains("  |     ^"));

    let output = run(&["check", "--format", "github", file.to_str().unwrap()]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("::warning"));
    assert!(text.contains("line=2,col=2,title=dotenv.duplicate-key"));
}

#[test]
fn check_github_uses_notice_for_info_diagnostics() {
    let dir = TempDir::new();
    let file = dir.file("bom.env", "\u{feff}A=1\n");
    let output = run(&["check", "--format=github", file.to_str().unwrap()]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("::notice"));
    assert!(stdout(&output).contains("title=dotenv.bom"));

    let output = run(&["check", "--format=json", file.to_str().unwrap()]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("\"version\":1"));
}

#[test]
fn diff_matrix_handles_two_files_and_export_differences() {
    let dir = TempDir::new();
    let left = dir.file("left.env", "export A=1\n");
    let right = dir.file("right.env", "A=1\n");

    let output = run(&[
        "diff",
        "--format",
        "matrix",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("key"));
    assert!(text.contains("export 1"));
    assert!(text.contains("| 1"));

    let output = run(&["diff", left.to_str().unwrap(), right.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("~ A: 1 (export) -> 1"));
}

#[test]
fn diff_json_reports_bare_and_empty_states() {
    let dir = TempDir::new();
    let base = dir.file("base.env", "A\nB=\nC=1\n");
    let candidate = dir.file("candidate.env", "A=1\nB\nC=1\n");
    let output = run(&[
        "diff",
        "--format",
        "json",
        base.to_str().unwrap(),
        candidate.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let json = stdout(&output);
    assert!(json.contains("\"state\":\"bare\""));
    assert!(json.contains("\"state\":\"empty\""));
    assert!(json.contains("\"kind\":\"state\""));
}

#[test]
fn check_json_wire_format_is_frozen() {
    let dir = TempDir::new();
    dir.file_at("good.env", "A = 1\n");
    dir.file_at("bad.env", "BAD KEY=value\n");

    let output = run_in(dir.path(), &["check", "--json", "good.env"]);
    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        "{\"version\":1,\"files\":[{\"path\":\"good.env\",\"ok\":true,\"diagnostics\":[]}]}\n"
    );

    let output = run_in(dir.path(), &["check", "--json", "bad.env"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        "{\"version\":1,\"files\":[{\"path\":\"bad.env\",\"ok\":false,\"diagnostics\":[{\"line\":1,\"severity\":\"error\",\"code\":\"dotenv.invalid-key\",\"message\":\"key does not match the selected dialect\",\"span\":{\"start\":0,\"end\":7},\"secondary_span\":null}]}]}\n"
    );

    let alias = run_in(dir.path(), &["check", "--format=json", "bad.env"]);
    assert_eq!(stdout(&alias), stdout(&output));
    assert_eq!(alias.status.code(), Some(1));
}

#[test]
fn github_annotation_wire_format_is_frozen() {
    let dir = TempDir::new();
    dir.file_at("warn.env", "A=1\nA=2\n");
    let output = run_in(dir.path(), &["check", "--format", "github", "warn.env"]);
    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        "::warning file=warn.env,line=2,col=1,title=dotenv.duplicate-key::duplicate key; lookup follows the selected dialect policy\n"
    );
}

#[test]
fn diff_json_wire_format_is_frozen() {
    let dir = TempDir::new();
    dir.file_at("base.env", "A=1\nB=2\n");
    dir.file_at("candidate.env", "A=2\nC=3\n");
    let output = run_in(
        dir.path(),
        &["diff", "--format", "json", "base.env", "candidate.env"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        "{\"version\":1,\"baseline\":\"base.env\",\"files\":[{\"path\":\"candidate.env\",\"ok\":true,\"differences\":[{\"key\":\"A\",\"kind\":\"value\",\"left\":{\"state\":\"value\",\"value\":\"1\",\"exported\":false},\"right\":{\"state\":\"value\",\"value\":\"2\",\"exported\":false}},{\"key\":\"B\",\"kind\":\"only-left\",\"left\":{\"state\":\"value\",\"value\":\"2\",\"exported\":false},\"right\":null},{\"key\":\"C\",\"kind\":\"only-right\",\"left\":null,\"right\":{\"state\":\"value\",\"value\":\"3\",\"exported\":false}}]}]}\n"
    );

    let alias = run_in(dir.path(), &["diff", "--json", "base.env", "candidate.env"]);
    assert_eq!(stdout(&alias), stdout(&output));
    assert_eq!(alias.status.code(), Some(1));
}

#[test]
fn exit_code_contract_is_frozen() {
    let dir = TempDir::new();
    dir.file_at("good.env", "A = 1\n");
    dir.file_at("bad.env", "BAD KEY=value\n");
    dir.file_at("base.env", "A=1\n");

    assert!(run_in(dir.path(), &["check", "good.env"]).status.success());
    assert_eq!(
        run_in(dir.path(), &["check", "bad.env"]).status.code(),
        Some(1)
    );
    assert_eq!(
        run_in(dir.path(), &["check", "missing.env"]).status.code(),
        Some(1)
    );
    assert_eq!(
        run_in(dir.path(), &["check", "--format", "nope", "good.env"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        run_in(dir.path(), &["diff", "base.env", "missing.env"],)
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        run_in(dir.path(), &["diff", "base.env", "bad.env"])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn usage_and_argument_errors_are_consistent() {
    let cases: &[(&[&str], &str)] = &[
        (&["get"], "usage: adotenv-cli get"),
        (&["get", "one"], "usage: adotenv-cli get"),
        (&["check"], "usage: adotenv-cli check"),
        (
            &["check", "--format"],
            "--format requires human, json or github",
        ),
        (&["check", "--exclude"], "--exclude requires a path"),
        (
            &["check", "--exclude", "x", "file"],
            "--exclude requires --recursive",
        ),
        (&["fmt"], "usage: adotenv-cli fmt"),
        (&["fmt", "--exclude"], "--exclude requires a path"),
        (
            &["fmt", "--exclude", "x", "file"],
            "--exclude requires --recursive",
        ),
        (&["diff"], "usage: adotenv-cli diff"),
        (
            &["diff", "--format"],
            "--format requires human, matrix or json",
        ),
        (&["diff", "baseline"], "usage: adotenv-cli diff"),
        (&["example"], "usage: adotenv-cli example"),
        (&["load"], "usage: adotenv-cli load"),
        (&["load", "a", "b", "c"], "usage: adotenv-cli load"),
        (&["set"], "usage: adotenv-cli set"),
        (&["set", "file", "key"], "usage: adotenv-cli set"),
        (&["run"], "usage: adotenv-cli run"),
        (&["run", "--"], "usage: adotenv-cli run"),
        (&["run", "dir", "--"], "usage: adotenv-cli run"),
        (&["run", "a", "b", "c", "--", "x"], "usage: adotenv-cli run"),
        (&["unknown"], "unknown command"),
    ];
    for (args, needle) in cases {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            stderr(&output).contains(needle),
            "{args:?}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn missing_files_and_invalid_documents_are_reported() {
    let dir = TempDir::new();
    let missing = dir.path().join("missing.env");
    let missing = missing.to_str().unwrap();
    for (args, code) in [
        (vec!["get", missing, "A"], 2),
        (vec!["fmt", missing], 1),
        (vec!["example", missing], 2),
        (vec!["set", missing, "A", "1"], 2),
        (vec!["check", "--source", missing], 1),
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(code), "{args:?}");
        assert!(stderr(&output).contains("missing.env"), "{args:?}");
    }

    let bad = dir.file("bad.env", "BAD KEY=value\n");
    let bad = bad.to_str().unwrap();
    for args in [vec!["get", bad, "BAD"], vec!["set", bad, "A", "1"]] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(stderr(&output).contains("invalid-key"), "{args:?}");
    }

    let duplicate = dir.file("duplicate.env", "A=1\nA=2\n");
    let output = run(&["set", duplicate.to_str().unwrap(), "A", "3"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("duplicate"));

    let output = run(&["fmt", "--write", dir.path().to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!stderr(&output).is_empty());
}

#[test]
fn run_and_load_report_invalid_modes_and_programs() {
    let dir = TempDir::new();
    let output = run(&["load", dir.path().to_str().unwrap(), "../bad"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("mode"));

    let output = run(&["run", dir.path().to_str().unwrap(), "../bad", "--", "echo"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("mode"));

    let output = run(&[
        "run",
        dir.path().to_str().unwrap(),
        "--",
        "adotenv-command-that-does-not-exist",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("failed to run"));
}

#[test]
fn fmt_prints_file_headers_for_multiple_inputs() {
    let dir = TempDir::new();
    let first = dir.file("first.env", "A=1\n");
    let second = dir.file("second.env", "B=2\n");
    let output = run(&["fmt", first.to_str().unwrap(), second.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("==> "));
    assert!(text.contains("first.env <=="));
    assert!(text.contains("second.env <=="));
    assert!(text.contains("A = 1"));
    assert!(text.contains("B = 2"));

    let empty = dir.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let output = run(&["fmt", "--recursive", empty.to_str().unwrap()]);
    assert!(output.status.success());
    assert!(stderr(&output).contains("no .env files found"));
}

#[test]
fn check_json_keeps_unusual_paths_valid() {
    let dir = TempDir::new();
    let file = dir.file("50% done,comma.env", "A=1\n");
    let output = run(&["check", "--json", file.to_str().unwrap()]);
    assert!(output.status.success());
    let json = stdout(&output);
    assert!(json.contains("50% done,comma.env"));
    assert!(json.starts_with("{\"version\":1,\"files\":["));
}

#[cfg(unix)]
#[test]
fn check_json_escapes_control_characters_in_paths() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let dir = TempDir::new();
    let name = OsString::from_vec(b"line\nname\t.env".to_vec());
    let path = dir.path().join(name);
    fs::write(&path, "A=1\n").unwrap();
    let output = run(&["check", "--json", path.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    let json = stdout(&output);
    assert!(json.contains("line\\nname\\t.env"), "{json}");
    assert!(!json.contains('\n') || json.lines().count() == 1);
}

#[test]
fn stdin_and_recursive_limits_are_enforced() {
    let output = run(&[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stdout(&output).contains("Commands:"));

    let output = run(&["--dialect"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("requires a value"));

    let output = run(&["fmt", "-", "-"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("at most once"));

    let output = run(&["diff", "-", "-"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("at most once"));
}

#[test]
fn check_json_includes_secondary_spans() {
    let dir = TempDir::new();
    let file = dir.file("duplicate.env", "A=1\nA=2\n");
    let output = run(&["check", "--json", file.to_str().unwrap()]);
    assert!(output.status.success());
    let json = stdout(&output);
    assert!(json.contains("\"secondary_span\":{\"start\":0,\"end\":1}"));
    assert!(json.contains("\"code\":\"dotenv.duplicate-key\""));
}

#[test]
fn diff_reports_missing_baselines_and_candidates() {
    let dir = TempDir::new();
    let base = dir.file("base.env", "A=1\n");
    let candidate = dir.file("candidate.env", "A=2\n");
    for args in [
        vec!["diff", "--json", "missing.env", candidate.to_str().unwrap()],
        vec![
            "diff",
            "--format=matrix",
            "missing.env",
            candidate.to_str().unwrap(),
        ],
        vec!["diff", "--json", base.to_str().unwrap(), "missing.env"],
        vec![
            "diff",
            "--format=matrix",
            base.to_str().unwrap(),
            "missing.env",
        ],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert_eq!(stdout(&output), "", "{args:?}");
        assert!(stderr(&output).contains("missing.env"), "{args:?}");
    }
}

#[test]
fn diff_matrix_escapes_control_characters_and_states() {
    let dir = TempDir::new();
    let base = dir.file("base.env", "A\nB=\nC=\"line\\nnext\"\n");
    let candidate = dir.file("candidate.env", "A=1\nB=2\nC=x\n");
    let output = run(&[
        "diff",
        "--format",
        "matrix",
        base.to_str().unwrap(),
        candidate.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("<bare>"), "{text}");
    assert!(text.contains("\"\""), "{text}");
    assert!(text.contains("line\\nnext"), "{text}");
}

#[test]
fn run_rejects_invalid_docker_child_keys() {
    let dir = TempDir::new();
    fs::write(dir.path().join(".env"), "A\0B=value\n").unwrap();
    let output = run(&[
        "--dialect",
        "docker",
        "run",
        dir.path().to_str().unwrap(),
        "--",
        "echo",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("invalid child environment key"));
}

#[cfg(unix)]
#[test]
fn recursive_roots_refuse_symlinks() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new();
    let outside = TempDir::new();
    outside.file_at(".env", "A=1\n");
    let link = dir.path().join("linked");
    symlink(outside.path(), &link).unwrap();

    let output = run(&["check", "--recursive", link.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("refusing to follow symbolic links"));
}

#[test]
fn recursive_stdin_and_file_roots_are_supported() {
    let output = run_with_stdin(&["check", "--json", "--recursive", "-"], "A=1\n");
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("\"ok\":true"));

    let dir = TempDir::new();
    let file = dir.file("one.env", "A=1\n");
    let output = run(&["check", "--json", "--recursive", file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("one.env"));
}

#[test]
fn fmt_reports_invalid_source_and_recursive_symlink_roots() {
    let dir = TempDir::new();
    let bad = dir.file("bad.env", "BAD KEY=value\n");
    let output = run(&["fmt", bad.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("invalid-key"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = TempDir::new();
        outside.file_at(".env", "A=1\n");
        let link = dir.path().join("linked");
        symlink(outside.path(), &link).unwrap();
        let output = run(&["fmt", "--recursive", link.to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(2));
        assert!(stderr(&output).contains("symbolic links"));
    }
}

#[test]
fn diff_reports_invalid_baselines_for_both_output_modes() {
    let dir = TempDir::new();
    let bad = dir.file("bad.env", "BAD KEY=value\n");
    let good = dir.file("good.env", "A=1\n");
    for args in [
        vec![
            "diff",
            "--json",
            bad.to_str().unwrap(),
            good.to_str().unwrap(),
        ],
        vec![
            "diff",
            "--format=matrix",
            bad.to_str().unwrap(),
            good.to_str().unwrap(),
        ],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(stderr(&output).contains("invalid-key"), "{args:?}");
        assert_eq!(stdout(&output), "", "{args:?}");
    }
}

#[test]
fn recursive_excludes_absolute_and_file_roots() {
    let dir = TempDir::new();
    dir.file_at(".env", "A=1\n");
    let file = dir.file("one.env", "A=1\n");

    let output = run(&[
        "check",
        "--json",
        "--recursive",
        "--exclude",
        dir.path().to_str().unwrap(),
        dir.path().to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("no .env files found"));

    let output = run(&[
        "check",
        "--json",
        "--recursive",
        "--exclude",
        file.to_str().unwrap(),
        file.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("no .env files found"));
}

#[test]
fn example_reports_diagnostics_and_still_succeeds() {
    let dir = TempDir::new();
    let file = dir.file("bad.env", "A=1\nBAD KEY=secret\n");
    let output = run(&["example", file.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("invalid-key"));
    assert!(!stdout(&output).contains("secret"));
}

#[test]
fn empty_excludes_are_ignored() {
    let dir = TempDir::new();
    dir.file_at(".env", "A=1\n");
    let output = run(&[
        "check",
        "--json",
        "--recursive",
        "--exclude",
        "",
        dir.path().to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("\"ok\":true"));
}

#[cfg(unix)]
#[test]
fn recursive_roots_reject_non_regular_files() {
    use std::os::unix::net::UnixListener;

    let dir = TempDir::new();
    let socket = dir.path().join("socket");
    let _listener = UnixListener::bind(&socket).unwrap();
    let output = run(&["check", "--recursive", socket.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("not a regular file or directory"));
}

#[test]
fn diff_matrix_handles_empty_keys_and_escaped_cells() {
    let dir = TempDir::new();
    let base = dir.file("base.env", "=value\nA=\"tab\\there\"\nB=a|b\n");
    let candidate = dir.file("candidate.env", "=other\nA=plain\nB=c|d\n");
    let output = run(&[
        "--dialect",
        "compose",
        "diff",
        "--format",
        "matrix",
        base.to_str().unwrap(),
        candidate.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("tab\\there"), "{text}");
    assert!(text.contains("a\\|b"), "{text}");
}
