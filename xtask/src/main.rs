//! Internal benchmark automation for adotenv.

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use adotenv_bench::competitors::Competitor;
use adotenv_bench::memory::{Implementation, Mode};
use adotenv_bench::{fixtures, Shape, Size};

type Result<T> = std::result::Result<T, String>;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fixtures") => command_fixtures(&args[1..]),
        Some("memory") => command_memory(&args[1..]),
        Some("micro") => command_micro(&args[1..]),
        Some("cli") => command_cli(&args[1..]),
        Some("report") => command_report(&args[1..]),
        Some("compare") => command_compare(&args[1..]),
        Some("fingerprint") => command_fingerprint(),
        Some("queue") => command_queue(&args[1..]),
        Some("help" | "--help" | "-h") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command: {other}\n\n{USAGE}")),
    }
}

const USAGE: &str =
    "adotenv-xtask <fixtures|memory|micro|cli|report|compare|fingerprint|queue> [options]

fixtures
memory  --out <path> [--sizes 8,128] [--shapes plain,...] [--modes values,...]
micro   [--bench fixtures|competitors] [--quick]
cli     [--out <path>] [--size <label>] [--shape <label>]
report  --input <path> [--out <path>]
compare --baseline <path> --current <path> [--out <path>]
fingerprint
queue   --remote-root <path> [--out <path>] [--remote-out <path>]
        [--sizes ...] [--shapes ...] [--modes ...] [--sync] [--dry-run]";

#[allow(clippy::unnecessary_wraps)]
fn command_fixtures(_args: &[String]) -> Result<()> {
    for fixture in fixtures() {
        println!(
            "{}\t{}\t{}\t{}",
            fixture.name(),
            fixture.sha256(),
            fixture.entries(),
            fixture.bytes()
        );
    }
    Ok(())
}

fn command_memory(args: &[String]) -> Result<()> {
    let out = option(args, "--out").unwrap_or("docs/benchmarks/memory.jsonl");
    let sizes = option(args, "--sizes").unwrap_or("8,128");
    let shapes = option(args, "--shapes").unwrap_or("plain");
    let modes = option(args, "--modes").unwrap_or("values,load,document,resolve");
    let out = PathBuf::from(out);
    ensure_parent(&out)?;
    let sizes = parse_sizes(sizes)?;
    let shapes = parse_shapes(shapes)?;
    let modes = parse_modes(modes)?;
    let memory = build_memory_binary()?;
    let mut lines = String::new();
    for size in sizes {
        for shape in &shapes {
            for mode in &modes {
                for implementation in implementations() {
                    if !implementation.supports(*mode) {
                        continue;
                    }
                    let output = Command::new(&memory)
                        .args([
                            "--mode",
                            mode.label(),
                            "--implementation",
                            implementation.label(),
                            "--size",
                            size.label(),
                            "--shape",
                            shape.label(),
                        ])
                        .output()
                        .map_err(|error| error.to_string())?;
                    if !output.status.success() {
                        return Err(format!(
                            "memory case failed: {} {} {} {}: {}",
                            size.label(),
                            shape.label(),
                            mode.label(),
                            implementation.label(),
                            String::from_utf8_lossy(&output.stderr)
                        ));
                    }
                    lines.push_str(String::from_utf8_lossy(&output.stdout).trim());
                    lines.push('\n');
                }
            }
        }
    }
    fs::write(&out, &lines).map_err(|error| error.to_string())?;
    let sidecar = out.with_extension("fingerprint.json");
    fs::write(&sidecar, fingerprint_json()).map_err(|error| error.to_string())?;
    println!("wrote {} and {}", out.display(), sidecar.display());
    Ok(())
}

fn command_micro(args: &[String]) -> Result<()> {
    let bench = option(args, "--bench").unwrap_or("fixtures");
    let mut command = Command::new(cargo());
    command.args(["bench", "-p", "adotenv-bench", "--bench", bench, "--locked"]);
    if has_flag(args, "--quick") {
        command.args(["--", "--quick"]);
    }
    let status = command.status().map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo bench exited with {status}"))
    }
}

fn command_cli(args: &[String]) -> Result<()> {
    let out = PathBuf::from(option(args, "--out").unwrap_or("docs/benchmarks/cli.json"));
    let size = Size::from_label(option(args, "--size").unwrap_or("128")).ok_or("unknown size")?;
    let shape =
        Shape::from_label(option(args, "--shape").unwrap_or("plain")).ok_or("unknown shape")?;
    ensure_parent(&out)?;
    let status = Command::new(cargo())
        .args(["build", "--release", "-p", "adotenv-cli", "--locked"])
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("building adotenv-cli exited with {status}"));
    }
    let root = workspace_root()?;
    let binary = root.join("target").join("release").join(if cfg!(windows) {
        "adotenv-cli.exe"
    } else {
        "adotenv-cli"
    });
    let fixture = adotenv_bench::fixture(size, shape);
    let path = root.join("target").join("bench-cli.env");
    fs::write(&path, fixture.source()).map_err(|error| error.to_string())?;
    let command = format!(
        "{} check {}",
        shell_quote(&binary.to_string_lossy()),
        shell_quote(&path.to_string_lossy())
    );
    let status = Command::new("hyperfine")
        .args(["--warmup", "2", "--min-runs", "5", "--export-json"])
        .arg(&out)
        .args(["--command-name", "adotenv-cli check"])
        .arg(command)
        .status()
        .map_err(|error| error.to_string())?;
    let _ = fs::remove_file(path);
    if !status.success() {
        return Err(format!("hyperfine exited with {status}"));
    }
    let sidecar = out.with_extension("fingerprint.json");
    fs::write(&sidecar, fingerprint_json()).map_err(|error| error.to_string())?;
    println!("wrote {} and {}", out.display(), sidecar.display());
    Ok(())
}

fn command_report(args: &[String]) -> Result<()> {
    let input = option(args, "--input").ok_or("missing --input")?;
    let output = option(args, "--out");
    let text = fs::read_to_string(input).map_err(|error| error.to_string())?;
    let mut rows = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|error| error.to_string())?;
        rows.push(value);
    }
    let mut report = String::new();
    let sidecar = Path::new(input).with_extension("fingerprint.json");
    if let Ok(fingerprint) = fs::read_to_string(&sidecar) {
        report.push_str("## Fingerprint\n\n```json\n");
        report.push_str(fingerprint.trim());
        report.push_str("\n```\n\n");
    }
    report.push_str(
        "| size | shape | mode | implementation | entries | source bytes | peak heap | retained heap | after drop | allocations | RSS before | RSS peak | RSS after |\n\
         |---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
    );
    for row in rows {
        writeln!(
            report,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            field(&row, "size"),
            field(&row, "shape"),
            field(&row, "mode"),
            field(&row, "implementation"),
            field(&row, "entries"),
            field(&row, "source_bytes"),
            field(&row, "peak_heap_bytes"),
            field(&row, "retained_heap_bytes"),
            field(&row, "heap_after_drop_bytes"),
            field(&row, "allocations"),
            field(&row, "rss_before_bytes"),
            field(&row, "rss_peak_bytes"),
            field(&row, "rss_after_bytes")
        )
        .expect("writing to String cannot fail");
    }
    if let Some(output) = output {
        let output = Path::new(output);
        ensure_parent(output)?;
        fs::write(output, report).map_err(|error| error.to_string())?;
        println!("wrote {}", output.display());
    } else {
        print!("{report}");
    }
    Ok(())
}

fn command_compare(args: &[String]) -> Result<()> {
    let baseline = option(args, "--baseline").ok_or("missing --baseline")?;
    let current = option(args, "--current").ok_or("missing --current")?;
    let baseline = read_rows(baseline)?;
    let current = read_rows(current)?;
    let mut report = String::from(
        "| size | shape | mode | implementation | metric | baseline | current | delta |\n\
         |---|---|---|---|---:|---:|---:|\n",
    );
    for (key, row) in &current {
        let Some(base) = baseline.get(key) else {
            continue;
        };
        for metric in [
            "peak_heap_bytes",
            "retained_heap_bytes",
            "heap_after_drop_bytes",
            "allocations",
        ] {
            let Some(base_value) = numeric(base, metric) else {
                continue;
            };
            let Some(current_value) = numeric(row, metric) else {
                continue;
            };
            let delta = if base_value == 0.0 {
                "n/a".to_owned()
            } else {
                format!("{:+.2}%", (current_value - base_value) / base_value * 100.0)
            };
            writeln!(
                report,
                "| {} | {} | {} | {} | {} | {} | {} | {} |",
                field(row, "size"),
                field(row, "shape"),
                field(row, "mode"),
                field(row, "implementation"),
                metric,
                base_value,
                current_value,
                delta
            )
            .expect("writing to String cannot fail");
        }
    }
    if let Some(output) = option(args, "--out") {
        let output = Path::new(output);
        ensure_parent(output)?;
        fs::write(output, report).map_err(|error| error.to_string())?;
        println!("wrote {}", output.display());
    } else {
        print!("{report}");
    }
    Ok(())
}

fn read_rows(path: &str) -> Result<BTreeMap<String, serde_json::Value>> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut rows = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|error| error.to_string())?;
        let key = format!(
            "{}|{}|{}|{}",
            field(&value, "size"),
            field(&value, "shape"),
            field(&value, "mode"),
            field(&value, "implementation")
        );
        rows.insert(key, value);
    }
    Ok(rows)
}

fn field(row: &serde_json::Value, name: &str) -> String {
    match row.get(name) {
        Some(serde_json::Value::String(value)) => value.clone(),
        Some(serde_json::Value::Null) | None => "-".to_owned(),
        Some(value) => value.to_string(),
    }
}

fn numeric(row: &serde_json::Value, name: &str) -> Option<f64> {
    row.get(name).and_then(serde_json::Value::as_f64)
}

#[allow(clippy::unnecessary_wraps)]
fn command_fingerprint() -> Result<()> {
    println!("{}", fingerprint_json());
    Ok(())
}

fn cpu_count() -> usize {
    fs::read_to_string("/proc/cpuinfo").map_or(0, |contents| {
        contents
            .lines()
            .filter(|line| line.starts_with("processor"))
            .count()
    })
}

fn fingerprint_json() -> String {
    let cpu = fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|contents| {
            contents
                .lines()
                .find_map(|line| line.strip_prefix("model name\t:"))
                .map(str::trim)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_owned());
    let load = fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|contents| contents.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned());
    serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "hostname": command_output("hostname", &[]),
        "kernel": command_output("uname", &["-sr"]),
        "cpu": cpu,
        "cpu_count": cpu_count(),
        "load1": load,
        "rustc": env::var("ADOTENV_RUSTC")
            .ok()
            .filter(|version| !version.trim().is_empty())
            .unwrap_or_else(|| command_output("rustc", &["--version"])),
        "git_sha": env::var("ADOTENV_GIT_SHA")
            .ok()
            .filter(|sha| !sha.trim().is_empty())
            .unwrap_or_else(|| command_output("git", &["rev-parse", "HEAD"])),
        "hyperfine": command_output("hyperfine", &["--version"]),
    })
    .to_string()
}

fn command_output(program: &str, arguments: &[&str]) -> String {
    Command::new(program)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(
            || "unknown".to_owned(),
            |output| String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        )
}

fn command_queue(args: &[String]) -> Result<()> {
    let remote_root = option(args, "--remote-root").ok_or("missing --remote-root")?;
    if remote_root.is_empty()
        || remote_root == "/"
        || !Path::new(remote_root).is_absolute()
        || remote_root.contains('~')
    {
        return Err("--remote-root must be an absolute, non-root, non-home path".to_owned());
    }
    let local_out = option(args, "--out").unwrap_or("docs/benchmarks/memory.jsonl");
    let remote_out = option(args, "--remote-out").unwrap_or("bench-memory.jsonl");
    let sizes = option(args, "--sizes").unwrap_or("8,128");
    let shapes = option(args, "--shapes").unwrap_or("plain");
    let modes = option(args, "--modes").unwrap_or("values,load,document,resolve");
    if has_flag(args, "--sync") && !has_flag(args, "--dry-run") {
        sync_binaries(remote_root)?;
    }
    let git_sha = command_output("git", &["rev-parse", "HEAD"]);
    let rustc = command_output("rustc", &["--version"]);
    let remote_fingerprint = Path::new(remote_out).with_extension("fingerprint.json");
    let mut memory_commands = Vec::new();
    for size in parse_sizes(sizes)? {
        for shape in parse_shapes(shapes)? {
            for mode in parse_modes(modes)? {
                for implementation in implementations() {
                    if !implementation.supports(mode) {
                        continue;
                    }
                    memory_commands.push(format!(
                        "./memory --mode {} --implementation {} --size {} --shape {}",
                        shell_quote(mode.label()),
                        shell_quote(implementation.label()),
                        shell_quote(size.label()),
                        shell_quote(shape.label())
                    ));
                }
            }
        }
    }
    let remote_command = format!(
        "cd {} && ADOTENV_GIT_SHA={} ADOTENV_RUSTC={} ./adotenv-xtask fingerprint > {} && ({} ) > {}",
        shell_quote(remote_root),
        shell_quote(&git_sha),
        shell_quote(&rustc),
        shell_quote(&remote_fingerprint.to_string_lossy()),
        memory_commands.join(" && "),
        shell_quote(remote_out)
    );
    let queue_command = format!(
        "PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml pueue add --group bench --working-directory {} --label adotenv-bench --print-task-id -- {}",
        shell_quote(remote_root),
        shell_quote(&remote_command)
    );
    if has_flag(args, "--dry-run") {
        println!("ssh kbench {queue_command}");
        return Ok(());
    }
    let task = ssh(&queue_command)?;
    let task = task.trim();
    if task.is_empty() {
        return Err("pueue did not return a task id".to_owned());
    }
    println!("queued kbench task {task}");
    ssh(&format!(
        "PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml pueue wait {task}"
    ))?;
    ensure_task_succeeded(task)?;
    ensure_parent(Path::new(local_out))?;
    let status = Command::new("scp")
        .arg(format!("kbench:{remote_root}/{remote_out}"))
        .arg(local_out)
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("scp exited with {status}"));
    }
    let local_fingerprint = Path::new(local_out).with_extension("fingerprint.json");
    let remote_fingerprint = Path::new(remote_out).with_extension("fingerprint.json");
    let status = Command::new("scp")
        .arg(format!(
            "kbench:{remote_root}/{}",
            remote_fingerprint.display()
        ))
        .arg(&local_fingerprint)
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("copying fingerprint exited with {status}"));
    }
    println!("copied {local_out} and {}", local_fingerprint.display());
    Ok(())
}

fn sync_binaries(remote_root: &str) -> Result<()> {
    if !command_output("git", &["status", "--porcelain"])
        .trim()
        .is_empty()
    {
        return Err("--sync requires a clean working tree".to_owned());
    }
    let memory = build_memory_binary()?;
    let status = Command::new(cargo())
        .args([
            "build",
            "--release",
            "-p",
            "adotenv-xtask",
            "--bin",
            "adotenv-xtask",
            "--locked",
        ])
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("building adotenv-xtask exited with {status}"));
    }
    let root = workspace_root()?;
    let xtask = root.join("target").join("release").join(if cfg!(windows) {
        "adotenv-xtask.exe"
    } else {
        "adotenv-xtask"
    });
    ssh(&format!(
        "sudo -n -u benchq mkdir -p {} && sudo -n -u benchq chmod 750 {}",
        shell_quote(remote_root),
        shell_quote(remote_root)
    ))?;
    for binary in [memory, xtask] {
        let name = binary.file_name().ok_or("binary has no name")?;
        let name = name.to_string_lossy();
        let staging = format!("/var/tmp/adotenv-{}-{name}", std::process::id());
        let status = Command::new("scp")
            .arg(&binary)
            .arg(format!("kbench:{staging}"))
            .status()
            .map_err(|error| error.to_string())?;
        if !status.success() {
            return Err(format!("scp {} exited with {status}", binary.display()));
        }
        let destination = format!("{remote_root}/{name}");
        ssh(&format!(
            "sudo -n -u benchq install -m 755 {} {}; status=$?; rm -f {}; exit $status",
            shell_quote(&staging),
            shell_quote(&destination),
            shell_quote(&staging)
        ))?;
    }
    Ok(())
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn workspace_root() -> Result<&'static Path> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask has no parent".to_owned())
}

fn ensure_task_succeeded(task: &str) -> Result<()> {
    let status = ssh("PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml pueue status --json")?;
    let value: serde_json::Value =
        serde_json::from_str(&status).map_err(|error| format!("parsing pueue status: {error}"))?;
    let result = value
        .get("tasks")
        .and_then(|tasks| tasks.get(task))
        .and_then(|task| task.get("status"))
        .and_then(|status| status.get("Done"))
        .and_then(|done| done.get("result"))
        .and_then(serde_json::Value::as_str);
    if result == Some("Success") {
        Ok(())
    } else {
        Err(format!(
            "kbench task {task} did not succeed (result: {})",
            result.unwrap_or("unknown")
        ))
    }
}

fn build_memory_binary() -> Result<PathBuf> {
    let status = Command::new(cargo())
        .args([
            "build",
            "--release",
            "-p",
            "adotenv-bench",
            "--bin",
            "memory",
            "--locked",
        ])
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("building memory binary exited with {status}"));
    }
    let root = workspace_root()?;
    let name = if cfg!(windows) {
        "memory.exe"
    } else {
        "memory"
    };
    Ok(root.join("target").join("release").join(name))
}

fn implementations() -> Vec<Implementation> {
    let mut implementations = vec![Implementation::Adotenv];
    implementations.extend(Competitor::ALL.into_iter().map(Implementation::Competitor));
    implementations
}

fn parse_sizes(value: &str) -> Result<Vec<Size>> {
    value
        .split(',')
        .map(|label| Size::from_label(label.trim()).ok_or_else(|| format!("unknown size: {label}")))
        .collect()
}

fn parse_shapes(value: &str) -> Result<Vec<Shape>> {
    value
        .split(',')
        .map(|label| {
            Shape::from_label(label.trim()).ok_or_else(|| format!("unknown shape: {label}"))
        })
        .collect()
}

fn parse_modes(value: &str) -> Result<Vec<Mode>> {
    value
        .split(',')
        .map(|label| Mode::from_label(label.trim()).ok_or_else(|| format!("unknown mode: {label}")))
        .collect()
}

fn option<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|argument| argument == name)
}

fn cargo() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}

fn ssh(command: &str) -> Result<String> {
    let output = Command::new("ssh")
        .arg("kbench")
        .arg(command)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes_and_modes() {
        assert_eq!(
            parse_sizes("8,128").unwrap(),
            vec![Size::Eight, Size::Typical]
        );
        assert_eq!(
            parse_modes("values,load").unwrap(),
            vec![Mode::Values, Mode::Load]
        );
    }

    #[test]
    fn shell_quotes_apostrophes() {
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn renders_json_fields_without_quotes() {
        let row = serde_json::json!({"size": "8", "peak_heap_bytes": 632, "missing": null});
        assert_eq!(field(&row, "size"), "8");
        assert_eq!(field(&row, "peak_heap_bytes"), "632");
        assert_eq!(field(&row, "missing"), "-");
        assert_eq!(numeric(&row, "peak_heap_bytes"), Some(632.0));
        assert_eq!(numeric(&row, "size"), None);
    }

    #[test]
    fn parses_comparable_shapes() {
        assert_eq!(
            parse_shapes("plain,long-line,empty").unwrap(),
            vec![Shape::Plain, Shape::LongLine, Shape::Empty]
        );
        assert!(parse_shapes("not-a-shape").is_err());
    }
}
