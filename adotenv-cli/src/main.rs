//! Small zero-dependency CLI for adotenv documents.

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::OsString,
    fmt, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use std::fmt::Write as _;

use adotenv::{
    load::{load_cascade_with, LoadedEnvironment},
    parse_strict_with, parse_with, AssignmentState, ComparedValue, Comparison, Diagnostic, Dialect,
    DifferenceKind, Document, FormatOptions, Severity,
};

/// Exit code: the command succeeded.
const SUCCESS: u8 = 0;
/// Exit code: diagnostics, differences, a required formatting change or a
/// missing key.
const FINDINGS: u8 = 1;
/// Exit code: a usage error, a read or write failure, or an input the command
/// cannot process. It takes precedence over [`FINDINGS`].
const FAILURE: u8 = 2;

/// Set once writing standard output failed; later output is skipped.
static STDOUT_FAILED: AtomicBool = AtomicBool::new(false);
/// Set once writing standard error failed.
static STDERR_FAILED: AtomicBool = AtomicBool::new(false);

/// Write to standard output without panicking; a failure makes the run exit 2.
macro_rules! out {
    ($($arg:tt)*) => {
        write_stdout(format_args!($($arg)*))
    };
}

/// Write a line to standard output without panicking.
macro_rules! outln {
    ($($arg:tt)*) => {
        write_stdout(format_args!("{}\n", format_args!($($arg)*)))
    };
}

/// Write a line to standard error without panicking.
macro_rules! errln {
    ($($arg:tt)*) => {
        write_stderr(format_args!("{}\n", format_args!($($arg)*)))
    };
}

fn write_stdout(arguments: fmt::Arguments<'_>) {
    if STDOUT_FAILED.load(Ordering::Relaxed) {
        return;
    }
    if let Err(error) = io::stdout().lock().write_fmt(arguments) {
        stdout_failed(&error);
    }
}

fn write_stderr(arguments: fmt::Arguments<'_>) {
    if io::stderr().lock().write_fmt(arguments).is_err() {
        STDERR_FAILED.store(true, Ordering::Relaxed);
    }
}

fn stdout_failed(error: &io::Error) {
    if !STDOUT_FAILED.swap(true, Ordering::Relaxed) {
        errln!("failed to write standard output: {error}");
    }
}

fn main() -> ExitCode {
    let args: Result<Vec<String>, OsString> =
        env::args_os().skip(1).map(OsString::into_string).collect();
    let code = match args {
        Ok(args) => run(&args),
        Err(_) => {
            errln!("arguments must be valid UTF-8");
            FAILURE
        }
    };
    if let Err(error) = io::stdout().flush() {
        stdout_failed(&error);
    }
    let output_failed =
        STDOUT_FAILED.load(Ordering::Relaxed) || STDERR_FAILED.load(Ordering::Relaxed);
    ExitCode::from(if output_failed { FAILURE } else { code })
}

fn run(args: &[String]) -> u8 {
    let mut args = args.to_vec();
    let dialect = match extract_dialect(&mut args) {
        Ok(dialect) => dialect,
        Err(()) => return 2,
    };
    let Some(command) = args.first().map(String::as_str) else {
        usage();
        return FAILURE;
    };
    match command {
        "-h" | "--help" | "help" => {
            usage();
            SUCCESS
        }
        "--version" | "version" => {
            outln!("adotenv-cli {}", env!("CARGO_PKG_VERSION"));
            SUCCESS
        }
        "get" => get(&args[1..], dialect),
        "check" => check(&args[1..], dialect),
        "fmt" => fmt(&args[1..], dialect),
        "diff" => diff(&args[1..], dialect),
        "example" => example(&args[1..], dialect),
        "load" => load(&args[1..], dialect),
        "set" => set(&args[1..], dialect),
        "run" => run_command(&args[1..], dialect),
        _ => {
            errln!("unknown command: {command}");
            usage();
            FAILURE
        }
    }
}

fn extract_dialect(args: &mut Vec<String>) -> Result<Dialect, ()> {
    let limit = args
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(args.len());
    let Some(position) = args[..limit].iter().position(|arg| arg == "--dialect") else {
        return Ok(Dialect::PORTABLE);
    };
    if position + 1 >= limit {
        errln!("--dialect requires a value");
        return Err(());
    }
    let dialect = match args[position + 1].as_str() {
        "portable" => Dialect::PORTABLE,
        "node" => Dialect::NODE_DOTENV,
        "python" => Dialect::PYTHON_DOTENV,
        "compose" => Dialect::COMPOSE,
        "docker" | "docker-raw" => Dialect::DOCKER_RAW,
        other => {
            errln!("unknown dialect: {other}");
            return Err(());
        }
    };
    args.drain(position..=position + 1);
    Ok(dialect)
}

const USAGE: &str = r#"adotenv-cli [--dialect <portable|node|python|compose|docker>] <command> [options]

Commands:
  get <file> <key>             Print an effective literal value
  check [--format human|json|github] [--json] [--source]
        [--recursive] [--exclude <path>] <file-or-dir>...
                                Report parse diagnostics
  fmt [--write|--check] [--sort] [--recursive] [--exclude <path>] <file-or-dir>...
                                Format documents (optionally sorted)
  diff [--format human|matrix|json] [--json] <baseline> <file>...
                                Compare effective keys
  example <file>               Print a value-free example
  load <directory> [mode]      Load the standard dotenv cascade
  set <file> <key> <value>     Set and write one declaration
  run <directory> [mode] -- <program> [args...]
                                Run a child with loaded values

File-reading commands accept '-' or '--stdin' for standard input. Recursive
commands skip .git, target and node_modules and do not follow symlinks.
--source may reveal secret values and is only valid for human output.
run never mutates the parent environment. Command substitution from files is
not enabled.
"#;

fn usage() {
    out!("{USAGE}");
}

fn get(args: &[String], dialect: Dialect) -> u8 {
    if args.len() != 2 {
        errln!("usage: adotenv-cli get <file> <key>");
        return FAILURE;
    }
    let path = &args[0];
    let source = match read_text(path) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let doc = match parse_strict_with(&source, dialect) {
        Ok(doc) => doc,
        Err(error) => return fail(format!("{path}: {error}")),
    };
    match doc.get(&args[1]) {
        Some(value) => {
            outln!("{value}");
            SUCCESS
        }
        None => {
            errln!("missing key: {}", args[1]);
            FINDINGS
        }
    }
}

const DEFAULT_IGNORED_DIRS: [&str; 3] = [".git", "target", "node_modules"];

fn is_env_file(name: &str) -> bool {
    name == ".env" || name.starts_with(".env.")
}

fn is_ignored_dir(name: &str) -> bool {
    DEFAULT_IGNORED_DIRS.contains(&name)
}

fn is_excluded(base: &Path, path: &Path, excludes: &[PathBuf]) -> bool {
    let relative = path.strip_prefix(base).unwrap_or(path);
    excludes.iter().any(|exclude| {
        if exclude.as_os_str().is_empty() {
            false
        } else if exclude.is_absolute() {
            path.starts_with(exclude)
        } else {
            let exclude = exclude.strip_prefix(".").unwrap_or(exclude);
            !exclude.as_os_str().is_empty()
                && (relative == exclude || relative.starts_with(exclude))
        }
    })
}

fn walk_env_files(
    base: &Path,
    directory: &Path,
    excludes: &[PathBuf],
    found: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries =
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if is_excluded(base, &path, excludes) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = entry.file_name();
            if !is_ignored_dir(&name.to_string_lossy()) {
                walk_env_files(base, &path, excludes, found)?;
            }
        } else if file_type.is_file() {
            let name = entry.file_name();
            if is_env_file(&name.to_string_lossy()) {
                found.push(path);
            }
        }
    }
    Ok(())
}

fn expand_paths(
    paths: &[String],
    recursive: bool,
    excludes: &[PathBuf],
) -> Result<Vec<PathBuf>, String> {
    if !recursive {
        return Ok(paths.iter().map(PathBuf::from).collect());
    }
    let roots: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.iter().map(PathBuf::from).collect()
    };
    let mut found = Vec::new();
    for root_path in roots {
        if is_stdin_path(&root_path) {
            found.push(root_path);
            continue;
        }
        let root = root_path.display().to_string();
        let metadata =
            fs::symlink_metadata(&root_path).map_err(|error| format!("{root}: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("{root}: refusing to follow symbolic links"));
        }
        if metadata.is_file() {
            let base = root_path.parent().unwrap_or_else(|| Path::new("."));
            if !is_excluded(base, &root_path, excludes) {
                found.push(root_path);
            }
        } else if metadata.is_dir() {
            walk_env_files(&root_path, &root_path, excludes, &mut found)?;
        } else {
            return Err(format!("{root}: not a regular file or directory"));
        }
    }
    found.sort();
    found.dedup();
    Ok(found)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CheckFormat {
    Human,
    Json,
    Github,
}

struct CheckArgs {
    paths: Vec<String>,
    format: CheckFormat,
    recursive: bool,
    excludes: Vec<PathBuf>,
    show_source: bool,
}

fn parse_check_format(value: &str) -> Result<CheckFormat, String> {
    match value {
        "human" => Ok(CheckFormat::Human),
        "json" => Ok(CheckFormat::Json),
        "github" => Ok(CheckFormat::Github),
        _ => Err("--format expects human, json or github".to_string()),
    }
}

fn set_check_format(current: &mut Option<CheckFormat>, next: CheckFormat) -> Result<(), String> {
    if current.is_some_and(|format| format != next) {
        return Err("conflicting output formats".to_string());
    }
    *current = Some(next);
    Ok(())
}

fn parse_check_args(args: &[String]) -> Result<CheckArgs, String> {
    let mut paths = Vec::new();
    let mut format = None;
    let mut recursive = false;
    let mut excludes = Vec::new();
    let mut show_source = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => set_check_format(&mut format, CheckFormat::Json)?,
            "--format" => {
                index += 1;
                if index >= args.len() {
                    return Err("--format requires human, json or github".to_string());
                }
                set_check_format(&mut format, parse_check_format(&args[index])?)?;
            }
            argument if argument.starts_with("--format=") => {
                let value = &argument["--format=".len()..];
                set_check_format(&mut format, parse_check_format(value)?)?;
            }
            "--recursive" | "-r" => recursive = true,
            "--exclude" => {
                index += 1;
                if index >= args.len() {
                    return Err("--exclude requires a path".to_string());
                }
                excludes.push(PathBuf::from(&args[index]));
            }
            "--source" => show_source = true,
            argument => paths.push(argument.to_string()),
        }
        index += 1;
    }
    let format = format.unwrap_or(CheckFormat::Human);
    if show_source && format != CheckFormat::Human {
        return Err("--source requires human output".to_string());
    }
    if !recursive && !excludes.is_empty() {
        return Err("--exclude requires --recursive".to_string());
    }
    Ok(CheckArgs {
        paths,
        format,
        recursive,
        excludes,
        show_source,
    })
}

struct FmtArgs {
    paths: Vec<String>,
    write: bool,
    check: bool,
    sort: bool,
    recursive: bool,
    excludes: Vec<PathBuf>,
}

fn parse_fmt_args(args: &[String]) -> Result<FmtArgs, String> {
    let mut paths = Vec::new();
    let mut write = false;
    let mut check = false;
    let mut sort = false;
    let mut recursive = false;
    let mut excludes = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--write" => write = true,
            "--check" => check = true,
            "--sort" => sort = true,
            "--recursive" | "-r" => recursive = true,
            "--exclude" => {
                index += 1;
                if index >= args.len() {
                    return Err("--exclude requires a path".to_string());
                }
                excludes.push(PathBuf::from(&args[index]));
            }
            argument => paths.push(argument.to_string()),
        }
        index += 1;
    }
    if !recursive && !excludes.is_empty() {
        return Err("--exclude requires --recursive".to_string());
    }
    Ok(FmtArgs {
        paths,
        write,
        check,
        sort,
        recursive,
        excludes,
    })
}

fn check(args: &[String], dialect: Dialect) -> u8 {
    let CheckArgs {
        paths: raw_paths,
        format,
        recursive,
        excludes,
        show_source,
    } = match parse_check_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            errln!("{error}");
            return FAILURE;
        }
    };
    if raw_paths.is_empty() && !recursive {
        errln!(
            "usage: adotenv-cli check [--format human|json|github] [--json] [--source] [--recursive] [--exclude <path>] <file-or-dir>..."
        );
        return FAILURE;
    }
    let paths = match expand_paths(&raw_paths, recursive, &excludes) {
        Ok(paths) => paths,
        Err(error) => {
            errln!("{error}");
            return FAILURE;
        }
    };
    if paths.is_empty() {
        errln!("no .env files found");
        if format == CheckFormat::Json {
            outln!("{{\"version\":1,\"files\":[]}}");
        }
        return SUCCESS;
    }
    if paths.iter().filter(|path| is_stdin_path(path)).count() > 1 {
        errln!("standard input may be used at most once");
        return FAILURE;
    }
    let mut code = SUCCESS;
    let mut files = Vec::new();
    let mut errors = 0usize;
    let mut warnings = 0usize;
    let mut infos = 0usize;
    let mut affected_files = 0usize;
    for path in &paths {
        let path_label = path.to_string_lossy();
        let source = match read_text(path) {
            Ok(source) => source,
            Err(error) => {
                code = code.max(FAILURE);
                errors += 1;
                affected_files += 1;
                match format {
                    CheckFormat::Json => files.push(format!(
                        "{{\"path\":{},\"ok\":false,\"error\":{},\"diagnostics\":[]}}",
                        json_string(&path_label),
                        json_string(&error)
                    )),
                    CheckFormat::Human => errln!("{error}"),
                    CheckFormat::Github => {
                        outln!("{}", github_read_error(&path_label, &error));
                    }
                }
                continue;
            }
        };
        let (_, diagnostics) = parse_with(&source, dialect);
        let mut file_has_error = false;
        let mut rendered = Vec::new();
        for diagnostic in &diagnostics {
            match diagnostic.severity() {
                Severity::Error => {
                    errors += 1;
                    file_has_error = true;
                }
                Severity::Warning => warnings += 1,
                _ => infos += 1,
            }
            match format {
                CheckFormat::Json => rendered.push(render_json_diagnostic(diagnostic)),
                CheckFormat::Human => {
                    outln!(
                        "{}",
                        render_human(&path_label, &source, diagnostic, show_source)
                    );
                }
                CheckFormat::Github => {
                    outln!("{}", github_annotation(&path_label, &source, diagnostic));
                }
            }
        }
        if file_has_error {
            code = code.max(FINDINGS);
        }
        if !diagnostics.is_empty() {
            affected_files += 1;
        }
        if format == CheckFormat::Json {
            files.push(format!(
                "{{\"path\":{},\"ok\":{},\"diagnostics\":[{}]}}",
                json_string(&path_label),
                !file_has_error,
                rendered.join(",")
            ));
        }
    }
    match format {
        CheckFormat::Json => outln!("{{\"version\":1,\"files\":[{}]}}", files.join(",")),
        CheckFormat::Human if affected_files > 0 => outln!(
            "{} error{}, {} warning{}, {} info in {} file{}",
            errors,
            if errors == 1 { "" } else { "s" },
            warnings,
            if warnings == 1 { "" } else { "s" },
            infos,
            affected_files,
            if affected_files == 1 { "" } else { "s" }
        ),
        CheckFormat::Human | CheckFormat::Github => {}
    }
    code
}

fn render_json_diagnostic(diagnostic: &Diagnostic) -> String {
    let secondary = diagnostic.secondary_span().map_or_else(
        || "null".to_string(),
        |span| format!("{{\"start\":{},\"end\":{}}}", span.start, span.end),
    );
    let span = diagnostic.span();
    format!(
        "{{\"line\":{},\"severity\":{},\"code\":{},\"message\":{},\"span\":{{\"start\":{},\"end\":{}}},\"secondary_span\":{}}}",
        diagnostic.line(),
        json_string(severity_name(diagnostic.severity())),
        json_string(diagnostic.code().as_str()),
        json_string(diagnostic.message()),
        span.start,
        span.end,
        secondary
    )
}

fn render_human(path: &str, source: &str, diagnostic: &Diagnostic, show_source: bool) -> String {
    let span = diagnostic.span();
    let (line_start, line_end) = line_bounds(source, span.start);
    let column = source_column(source, line_start, span.start);
    let mut output = format!(
        "{}:{}:{}: {}[{}]: {}",
        path,
        diagnostic.line(),
        column,
        severity_name(diagnostic.severity()),
        diagnostic.code().as_str(),
        diagnostic.message()
    );
    if show_source {
        let line_text = expand_tabs(&source[line_start..line_end]);
        let caret_start = floor_char_boundary(source, span.start.clamp(line_start, line_end));
        let caret_end = floor_char_boundary(source, span.end.clamp(caret_start, line_end));
        let padding = display_padding(&source[line_start..caret_start]);
        let mut carets = display_carets(&source[caret_start..caret_end]);
        if carets.is_empty() {
            carets.push('^');
        }
        let width = diagnostic.line().to_string().len();
        output.push_str(&format!(
            "\n{:>width$} | {line_text}\n{:>width$} | {padding}{carets}",
            diagnostic.line(),
            "",
            width = width
        ));
    }
    output
}

fn floor_char_boundary(source: &str, offset: usize) -> usize {
    let mut offset = offset.min(source.len());
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn line_bounds(source: &str, offset: usize) -> (usize, usize) {
    let offset = floor_char_boundary(source, offset);
    let line_start = source[..offset]
        .char_indices()
        .rev()
        .find(|(_, character)| matches!(character, '\n' | '\r'))
        .map_or(0, |(index, _)| index + 1);
    let line_end = source[offset..]
        .char_indices()
        .find(|(_, character)| matches!(character, '\n' | '\r'))
        .map_or(source.len(), |(index, _)| offset + index);
    (line_start, line_end)
}

fn source_column(source: &str, line_start: usize, offset: usize) -> usize {
    let offset = floor_char_boundary(source, offset.max(line_start).min(source.len()));
    source[line_start..offset].chars().count() + 1
}

fn expand_tabs(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        if character == '\t' {
            output.push_str("    ");
        } else {
            output.push(character);
        }
    }
    output
}

fn display_padding(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        if character == '\t' {
            output.push_str("    ");
        } else {
            output.push(' ');
        }
    }
    output
}

fn display_carets(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        if character == '\t' {
            output.push_str("^^^^");
        } else {
            output.push('^');
        }
    }
    output
}

fn github_annotation(path: &str, source: &str, diagnostic: &Diagnostic) -> String {
    let span = diagnostic.span();
    let (line_start, _) = line_bounds(source, span.start);
    let column = source_column(source, line_start, span.start);
    format!(
        "::{} file={},line={},col={},title={}::{}",
        github_severity(diagnostic.severity()),
        github_escape_property(path),
        diagnostic.line(),
        column,
        github_escape_property(diagnostic.code().as_str()),
        github_escape_data(diagnostic.message())
    )
}

fn github_read_error(path: &str, message: &str) -> String {
    format!(
        "::error file={},title=adotenv::{}",
        github_escape_property(path),
        github_escape_data(message)
    )
}

fn github_severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        _ => "notice",
    }
}

fn github_escape_property(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '%' => output.push_str("%25"),
            '\r' => output.push_str("%0D"),
            '\n' => output.push_str("%0A"),
            ':' => output.push_str("%3A"),
            ',' => output.push_str("%2C"),
            character => output.push(character),
        }
    }
    output
}

fn github_escape_data(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '%' => output.push_str("%25"),
            '\r' => output.push_str("%0D"),
            '\n' => output.push_str("%0A"),
            character => output.push(character),
        }
    }
    output
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        _ => "info",
    }
}

fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            character if (character as u32) < 0x20 => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn fmt(args: &[String], dialect: Dialect) -> u8 {
    let FmtArgs {
        paths: raw_paths,
        write,
        check,
        sort,
        recursive,
        excludes,
    } = match parse_fmt_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            errln!("{error}");
            return FAILURE;
        }
    };
    if raw_paths.is_empty() && !recursive {
        errln!(
            "usage: adotenv-cli fmt [--write|--check] [--sort] [--recursive] [--exclude <path>] <file-or-dir>..."
        );
        return FAILURE;
    }
    let paths = match expand_paths(&raw_paths, recursive, &excludes) {
        Ok(paths) => paths,
        Err(error) => {
            errln!("{error}");
            return FAILURE;
        }
    };
    if paths.is_empty() {
        errln!("no .env files found");
        return SUCCESS;
    }
    if write && check {
        errln!("--write and --check are mutually exclusive");
        return FAILURE;
    }
    if paths.iter().filter(|path| is_stdin_path(path)).count() > 1 {
        errln!("standard input may be used at most once");
        return FAILURE;
    }
    if write && paths.iter().any(|path| is_stdin_path(path)) {
        errln!("--write cannot write standard input");
        return FAILURE;
    }
    let mut code = SUCCESS;
    for path in &paths {
        let source = match read_text(path) {
            Ok(source) => source,
            Err(error) => {
                code = code.max(fail(error));
                continue;
            }
        };
        let (doc, diagnostics) = parse_with(&source, dialect);
        let has_errors = diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error);
        for diagnostic in diagnostics {
            errln!("{}: {diagnostic}", path.display());
        }
        let formatting_source;
        let formatting_doc;
        let doc_ref = if sort {
            let sorted = match doc.sort() {
                Ok(sorted) => sorted,
                Err(error) => {
                    code = code.max(fail(format!("{}: {error}", path.display())));
                    continue;
                }
            };
            formatting_source = sorted.into_string();
            formatting_doc = match parse_strict_with(&formatting_source, dialect) {
                Ok(doc) => doc,
                Err(error) => {
                    code = code.max(fail(format!("{}: {error}", path.display())));
                    continue;
                }
            };
            &formatting_doc
        } else {
            &doc
        };
        let formatted = match doc_ref.format(&FormatOptions::normalize_groups()) {
            Ok(formatted) => formatted,
            Err(error) => {
                code = code.max(fail(format!("{}: {error}", path.display())));
                continue;
            }
        };
        if check {
            if formatted.as_str() != source {
                outln!("{}: would reformat", path.display());
                code = code.max(FINDINGS);
            }
        } else if write {
            if let Err(error) = write_atomic(path, formatted.as_str()) {
                code = code.max(fail(format!("{}: {error}", path.display())));
            }
        } else if paths.len() > 1 {
            outln!("==> {} <==", path.display());
            out!("{}", formatted.as_str());
        } else {
            out!("{}", formatted.as_str());
        }
        if has_errors {
            code = code.max(FINDINGS);
        }
    }
    code
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiffFormat {
    Human,
    Matrix,
    Json,
}

struct DiffArgs {
    paths: Vec<String>,
    format: DiffFormat,
}

fn parse_diff_format(value: &str) -> Result<DiffFormat, String> {
    match value {
        "human" => Ok(DiffFormat::Human),
        "matrix" => Ok(DiffFormat::Matrix),
        "json" => Ok(DiffFormat::Json),
        _ => Err("--format expects human, matrix or json".to_string()),
    }
}

fn set_diff_format(current: &mut Option<DiffFormat>, next: DiffFormat) -> Result<(), String> {
    if current.is_some_and(|format| format != next) {
        return Err("conflicting output formats".to_string());
    }
    *current = Some(next);
    Ok(())
}

fn parse_diff_args(args: &[String]) -> Result<DiffArgs, String> {
    let mut paths = Vec::new();
    let mut format = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => set_diff_format(&mut format, DiffFormat::Json)?,
            "--format" => {
                index += 1;
                if index >= args.len() {
                    return Err("--format requires human, matrix or json".to_string());
                }
                set_diff_format(&mut format, parse_diff_format(&args[index])?)?;
            }
            argument if argument.starts_with("--format=") => {
                let value = &argument["--format=".len()..];
                set_diff_format(&mut format, parse_diff_format(value)?)?;
            }
            argument => paths.push(argument.to_string()),
        }
        index += 1;
    }
    Ok(DiffArgs {
        paths,
        format: format.unwrap_or(DiffFormat::Human),
    })
}

fn diff(args: &[String], dialect: Dialect) -> u8 {
    let DiffArgs { paths, format } = match parse_diff_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            errln!("{error}");
            return FAILURE;
        }
    };
    if paths.len() < 2 {
        errln!(
            "usage: adotenv-cli diff [--format human|matrix|json] [--json] <baseline> <file>..."
        );
        return FAILURE;
    }
    if paths.iter().filter(|path| is_stdin(path)).count() > 1 {
        errln!("standard input may be used at most once");
        return FAILURE;
    }
    if format != DiffFormat::Matrix {
        return diff_streaming(&paths, format, dialect);
    }

    diff_matrix_files(&paths, dialect)
}

fn diff_streaming(paths: &[String], format: DiffFormat, dialect: Dialect) -> u8 {
    let baseline_source = match read_text(&paths[0]) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let baseline = match parse_strict_with(&baseline_source, dialect) {
        Ok(document) => document,
        Err(error) => return fail(format!("{}: {error}", paths[0])),
    };

    let mut output = String::new();
    let multi = paths.len() > 2;
    if format == DiffFormat::Json {
        write!(
            output,
            "{{\"version\":1,\"baseline\":{},\"files\":[",
            json_string(&paths[0])
        )
        .expect("writing to String cannot fail");
    }

    let mut differs = false;
    for (index, path) in paths.iter().enumerate().skip(1) {
        let source = match read_text(path) {
            Ok(source) => source,
            Err(error) => return fail(error),
        };
        let document = match parse_strict_with(&source, dialect) {
            Ok(document) => document,
            Err(error) => return fail(format!("{path}: {error}")),
        };
        let comparison = baseline.compare(&document);
        if !comparison.is_empty() {
            differs = true;
        }

        match format {
            DiffFormat::Human => {
                if multi {
                    writeln!(output, "{} -> {}", paths[0], path)
                        .expect("writing to String cannot fail");
                    if comparison.is_empty() {
                        output.push_str("(no differences)\n");
                    }
                }
                write_comparison(&mut output, &comparison);
            }
            DiffFormat::Json => {
                if index > 1 {
                    output.push(',');
                }
                write!(
                    output,
                    "{{\"path\":{},\"ok\":true,\"differences\":[{}]}}",
                    json_string(path),
                    render_comparison_json(&comparison)
                )
                .expect("writing to String cannot fail");
            }
            DiffFormat::Matrix => unreachable!("matrix output uses the retained document set"),
        }
    }

    if format == DiffFormat::Json {
        output.push_str("]}\n");
    }
    out!("{output}");
    if differs {
        FINDINGS
    } else {
        SUCCESS
    }
}

struct MatrixRows {
    keys: Vec<String>,
    rows: Vec<Vec<String>>,
    seen: BTreeSet<String>,
}

impl MatrixRows {
    fn new() -> Self {
        Self {
            keys: Vec::new(),
            rows: Vec::new(),
            seen: BTreeSet::new(),
        }
    }

    fn push(&mut self, document: &Document<'_>) {
        let (keys, values) = matrix_column(document);
        let prior_columns = self.rows.first().map_or(0, Vec::len);
        for (key, row) in self.keys.iter().zip(self.rows.iter_mut()) {
            row.push(
                values
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| "<unset>".to_string()),
            );
        }
        for key in keys {
            if self.seen.insert(key.clone()) {
                let mut row = vec!["<unset>".to_string(); prior_columns];
                row.push(values.get(&key).cloned().unwrap_or_default());
                self.keys.push(key);
                self.rows.push(row);
            }
        }
    }
}

fn diff_matrix_files(paths: &[String], dialect: Dialect) -> u8 {
    let baseline_source = match read_text(&paths[0]) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let baseline = match parse_strict_with(&baseline_source, dialect) {
        Ok(document) => document,
        Err(error) => return fail(format!("{}: {error}", paths[0])),
    };
    let mut matrix = MatrixRows::new();
    matrix.push(&baseline);

    let mut differs = false;
    for path in paths.iter().skip(1) {
        let source = match read_text(path) {
            Ok(source) => source,
            Err(error) => return fail(error),
        };
        let document = match parse_strict_with(&source, dialect) {
            Ok(document) => document,
            Err(error) => return fail(format!("{path}: {error}")),
        };
        if !baseline.compare(&document).is_empty() {
            differs = true;
        }
        matrix.push(&document);
    }
    print_matrix(paths, &matrix);
    if differs {
        FINDINGS
    } else {
        SUCCESS
    }
}

fn matrix_column(document: &Document<'_>) -> (Vec<String>, BTreeMap<String, String>) {
    let index = document.lookup_index();
    let mut keys = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in document.entries() {
        if !entry.participates_in_lookup() {
            continue;
        }
        let key = entry.key();
        if seen.insert(key.to_string()) {
            keys.push(key.to_string());
        }
    }
    let values = keys
        .iter()
        .map(|key| {
            let entry = if key.is_empty() {
                document.get_entry(key)
            } else {
                index.get_entry(key)
            };
            (key.clone(), matrix_entry_value(entry))
        })
        .collect();
    (keys, values)
}

fn matrix_entry_value(entry: Option<adotenv::Entry<'_>>) -> String {
    let Some(entry) = entry else {
        return "<unset>".to_string();
    };
    let value = match (entry.state(), entry.value()) {
        (AssignmentState::Bare, None) => "<bare>".to_string(),
        (AssignmentState::Empty, _) => "\"\"".to_string(),
        (_, Some(value)) => value.to_string(),
        (_, None) => "<unset>".to_string(),
    };
    if entry.exported() {
        format!("export {value}")
    } else {
        value
    }
}

fn print_matrix(paths: &[String], matrix: &MatrixRows) {
    let columns = paths.len() + 1;
    let mut widths = vec![0usize; columns];
    widths[0] = escape_table("key").chars().count();
    for (index, path) in paths.iter().enumerate() {
        widths[index + 1] = escape_table(path).chars().count();
    }
    for (key, row) in matrix.keys.iter().zip(&matrix.rows) {
        widths[0] = widths[0].max(escape_table(key).chars().count());
        for (index, value) in row.iter().enumerate() {
            widths[index + 1] = widths[index + 1].max(escape_table(value).chars().count());
        }
    }

    let mut header = Vec::with_capacity(columns);
    header.push("key".to_string());
    header.extend(paths.iter().cloned());
    print_table_row(&header, &widths);
    outln!(
        "{}",
        widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("-+-")
    );
    for (key, row) in matrix.keys.iter().zip(&matrix.rows) {
        let mut cells = Vec::with_capacity(columns);
        cells.push(key.clone());
        cells.extend(row.iter().cloned());
        print_table_row(&cells, &widths);
    }
}

fn print_table_row(cells: &[String], widths: &[usize]) {
    let mut line = String::new();
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            line.push_str(" | ");
        }
        let escaped = escape_table(cell);
        line.push_str(&format!("{escaped:<width$}", width = widths[index]));
    }
    outln!("{line}");
}

fn escape_table(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '|' => output.push_str("\\|"),
            character => output.push(character),
        }
    }
    output
}

fn render_comparison_json(comparison: &Comparison) -> String {
    comparison
        .differences()
        .iter()
        .map(|entry| {
            format!(
                "{{\"key\":{},\"kind\":{},\"left\":{},\"right\":{}}}",
                json_string(&entry.key),
                json_string(difference_kind_name(entry.kind)),
                render_compared_value_json(entry.left.as_ref()),
                render_compared_value_json(entry.right.as_ref())
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn render_compared_value_json(value: Option<&ComparedValue>) -> String {
    let Some(value) = value else {
        return "null".to_string();
    };
    format!(
        "{{\"state\":{},\"value\":{},\"exported\":{}}}",
        json_string(assignment_state_name(value.state)),
        value
            .value
            .as_deref()
            .map_or_else(|| "null".to_string(), json_string),
        value.exported
    )
}

fn assignment_state_name(state: AssignmentState) -> &'static str {
    match state {
        AssignmentState::Bare => "bare",
        AssignmentState::Empty => "empty",
        AssignmentState::Value => "value",
        _ => "unknown",
    }
}

fn difference_kind_name(kind: DifferenceKind) -> &'static str {
    match kind {
        DifferenceKind::OnlyLeft => "only-left",
        DifferenceKind::OnlyRight => "only-right",
        DifferenceKind::State => "state",
        DifferenceKind::Value => "value",
        DifferenceKind::Export => "export",
        _ => "unknown",
    }
}

fn example(args: &[String], dialect: Dialect) -> u8 {
    if args.len() != 1 {
        errln!("usage: adotenv-cli example <file>");
        return FAILURE;
    }
    let source = match read_text(&args[0]) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let (doc, diagnostics) = parse_with(&source, dialect);
    for diagnostic in diagnostics {
        errln!("{}: {diagnostic}", args[0]);
    }
    out!("{}", doc.example());
    SUCCESS
}

fn load(args: &[String], dialect: Dialect) -> u8 {
    if args.is_empty() || args.len() > 2 {
        errln!("usage: adotenv-cli load <directory> [mode]");
        return FAILURE;
    }
    let mode = args.get(1).map_or("", String::as_str);
    let loaded = match load_cascade_with(&args[0], mode, dialect) {
        Ok(loaded) => loaded,
        Err(error) => return fail(error.to_string()),
    };
    for (key, value) in loaded.values() {
        outln!("{key}={value}");
    }
    SUCCESS
}

fn set(args: &[String], dialect: Dialect) -> u8 {
    if args.len() != 3 {
        errln!("usage: adotenv-cli set <file> <key> <value>");
        return FAILURE;
    }
    if is_stdin(&args[0]) {
        errln!("set cannot write standard input");
        return FAILURE;
    }
    let source = match read_text(&args[0]) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let doc = match parse_strict_with(&source, dialect) {
        Ok(doc) => doc,
        Err(error) => return fail(format!("{}: {error}", args[0])),
    };
    let edited = match doc.set(&args[1], &args[2]) {
        Ok(edited) => edited,
        Err(error) => return fail(format!("{}: {error}", args[0])),
    };
    if let Err(error) = write_atomic(Path::new(&args[0]), edited.as_str()) {
        return fail(format!("{}: {error}", args[0]));
    }
    SUCCESS
}

fn run_command(args: &[String], dialect: Dialect) -> u8 {
    let Some(separator) = args.iter().position(|arg| arg == "--") else {
        errln!("usage: adotenv-cli run <directory> [mode] -- <program> [args...]");
        return FAILURE;
    };
    if separator == 0 || separator + 1 >= args.len() || separator > 2 {
        errln!("usage: adotenv-cli run <directory> [mode] -- <program> [args...]");
        return FAILURE;
    }
    let directory = &args[0];
    let mode = args
        .get(1)
        .filter(|_| separator == 2)
        .map_or("", String::as_str);
    let program = &args[separator + 1];
    let program_args = &args[separator + 2..];
    let loaded = match load_cascade_with(directory, mode, dialect) {
        Ok(loaded) => loaded,
        Err(error) => return fail(error.to_string()),
    };
    if let Err(error) = validate_child_environment(&loaded) {
        return fail(error);
    }
    let status = Command::new(program)
        .args(program_args)
        .envs(loaded.values())
        .status();
    match status {
        Ok(status) => status.code().unwrap_or(1).clamp(0, 255) as u8,
        Err(error) => fail(format!("failed to run {program}: {error}")),
    }
}

fn validate_child_environment(loaded: &LoadedEnvironment) -> Result<(), String> {
    for (key, value) in loaded.values() {
        if key.is_empty() || key.bytes().any(|byte| byte == 0 || byte == b'=') {
            return Err(format!("invalid child environment key: {key:?}"));
        }
        if value.as_bytes().contains(&0) {
            return Err(format!("child environment value for {key:?} contains NUL"));
        }
    }
    Ok(())
}

fn write_comparison(output: &mut String, comparison: &Comparison) {
    for entry in comparison.differences() {
        let result = match entry.kind {
            DifferenceKind::OnlyLeft => {
                writeln!(output, "- {}={}", entry.key, describe(entry.left.as_ref()))
            }
            DifferenceKind::OnlyRight => {
                writeln!(output, "+ {}={}", entry.key, describe(entry.right.as_ref()))
            }
            DifferenceKind::State | DifferenceKind::Value | DifferenceKind::Export => writeln!(
                output,
                "~ {}: {} -> {}",
                entry.key,
                describe(entry.left.as_ref()),
                describe(entry.right.as_ref())
            ),
            _ => writeln!(output, "~ {}: <unknown difference>", entry.key),
        };
        result.expect("writing to String cannot fail");
    }
}

fn describe(value: Option<&ComparedValue>) -> String {
    let Some(value) = value else {
        return "<unset>".to_string();
    };
    let rendered = match (value.state, value.value.as_deref()) {
        (AssignmentState::Bare, None) => "<bare>".to_string(),
        (AssignmentState::Empty, _) => "\"\"".to_string(),
        (_, Some(value)) => value.to_string(),
        (_, None) => "<unset>".to_string(),
    };
    if value.exported {
        format!("{rendered} (export)")
    } else {
        rendered
    }
}

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

/// Replace `path` with `contents` through a temporary file in the same directory.
///
/// When `path` is a symbolic link, the file it finally points to is replaced
/// in that file's directory and the link is kept. The temporary file is created
/// private on Unix (mode `0600`), receives the existing file's owner and group
/// where possible, and receives its permissions only after the contents are
/// written. A file that does not exist yet stays private.
fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let target = replacement_target(path)?;
    let path = target.as_path();
    let existing = match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path is a directory",
            ));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path is not a regular file",
            ));
        }
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let (temporary, file) = create_temporary(parent, name)?;
    if let Err(error) = fill_temporary(file, &temporary, contents, existing.as_ref()) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    #[cfg(unix)]
    {
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        sync_directory(parent);
        Ok(())
    }
    #[cfg(not(unix))]
    {
        match fs::rename(&temporary, path) {
            Ok(()) => Ok(()),
            Err(replace_error) => match replace_with_backup(path, &temporary) {
                Ok(()) => Ok(()),
                Err(backup_error) => Err(io::Error::new(
                    replace_error.kind(),
                    format!(
                        "direct replacement failed: {replace_error}; backup swap failed: {backup_error}"
                    ),
                )),
            },
        }
    }
}

/// Create a new, empty temporary file next to the target.
///
/// On Unix the file is created with mode `0600` (further narrowed by the
/// umask), so no other user can read it while it is being filled.
fn create_temporary(parent: &Path, name: &std::ffi::OsStr) -> io::Result<(PathBuf, fs::File)> {
    for _ in 0..16 {
        let candidate = parent.join(format!(
            ".{}.adotenv-{}-{}",
            name.to_string_lossy(),
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not create a temporary file",
    ))
}

/// The file a write to `path` replaces: the final target of a symbolic link,
/// or `path` itself.
fn replacement_target(path: &Path) -> io::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path),
        _ => Ok(path.to_path_buf()),
    }
}

/// Write and flush the temporary file after giving it the existing file's
/// owner, group and permissions.
#[cfg(unix)]
fn fill_temporary(
    mut file: fs::File,
    _temporary: &Path,
    contents: &str,
    existing: Option<&fs::Metadata>,
) -> io::Result<()> {
    if let Some(metadata) = existing {
        keep_ownership(&file, metadata)?;
    }
    file.write_all(contents.as_bytes())?;
    if let Some(metadata) = existing {
        file.set_permissions(metadata.permissions())?;
    }
    file.sync_all()
}

/// Give the temporary file the original owner and group where possible.
///
/// Keeping another user's ownership needs privileges, so an unprivileged
/// replacement belongs to the running user, who could already read and replace
/// the file. The group is kept when the running user may set it. When it cannot
/// be kept and the original mode grants the group any access, the write fails:
/// the copied mode would otherwise grant that access to a different group.
#[cfg(unix)]
fn keep_ownership(file: &fs::File, original: &fs::Metadata) -> io::Result<()> {
    use std::os::unix::fs::{fchown, MetadataExt};

    let created = file.metadata()?;
    let owner = (created.uid() != original.uid()).then_some(original.uid());
    let group = (created.gid() != original.gid()).then_some(original.gid());
    if owner.is_none() && group.is_none() {
        return Ok(());
    }
    if fchown(file, owner, group).is_ok() {
        return Ok(());
    }
    let Some(group) = group else {
        return Ok(());
    };
    match fchown(file, None, Some(group)) {
        Ok(()) => Ok(()),
        Err(_) if !grants_group_access(original.mode()) => Ok(()),
        Err(error) => Err(io::Error::new(
            error.kind(),
            format!("cannot keep the file's group {group}: {error}"),
        )),
    }
}

/// Whether a Unix mode grants the owning group any access.
#[cfg(unix)]
const fn grants_group_access(mode: u32) -> bool {
    mode & 0o070 != 0
}

/// Write and flush the temporary file, then give it the existing file's permissions.
#[cfg(not(unix))]
fn fill_temporary(
    mut file: fs::File,
    temporary: &Path,
    contents: &str,
    existing: Option<&fs::Metadata>,
) -> io::Result<()> {
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    match existing {
        Some(metadata) => fs::set_permissions(temporary, metadata.permissions()),
        None => Ok(()),
    }
}

/// Flush a directory entry change to disk. This is best effort: the
/// replacement has already happened, and some file systems cannot sync
/// directories.
#[cfg(unix)]
fn sync_directory(directory: &Path) {
    if let Ok(directory) = fs::File::open(directory) {
        let _ = directory.sync_all();
    }
}

#[cfg(not(unix))]
fn replace_with_backup(path: &Path, temporary: &Path) -> io::Result<()> {
    let backup = match backup_existing(path) {
        Ok(backup) => backup,
        Err(error) => {
            let _ = fs::remove_file(temporary);
            return Err(error);
        }
    };
    match fs::rename(temporary, path) {
        Ok(()) => {
            if let Some(backup) = backup {
                if let Err(error) = remove_backup(&backup) {
                    return Err(io::Error::new(
                        error.kind(),
                        format!(
                            "replacement succeeded but backup {} could not be removed: {error}",
                            backup.display()
                        ),
                    ));
                }
            }
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(temporary);
            if let Some(backup) = backup {
                if let Err(restore_error) = fs::rename(&backup, path) {
                    return Err(io::Error::new(
                        error.kind(),
                        format!(
                            "replacement failed: {error}; restoring backup {} failed: {restore_error}",
                            backup.display()
                        ),
                    ));
                }
            }
            Err(error)
        }
    }
}

#[cfg(not(unix))]
fn backup_existing(path: &Path) -> io::Result<Option<PathBuf>> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    for _ in 0..16 {
        let candidate = parent.join(format!(
            ".{}.adotenv-backup-{}-{}",
            name.to_string_lossy(),
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::rename(path, &candidate) {
            Ok(()) => return Ok(Some(candidate)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not create a backup file",
    ))
}

#[cfg(not(unix))]
fn remove_backup(path: &Path) -> io::Result<()> {
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        if permissions.readonly() {
            permissions.set_readonly(false);
            let _ = fs::set_permissions(path, permissions);
        }
    }
    fs::remove_file(path)
}

fn is_stdin(path: &str) -> bool {
    is_stdin_path(Path::new(path))
}

fn is_stdin_path(path: &Path) -> bool {
    path == Path::new("-") || path == Path::new("--stdin")
}

fn read_text<P: AsRef<Path>>(path: P) -> Result<String, String> {
    let path = path.as_ref();
    let bytes = if is_stdin_path(path) {
        let mut bytes = Vec::new();
        io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|error| format!("-: {error}"))?;
        bytes
    } else {
        fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?
    };
    String::from_utf8(bytes).map_err(|_| format!("{}: invalid UTF-8", path.display()))
}

/// Report an error and return [`FAILURE`].
fn fail(message: String) -> u8 {
    errln!("{message}");
    FAILURE
}

#[cfg(test)]
mod tests {
    use super::{github_escape_data, github_escape_property, json_string, write_atomic, NEXT_TEMP};
    use std::sync::atomic::Ordering;

    #[cfg(not(unix))]
    #[test]
    fn backup_swap_replaces_and_cleans_up() {
        use super::replace_with_backup;
        use std::{
            fs,
            path::PathBuf,
            process,
            sync::atomic::{AtomicUsize, Ordering},
        };

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = PathBuf::from(std::env::temp_dir()).join(format!(
            "adotenv-write-{}-{}",
            process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let target = directory.join("target.env");
        let temporary = directory.join("temporary.env");
        fs::write(&target, "old").unwrap();
        fs::write(&temporary, "new").unwrap();
        replace_with_backup(&target, &temporary).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert!(!temporary.exists());
        let leftovers: Vec<_> = fs::read_dir(&directory).unwrap().collect();
        assert_eq!(leftovers.len(), 1);
        fs::remove_dir_all(&directory).unwrap();
    }

    #[cfg(not(unix))]
    #[test]
    fn backup_swap_restores_target_when_install_fails() {
        use super::replace_with_backup;
        use std::{
            fs,
            path::PathBuf,
            process,
            sync::atomic::{AtomicUsize, Ordering},
        };

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = PathBuf::from(std::env::temp_dir()).join(format!(
            "adotenv-write-failure-{}-{}",
            process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let target = directory.join("target.env");
        let missing_temporary = directory.join("missing.env");
        fs::write(&target, "old").unwrap();
        assert!(replace_with_backup(&target, &missing_temporary).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
        let leftovers: Vec<_> = fs::read_dir(&directory).unwrap().collect();
        assert_eq!(leftovers.len(), 1);
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn github_workflow_values_are_escaped() {
        assert_eq!(github_escape_property("a,b:c%\n"), "a%2Cb%3Ac%25%0A");
        assert_eq!(github_escape_data("a,b:c%\r\n"), "a,b:c%25%0D%0A");
    }

    #[test]
    fn write_atomic_creates_missing_files_without_leftovers() {
        let directory = std::env::temp_dir().join(format!(
            "adotenv-write-missing-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let target = directory.join("new.env");
        write_atomic(&target, "A=1\n").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "A=1\n");
        let leftovers: Vec<_> = std::fs::read_dir(&directory).unwrap().collect();
        assert_eq!(leftovers.len(), 1);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn temporary_files_are_private_before_content_is_written() {
        use super::create_temporary;
        use std::os::unix::fs::PermissionsExt;

        let directory = std::env::temp_dir().join(format!(
            "adotenv-write-mode-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let (temporary, file) =
            create_temporary(&directory, std::ffi::OsStr::new("app.env")).unwrap();
        let mode = file.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(file.metadata().unwrap().len(), 0);
        drop(file);
        std::fs::remove_dir_all(&directory).unwrap();
        assert!(!temporary.exists());
        assert_eq!(mode, 0o600, "temporary file mode {mode:o}");
    }

    #[cfg(unix)]
    #[test]
    fn group_access_decides_whether_a_group_change_is_fatal() {
        use super::grants_group_access;

        for mode in [0o600, 0o700, 0o604, 0o100_600, 0o4600] {
            assert!(!grants_group_access(mode), "{mode:o}");
        }
        for mode in [0o640, 0o620, 0o610, 0o660, 0o100_640] {
            assert!(grants_group_access(mode), "{mode:o}");
        }
    }

    #[test]
    fn json_strings_escape_controls_quotes_and_backslashes() {
        assert_eq!(
            json_string("quote\" slash\\ line\n tab\t control\u{1}"),
            "\"quote\\\" slash\\\\ line\\n tab\\t control\\u0001\""
        );
    }
}
