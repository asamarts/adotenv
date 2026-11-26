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
queue   [--host <host>] [--remote-root <path>] [--pueue-config <path>]
        [--group <name>] [--queue-user <user>] [--out <path>] [--remote-out <path>]
        [--sizes ...] [--shapes ...] [--modes ...] [--compact] [--sync] [--dry-run]
queue-speed [--host <host>] [--remote-root <path>] [--pueue-config <path>]
        [--group <name>] [--queue-user <user>] [--out <path>] [--remote-out <path>]
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
    validate_local_out(out)?;
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
    validate_bench_target(bench)?;
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
    let out_arg = parsed.get("--out").unwrap_or("docs/benchmarks/speed.txt");
    validate_speed_out(out_arg)?;
    let out = PathBuf::from(out_arg);
    let bench = parsed.get("--bench").unwrap_or("competitors");
    validate_bench_target(bench)?;
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
    let out_arg = parsed.get("--out").unwrap_or("docs/benchmarks/cli.json");
    validate_local_out(out_arg)?;
    let out = PathBuf::from(out_arg);
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
    if let Some(output) = output {
        validate_local_out(output)?;
    }
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
    if let Some(output) = parsed.get("--out") {
        validate_local_out(output)?;
    }
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
        "hostname": "unknown",
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

#[derive(Debug, PartialEq, Eq)]
struct RemoteConfig {
    host: String,
    remote_root: String,
    pueue_config: Option<String>,
    group: String,
    queue_user: Option<String>,
}

impl RemoteConfig {
    fn from_parsed(parsed: &ParsedArgs<'_>) -> Result<Self> {
        Self::from_parsed_with_env(parsed, |key| env::var(key).ok())
    }

    fn from_parsed_with_env(
        parsed: &ParsedArgs<'_>,
        get_env: impl Fn(&str) -> Option<String>,
    ) -> Result<Self> {
        let host = match parsed.get("--host") {
            Some(value) => value.to_owned(),
            None => get_env("ADOTENV_BENCH_HOST")
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "missing --host (or ADOTENV_BENCH_HOST)".to_owned())?,
        };
        validate_remote_host(&host)?;

        let remote_root = match parsed.get("--remote-root") {
            Some(value) => value.to_owned(),
            None => get_env("ADOTENV_BENCH_ROOT")
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "missing --remote-root (or ADOTENV_BENCH_ROOT)".to_owned())?,
        };
        validate_remote_root(&remote_root)?;

        let pueue_config = match parsed.get("--pueue-config") {
            Some(value) => Some(value.to_owned()),
            None => get_env("PUEUE_CONFIG_PATH").filter(|value| !value.trim().is_empty()),
        };
        if let Some(path) = &pueue_config {
            validate_pueue_config(path)?;
        }

        let group = match parsed.get("--group") {
            Some(value) => value.to_owned(),
            None => get_env("ADOTENV_BENCH_GROUP")
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "bench".to_owned()),
        };
        validate_simple_token("--group", &group)?;

        let queue_user = match parsed.get("--queue-user") {
            Some(value) => Some(value.to_owned()),
            None => get_env("ADOTENV_BENCH_USER").filter(|value| !value.trim().is_empty()),
        };
        if let Some(user) = &queue_user {
            validate_simple_token("--queue-user", user)?;
        }

        Ok(Self {
            host,
            remote_root,
            pueue_config,
            group,
            queue_user,
        })
    }

    fn pueue_prefix(&self) -> String {
        match &self.pueue_config {
            Some(path) => format!("PUEUE_CONFIG_PATH={} ", shell_quote(path)),
            None => String::new(),
        }
    }

    fn sudo_prefix(&self) -> String {
        match &self.queue_user {
            Some(user) => format!("sudo -n -u {} ", shell_quote(user)),
            None => String::new(),
        }
    }

    fn queue_add_command(&self, label: &str, remote_command: &str) -> String {
        format!(
            "{}pueue add --group {} --working-directory {} --label {} --print-task-id -- {}",
            self.pueue_prefix(),
            shell_quote(&self.group),
            shell_quote(&self.remote_root),
            shell_quote(label),
            shell_quote(remote_command)
        )
    }

    fn dry_run_ssh_command(&self, queue_command: &str) -> String {
        format!(
            "ssh -- {} {}",
            shell_quote(&self.host),
            shell_quote(queue_command)
        )
    }

    fn pueue_wait_command(&self, task: &str) -> String {
        format!("{}pueue wait {task}", self.pueue_prefix())
    }

    fn pueue_status_command(&self) -> String {
        format!("{}pueue status --json", self.pueue_prefix())
    }

    fn prepare_root_command(&self) -> String {
        let sudo = self.sudo_prefix();
        let root = shell_quote(&self.remote_root);
        format!("{sudo}mkdir -p {root} && {sudo}chmod 750 {root}")
    }

    fn install_binary_command(&self, staging_dir: &str, name: &str, destination: &str) -> String {
        let sudo = self.sudo_prefix();
        let dir = shell_quote(staging_dir);
        let src = shell_quote(&format!("{staging_dir}/{name}"));
        let dst = shell_quote(destination);
        format!("{sudo}install -m 755 {src} {dst}; status=$?; rm -rf {dir}; exit $status")
    }
}

fn is_valid_host_segment(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn validate_remote_host(host: &str) -> Result<()> {
    let valid = match host.split_once('@') {
        Some((user, hostname)) => {
            !hostname.contains('@')
                && is_valid_host_segment(user)
                && is_valid_host_segment(hostname)
        }
        None => is_valid_host_segment(host),
    };
    if !valid {
        return Err("--host must be a valid SSH host name or user@host".to_owned());
    }
    Ok(())
}

fn first_non_dot_segment(path: &str) -> Option<&str> {
    path.split('/')
        .find(|part| !part.is_empty() && *part != ".")
}

fn validate_pueue_config(path: &str) -> Result<()> {
    if path.is_empty()
        || !path.starts_with('/')
        || path.ends_with('/')
        || first_non_dot_segment(path).is_none()
        || path.split('/').any(|part| part == "..")
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'/' | b'-'))
    {
        return Err("--pueue-config must be an absolute non-root path".to_owned());
    }
    Ok(())
}

fn validate_local_out(path: &str) -> Result<()> {
    let out = Path::new(path);
    if path.trim().is_empty()
        || path.starts_with('-')
        || path.bytes().any(|byte| byte.is_ascii_control())
        || path == "."
        || path == ".."
        || path.ends_with('/')
        || path.ends_with('\\')
        || path.ends_with("/.")
        || path.ends_with("\\.")
        || path.ends_with("/..")
        || path.ends_with("\\..")
        || out.file_name().is_none()
        || out.is_dir()
    {
        return Err(format!("--out must be a non-directory file path: {path:?}"));
    }
    Ok(())
}

fn validate_speed_out(path: &str) -> Result<()> {
    validate_local_out(path)?;
    if Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return Err(format!(
            "--out for speed output must not end with .md (markdown report uses the .md sibling): {path:?}"
        ));
    }
    Ok(())
}

fn validate_simple_token(flag: &str, value: &str) -> Result<()> {
    if !is_valid_host_segment(value) {
        return Err(format!("{flag} must be a simple identifier"));
    }
    Ok(())
}

fn validate_bench_target(bench: &str) -> Result<()> {
    if bench.is_empty()
        || bench.starts_with('-')
        || !bench
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("--bench must be a simple target name".to_owned());
    }
    Ok(())
}

fn scp_local_arg(path: &Path) -> String {
    let text = path.to_string_lossy();
    let has_prefix = matches!(
        path.components().next(),
        Some(std::path::Component::Prefix(_))
    );
    let first_sep = if cfg!(windows) {
        text.find(['/', '\\'])
    } else {
        text.find('/')
    };
    let colon_before_sep = match (text.find(':'), first_sep) {
        (Some(colon), Some(sep)) => colon < sep,
        (Some(_), None) => true,
        (None, _) => false,
    };
    if !path.is_absolute()
        && !has_prefix
        && !text.starts_with("./")
        && !text.starts_with("../")
        && colon_before_sep
    {
        format!("./{text}")
    } else {
        text.into_owned()
    }
}

fn submit_and_fetch_remote_task(
    remote: &RemoteConfig,
    remote_out: &str,
    local_out: &str,
    label: &str,
    remote_command: &str,
    dry_run: bool,
) -> Result<()> {
    let queue_command = remote.queue_add_command(label, remote_command);
    if dry_run {
        println!("{}", remote.dry_run_ssh_command(&queue_command));
        return Ok(());
    }
    let task = ssh(&remote.host, &queue_command)?;
    let task = task.trim();
    if task.is_empty() || !task.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("pueue returned invalid task id: {task:?}"));
    }
    println!("queued remote task {task}");
    ssh(&remote.host, &remote.pueue_wait_command(task))?;
    ensure_task_succeeded(remote, task)?;
    let local_out_path = Path::new(local_out);
    ensure_parent(local_out_path)?;
    let status = Command::new("scp")
        .arg("--")
        .arg(format!(
            "{}:{}/{remote_out}",
            remote.host, remote.remote_root
        ))
        .arg(scp_local_arg(local_out_path))
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!("scp exited with {status}"));
    }
    let local_fingerprint = local_out_path.with_extension("fingerprint.json");
    let remote_fingerprint = Path::new(remote_out).with_extension("fingerprint.json");
    let status = Command::new("scp")
        .arg("--")
        .arg(format!(
            "{}:{}/{}",
            remote.host,
            remote.remote_root,
            remote_fingerprint.display()
        ))
        .arg(scp_local_arg(&local_fingerprint))
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
            "--host",
            "--remote-root",
            "--pueue-config",
            "--group",
            "--queue-user",
            "--out",
            "--remote-out",
            "--sizes",
            "--shapes",
            "--modes",
        ],
        &["--compact", "--sync", "--dry-run"],
    )?;
    let remote = RemoteConfig::from_parsed(&parsed)?;
    let compact = parsed.has("--compact");
    let local_out = parsed.get("--out").unwrap_or(if compact {
        "docs/benchmarks/compact-memory.jsonl"
    } else {
        "docs/benchmarks/memory.jsonl"
    });
    validate_local_out(local_out)?;
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
    let parsed_sizes = parse_sizes(sizes)?;
    let parsed_shapes = parse_shapes(shapes)?;
    let parsed_modes = parse_modes(modes)?;
    let dry_run = parsed.has("--dry-run");
    if parsed.has("--sync") && !dry_run {
        sync_binaries(&remote)?;
    }
    let git_sha = require_command_output("git", &["rev-parse", "HEAD"])?;
    let rustc = require_command_output("rustc", &["--version"])?;
    let remote_fingerprint = Path::new(remote_out).with_extension("fingerprint.json");
    let mut memory_commands = Vec::new();
    for size in parsed_sizes {
        for shape in &parsed_shapes {
            for mode in &parsed_modes {
                for implementation in implementations(compact) {
                    if !implementation.supports(*mode) {
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
        shell_quote(&remote.remote_root),
        shell_quote(&git_sha),
        shell_quote(&rustc),
        shell_quote(&remote_fingerprint.to_string_lossy()),
        memory_commands.join(" && "),
        shell_quote(remote_out)
    );
    submit_and_fetch_remote_task(
        &remote,
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
            "--host",
            "--remote-root",
            "--pueue-config",
            "--group",
            "--queue-user",
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
    let remote = RemoteConfig::from_parsed(&parsed)?;
    let local_out = parsed.get("--out").unwrap_or("docs/benchmarks/speed.txt");
    validate_speed_out(local_out)?;
    let remote_out = parsed.get("--remote-out").unwrap_or("bench-speed.txt");
    validate_remote_out(remote_out)?;
    let bench = parsed.get("--bench").unwrap_or("competitors");
    validate_bench_target(bench)?;
    let mut bench_args = vec!["--bench", "--output-format", "bencher", "--noplot"];
    if parsed.has("--quick") {
        bench_args.push("--quick");
    }
    if let Some(filter) = parsed.get("--filter") {
        validate_filter(filter)?;
        bench_args.push(filter);
    }
    if let Some(value) = parsed.get("--sample-size") {
        validate_sample_size(value)?;
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
        sync_speed_binaries(&remote, bench)?;
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
        shell_quote(&remote.remote_root),
        shell_quote(&git_sha),
        shell_quote(&rustc),
        shell_quote(&remote_fingerprint.to_string_lossy()),
        shell_quote(bench),
        bench_args,
        shell_quote(remote_out)
    );
    submit_and_fetch_remote_task(
        &remote,
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

fn sync_speed_binaries(remote: &RemoteConfig, bench: &str) -> Result<()> {
    ensure_clean_worktree()?;
    let bench_binary = build_bench_binary(bench)?;
    let xtask = build_xtask_binary()?;
    prepare_remote_root(remote)?;
    copy_binary(remote, &bench_binary, bench)?;
    copy_binary(remote, &xtask, "adotenv-xtask")?;
    Ok(())
}

fn validate_filter(filter: &str) -> Result<()> {
    if filter.is_empty() || filter.starts_with('-') {
        return Err("--filter must be non-empty and must not start with '-'".to_owned());
    }
    Ok(())
}

fn validate_sample_size(value: &str) -> Result<()> {
    if value.starts_with('+') {
        return Err("invalid --sample-size: must be an unsigned integer >= 10".to_owned());
    }
    let parsed = value
        .parse::<u64>()
        .map_err(|error| format!("invalid --sample-size: {error}"))?;
    if parsed < 10 {
        return Err("invalid --sample-size: Criterion requires at least 10 samples".to_owned());
    }
    Ok(())
}

fn validate_positive_f64(flag: &str, value: &str) -> Result<()> {
    if value.starts_with('+') {
        return Err(format!("invalid {flag}: must not start with '+'"));
    }
    let parsed = value
        .parse::<f64>()
        .map_err(|error| format!("invalid {flag}: {error}"))?;
    if !parsed.is_finite() || parsed <= 0.0 {
        return Err(format!("invalid {flag}: must be a finite positive number"));
    }
    Ok(())
}

fn validate_remote_root(remote_root: &str) -> Result<()> {
    let first = first_non_dot_segment(remote_root);
    if remote_root.is_empty()
        || !remote_root.starts_with('/')
        || remote_root.ends_with('/')
        || first.is_none()
        || first == Some("home")
        || remote_root.split('/').any(|part| part == "..")
        || !remote_root
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'/' | b'-'))
    {
        return Err("--remote-root must be an absolute, non-root, non-home path".to_owned());
    }
    Ok(())
}

fn validate_staging_dir(staging_dir: &str) -> Result<()> {
    let valid = staging_dir
        .strip_prefix("/var/tmp/adotenv.")
        .is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        });
    if !valid {
        return Err(format!("unexpected remote staging dir: {staging_dir:?}"));
    }
    Ok(())
}

fn validate_remote_out(remote_out: &str) -> Result<()> {
    if remote_out.is_empty()
        || remote_out.starts_with('-')
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

fn prepare_remote_root(remote: &RemoteConfig) -> Result<()> {
    ssh(&remote.host, &remote.prepare_root_command())?;
    Ok(())
}

fn copy_binary(remote: &RemoteConfig, binary: &Path, name: &str) -> Result<()> {
    let staging_dir = ssh(
        &remote.host,
        "dir=$(mktemp -d /var/tmp/adotenv.XXXXXX) && chmod 755 \"$dir\" && printf '%s\\n' \"$dir\"",
    )?;
    let staging_dir = staging_dir.trim();
    validate_staging_dir(staging_dir)?;
    let status = Command::new("scp")
        .arg("--")
        .arg(scp_local_arg(binary))
        .arg(format!("{}:{staging_dir}/{name}", remote.host))
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        let _ = ssh(
            &remote.host,
            &format!("rm -rf {}", shell_quote(staging_dir)),
        );
        return Err(format!("scp {} exited with {status}", binary.display()));
    }
    let destination = format!("{}/{name}", remote.remote_root);
    ssh(
        &remote.host,
        &remote.install_binary_command(staging_dir, name, &destination),
    )?;
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

fn sync_binaries(remote: &RemoteConfig) -> Result<()> {
    ensure_clean_worktree()?;
    let memory = build_memory_binary()?;
    let xtask = build_xtask_binary()?;
    prepare_remote_root(remote)?;
    copy_binary(remote, &memory, "memory")?;
    copy_binary(remote, &xtask, "adotenv-xtask")?;
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

fn ensure_task_succeeded(remote: &RemoteConfig, task: &str) -> Result<()> {
    let status = ssh(&remote.host, &remote.pueue_status_command())?;
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
            "remote task {task} did not succeed (result: {})",
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

fn ssh(host: &str, command: &str) -> Result<String> {
    let output = Command::new("ssh")
        .arg("--")
        .arg(host)
        .arg(command)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            Err(format!("ssh exited with {}", output.status))
        } else {
            Err(format!("ssh exited with {}: {stderr}", output.status))
        }
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
        assert!(validate_remote_out("-flag.jsonl").is_err());
        assert!(validate_remote_out("../evil.jsonl").is_err());
        assert!(validate_remote_out("bad name.jsonl").is_err());

        assert!(validate_remote_root("/var/lib/bench-runner/adotenv-bench").is_ok());
        assert!(validate_remote_root("/").is_err());
        assert!(validate_remote_root("//").is_err());
        assert!(validate_remote_root("/.").is_err());
        assert!(validate_remote_root("/./.").is_err());
        assert!(validate_remote_root("/home").is_err());
        assert!(validate_remote_root("/home/user/bench").is_err());
        assert!(validate_remote_root("//home/user/bench").is_err());
        assert!(validate_remote_root("/./home/user/bench").is_err());
        assert!(validate_remote_root("/tmp/..\n").is_err());
        assert!(validate_remote_root("/tmp/bad\npath").is_err());
        assert!(validate_remote_root("~/bench").is_err());

        assert!(validate_filter("plain/128").is_ok());
        assert!(validate_filter("").is_err());
        assert!(validate_filter("-foo").is_err());
        assert!(validate_bench_target("competitors").is_ok());
        assert!(validate_bench_target("").is_err());
        assert!(validate_bench_target("-bad").is_err());
        assert!(validate_sample_size("10").is_ok());
        assert!(validate_sample_size("20").is_ok());
        assert!(validate_sample_size("9").is_err());
        assert!(validate_sample_size("0").is_err());
        assert!(validate_sample_size("+20").is_err());
        assert!(validate_sample_size("abc").is_err());
        assert!(validate_positive_f64("--warm-up-time", "1.5").is_ok());
        assert!(validate_positive_f64("--warm-up-time", "+1.5").is_err());
        assert!(validate_positive_f64("--warm-up-time", "0").is_err());
        assert!(validate_positive_f64("--warm-up-time", "NaN").is_err());

        assert_eq!(
            scp_local_arg(Path::new("2026-11-26T00:00:00.jsonl")),
            "./2026-11-26T00:00:00.jsonl"
        );
        assert_eq!(
            scp_local_arg(Path::new("docs/benchmarks/2026:00.jsonl")),
            "docs/benchmarks/2026:00.jsonl"
        );
        assert_eq!(
            scp_local_arg(Path::new("/tmp/2026:00.jsonl")),
            "/tmp/2026:00.jsonl"
        );
    }

    #[test]
    fn remote_config_resolves_flags_env_and_builds_safe_commands() {
        let args = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let value_flags = [
            "--host",
            "--remote-root",
            "--pueue-config",
            "--group",
            "--queue-user",
        ];

        // Missing host or remote-root fails cleanly.
        let empty_raw = args(&[]);
        let empty = ParsedArgs::parse(&empty_raw, &value_flags, &[]).unwrap();
        assert!(RemoteConfig::from_parsed_with_env(&empty, |_| None).is_err());
        assert!(RemoteConfig::from_parsed_with_env(&empty, |key| match key {
            "ADOTENV_BENCH_HOST" => Some("bench-host".to_owned()),
            _ => None,
        })
        .is_err());

        // Environment fallback without pueue_config or queue_user omits both prefixes.
        let from_env = RemoteConfig::from_parsed_with_env(&empty, |key| match key {
            "ADOTENV_BENCH_HOST" => Some("bench-host".to_owned()),
            "ADOTENV_BENCH_ROOT" => Some("/var/lib/bench-runner/adotenv".to_owned()),
            _ => None,
        })
        .unwrap();
        assert_eq!(from_env.host, "bench-host");
        assert_eq!(from_env.remote_root, "/var/lib/bench-runner/adotenv");
        assert_eq!(from_env.group, "bench");
        assert_eq!(from_env.pueue_config, None);
        assert_eq!(from_env.queue_user, None);
        let qcmd = from_env.queue_add_command("adotenv-bench", "echo ok");
        assert!(qcmd.starts_with("pueue add --group 'bench' "));
        let dry = from_env.dry_run_ssh_command(&qcmd);
        assert!(dry.starts_with("ssh -- 'bench-host' 'pueue add "));
        assert_eq!(
            from_env.prepare_root_command(),
            "mkdir -p '/var/lib/bench-runner/adotenv' && chmod 750 '/var/lib/bench-runner/adotenv'"
        );
        assert!(from_env
            .install_binary_command(
                "/var/tmp/adotenv.abc",
                "memory",
                "/var/lib/bench-runner/adotenv/memory",
            )
            .starts_with("install -m 755 '/var/tmp/adotenv.abc/memory' "));

        // Explicit empty CLI flags fail rather than falling back silently.
        let empty_group_raw = args(&[
            "--host",
            "bench-host",
            "--remote-root",
            "/var/lib/bench-runner/adotenv",
            "--group",
            "",
        ]);
        let empty_group = ParsedArgs::parse(&empty_group_raw, &value_flags, &[]).unwrap();
        assert!(RemoteConfig::from_parsed_with_env(&empty_group, |_| None).is_err());

        // Explicit CLI flags override env and emit quoted PUEUE_CONFIG_PATH and sudo prefixes.
        let cli_raw = args(&[
            "--host",
            "runner@bench.example",
            "--remote-root",
            "/var/lib/bench-runner/adotenv",
            "--pueue-config",
            "/etc/bench-runner/pueue.yml",
            "--group",
            "serial",
            "--queue-user",
            "runner",
        ]);
        let cli = ParsedArgs::parse(&cli_raw, &value_flags, &[]).unwrap();
        let from_cli =
            RemoteConfig::from_parsed_with_env(&cli, |_| Some("ignored".to_owned())).unwrap();
        assert_eq!(from_cli.host, "runner@bench.example");
        assert_eq!(from_cli.group, "serial");
        assert_eq!(
            from_cli.pueue_config.as_deref(),
            Some("/etc/bench-runner/pueue.yml")
        );
        assert_eq!(from_cli.queue_user.as_deref(), Some("runner"));
        assert!(from_cli
            .queue_add_command("adotenv-bench", "echo ok")
            .starts_with(
                "PUEUE_CONFIG_PATH='/etc/bench-runner/pueue.yml' pueue add --group 'serial' "
            ));
        assert!(from_cli
            .prepare_root_command()
            .starts_with("sudo -n -u 'runner' mkdir -p "));
        assert!(from_cli
            .install_binary_command(
                "/var/tmp/adotenv.abc",
                "memory",
                "/var/lib/bench-runner/adotenv/memory",
            )
            .starts_with("sudo -n -u 'runner' install -m 755 "));
    }

    #[test]
    fn remote_host_config_and_fingerprint_validation() {
        // Host, pueue config, and token validators reject malformed values.
        for bad_host in [
            "-oProxyCommand=evil",
            "host:22",
            "bad host",
            "@",
            "user@",
            "@host",
            "a@b@c",
            "user@-host",
            ".",
            "..",
        ] {
            assert!(validate_remote_host(bad_host).is_err(), "{bad_host}");
        }
        assert!(validate_pueue_config("relative/pueue.yml").is_err());
        assert!(validate_pueue_config("/etc/../pueue.yml").is_err());
        assert!(validate_pueue_config("/.").is_err());
        assert!(validate_pueue_config("/etc/pueue/").is_err());
        assert!(validate_pueue_config("/etc/bench path/pueue.yml").is_err());
        assert!(validate_pueue_config("/etc/bench*/pueue.yml").is_err());
        assert!(validate_remote_root("/var/lib/bench/").is_err());
        assert!(validate_remote_root("/var/lib/bench ").is_err());
        assert!(validate_remote_root("/var/lib/*").is_err());
        assert!(validate_remote_root("/var/lib/a;id").is_err());
        assert!(validate_remote_root("/var/lib/$(id)").is_err());
        assert!(validate_staging_dir("/var/tmp/adotenv.AbC_12-x").is_ok());
        for bad_staging in [
            "/var/tmp/adotenv.",
            "/var/tmp/adotenv.abc/def",
            "/var/tmp/adotenv.abc def",
            "/var/tmp/adotenv.abc;id",
            "/var/tmp/adotenv.*",
            "/tmp/adotenv.abc",
        ] {
            assert!(validate_staging_dir(bad_staging).is_err(), "{bad_staging}");
        }
        assert!(validate_local_out("docs/benchmarks/memory.jsonl").is_ok());
        for bad_out in [
            "",
            "   ",
            ".",
            "..",
            "/",
            "docs/benchmarks/",
            "a/b/.",
            "a/b/..",
            "-out.json",
            "foo\0bar.jsonl",
            "foo\nbar.jsonl",
            env!("CARGO_MANIFEST_DIR"),
        ] {
            assert!(validate_local_out(bad_out).is_err(), "{bad_out:?}");
        }
        assert!(validate_speed_out("docs/benchmarks/speed.txt").is_ok());
        assert!(validate_speed_out("docs/benchmarks/speed.md").is_err());
        assert!(validate_speed_out("docs/benchmarks/speed.MD").is_err());
        #[cfg(not(windows))]
        {
            assert_eq!(
                scp_local_arg(Path::new(r"foo\bar:baz.jsonl")),
                r"./foo\bar:baz.jsonl"
            );
            assert_eq!(
                scp_local_arg(Path::new(r".\2026:00.jsonl")),
                r"./.\2026:00.jsonl"
            );
        }
        assert!(validate_simple_token("--group", "-bad").is_err());
        assert!(validate_simple_token("--group", ".").is_err());
        assert!(validate_simple_token("--group", "..").is_err());
        assert!(validate_simple_token("--queue-user", "user;id").is_err());

        // Fingerprints never expose the host's local hostname.
        let fp: serde_json::Value = serde_json::from_str(&fingerprint_json()).unwrap();
        assert_eq!(fp["hostname"], serde_json::json!("unknown"));
    }
}
