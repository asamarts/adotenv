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

const KNOWN_COMMANDS: &[&str] = &[
    "fixtures",
    "memory",
    "micro",
    "speed",
    "cli",
    "report",
    "compare",
    "gate",
    "fingerprint",
    "queue",
    "queue-speed",
];

enum CommandAction<'a> {
    Help,
    Dispatch(&'a str, &'a [String]),
}

fn classify_command(args: &[String]) -> Result<CommandAction<'_>> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(CommandAction::Help);
    };
    let rest = &args[1..];
    if matches!(command, "help" | "--help" | "-h") {
        if let Some(extra) = rest.first() {
            return Err(format!("unexpected argument for {command}: {extra}"));
        }
        return Ok(CommandAction::Help);
    }
    if !KNOWN_COMMANDS.contains(&command) {
        return Err(format!("unknown command: {command}\n\n{USAGE}"));
    }
    if rest
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return Ok(CommandAction::Help);
    }
    Ok(CommandAction::Dispatch(command, rest))
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    match classify_command(&args)? {
        CommandAction::Help => {
            println!("{USAGE}");
            Ok(())
        }
        CommandAction::Dispatch("fixtures", rest) => command_fixtures(rest),
        CommandAction::Dispatch("memory", rest) => command_memory(rest),
        CommandAction::Dispatch("micro", rest) => command_micro(rest),
        CommandAction::Dispatch("speed", rest) => command_speed(rest),
        CommandAction::Dispatch("cli", rest) => command_cli(rest),
        CommandAction::Dispatch("report", rest) => command_report(rest),
        CommandAction::Dispatch("compare", rest) => command_compare(rest),
        CommandAction::Dispatch("gate", rest) => command_gate(rest),
        CommandAction::Dispatch("fingerprint", rest) => command_fingerprint(rest),
        CommandAction::Dispatch("queue", rest) => command_queue(rest),
        CommandAction::Dispatch("queue-speed", rest) => command_queue_speed(rest),
        CommandAction::Dispatch(other, _) => Err(format!("unknown command: {other}\n\n{USAGE}")),
    }
}

const USAGE: &str =
    "adotenv-xtask <fixtures|memory|micro|speed|cli|report|compare|gate|fingerprint|queue|queue-speed|help> [options]

fixtures
memory  --out <path> [--sizes 8,128] [--shapes plain,...] [--modes values,...] [--compact]
micro   [--bench fixtures|competitors] [--quick]
speed   [--out <path>] [--bench competitors] [--filter <text>] [--quick]
cli     [--out <path>] [--size <label>] [--shape <label>]
report  --input <path> [--out <path>]
compare --baseline <path> --current <path> [--out <path>]
gate    --baseline <path> --current <path> [--implementation <label>]
        [--max-heap-percent 5] [--max-heap-bytes 64] [--max-allocation-delta 0]
fingerprint
queue   --remote-root <path> [--out <path>] [--remote-out <path>]
        [--sizes ...] [--shapes ...] [--modes ...] [--compact] [--sync] [--dry-run]
queue-speed --remote-root <path> [--out <path>] [--remote-out <path>]
        [--bench competitors] [--filter <text>] [--quick] [--sample-size N]
        [--warm-up-time SECS] [--measurement-time SECS] [--sync] [--dry-run]";

fn command_fixtures(args: &[String]) -> Result<()> {
    let _ = ParsedArgs::parse(args, &[], &[])?;
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
    let parsed = ParsedArgs::parse(
        args,
        &["--out", "--sizes", "--shapes", "--modes"],
        &["--compact"],
    )?;
    let compact = parsed.has("--compact");
    let out = parsed.get("--out").unwrap_or(if compact {
        "docs/benchmarks/compact-memory.jsonl"
    } else {
        "docs/benchmarks/memory.jsonl"
    });
    let sizes = parsed.get("--sizes").unwrap_or("8,128");
    let shapes = parsed.get("--shapes").unwrap_or("plain");
    let modes = parsed
        .get("--modes")
        .unwrap_or("values,load,document,resolve");
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
    let parsed = ParsedArgs::parse(args, &["--bench"], &["--quick"])?;
    let bench = parsed.get("--bench").unwrap_or("fixtures");
    let mut command = Command::new(cargo());
    command.args(["bench", "-p", "adotenv-bench", "--bench", bench, "--locked"]);
    if parsed.has("--quick") {
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
    let parsed = ParsedArgs::parse(args, &["--out", "--bench", "--filter"], &["--quick"])?;
    let out = PathBuf::from(parsed.get("--out").unwrap_or("docs/benchmarks/speed.txt"));
    let bench = parsed.get("--bench").unwrap_or("competitors");
    ensure_parent(&out)?;
    let binary = build_bench_binary(bench)?;
    let mut command = Command::new(binary);
    command.args(["--bench", "--output-format", "bencher", "--noplot"]);
    if parsed.has("--quick") {
        command.arg("--quick");
    }
    if let Some(filter) = parsed.get("--filter") {
        validate_filter(filter)?;
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

fn pretty_json(text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| text.to_owned())
}

fn write_speed_markdown(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut report = String::from("# Speed benchmark results\n\n");
    report
        .push_str("Click a column header to sort. Use the filter to narrow large result sets.\n\n");
    let sidecar = path.with_extension("fingerprint.json");
    if let Ok(fingerprint) = fs::read_to_string(&sidecar) {
        report.push_str("<details>\n<summary>Provenance fingerprint</summary>\n\n```json\n");
        report.push_str(&pretty_json(fingerprint.trim()));
        report.push_str("\n```\n\n</details>\n\n");
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
    let parsed = ParsedArgs::parse(args, &["--out", "--size", "--shape"], &[])?;
    let out = PathBuf::from(parsed.get("--out").unwrap_or("docs/benchmarks/cli.json"));
    let size = Size::from_label(parsed.get("--size").unwrap_or("128")).ok_or("unknown size")?;
    let shape =
        Shape::from_label(parsed.get("--shape").unwrap_or("plain")).ok_or("unknown shape")?;
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
    let parsed = ParsedArgs::parse(args, &["--input", "--out"], &[])?;
    let input = parsed.get("--input").ok_or("missing --input")?;
    let output = parsed.get("--out");
    let text = fs::read_to_string(input).map_err(|error| error.to_string())?;
    let mut rows = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|error| error.to_string())?;
        rows.push(value);
    }
    let mut report = String::from("# Memory benchmark results\n\n");
    report
        .push_str("Click a column header to sort. Use the filter to narrow large result sets.\n\n");
    let sidecar = Path::new(input).with_extension("fingerprint.json");
    if let Ok(fingerprint) = fs::read_to_string(&sidecar) {
        report.push_str("<details>\n<summary>Provenance fingerprint</summary>\n\n```json\n");
        report.push_str(&pretty_json(fingerprint.trim()));
        report.push_str("\n```\n\n</details>\n\n");
    }
    report.push_str(
        "| size | shape | mode | implementation | entries | source bytes | peak heap | retained heap | after drop | allocations | RSS before | RSS peak | RSS after |\n\
         |---|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
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
    let parsed = ParsedArgs::parse(args, &["--baseline", "--current", "--out"], &[])?;
    let baseline = parsed.get("--baseline").ok_or("missing --baseline")?;
    let current = parsed.get("--current").ok_or("missing --current")?;
    let baseline = read_rows(baseline)?;
    let current = read_rows(current)?;
    let mut report = String::from(
        "| size | shape | mode | implementation | metric | baseline | current | delta |\n\
         |---|---|---|---|---|---:|---:|---:|\n",
    );
    let mut only_current = 0usize;
    for (key, row) in &current {
        let Some(base) = baseline.get(key) else {
            only_current += 1;
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
                if current_value == 0.0 {
                    "0.00%".to_owned()
                } else {
                    "+inf".to_owned()
                }
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
    let only_baseline = baseline
        .keys()
        .filter(|key| !current.contains_key(*key))
        .count();
    if only_baseline > 0 || only_current > 0 {
        eprintln!(
            "compare: skipped {only_baseline} baseline-only and {only_current} current-only case(s)"
        );
    }
    if let Some(output) = parsed.get("--out") {
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
    let parsed = ParsedArgs::parse(
        args,
        &[
            "--baseline",
            "--current",
            "--implementation",
            "--max-heap-percent",
            "--max-heap-bytes",
            "--max-allocation-delta",
        ],
        &[],
    )?;
    let baseline = parsed.get("--baseline").ok_or("missing --baseline")?;
    let current = parsed.get("--current").ok_or("missing --current")?;
    let implementation = parsed.get("--implementation").unwrap_or("adotenv");
    let max_heap_percent = parsed
        .get("--max-heap-percent")
        .unwrap_or("5")
        .parse::<u64>()
        .map_err(|error| format!("invalid --max-heap-percent: {error}"))?;
    let max_heap_bytes = parsed
        .get("--max-heap-bytes")
        .unwrap_or("64")
        .parse::<u64>()
        .map_err(|error| format!("invalid --max-heap-bytes: {error}"))?;
    let max_allocation_delta = parsed
        .get("--max-allocation-delta")
        .unwrap_or("0")
        .parse::<u64>()
        .map_err(|error| format!("invalid --max-allocation-delta: {error}"))?;
    let baseline = read_rows(baseline)?;
    let current = read_rows(current)?;
    let violations = check_memory_gate(
        &baseline,
        &current,
        implementation,
        max_heap_percent,
        max_heap_bytes,
        max_allocation_delta,
    );
    let checked = baseline
        .values()
        .filter(|row| field(row, "implementation") == implementation)
        .count();
    if checked == 0 {
        return Err(format!(
            "memory gate found no baseline cases for implementation {implementation}"
        ));
    }
    if violations.is_empty() {
        println!("memory gate: {checked} {implementation} cases passed");
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

type RowKey = (String, String, String, String);

fn format_row_key(key: &RowKey) -> String {
    format!("{}|{}|{}|{}", key.0, key.1, key.2, key.3)
}

fn check_memory_gate(
    baseline: &BTreeMap<RowKey, serde_json::Value>,
    current: &BTreeMap<RowKey, serde_json::Value>,
    implementation: &str,
    max_heap_percent: u64,
    max_heap_bytes: u64,
    max_allocation_delta: u64,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (key, base) in baseline {
        if field(base, "implementation") != implementation {
            continue;
        }
        let label = format_row_key(key);
        let Some(cur) = current.get(key) else {
            violations.push(format!("{label}: missing current case"));
            continue;
        };
        let base_hash = base
            .get("fixture_sha256")
            .and_then(serde_json::Value::as_str)
            .filter(|hash| !hash.is_empty());
        let cur_hash = cur
            .get("fixture_sha256")
            .and_then(serde_json::Value::as_str)
            .filter(|hash| !hash.is_empty());
        match (base_hash, cur_hash) {
            (None, _) => violations.push(format!("{label}: missing baseline fixture_sha256")),
            (Some(_), None) => violations.push(format!("{label}: missing current fixture_sha256")),
            (Some(base_hash), Some(cur_hash)) if cur_hash != base_hash => {
                violations.push(format!(
                    "{label}: fixture hash changed ({base_hash} -> {cur_hash})"
                ));
            }
            (Some(_), Some(_)) => {}
        }
        for metric in ["peak_heap_bytes", "retained_heap_bytes"] {
            let Some(base_value) = unsigned(base, metric) else {
                violations.push(format!("{label}: missing baseline {metric}"));
                continue;
            };
            let Some(cur_value) = unsigned(cur, metric) else {
                violations.push(format!("{label}: missing current {metric}"));
                continue;
            };
            let percent_delta = base_value.saturating_mul(max_heap_percent) / 100;
            let allowed = base_value
                .saturating_add(percent_delta)
                .max(base_value.saturating_add(max_heap_bytes));
            if cur_value > allowed {
                violations.push(format!(
                    "{label}: {metric} {cur_value} > allowed {allowed} (baseline {base_value}, +{max_heap_percent}% or +{max_heap_bytes} B)"
                ));
            }
        }
        match (
            unsigned(base, "heap_after_drop_bytes"),
            unsigned(cur, "heap_after_drop_bytes"),
        ) {
            (None, _) => {
                violations.push(format!("{label}: missing baseline heap_after_drop_bytes"));
            }
            (Some(_), None) => {
                violations.push(format!("{label}: missing current heap_after_drop_bytes"));
            }
            (Some(base_value), Some(cur_value)) if cur_value > base_value => {
                violations.push(format!(
                    "{label}: heap_after_drop_bytes {cur_value} > baseline {base_value}"
                ));
            }
            (Some(_), Some(_)) => {}
        }
        match (unsigned(base, "allocations"), unsigned(cur, "allocations")) {
            (None, _) => violations.push(format!("{label}: missing baseline allocations")),
            (Some(_), None) => violations.push(format!("{label}: missing current allocations")),
            (Some(base_value), Some(cur_value)) => {
                let allowed = base_value.saturating_add(max_allocation_delta);
                if cur_value > allowed {
                    violations.push(format!(
                        "{label}: allocations {cur_value} > allowed {allowed} (baseline {base_value}, +{max_allocation_delta})"
                    ));
                }
            }
        }
    }
    violations
}

fn read_rows(path: &str) -> Result<BTreeMap<RowKey, serde_json::Value>> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    parse_rows(&text, path)
}

fn parse_rows(text: &str, source: &str) -> Result<BTreeMap<RowKey, serde_json::Value>> {
    let mut rows = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|error| error.to_string())?;
        if !value.is_object() {
            return Err(format!("benchmark row in {source} is not a JSON object"));
        }
        let mut parts = Vec::with_capacity(4);
        for name in ["size", "shape", "mode", "implementation"] {
            let part = value
                .get(name)
                .and_then(serde_json::Value::as_str)
                .filter(|part| !part.is_empty())
                .ok_or_else(|| {
                    format!("benchmark row in {source} is missing string field {name}")
                })?;
            parts.push(part.to_owned());
        }
        let mut iter = parts.into_iter();
        let key: RowKey = (
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
        );
        if rows.insert(key.clone(), value).is_some() {
            return Err(format!(
                "duplicate benchmark row for {} in {source}",
                format_row_key(&key)
            ));
        }
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

fn command_fingerprint(args: &[String]) -> Result<()> {
    let _ = ParsedArgs::parse(args, &[], &[])?;
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

fn require_command_output(program: &str, arguments: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| format!("running {program}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} {} exited with {}: {}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if text.is_empty() {
        return Err(format!(
            "{program} {} returned empty output",
            arguments.join(" ")
        ));
    }
    Ok(text)
}

fn submit_and_fetch_remote_task(
    remote_root: &str,
    remote_out: &str,
    local_out: &str,
    label: &str,
    remote_command: &str,
    dry_run: bool,
) -> Result<()> {
    let queue_command = format!(
        "PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml pueue add --group bench --working-directory {} --label {} --print-task-id -- {}",
        shell_quote(remote_root),
        shell_quote(label),
        shell_quote(remote_command)
    );
    if dry_run {
        println!("ssh kbench {queue_command}");
        return Ok(());
    }
    let task = ssh(&queue_command)?;
    let task = task.trim();
    if task.is_empty() || !task.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("pueue returned invalid task id: {task:?}"));
    }
    println!("queued kbench task {task}");
    ssh(&format!(
        "PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml pueue wait {task}"
    ))?;
    ensure_task_succeeded(task)?;
    ensure_parent(Path::new(local_out))?;
    let status = Command::new("scp")
        .arg("--")
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
        .arg("--")
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

fn command_queue(args: &[String]) -> Result<()> {
    let parsed = ParsedArgs::parse(
        args,
        &[
            "--remote-root",
            "--out",
            "--remote-out",
            "--sizes",
            "--shapes",
            "--modes",
        ],
        &["--compact", "--sync", "--dry-run"],
    )?;
    let remote_root = parsed.get("--remote-root").ok_or("missing --remote-root")?;
    validate_remote_root(remote_root)?;
    let compact = parsed.has("--compact");
    let local_out = parsed.get("--out").unwrap_or(if compact {
        "docs/benchmarks/compact-memory.jsonl"
    } else {
        "docs/benchmarks/memory.jsonl"
    });
    let remote_out = parsed.get("--remote-out").unwrap_or(if compact {
        "bench-compact-memory.jsonl"
    } else {
        "bench-memory.jsonl"
    });
    validate_remote_out(remote_out)?;
    let sizes = parsed.get("--sizes").unwrap_or("8,128");
    let shapes = parsed.get("--shapes").unwrap_or("plain");
    let modes = parsed
        .get("--modes")
        .unwrap_or("values,load,document,resolve");
    let dry_run = parsed.has("--dry-run");
    if parsed.has("--sync") && !dry_run {
        sync_binaries(remote_root)?;
    }
    let git_sha = require_command_output("git", &["rev-parse", "HEAD"])?;
    let rustc = require_command_output("rustc", &["--version"])?;
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
    submit_and_fetch_remote_task(
        remote_root,
        remote_out,
        local_out,
        "adotenv-bench",
        &remote_command,
        dry_run,
    )
}

fn command_queue_speed(args: &[String]) -> Result<()> {
    let parsed = ParsedArgs::parse(
        args,
        &[
            "--remote-root",
            "--out",
            "--remote-out",
            "--bench",
            "--filter",
            "--sample-size",
            "--warm-up-time",
            "--measurement-time",
        ],
        &["--quick", "--sync", "--dry-run"],
    )?;
    let remote_root = parsed.get("--remote-root").ok_or("missing --remote-root")?;
    validate_remote_root(remote_root)?;
    let local_out = parsed.get("--out").unwrap_or("docs/benchmarks/speed.txt");
    let remote_out = parsed.get("--remote-out").unwrap_or("bench-speed.txt");
    validate_remote_out(remote_out)?;
    let bench = parsed.get("--bench").unwrap_or("competitors");
    if !bench
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("--bench must be a simple target name".to_owned());
    }
    let mut bench_args = vec!["--bench", "--output-format", "bencher", "--noplot"];
    if parsed.has("--quick") {
        bench_args.push("--quick");
    }
    if let Some(filter) = parsed.get("--filter") {
        validate_filter(filter)?;
        bench_args.push(filter);
    }
    if let Some(value) = parsed.get("--sample-size") {
        validate_positive_u64("--sample-size", value)?;
        bench_args.push("--sample-size");
        bench_args.push(value);
    }
    for name in ["--warm-up-time", "--measurement-time"] {
        if let Some(value) = parsed.get(name) {
            validate_positive_f64(name, value)?;
            bench_args.push(name);
            bench_args.push(value);
        }
    }
    let dry_run = parsed.has("--dry-run");
    if parsed.has("--sync") && !dry_run {
        sync_speed_binaries(remote_root, bench)?;
    }
    let git_sha = require_command_output("git", &["rev-parse", "HEAD"])?;
    let rustc = require_command_output("rustc", &["--version"])?;
    let remote_fingerprint = Path::new(remote_out).with_extension("fingerprint.json");
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
    submit_and_fetch_remote_task(
        remote_root,
        remote_out,
        local_out,
        "adotenv-speed",
        &remote_command,
        dry_run,
    )?;
    if !dry_run {
        write_speed_markdown(Path::new(local_out))?;
    }
    Ok(())
}

fn ensure_clean_worktree() -> Result<()> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .map_err(|error| format!("running git status: {error}"))?;
    if !output.status.success() {
        return Err(format!("git status exited with {}", output.status));
    }
    if !String::from_utf8_lossy(&output.stdout).trim().is_empty() {
        return Err("--sync requires a clean working tree".to_owned());
    }
    Ok(())
}

fn sync_speed_binaries(remote_root: &str, bench: &str) -> Result<()> {
    ensure_clean_worktree()?;
    let bench_binary = build_bench_binary(bench)?;
    let xtask = build_xtask_binary()?;
    prepare_remote_root(remote_root)?;
    copy_binary(remote_root, &bench_binary, bench)?;
    copy_binary(remote_root, &xtask, "adotenv-xtask")?;
    Ok(())
}

fn validate_filter(filter: &str) -> Result<()> {
    if filter.is_empty() || filter.starts_with('-') {
        return Err("--filter must be non-empty and must not start with '-'".to_owned());
    }
    Ok(())
}

fn validate_positive_u64(flag: &str, value: &str) -> Result<()> {
    let parsed = value
        .parse::<u64>()
        .map_err(|error| format!("invalid {flag}: {error}"))?;
    if parsed == 0 {
        return Err(format!("invalid {flag}: must be greater than zero"));
    }
    Ok(())
}

fn validate_positive_f64(flag: &str, value: &str) -> Result<()> {
    let parsed = value
        .parse::<f64>()
        .map_err(|error| format!("invalid {flag}: {error}"))?;
    if !parsed.is_finite() || parsed <= 0.0 {
        return Err(format!("invalid {flag}: must be a finite positive number"));
    }
    Ok(())
}

fn validate_remote_root(remote_root: &str) -> Result<()> {
    if remote_root.is_empty()
        || !remote_root.starts_with('/')
        || remote_root.trim_end_matches('/').is_empty()
        || remote_root == "/home"
        || remote_root.starts_with("/home/")
        || remote_root.contains('~')
        || remote_root.contains('\\')
        || remote_root.bytes().any(|byte| byte.is_ascii_control())
        || remote_root.split('/').any(|part| part == "..")
    {
        return Err("--remote-root must be an absolute, non-root, non-home path".to_owned());
    }
    Ok(())
}

fn validate_remote_out(remote_out: &str) -> Result<()> {
    if remote_out.is_empty()
        || remote_out == "."
        || remote_out == ".."
        || remote_out.contains("..")
        || !remote_out
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err("--remote-out must be a simple filename".to_owned());
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
        .arg("--")
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
    ensure_clean_worktree()?;
    let memory = build_memory_binary()?;
    let xtask = build_xtask_binary()?;
    prepare_remote_root(remote_root)?;
    copy_binary(remote_root, &memory, "memory")?;
    copy_binary(remote_root, &xtask, "adotenv-xtask")?;
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
        implementations.push(Implementation::AdotenvCompactEntries);
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

struct ParsedArgs<'a> {
    values: BTreeMap<&'a str, &'a str>,
    flags: BTreeMap<&'a str, bool>,
}

impl<'a> ParsedArgs<'a> {
    fn parse(args: &'a [String], value_flags: &[&str], bool_flags: &[&str]) -> Result<Self> {
        let mut values = BTreeMap::new();
        let mut flags = BTreeMap::new();
        let mut index = 0;
        while index < args.len() {
            let argument = args[index].as_str();
            if value_flags.contains(&argument) {
                let value = args
                    .get(index + 1)
                    .map(String::as_str)
                    .ok_or_else(|| format!("missing value for {argument}"))?;
                if value.starts_with("--") || matches!(value, "-h") {
                    return Err(format!("missing value for {argument} (got flag {value})"));
                }
                if values.insert(argument, value).is_some() {
                    return Err(format!("duplicate argument: {argument}"));
                }
                index += 2;
            } else if bool_flags.contains(&argument) {
                if flags.insert(argument, true).is_some() {
                    return Err(format!("duplicate argument: {argument}"));
                }
                index += 1;
            } else {
                return Err(format!("unknown argument: {argument}"));
            }
        }
        Ok(Self { values, flags })
    }

    fn get(&self, name: &str) -> Option<&'a str> {
        self.values.get(name).copied()
    }

    fn has(&self, name: &str) -> bool {
        self.flags.get(name).copied().unwrap_or(false)
    }
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

    fn row_key(size: &str, shape: &str, mode: &str, implementation: &str) -> RowKey {
        (
            size.to_owned(),
            shape.to_owned(),
            mode.to_owned(),
            implementation.to_owned(),
        )
    }

    #[test]
    fn memory_gate_detects_allocation_and_heap_regressions() {
        let key = row_key("8", "plain", "values", "adotenv");
        let base = serde_json::json!({
            "size": "8", "shape": "plain", "mode": "values",
            "implementation": "adotenv", "fixture_sha256": "abc",
            "peak_heap_bytes": 1000, "retained_heap_bytes": 800,
            "heap_after_drop_bytes": 0, "allocations": 10
        });
        let regressed = serde_json::json!({
            "size": "8", "shape": "plain", "mode": "values",
            "implementation": "adotenv", "fixture_sha256": "abc",
            "peak_heap_bytes": 1000, "retained_heap_bytes": 800,
            "heap_after_drop_bytes": 0, "allocations": 11
        });
        let mut baseline = BTreeMap::new();
        baseline.insert(key.clone(), base);
        let mut current = BTreeMap::new();
        current.insert(key, regressed);
        assert_eq!(
            check_memory_gate(&baseline, &current, "adotenv", 5, 0, 0).len(),
            1
        );
    }

    #[test]
    fn memory_gate_filters_the_requested_implementation() {
        let map_key = row_key("8", "plain", "values", "adotenv");
        let compact_key = row_key("8", "plain", "values", "adotenv-compact");
        let map = serde_json::json!({
            "size": "8", "shape": "plain", "mode": "values",
            "implementation": "adotenv", "fixture_sha256": "abc",
            "peak_heap_bytes": 1000, "retained_heap_bytes": 800,
            "heap_after_drop_bytes": 0, "allocations": 10
        });
        let compact = serde_json::json!({
            "size": "8", "shape": "plain", "mode": "values",
            "implementation": "adotenv-compact", "fixture_sha256": "abc",
            "peak_heap_bytes": 500, "retained_heap_bytes": 400,
            "heap_after_drop_bytes": 0, "allocations": 5
        });
        let mut baseline = BTreeMap::new();
        baseline.insert(map_key.clone(), map.clone());
        baseline.insert(compact_key.clone(), compact.clone());
        let mut current = BTreeMap::new();
        current.insert(map_key, map);
        let mut regressed_compact = compact;
        regressed_compact["allocations"] = serde_json::json!(6);
        current.insert(compact_key, regressed_compact);
        assert_eq!(
            check_memory_gate(&baseline, &current, "adotenv", 5, 0, 0).len(),
            0
        );
        assert_eq!(
            check_memory_gate(&baseline, &current, "adotenv-compact", 5, 0, 0).len(),
            1
        );
    }

    #[test]
    fn compact_implementations_include_values_and_entries() {
        let plain = implementations(false);
        assert!(plain.contains(&Implementation::Adotenv));
        assert!(!plain.contains(&Implementation::AdotenvCompact));
        assert!(!plain.contains(&Implementation::AdotenvCompactEntries));

        let compact = implementations(true);
        assert!(compact.contains(&Implementation::Adotenv));
        assert!(compact.contains(&Implementation::AdotenvCompact));
        assert!(compact.contains(&Implementation::AdotenvCompactEntries));
    }

    #[test]
    fn parsed_args_rejects_unknown_flags_duplicates_and_flag_values() {
        let args = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let valid = args(&["--implementation", "adotenv-compact-entries", "--compact"]);
        let ok = ParsedArgs::parse(&valid, &["--implementation"], &["--compact"]).unwrap();
        assert_eq!(ok.get("--implementation"), Some("adotenv-compact-entries"));
        assert!(ok.has("--compact"));

        let typo = args(&["--implementations", "adotenv-compact-entries"]);
        assert!(ParsedArgs::parse(&typo, &["--implementation"], &[]).is_err());
        let missing = args(&["--implementation"]);
        assert!(ParsedArgs::parse(&missing, &["--implementation"], &[]).is_err());
        let swallowed_flag = args(&["--out", "--dry-run"]);
        assert!(ParsedArgs::parse(&swallowed_flag, &["--out"], &["--dry-run"]).is_err());
        let swallowed_typo = args(&["--out", "--dryrun"]);
        assert!(ParsedArgs::parse(&swallowed_typo, &["--out"], &["--dry-run"]).is_err());
        let duplicate = args(&["--out", "a.jsonl", "--out", "b.jsonl"]);
        assert!(ParsedArgs::parse(&duplicate, &["--out"], &[]).is_err());
        let duplicate_bool = args(&["--sync", "--sync"]);
        assert!(ParsedArgs::parse(&duplicate_bool, &[], &["--sync"]).is_err());
    }

    #[test]
    fn classify_command_handles_help_and_rejects_unknown_commands() {
        let args = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(matches!(
            classify_command(&args(&[])),
            Ok(CommandAction::Help)
        ));
        assert!(matches!(
            classify_command(&args(&["help"])),
            Ok(CommandAction::Help)
        ));
        assert!(classify_command(&args(&["help", "extra"])).is_err());
        assert!(classify_command(&args(&["bogus", "--help"])).is_err());
        assert!(matches!(
            classify_command(&args(&["gate", "--baseline", "a.jsonl", "--help"])),
            Ok(CommandAction::Help)
        ));
        assert!(matches!(
            classify_command(&args(&["gate", "--baseline", "a.jsonl"])),
            Ok(CommandAction::Dispatch("gate", _))
        ));
    }

    #[test]
    fn memory_gate_checks_missing_fields_leaks_and_compact_entries() {
        let key = row_key("8", "plain", "values", "adotenv-compact-entries");
        let base = serde_json::json!({
            "size": "8", "shape": "plain", "mode": "values",
            "implementation": "adotenv-compact-entries",
            "fixture_sha256": "abc",
            "peak_heap_bytes": 500,
            "retained_heap_bytes": 400,
            "heap_after_drop_bytes": 0,
            "allocations": 5
        });
        let mut baseline = BTreeMap::new();
        baseline.insert(key.clone(), base.clone());

        // Missing fixture_sha256, heap_after_drop_bytes, and allocations must fail.
        let incomplete = serde_json::json!({
            "size": "8", "shape": "plain", "mode": "values",
            "implementation": "adotenv-compact-entries",
            "peak_heap_bytes": 500,
            "retained_heap_bytes": 400
        });
        let mut current = BTreeMap::new();
        current.insert(key.clone(), incomplete);
        let violations =
            check_memory_gate(&baseline, &current, "adotenv-compact-entries", 5, 64, 0);
        assert_eq!(violations.len(), 3, "{violations:?}");

        // Non-zero heap_after_drop_bytes leak must fail.
        let mut leaked = base.clone();
        leaked["heap_after_drop_bytes"] = serde_json::json!(16);
        current.insert(key.clone(), leaked);
        let leak_violations =
            check_memory_gate(&baseline, &current, "adotenv-compact-entries", 5, 64, 0);
        assert_eq!(leak_violations.len(), 1, "{leak_violations:?}");
        assert!(leak_violations[0].contains("heap_after_drop_bytes"));

        // Missing baseline fields must all report violations rather than short-circuiting.
        let mut bad_base = BTreeMap::new();
        bad_base.insert(
            key.clone(),
            serde_json::json!({
                "size": "8", "shape": "plain", "mode": "values",
                "implementation": "adotenv-compact-entries",
                "fixture_sha256": ""
            }),
        );
        let mut ok_cur = BTreeMap::new();
        ok_cur.insert(key, base);
        let bad_base_violations =
            check_memory_gate(&bad_base, &ok_cur, "adotenv-compact-entries", 5, 64, 0);
        assert_eq!(bad_base_violations.len(), 5, "{bad_base_violations:?}");
        assert!(bad_base_violations[0].contains("missing baseline fixture_sha256"));
    }

    #[test]
    fn parse_rows_and_remote_validators_reject_malformed_inputs() {
        let row = "{\"size\":\"8\",\"shape\":\"plain\",\"mode\":\"values\",\"implementation\":\"adotenv\"}";
        assert!(parse_rows(row, "single").is_ok());
        assert!(parse_rows(&format!("{row}\n{row}\n"), "dup").is_err());
        assert!(parse_rows("123\n", "non-object").is_err());
        assert!(parse_rows("{\"size\":\"8\"}\n", "missing-fields").is_err());

        // Pipe characters inside fields do not collide under tuple keys.
        let pipe_a =
            "{\"size\":\"a|b\",\"shape\":\"c\",\"mode\":\"values\",\"implementation\":\"adotenv\"}";
        let pipe_b =
            "{\"size\":\"a\",\"shape\":\"b|c\",\"mode\":\"values\",\"implementation\":\"adotenv\"}";
        assert_eq!(
            parse_rows(&format!("{pipe_a}\n{pipe_b}\n"), "pipes")
                .unwrap()
                .len(),
            2
        );

        assert!(validate_remote_out("bench-compact-memory.jsonl").is_ok());
        assert!(validate_remote_out("").is_err());
        assert!(validate_remote_out("../evil.jsonl").is_err());
        assert!(validate_remote_out("bad name.jsonl").is_err());

        assert!(validate_remote_root("/var/lib/benchq/adotenv-bench").is_ok());
        assert!(validate_remote_root("/").is_err());
        assert!(validate_remote_root("//").is_err());
        assert!(validate_remote_root("/home").is_err());
        assert!(validate_remote_root("/home/user/bench").is_err());
        assert!(validate_remote_root("/tmp/..\n").is_err());
        assert!(validate_remote_root("/tmp/bad\npath").is_err());
        assert!(validate_remote_root("~/bench").is_err());

        assert!(validate_filter("plain/128").is_ok());
        assert!(validate_filter("").is_err());
        assert!(validate_filter("-foo").is_err());
        assert!(validate_positive_u64("--sample-size", "20").is_ok());
        assert!(validate_positive_u64("--sample-size", "0").is_err());
        assert!(validate_positive_u64("--sample-size", "abc").is_err());
        assert!(validate_positive_f64("--warm-up-time", "1.5").is_ok());
        assert!(validate_positive_f64("--warm-up-time", "0").is_err());
        assert!(validate_positive_f64("--warm-up-time", "NaN").is_err());
    }
}
