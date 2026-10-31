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
fn load_output_parses_back_to_the_loaded_values() {
    let dir = TempDir::new();
    fs::write(
        dir.path().join(".env"),
        concat!(
            "FORGED=\"x\\nB=forged\"\n",
            "CRLF=\"a\\r\\nb\"\n",
            "TAB=\"a\\tb\"\n",
            "DOUBLE='say \"hi\"'\n",
            "SINGLE=\"'q'\"\n",
            "HASH=\"#x\"\n",
            "INLINE=\"a #b\"\n",
            "DOLLAR='$HOME ${A} $(cmd) `cmd`'\n",
            "LEAD=\" lead\"\n",
            "TRAIL=\"trail \"\n",
            "BACKSLASH='back\\slash \\n'\n",
            "UNICODE=\u{e9}\n",
            "EQUALS=a=b\n",
            "EMPTY=x\n",
        ),
    )
    .unwrap();
    fs::write(dir.path().join(".env.local"), "EMPTY=\nPLAIN=1\n").unwrap();

    let output = run(&["load", dir.path().to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(!text.lines().any(|line| line.starts_with("B=")), "{text}");

    let loaded = adotenv::load::load_cascade(dir.path(), "").unwrap();
    let expected: Vec<(&str, &str)> = loaded.values().collect();
    assert_eq!(text.lines().count(), expected.len(), "{text}");
    let printed = adotenv::parse_strict(&text).unwrap();
    let actual: Vec<(&str, &str)> = printed
        .entries()
        .map(|entry| (entry.key(), printed.get(entry.key()).unwrap()))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(printed.get("FORGED"), Some("x\nB=forged"));
    assert_eq!(printed.get("CRLF"), Some("a\r\nb"));
    assert_eq!(printed.get("DOUBLE"), Some("say \"hi\""));
    assert_eq!(printed.get("HASH"), Some("#x"));
    assert_eq!(printed.get("DOLLAR"), Some("$HOME ${A} $(cmd) `cmd`"));
    assert_eq!(printed.get("LEAD"), Some(" lead"));
    assert_eq!(printed.get("TRAIL"), Some("trail "));
    assert_eq!(printed.get("BACKSLASH"), Some("back\\slash \\n"));
    assert_eq!(printed.get("EMPTY"), Some(""));
    assert_eq!(printed.get("PLAIN"), Some("1"));

    let node = run(&["--dialect", "node", "load", dir.path().to_str().unwrap()]);
    assert!(node.status.success(), "{}", stderr(&node));
    adotenv::parse_strict(&stdout(&node)).unwrap();
}

#[test]
fn load_fails_without_output_when_a_key_or_value_cannot_be_printed() {
    let cases: &[(&str, &str, &str)] = &[
        ("portable", "A=1\nCONTROL=s3cret\u{1}\nZ=1\n", "\"CONTROL\""),
        (
            "portable",
            "A=1\nESCAPE=\u{1b}[31ms3cret\nZ=1\n",
            "\"ESCAPE\"",
        ),
        ("python", "A=1\nA~B=s3cret\nZ=1\n", "\"A~B\""),
        ("compose", "A=1\n\u{f1}=s3cret\nZ=1\n", "\"\u{f1}\""),
        ("docker", "A=1\nA\0B=s3cret\nZ=1\n", "\"A\\0B\""),
    ];
    for (dialect, source, needle) in cases {
        let dir = TempDir::new();
        fs::write(dir.path().join(".env"), source).unwrap();
        let output = run(&["--dialect", dialect, "load", dir.path().to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(2), "{source:?}");
        assert_eq!(stdout(&output), "", "{source:?}");
        assert!(stderr(&output).contains(needle), "{}", stderr(&output));
        assert!(!stderr(&output).contains("s3cret"), "{}", stderr(&output));
    }
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

#[cfg(unix)]
#[test]
fn run_reports_a_signaled_child_like_a_shell() {
    let dir = TempDir::new();
    fs::write(dir.path().join(".env"), "A=1\n").unwrap();
    for (signal, code) in [("TERM", 143), ("KILL", 137)] {
        let script = format!("kill -{signal} $$");
        let output = run(&[
            "run",
            dir.path().to_str().unwrap(),
            "--",
            "sh",
            "-c",
            &script,
        ]);
        assert_eq!(output.status.code(), Some(code), "SIG{signal}");
    }
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
    assert_eq!(output.status.code(), Some(2));
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
    assert_eq!(output.status.code(), Some(2));
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

/// Files shared by the exit code contract tests.
fn exit_code_fixture() -> TempDir {
    let dir = TempDir::new();
    dir.file_at("good.env", "A = 1\n");
    dir.file_at("bad.env", "BAD KEY=value\n");
    dir.file_at("unformatted.env", "A=1\n");
    dir.file_at("base.env", "A=1\n");
    dir.file_at("other.env", "A=2\n");
    dir.file_at("cascade/.env", "A=1\n");
    fs::write(dir.path().join("binary.env"), b"A=\xff\n").unwrap();
    fs::create_dir(dir.path().join("broken")).unwrap();
    fs::write(dir.path().join("broken/.env"), b"A=\xff\n").unwrap();
    dir.file_at("invalid/.env", "BAD KEY=value\n");
    dir
}

fn assert_exit_codes(dir: &Path, cases: &[(Vec<&str>, i32)]) {
    for (args, code) in cases {
        let output = run_in(dir, args);
        assert_eq!(
            output.status.code(),
            Some(*code),
            "{args:?}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn exit_code_contract_is_frozen() {
    let dir = exit_code_fixture();
    let cli = env!("CARGO_BIN_EXE_adotenv-cli");
    dir.file_at("writable.env", "A=1\n");
    assert_exit_codes(
        dir.path(),
        &[
            // 0: success.
            (vec!["check", "good.env"], 0),
            (vec!["fmt", "good.env"], 0),
            (vec!["fmt", "--check", "good.env"], 0),
            (vec!["fmt", "--write", "good.env"], 0),
            (vec!["diff", "base.env", "base.env"], 0),
            (vec!["example", "good.env"], 0),
            (vec!["get", "good.env", "A"], 0),
            (vec!["load", "cascade"], 0),
            (vec!["set", "writable.env", "A", "2"], 0),
            (vec!["run", "cascade", "--", cli, "--version"], 0),
            // 1: findings.
            (vec!["check", "bad.env"], 1),
            (vec!["check", "good.env", "bad.env"], 1),
            (vec!["fmt", "bad.env"], 1),
            (vec!["fmt", "--check", "unformatted.env"], 1),
            (vec!["diff", "base.env", "other.env"], 1),
            (vec!["get", "good.env", "MISSING"], 1),
            // 2: usage errors.
            (vec!["check", "--format", "nope", "good.env"], 2),
            (vec!["fmt", "--write", "--check", "good.env"], 2),
            // 2: read failures, for every command.
            (vec!["check", "missing.env"], 2),
            (vec!["check", "--format", "json", "missing.env"], 2),
            (vec!["check", "--format", "github", "missing.env"], 2),
            (vec!["check", "binary.env"], 2),
            (vec!["fmt", "missing.env"], 2),
            (vec!["fmt", "binary.env"], 2),
            (vec!["fmt", "--check", "missing.env"], 2),
            (vec!["fmt", "--write", "missing.env"], 2),
            (vec!["fmt", "--write", "cascade"], 2),
            (vec!["diff", "base.env", "missing.env"], 2),
            (vec!["diff", "missing.env", "base.env"], 2),
            (
                vec!["diff", "--format", "json", "base.env", "missing.env"],
                2,
            ),
            (
                vec!["diff", "--format", "matrix", "base.env", "missing.env"],
                2,
            ),
            (vec!["example", "missing.env"], 2),
            (vec!["get", "missing.env", "A"], 2),
            (vec!["load", "broken"], 2),
            (vec!["set", "missing.env", "A", "1"], 2),
            (vec!["run", "broken", "--", cli, "--version"], 2),
            // 2: inputs that a command cannot process.
            (vec!["diff", "base.env", "bad.env"], 2),
            (vec!["get", "bad.env", "A"], 2),
            (vec!["load", "invalid"], 2),
            (vec!["set", "bad.env", "A", "1"], 2),
            (vec!["run", "invalid", "--", cli, "--version"], 2),
            (
                vec![
                    "run",
                    "cascade",
                    "--",
                    "adotenv-command-that-does-not-exist",
                ],
                2,
            ),
            // 2 wins over 1 when one file has findings and another fails.
            (vec!["check", "bad.env", "missing.env"], 2),
            (vec!["check", "missing.env", "bad.env"], 2),
            (
                vec!["check", "--format", "json", "bad.env", "missing.env"],
                2,
            ),
            (vec!["fmt", "--check", "unformatted.env", "missing.env"], 2),
            (vec!["fmt", "bad.env", "missing.env"], 2),
        ],
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("writable.env")).unwrap(),
        "A=2\n"
    );
}

/// Makes a directory read-only and restores it when dropped.
#[cfg(unix)]
struct ReadOnlyDirectory(PathBuf);

#[cfg(unix)]
impl ReadOnlyDirectory {
    fn new(path: PathBuf) -> Self {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o555)).unwrap();
        Self(path)
    }
}

#[cfg(unix)]
impl Drop for ReadOnlyDirectory {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

#[cfg(unix)]
#[test]
fn exit_code_contract_covers_write_failures() {
    let dir = exit_code_fixture();
    dir.file_at("locked/app.env", "A=1\n");
    let locked = ReadOnlyDirectory::new(dir.path().join("locked"));
    if fs::File::create(locked.0.join("probe")).is_ok() {
        eprintln!("skipping write failure cases: permissions are not enforced");
        return;
    }
    assert_exit_codes(
        dir.path(),
        &[
            (vec!["fmt", "--write", "locked/app.env"], 2),
            (vec!["fmt", "--write", "bad.env", "locked/app.env"], 2),
            (vec!["set", "locked/app.env", "A", "2"], 2),
        ],
    );
    assert_eq!(
        fs::read_to_string(locked.0.join("app.env")).unwrap(),
        "A=1\n"
    );
}

#[cfg(unix)]
#[test]
fn exit_code_contract_covers_a_closed_output_pipe() {
    let dir = TempDir::new();
    let source: String = (0..20_000).map(|index| format!("K{index}=v\n")).collect();
    fs::write(dir.path().join(".env"), source).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_adotenv-cli"))
        .args(["load", dir.path().to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(stderr(&output), "");
}

#[cfg(unix)]
#[test]
fn exit_code_contract_rejects_non_utf8_arguments() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let output = Command::new(env!("CARGO_BIN_EXE_adotenv-cli"))
        .arg("check")
        .arg(OsStr::from_bytes(b"\xff.env"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("UTF-8"), "{}", stderr(&output));
}

#[cfg(target_os = "linux")]
#[test]
fn exit_code_contract_covers_standard_output_failures() {
    let dir = exit_code_fixture();
    let cases: &[(&[&str], i32)] = &[
        (&["--help"], 2),
        (&["--version"], 2),
        (&["check", "good.env"], 0),
        (&["check", "bad.env"], 2),
        (&["check", "--format", "json", "good.env"], 2),
        (&["check", "--format", "github", "bad.env"], 2),
        (&["fmt", "good.env"], 2),
        (&["fmt", "--check", "good.env"], 0),
        (&["fmt", "--check", "unformatted.env"], 2),
        (&["diff", "base.env", "other.env"], 2),
        (&["diff", "--format", "json", "base.env", "base.env"], 2),
        (&["diff", "--format", "matrix", "base.env", "other.env"], 2),
        (&["example", "good.env"], 2),
        (&["get", "good.env", "A"], 2),
        (&["load", "cascade"], 2),
    ];
    for (args, code) in cases {
        let full = fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_adotenv-cli"))
            .current_dir(dir.path())
            .args(*args)
            .stdout(full)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(*code),
            "{args:?}: {}",
            stderr(&output)
        );
        if *code == 2 {
            assert!(
                stderr(&output).contains("failed to write standard output"),
                "{args:?}: {}",
                stderr(&output)
            );
        }
    }
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
        (vec!["fmt", missing], 2),
        (vec!["example", missing], 2),
        (vec!["set", missing, "A", "1"], 2),
        (vec!["check", "--source", missing], 2),
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
    assert_eq!(output.status.code(), Some(2));
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

/// A parsed JSON value. Objects keep their fields in output order.
#[derive(Debug)]
enum Json {
    Null,
    Bool,
    Number,
    String,
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

/// Parse one complete JSON document, panicking on invalid input.
fn parse_json(text: &str) -> Json {
    struct Reader<'a> {
        bytes: &'a [u8],
        position: usize,
    }
    impl Reader<'_> {
        fn skip_whitespace(&mut self) {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                self.position += 1;
            }
        }
        fn peek(&self) -> Option<u8> {
            self.bytes.get(self.position).copied()
        }
        fn expect(&mut self, byte: u8) {
            self.skip_whitespace();
            assert_eq!(self.peek(), Some(byte), "at byte {}", self.position);
            self.position += 1;
        }
        fn literal(&mut self, word: &str) {
            assert!(self.bytes[self.position..].starts_with(word.as_bytes()));
            self.position += word.len();
        }
        fn string(&mut self) -> String {
            self.expect(b'"');
            let mut output = String::new();
            loop {
                let rest = std::str::from_utf8(&self.bytes[self.position..]).unwrap();
                let character = rest.chars().next().expect("unterminated string");
                self.position += character.len_utf8();
                match character {
                    '"' => return output,
                    '\\' => {
                        let escape = self.bytes[self.position];
                        self.position += 1;
                        match escape {
                            b'"' => output.push('"'),
                            b'\\' => output.push('\\'),
                            b'/' => output.push('/'),
                            b'b' => output.push('\u{8}'),
                            b'f' => output.push('\u{c}'),
                            b'n' => output.push('\n'),
                            b'r' => output.push('\r'),
                            b't' => output.push('\t'),
                            b'u' => {
                                let hex = std::str::from_utf8(
                                    &self.bytes[self.position..self.position + 4],
                                )
                                .unwrap();
                                self.position += 4;
                                let code = u32::from_str_radix(hex, 16).unwrap();
                                output.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                            }
                            other => panic!("invalid escape {other}"),
                        }
                    }
                    character => {
                        assert!(u32::from(character) >= 0x20, "raw control in string");
                        output.push(character);
                    }
                }
            }
        }
        fn value(&mut self) -> Json {
            self.skip_whitespace();
            match self.peek().expect("unexpected end") {
                b'{' => {
                    self.position += 1;
                    let mut fields = Vec::new();
                    self.skip_whitespace();
                    if self.peek() == Some(b'}') {
                        self.position += 1;
                        return Json::Object(fields);
                    }
                    loop {
                        self.skip_whitespace();
                        let key = self.string();
                        self.expect(b':');
                        fields.push((key, self.value()));
                        self.skip_whitespace();
                        match self.peek() {
                            Some(b',') => self.position += 1,
                            Some(b'}') => {
                                self.position += 1;
                                return Json::Object(fields);
                            }
                            other => panic!("unexpected {other:?} in object"),
                        }
                    }
                }
                b'[' => {
                    self.position += 1;
                    let mut items = Vec::new();
                    self.skip_whitespace();
                    if self.peek() == Some(b']') {
                        self.position += 1;
                        return Json::Array(items);
                    }
                    loop {
                        items.push(self.value());
                        self.skip_whitespace();
                        match self.peek() {
                            Some(b',') => self.position += 1,
                            Some(b']') => {
                                self.position += 1;
                                return Json::Array(items);
                            }
                            other => panic!("unexpected {other:?} in array"),
                        }
                    }
                }
                b'"' => {
                    self.string();
                    Json::String
                }
                b't' => {
                    self.literal("true");
                    Json::Bool
                }
                b'f' => {
                    self.literal("false");
                    Json::Bool
                }
                b'n' => {
                    self.literal("null");
                    Json::Null
                }
                _ => {
                    let start = self.position;
                    while matches!(
                        self.peek(),
                        Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                    ) {
                        self.position += 1;
                    }
                    assert!(self.position > start, "invalid value at byte {start}");
                    Json::Number
                }
            }
        }
    }
    let mut reader = Reader {
        bytes: text.as_bytes(),
        position: 0,
    };
    let value = reader.value();
    reader.skip_whitespace();
    assert_eq!(reader.position, text.len(), "trailing data");
    value
}

/// Field lists observed per object role, in output order.
type Observed = std::collections::BTreeMap<&'static str, Vec<Vec<String>>>;

fn observe<'a>(
    observed: &mut Observed,
    role: &'static str,
    value: &'a Json,
) -> &'a [(String, Json)] {
    let Json::Object(fields) = value else {
        panic!("{role} is not an object: {value:?}");
    };
    observed
        .entry(role)
        .or_default()
        .push(fields.iter().map(|(name, _)| name.clone()).collect());
    fields
}

fn field<'a>(fields: &'a [(String, Json)], name: &str) -> &'a Json {
    &fields.iter().find(|(key, _)| key == name).unwrap().1
}

fn items(value: &Json) -> &[Json] {
    let Json::Array(items) = value else {
        panic!("not an array: {value:?}");
    };
    items
}

/// Field names per object from the tables in `docs/design/cli.md`.
fn documented_fields(text: &str) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut documented = std::collections::BTreeMap::<String, Vec<String>>::new();
    for line in text.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 5 || !cells[0].is_empty() {
            continue;
        }
        let Some(name) = cells[2]
            .strip_prefix('`')
            .and_then(|cell| cell.strip_suffix('`'))
        else {
            continue;
        };
        documented
            .entry(cells[1].to_string())
            .or_default()
            .push(name.to_string());
    }
    documented
}

#[test]
fn json_fields_match_the_documented_schema() {
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/design/cli.md");
    let Ok(docs) = fs::read_to_string(docs) else {
        eprintln!("skipping schema test: docs/design/cli.md is not available");
        return;
    };
    let documented = documented_fields(&docs);

    let dir = TempDir::new();
    dir.file_at("good.env", "A=1\n");
    dir.file_at("findings.env", "A=1\nA=2\nBAD KEY=value\n");
    dir.file_at("base.env", "A\nB=\nC=1\nexport D=1\nE=1\n");
    dir.file_at("candidate.env", "A=1\nB\nC=2\nD=1\nF=1\n");
    let mut observed = Observed::new();

    let output = run_in(
        dir.path(),
        &["check", "--json", "good.env", "findings.env", "missing.env"],
    );
    assert_eq!(output.status.code(), Some(2));
    let json = parse_json(&stdout(&output));
    let report = observe(&mut observed, "check report", &json);
    for file in items(field(report, "files")) {
        let file = observe(&mut observed, "check file", file);
        for diagnostic in items(field(file, "diagnostics")) {
            let diagnostic = observe(&mut observed, "diagnostic", diagnostic);
            observe(&mut observed, "span", field(diagnostic, "span"));
            if let Json::Object(_) = field(diagnostic, "secondary_span") {
                observe(&mut observed, "span", field(diagnostic, "secondary_span"));
            }
        }
    }

    let output = run_in(
        dir.path(),
        &["diff", "--json", "base.env", "candidate.env", "base.env"],
    );
    assert_eq!(output.status.code(), Some(1));
    let json = parse_json(&stdout(&output));
    let report = observe(&mut observed, "diff report", &json);
    for file in items(field(report, "files")) {
        let file = observe(&mut observed, "diff file", file);
        for difference in items(field(file, "differences")) {
            let difference = observe(&mut observed, "difference", difference);
            for side in ["left", "right"] {
                if let Json::Object(_) = field(difference, side) {
                    observe(&mut observed, "value", field(difference, side));
                }
            }
        }
    }

    for (role, lists) in &observed {
        let expected = documented
            .get(*role)
            .unwrap_or_else(|| panic!("cli.md documents no fields for {role}"));
        let mut seen: Vec<&String> = lists.iter().flatten().collect();
        seen.sort();
        seen.dedup();
        let mut listed: Vec<&String> = expected.iter().collect();
        listed.sort();
        assert_eq!(seen, listed, "{role}: emitted and documented fields differ");
        for fields in lists {
            let ordered: Vec<&String> = expected
                .iter()
                .filter(|name| fields.contains(name))
                .collect();
            assert_eq!(
                fields.iter().collect::<Vec<_>>(),
                ordered,
                "{role}: fields are not in documented order"
            );
        }
    }
    let documented_roles: Vec<&String> = documented.keys().collect();
    let observed_roles: Vec<&str> = observed.keys().copied().collect();
    assert_eq!(documented_roles, observed_roles, "documented objects");
}
