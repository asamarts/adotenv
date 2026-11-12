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
        Some("speed") => command_speed(&args[1..]),
        Some("cli") => command_cli(&args[1..]),
        Some("report") => command_report(&args[1..]),
        Some("compare") => command_compare(&args[1..]),
        Some("gate") => command_gate(&args[1..]),
        Some("fingerprint") => command_fingerprint(),
        Some("queue") => command_queue(&args[1..]),
        Some("queue-speed") => command_queue_speed(&args[1..]),
        Some("help" | "--help" | "-h") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command: {other}\n\n{USAGE}")),
    }
}

const USAGE: &str =
    "adotenv-xtask <fixtures|memory|micro|speed|cli|report|compare|fingerprint|queue|queue-speed> [options]

fixtures
memory  --out <path> [--sizes 8,128] [--shapes plain,...] [--modes values,...] [--compact]
micro   [--bench fixtures|competitors] [--quick]
speed   [--out <path>] [--bench competitors] [--filter <text>] [--quick]
cli     [--out <path>] [--size <label>] [--shape <label>]
report  --input <path> [--out <path>]
compare --baseline <path> --current <path> [--out <path>]
gate    --baseline <path> --current <path> [--max-heap-percent 5]
        [--max-heap-bytes 64] [--max-allocation-delta 0]
fingerprint
queue   --remote-root <path> [--out <path>] [--remote-out <path>]
        [--sizes ...] [--shapes ...] [--modes ...] [--compact] [--sync] [--dry-run]
queue-speed --remote-root <path> [--out <path>] [--remote-out <path>]
        [--bench competitors] [--filter <text>] [--quick] [--sample-size N]
        [--warm-up-time SECS] [--measurement-time SECS] [--sync] [--dry-run]";

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
    let compact = has_flag(args, "--compact");
    let out = option(args, "--out").unwrap_or(if compact {
        "docs/benchmarks/compact-memory.jsonl"
    } else {
        "docs/benchmarks/memory.jsonl"
    });
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
                for implementation in implementations(compact) {
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

fn command_speed(args: &[String]) -> Result<()> {
    let out = PathBuf::from(option(args, "--out").unwrap_or("docs/benchmarks/speed.txt"));
    let bench = option(args, "--bench").unwrap_or("competitors");
    ensure_parent(&out)?;
    let binary = build_bench_binary(bench)?;
    let mut command = Command::new(binary);
    command.args(["--bench", "--output-format", "bencher", "--noplot"]);
    if has_flag(args, "--quick") {
        command.arg("--quick");
    }
    if let Some(filter) = option(args, "--filter") {
        command.arg(filter);
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "criterion exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    fs::write(&out, &output.stdout).map_err(|error| error.to_string())?;
    let sidecar = out.with_extension("fingerprint.json");
    fs::write(&sidecar, fingerprint_json()).map_err(|error| error.to_string())?;
    write_speed_markdown(&out)?;
    println!("wrote {} and {}", out.display(), sidecar.display());
    Ok(())
}

fn write_speed_markdown(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut report = String::from("# Speed benchmark results\n\n");
    let sidecar = path.with_extension("fingerprint.json");
    if let Ok(fingerprint) = fs::read_to_string(&sidecar) {
        report.push_str("## Fingerprint\n\n```json\n");
        report.push_str(fingerprint.trim());
        report.push_str("\n```\n\n");
    }
    report.push_str("| benchmark | time |\n|---|---:|\n");
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("test ") else {
            continue;
        };
        let Some((name, value)) = rest.split_once(" ... bench:") else {
            continue;
        };
        writeln!(report, "| {} | {} |", name.trim(), value.trim())
            .expect("writing to String cannot fail");
    }
    let markdown = path.with_extension("md");
    fs::write(&markdown, report).map_err(|error| error.to_string())?;
    println!("wrote {}", markdown.display());
    Ok(())
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
    let mut report = String::from("# Memory benchmark results\n\n");
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

fn command_gate(args: &[String]) -> Result<()> {
    let baseline = option(args, "--baseline").ok_or("missing --baseline")?;
    let current = option(args, "--current").ok_or("missing --current")?;
    let max_heap_percent = option(args, "--max-heap-percent")
        .unwrap_or("5")
        .parse::<u64>()
        .map_err(|error| format!("invalid --max-heap-percent: {error}"))?;
    let max_heap_bytes = option(args, "--max-heap-bytes")
        .unwrap_or("64")
        .parse::<u64>()
        .map_err(|error| format!("invalid --max-heap-bytes: {error}"))?;
    let max_allocation_delta = option(args, "--max-allocation-delta")
        .unwrap_or("0")
        .parse::<u64>()
        .map_err(|error| format!("invalid --max-allocation-delta: {error}"))?;
    let baseline = read_rows(baseline)?;
    let current = read_rows(current)?;
    let violations = check_memory_gate(
        &baseline,
        &current,
        max_heap_percent,
        max_heap_bytes,
        max_allocation_delta,
    );
    if violations.is_empty() {
        let checked = baseline
            .values()
            .filter(|row| field(row, "implementation") == "adotenv")
            .count();
        println!("memory gate: {checked} adotenv cases passed");
        Ok(())
    } else {
        for violation in &violations {
            eprintln!("{violation}");
        }
        Err(format!(
            "memory gate failed with {} violation(s)",
            violations.len()
        ))
    }
}

fn check_memory_gate(
    baseline: &BTreeMap<String, serde_json::Value>,
    current: &BTreeMap<String, serde_json::Value>,
    max_heap_percent: u64,
    max_heap_bytes: u64,
    max_allocation_delta: u64,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (key, base) in baseline {
        if field(base, "implementation") != "adotenv" {
            continue;
        }
        let Some(cur) = current.get(key) else {
            violations.push(format!("{key}: missing current case"));
            continue;
        };
        if let (Some(base_hash), Some(cur_hash)) = (
            base.get("fixture_sha256")
                .and_then(serde_json::Value::as_str),
            cur.get("fixture_sha256")
                .and_then(serde_json::Value::as_str),
        ) {
            if base_hash != cur_hash {
                violations.push(format!(
                    "{key}: fixture hash changed ({base_hash} -> {cur_hash})"
                ));
            }
        }
        for metric in ["peak_heap_bytes", "retained_heap_bytes"] {
            let Some(base_value) = unsigned(base, metric) else {
                continue;
            };
            let Some(cur_value) = unsigned(cur, metric) else {
                violations.push(format!("{key}: missing current {metric}"));
                continue;
            };
            let percent_delta = base_value.saturating_mul(max_heap_percent) / 100;
            let allowed = base_value
                .saturating_add(percent_delta)
                .max(base_value.saturating_add(max_heap_bytes));
            if cur_value > allowed {
                violations.push(format!(
                    "{key}: {metric} {cur_value} > allowed {allowed} (baseline {base_value}, +{max_heap_percent}% or +{max_heap_bytes} B)"
                ));
            }
        }
        if let (Some(base_value), Some(cur_value)) =
            (unsigned(base, "allocations"), unsigned(cur, "allocations"))
        {
            let allowed = base_value.saturating_add(max_allocation_delta);
            if cur_value > allowed {
                violations.push(format!(
                    "{key}: allocations {cur_value} > allowed {allowed} (baseline {base_value}, +{max_allocation_delta})"
                ));
            }
        }
    }
    violations
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

fn unsigned(row: &serde_json::Value, name: &str) -> Option<u64> {
    row.get(name).and_then(serde_json::Value::as_u64)
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
    validate_remote_root(remote_root)?;
    let compact = has_flag(args, "--compact");
    let local_out = option(args, "--out").unwrap_or(if compact {
        "docs/benchmarks/compact-memory.jsonl"
    } else {
        "docs/benchmarks/memory.jsonl"
    });
    let remote_out = option(args, "--remote-out").unwrap_or(if compact {
        "bench-compact-memory.jsonl"
    } else {
        "bench-memory.jsonl"
    });
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
                for implementation in implementations(compact) {
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

fn command_queue_speed(args: &[String]) -> Result<()> {
    let remote_root = option(args, "--remote-root").ok_or("missing --remote-root")?;
    validate_remote_root(remote_root)?;
    let local_out = option(args, "--out").unwrap_or("docs/benchmarks/speed.txt");
    let remote_out = option(args, "--remote-out").unwrap_or("bench-speed.txt");
    let bench = option(args, "--bench").unwrap_or("competitors");
    if !bench
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("--bench must be a simple target name".to_owned());
    }
    if has_flag(args, "--sync") && !has_flag(args, "--dry-run") {
        sync_speed_binaries(remote_root, bench)?;
    }
    let git_sha = command_output("git", &["rev-parse", "HEAD"]);
    let rustc = command_output("rustc", &["--version"]);
    let remote_fingerprint = Path::new(remote_out).with_extension("fingerprint.json");
    let mut bench_args = vec!["--bench", "--output-format", "bencher", "--noplot"];
    if has_flag(args, "--quick") {
        bench_args.push("--quick");
    }
    if let Some(filter) = option(args, "--filter") {
        bench_args.push(filter);
    }
    for (name, flag) in [
        ("--sample-size", "--sample-size"),
        ("--warm-up-time", "--warm-up-time"),
        ("--measurement-time", "--measurement-time"),
    ] {
        if let Some(value) = option(args, name) {
            bench_args.push(flag);
            bench_args.push(value);
        }
    }
    let bench_args = bench_args
        .iter()
        .map(|argument| shell_quote(argument))
        .collect::<Vec<_>>()
        .join(" ");
    let remote_command = format!(
        "cd {} && ADOTENV_GIT_SHA={} ADOTENV_RUSTC={} ./adotenv-xtask fingerprint > {} && ./{} {} > {}",
        shell_quote(remote_root),
        shell_quote(&git_sha),
        shell_quote(&rustc),
        shell_quote(&remote_fingerprint.to_string_lossy()),
        shell_quote(bench),
        bench_args,
        shell_quote(remote_out)
    );
    let queue_command = format!(
        "PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml pueue add --group bench --working-directory {} --label adotenv-speed --print-task-id -- {}",
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
    write_speed_markdown(Path::new(local_out))?;
    println!("copied {local_out} and {}", local_fingerprint.display());
    Ok(())
}

fn sync_speed_binaries(remote_root: &str, bench: &str) -> Result<()> {
    if !command_output("git", &["status", "--porcelain"])
        .trim()
        .is_empty()
    {
        return Err("--sync requires a clean working tree".to_owned());
    }
    let bench_binary = build_bench_binary(bench)?;
    let xtask = build_xtask_binary()?;
    prepare_remote_root(remote_root)?;
    copy_binary(remote_root, &bench_binary, bench)?;
    copy_binary(remote_root, &xtask, "adotenv-xtask")?;
    Ok(())
}

fn validate_remote_root(remote_root: &str) -> Result<()> {
    if remote_root.is_empty()
        || remote_root == "/"
        || !Path::new(remote_root).is_absolute()
        || remote_root.contains('~')
    {
        return Err("--remote-root must be an absolute, non-root, non-home path".to_owned());
    }
    Ok(())
}

fn prepare_remote_root(remote_root: &str) -> Result<()> {
    ssh(&format!(
        "sudo -n -u benchq mkdir -p {} && sudo -n -u benchq chmod 750 {}",
        shell_quote(remote_root),
        shell_quote(remote_root)
    ))?;
    Ok(())
}

fn copy_binary(remote_root: &str, binary: &Path, name: &str) -> Result<()> {
    let staging = format!("/var/tmp/adotenv-{}-{name}", std::process::id());
    let status = Command::new("scp")
        .arg(binary)
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
    Ok(())
}

fn build_bench_binary(bench: &str) -> Result<PathBuf> {
    let output = Command::new(cargo())
        .args([
            "bench",
            "-p",
            "adotenv-bench",
            "--bench",
            bench,
            "--no-run",
            "--locked",
            "--message-format=json",
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let mut executable = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("reason").and_then(serde_json::Value::as_str) == Some("compiler-artifact")
            && value
                .get("target")
                .and_then(|target| target.get("name"))
                .and_then(serde_json::Value::as_str)
                == Some(bench)
        {
            if let Some(path) = value.get("executable").and_then(serde_json::Value::as_str) {
                executable = Some(PathBuf::from(path));
            }
        }
    }
    executable.ok_or_else(|| format!("cargo did not report an executable for {bench}"))
}

fn build_xtask_binary() -> Result<PathBuf> {
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
    Ok(root.join("target").join("release").join(if cfg!(windows) {
        "adotenv-xtask.exe"
    } else {
        "adotenv-xtask"
    }))
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

fn implementations(compact: bool) -> Vec<Implementation> {
    let mut implementations = vec![Implementation::Adotenv];
    if compact {
        implementations.push(Implementation::AdotenvCompact);
    }
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

    #[test]
    fn memory_gate_detects_allocation_and_heap_regressions() {
        let key = "8|plain|values|adotenv";
        let base = serde_json::json!({
            "size": "8", "shape": "plain", "mode": "values",
            "implementation": "adotenv", "peak_heap_bytes": 1000,
            "retained_heap_bytes": 800, "allocations": 10
        });
        let regressed = serde_json::json!({
            "size": "8", "shape": "plain", "mode": "values",
            "implementation": "adotenv", "peak_heap_bytes": 1000,
            "retained_heap_bytes": 800, "allocations": 11
        });
        let mut baseline = BTreeMap::new();
        baseline.insert(key.to_owned(), base);
        let mut current = BTreeMap::new();
        current.insert(key.to_owned(), regressed);
        assert_eq!(check_memory_gate(&baseline, &current, 5, 0, 0).len(), 1);
    }
}
