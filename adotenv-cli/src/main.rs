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

/// Replacement for a value under `diff --mask`.
const MASK: &str = "***";

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
    // A reader that closed the pipe early, as `head` does, already knows.
    if !STDOUT_FAILED.swap(true, Ordering::Relaxed) && error.kind() != io::ErrorKind::BrokenPipe {
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
    // Global dialect options may appear before the command, or immediately
    // after it before the command's first positional argument. Stopping at
    // the first positional keeps `set file KEY --dialect=node` and similar
    // values intact.
    let mut selected = None;
    let mut index = 0;
    while index < args.len() {
        if let Some((end, value)) = dialect_option(args, index)? {
            if selected.replace((index, end, value)).is_some() {
                errln!("--dialect may be specified only once");
                return Err(());
            }
            index = end;
        } else {
            break;
        }
    }
    let command = index;
    if command < args.len() {
        index = command + 1;
        while index < args.len() {
            if let Some((end, value)) = dialect_option(args, index)? {
                if selected.replace((index, end, value)).is_some() {
                    errln!("--dialect may be specified only once");
                    return Err(());
                }
                index = end;
            } else {
                break;
            }
        }
    }
    if let Some((start, end, dialect)) = selected {
        args.drain(start..end);
        Ok(dialect)
    } else {
        Ok(Dialect::PORTABLE)
    }
}

fn dialect_option(args: &[String], index: usize) -> Result<Option<(usize, Dialect)>, ()> {
    let (end, value) = if args[index] == "--dialect" {
        if index + 1 >= args.len() {
            errln!("--dialect requires a value");
            return Err(());
        }
        (index + 2, args[index + 1].as_str())
    } else if let Some(value) = args[index].strip_prefix("--dialect=") {
        (index + 1, value)
    } else {
        return Ok(None);
    };
    let dialect = match value {
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
    Ok(Some((end, dialect)))
}
const USAGE: &str = r#"adotenv-cli [--dialect <portable|node|python|compose|docker>] [--dialect=<value>] <command> [options]

Commands:
  get <file> <key>             Print an effective literal value
  check [--format human|json|github] [--json] [--source]
        [--recursive] [--exclude <path>] <file-or-dir>...
                                Report parse diagnostics
  fmt [--write|--check] [--sort] [--recursive] [--exclude <path>] <file-or-dir>...
                                Format documents (optionally sorted)
  diff [--format human|matrix|json] [--json] [--mask] <baseline> <file>...
                                Compare effective keys, optionally masking values
  example <file>               Print a value-free example (comments are kept)
  load <directory> [mode]      Print the standard dotenv cascade as Portable lines
  set <file> <key> <value>     Set and write one declaration
  run <directory> [mode] -- <program> [args...]
                                Run a child with the environment plus loaded values

File-reading commands accept '-' or '--stdin' for standard input. Recursive
commands skip .git, target and node_modules and do not follow symlinks.
--source may reveal secret values and is only valid for human output. get,
load and diff print values. run passes the parent environment and the loaded
values, which take precedence, and never changes the parent environment.
Command substitution from files is not enabled.

Exit codes: 0 success, 1 findings, 2 usage errors, read or write failures and
input a command cannot process. run exits with the child's code.
"#;

fn usage() {
    out!("{USAGE}");
}

fn get(args: &[String], dialect: Dialect) -> u8 {
    let args = args.strip_prefix(&[String::from("--")]).unwrap_or(args);
    if args.len() != 2 {
        errln!("usage: adotenv-cli get <file> <key>");
        return FAILURE;
    }
    let path = &args[0];
    if let Err(error) = reject_unknown_option(path) {
        errln!("{error}");
        return FAILURE;
    }
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

#[cfg(windows)]
fn is_env_file(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == ".env" || name.starts_with(".env.")
}

#[cfg(not(windows))]
fn is_env_file(name: &str) -> bool {
    name == ".env" || name.starts_with(".env.")
}

#[cfg(windows)]
fn is_ignored_dir(name: &str) -> bool {
    DEFAULT_IGNORED_DIRS
        .iter()
        .any(|ignored| name.eq_ignore_ascii_case(ignored))
}

#[cfg(not(windows))]
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

struct Discovery {
    paths: Vec<PathBuf>,
    errors: Vec<DiscoveryError>,
}

struct DiscoveryError {
    path: PathBuf,
    message: String,
}

fn walk_env_files(
    base: &Path,
    directory: &Path,
    excludes: &[PathBuf],
    found: &mut Vec<PathBuf>,
    errors: &mut Vec<DiscoveryError>,
) {
    let reader = match fs::read_dir(directory) {
        Ok(reader) => reader,
        Err(error) => {
            errors.push(DiscoveryError {
                path: directory.to_path_buf(),
                message: error.to_string(),
            });
            return;
        }
    };
    let mut entries = Vec::new();
    for entry in reader {
        match entry {
            Ok(entry) => entries.push(entry),
            Err(error) => errors.push(DiscoveryError {
                path: directory.to_path_buf(),
                message: error.to_string(),
            }),
        }
    }
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if is_excluded(base, &path, excludes) {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                errors.push(DiscoveryError {
                    path,
                    message: error.to_string(),
                });
                continue;
            }
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = entry.file_name();
            if !is_ignored_dir(&name.to_string_lossy()) {
                walk_env_files(base, &path, excludes, found, errors);
            }
        } else if file_type.is_file() {
            let name = entry.file_name();
            if is_env_file(&name.to_string_lossy()) {
                found.push(path);
            }
        }
    }
}

fn expand_paths(paths: &[String], recursive: bool, excludes: &[PathBuf]) -> Discovery {
    if !recursive {
        return Discovery {
            paths: paths.iter().map(PathBuf::from).collect(),
            errors: Vec::new(),
        };
    }
    let roots: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.iter().map(PathBuf::from).collect()
    };
    let mut discovery = Discovery {
        paths: Vec::new(),
        errors: Vec::new(),
    };
    for root_path in roots {
        if is_stdin_path(&root_path) {
            discovery.paths.push(root_path);
            continue;
        }
        let metadata = match fs::symlink_metadata(&root_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                discovery.errors.push(DiscoveryError {
                    path: root_path,
                    message: error.to_string(),
                });
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            discovery.errors.push(DiscoveryError {
                path: root_path,
                message: "refusing to follow symbolic links".to_string(),
            });
            continue;
        }
        if metadata.is_file() {
            let base = root_path.parent().unwrap_or_else(|| Path::new("."));
            if !is_excluded(base, &root_path, excludes) {
                discovery.paths.push(root_path);
            }
        } else if metadata.is_dir() {
            walk_env_files(
                &root_path,
                &root_path,
                excludes,
                &mut discovery.paths,
                &mut discovery.errors,
            );
        } else {
            discovery.errors.push(DiscoveryError {
                path: root_path,
                message: "not a regular file or directory".to_string(),
            });
        }
    }
    discovery.paths.sort();
    let mut seen_paths = BTreeSet::new();
    discovery.paths.retain(|path| {
        let key = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        seen_paths.insert(key)
    });
    let mut seen_errors = BTreeSet::new();
    discovery
        .errors
        .retain(|error| seen_errors.insert((error.path.clone(), error.message.clone())));
    discovery
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
            "--" => {
                paths.extend(args[index + 1..].iter().cloned());
                break;
            }
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
            argument => {
                reject_unknown_option(argument)?;
                paths.push(argument.to_string());
            }
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
            "--" => {
                paths.extend(args[index + 1..].iter().cloned());
                break;
            }
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
            argument => {
                reject_unknown_option(argument)?;
                paths.push(argument.to_string());
            }
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
    let Discovery {
        paths,
        errors: discovery_errors,
    } = expand_paths(&raw_paths, recursive, &excludes);
    let mut code = if discovery_errors.is_empty() {
        SUCCESS
    } else {
        FAILURE
    };
    let mut files = Vec::new();
    for error in &discovery_errors {
        let message = format!("{}: {}", error.path.display(), error.message);
        match format {
            CheckFormat::Json => files.push(format!(
                "{{\"path\":{},\"ok\":false,\"error\":{},\"diagnostics\":[]}}",
                json_string(&error.path.to_string_lossy()),
                json_string(&message)
            )),
            CheckFormat::Human => errln!("{message}"),
            CheckFormat::Github => outln!(
                "{}",
                github_read_error(&error.path.to_string_lossy(), &error.message)
            ),
        }
    }
    if paths.is_empty() {
        if discovery_errors.is_empty() {
            errln!("no .env files found");
            if format == CheckFormat::Json {
                outln!("{{\"version\":1,\"files\":[]}}");
            }
            return SUCCESS;
        }
        if format == CheckFormat::Json {
            outln!("{{\"version\":1,\"files\":[{}]}}", files.join(","));
        }
        return FAILURE;
    }
    if paths.iter().filter(|path| is_stdin_path(path)).count() > 1 {
        errln!("standard input may be used at most once");
        return FAILURE;
    }
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
        let content_start = line_content_start(source, line_start);
        let line_text = expand_tabs(&source[content_start..line_end]);
        let caret_start = floor_char_boundary(source, span.start.clamp(content_start, line_end));
        let caret_end = floor_char_boundary(source, span.end.clamp(caret_start, line_end));
        let padding = display_padding(&source[content_start..caret_start]);
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

fn line_content_start(source: &str, line_start: usize) -> usize {
    if line_start == 0 && source.starts_with('\u{feff}') {
        line_start + '\u{feff}'.len_utf8()
    } else {
        line_start
    }
}

fn source_column(source: &str, line_start: usize, offset: usize) -> usize {
    let content_start = line_content_start(source, line_start);
    let offset = floor_char_boundary(source, offset.max(content_start).min(source.len()));
    source[content_start..offset].chars().count() + 1
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
    if write && check {
        errln!("--write and --check are mutually exclusive");
        return FAILURE;
    }
    if raw_paths.is_empty() && !recursive {
        errln!(
            "usage: adotenv-cli fmt [--write|--check] [--sort] [--recursive] [--exclude <path>] <file-or-dir>..."
        );
        return FAILURE;
    }
    let Discovery {
        paths,
        errors: discovery_errors,
    } = expand_paths(&raw_paths, recursive, &excludes);
    let mut code = if discovery_errors.is_empty() {
        SUCCESS
    } else {
        FAILURE
    };
    for error in &discovery_errors {
        errln!("{}: {}", error.path.display(), error.message);
    }
    if paths.is_empty() {
        if discovery_errors.is_empty() {
            errln!("no .env files found");
            return SUCCESS;
        }
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
    for path in &paths {
        let write_target = if write {
            match resolve_write(path) {
                Ok(target) => Some(target),
                Err(error) => {
                    code = code.max(fail(format!("{}: {error}", path.display())));
                    continue;
                }
            }
        } else {
            None
        };
        let read_path = write_target
            .as_ref()
            .map_or_else(|| path.as_path(), |(target, _)| target.as_path());
        let source = match read_text(read_path) {
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
        } else if let Some((target, identity)) = &write_target {
            if formatted.as_str() != source {
                if let Err(error) = write_atomic_checked(target, formatted.as_str(), *identity) {
                    code = code.max(fail(format!("{}: {error}", path.display())));
                }
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
    mask: bool,
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
    let mut mask = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--" => {
                paths.extend(args[index + 1..].iter().cloned());
                break;
            }
            "--json" => set_diff_format(&mut format, DiffFormat::Json)?,
            "--mask" => mask = true,
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
            argument => {
                reject_unknown_option(argument)?;
                paths.push(argument.to_string());
            }
        }
        index += 1;
    }
    Ok(DiffArgs {
        paths,
        format: format.unwrap_or(DiffFormat::Human),
        mask,
    })
}

fn diff(args: &[String], dialect: Dialect) -> u8 {
    let DiffArgs {
        paths,
        format,
        mask,
    } = match parse_diff_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            errln!("{error}");
            return FAILURE;
        }
    };
    if paths.len() < 2 {
        errln!(
            "usage: adotenv-cli diff [--format human|matrix|json] [--json] [--mask] <baseline> <file>..."
        );
        return FAILURE;
    }
    if paths.iter().filter(|path| is_stdin(path)).count() > 1 {
        errln!("standard input may be used at most once");
        return FAILURE;
    }
    if format != DiffFormat::Matrix {
        return diff_streaming(&paths, format, dialect, mask);
    }

    diff_matrix_files(&paths, dialect, mask)
}

fn diff_streaming(paths: &[String], format: DiffFormat, dialect: Dialect, mask: bool) -> u8 {
    let baseline_source = match read_text(&paths[0]) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let baseline = match parse_strict_with(&baseline_source, dialect) {
        Ok(document) => document,
        Err(error) => return fail(format!("{}: {error}", paths[0])),
    };

    let mut output = String::new();
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
                writeln!(output, "{} -> {}", paths[0], path)
                    .expect("writing to String cannot fail");
                if comparison.is_empty() {
                    output.push_str("(no differences)\n");
                }
                write_comparison(&mut output, &comparison, mask);
            }
            DiffFormat::Json => {
                if index > 1 {
                    output.push(',');
                }
                write!(
                    output,
                    "{{\"path\":{},\"ok\":true,\"differences\":[{}]}}",
                    json_string(path),
                    render_comparison_json(&comparison, mask)
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
    pushed: usize,
}

impl MatrixRows {
    fn new() -> Self {
        Self {
            keys: Vec::new(),
            rows: Vec::new(),
            seen: BTreeSet::new(),
            pushed: 0,
        }
    }

    fn push(&mut self, document: &Document<'_>, mask: bool) {
        let (keys, values) = matrix_column(document, mask);
        let prior_columns = self.pushed;
        self.pushed += 1;
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

fn diff_matrix_files(paths: &[String], dialect: Dialect, mask: bool) -> u8 {
    let baseline_source = match read_text(&paths[0]) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let baseline = match parse_strict_with(&baseline_source, dialect) {
        Ok(document) => document,
        Err(error) => return fail(format!("{}: {error}", paths[0])),
    };
    let mut matrix = MatrixRows::new();
    matrix.push(&baseline, mask);

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
        matrix.push(&document, mask);
    }
    print_matrix(paths, &matrix);
    if differs {
        FINDINGS
    } else {
        SUCCESS
    }
}

fn matrix_column(document: &Document<'_>, mask: bool) -> (Vec<String>, BTreeMap<String, String>) {
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
            (key.clone(), matrix_entry_value(entry, mask))
        })
        .collect();
    (keys, values)
}

fn matrix_entry_value(entry: Option<adotenv::Entry<'_>>, mask: bool) -> String {
    let Some(entry) = entry else {
        return "<unset>".to_string();
    };
    let value = match (entry.state(), entry.value()) {
        (AssignmentState::Bare, None) => "<bare>".to_string(),
        (AssignmentState::Empty, _) => "\"\"".to_string(),
        (_, Some(_)) if mask => MASK.to_string(),
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

fn render_comparison_json(comparison: &Comparison, mask: bool) -> String {
    comparison
        .differences()
        .iter()
        .map(|entry| {
            format!(
                "{{\"key\":{},\"kind\":{},\"left\":{},\"right\":{}}}",
                json_string(&entry.key),
                json_string(difference_kind_name(entry.kind)),
                render_compared_value_json(entry.left.as_ref(), mask),
                render_compared_value_json(entry.right.as_ref(), mask)
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn render_compared_value_json(value: Option<&ComparedValue>, mask: bool) -> String {
    let Some(value) = value else {
        return "null".to_string();
    };
    format!(
        "{{\"state\":{},\"value\":{},\"exported\":{}}}",
        json_string(assignment_state_name(value.state)),
        value.value.as_deref().map_or_else(
            || "null".to_string(),
            |value| {
                if mask && !value.is_empty() {
                    json_string(MASK)
                } else {
                    json_string(value)
                }
            }
        ),
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
    let args = args.strip_prefix(&[String::from("--")]).unwrap_or(args);
    if args.len() != 1 {
        errln!("usage: adotenv-cli example <file>");
        return FAILURE;
    }
    if let Err(error) = reject_unknown_option(&args[0]) {
        errln!("{error}");
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
    let args = args.strip_prefix(&[String::from("--")]).unwrap_or(args);
    if args.is_empty() || args.len() > 2 {
        errln!("usage: adotenv-cli load <directory> [mode]");
        return FAILURE;
    }
    if let Err(error) = reject_unknown_option(&args[0]) {
        errln!("{error}");
        return FAILURE;
    }
    if is_stdin(&args[0]) {
        errln!("load does not read standard input");
        return FAILURE;
    }
    let mode = args.get(1).map_or("", String::as_str);
    if let Err(error) = reject_unknown_option(mode) {
        errln!("{error}");
        return FAILURE;
    }
    let loaded = match load_cascade_with(&args[0], mode, dialect) {
        Ok(loaded) => loaded,
        Err(error) => return fail(error.to_string()),
    };
    match render_portable(&loaded) {
        Ok(output) => {
            out!("{output}");
            SUCCESS
        }
        Err(error) => fail(error),
    }
}

/// Render loaded values as Portable `KEY=value` lines that parse back to the
/// same keys and values.
///
/// Values are encoded with [`adotenv::encode_value`], which quotes and escapes
/// as needed and has no spelling for control characters other than tab, CR
/// and LF. The whole output is built first, so nothing is printed when one
/// key or value has no exact Portable spelling.
fn render_portable(loaded: &LoadedEnvironment) -> Result<String, String> {
    let mut output = String::new();
    for (key, value) in loaded.values() {
        let token = adotenv::encode_value(value).ok_or_else(|| {
            format!("cannot print {key:?}: the value has no exact Portable spelling")
        })?;
        let line = format!("{key}={token}\n");
        let exact = parse_strict_with(&line, Dialect::PORTABLE).is_ok_and(|document| {
            document.entries().count() == 1 && document.get(key) == Some(value)
        });
        if !exact {
            return Err(format!(
                "cannot print {key:?}: the key is not a Portable key"
            ));
        }
        output.push_str(&line);
    }
    Ok(output)
}

fn set(args: &[String], dialect: Dialect) -> u8 {
    let args = args.strip_prefix(&[String::from("--")]).unwrap_or(args);
    if args.len() != 3 {
        errln!("usage: adotenv-cli set <file> <key> <value>");
        return FAILURE;
    }
    if is_stdin(&args[0]) {
        errln!("set cannot write standard input");
        return FAILURE;
    }
    let (target, identity) = match resolve_write(Path::new(&args[0])) {
        Ok(target) => target,
        Err(error) => return fail(format!("{}: {error}", args[0])),
    };
    let source = match read_text(&target) {
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
    if edited.as_str() != source {
        if let Err(error) = write_atomic_checked(&target, edited.as_str(), identity) {
            return fail(format!("{}: {error}", args[0]));
        }
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
    for argument in &args[..separator] {
        if let Err(error) = reject_unknown_option(argument) {
            errln!("{error}");
            return FAILURE;
        }
    }
    if is_stdin(&args[0]) {
        errln!("run does not read standard input");
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
        Ok(status) => child_exit_code(status),
        Err(error) => fail(format!("failed to run {program}: {error}")),
    }
}

/// Exit code for a child status that has no exact 8-bit equivalent.
const CHILD_UNREPRESENTABLE: u8 = 255;

/// The exit code that reports a child's status.
fn child_exit_code(status: std::process::ExitStatus) -> u8 {
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    };
    #[cfg(not(unix))]
    let signal = None;
    exit_code_for(status.code(), signal)
}

/// Map a child's exit code or terminating signal to an 8-bit exit code.
///
/// Codes `0` to `255` are kept. A Unix signal `N` becomes `128 + N`, as in
/// POSIX shells. Anything else, such as a negative Windows `NTSTATUS` or a code
/// above `255`, becomes `255`, so a failure is never reported as success.
fn exit_code_for(code: Option<i32>, signal: Option<i32>) -> u8 {
    if let Some(code) = code {
        return u8::try_from(code).unwrap_or(CHILD_UNREPRESENTABLE);
    }
    signal
        .filter(|signal| (1..128).contains(signal))
        .and_then(|signal| u8::try_from(128 + signal).ok())
        .unwrap_or(CHILD_UNREPRESENTABLE)
}

fn validate_child_environment(loaded: &LoadedEnvironment) -> Result<(), String> {
    #[cfg(windows)]
    let mut seen = BTreeSet::new();
    for (key, value) in loaded.values() {
        if key.is_empty() || key.bytes().any(|byte| byte == 0 || byte == b'=') {
            return Err(format!("invalid child environment key: {key:?}"));
        }
        if value.as_bytes().contains(&0) {
            return Err(format!("child environment value for {key:?} contains NUL"));
        }
        #[cfg(windows)]
        {
            if !seen.insert(key.to_lowercase()) {
                return Err(format!(
                    "child environment key collides case-insensitively: {key:?}"
                ));
            }
            if value.encode_utf16().count() > 32_767 {
                return Err(format!(
                    "child environment value for {key:?} exceeds 32767 UTF-16 code units"
                ));
            }
        }
    }
    Ok(())
}

fn write_comparison(output: &mut String, comparison: &Comparison, mask: bool) {
    for entry in comparison.differences() {
        let result = match entry.kind {
            DifferenceKind::OnlyLeft => {
                writeln!(
                    output,
                    "- {}={}",
                    entry.key,
                    describe(entry.left.as_ref(), mask)
                )
            }
            DifferenceKind::OnlyRight => {
                writeln!(
                    output,
                    "+ {}={}",
                    entry.key,
                    describe(entry.right.as_ref(), mask)
                )
            }
            DifferenceKind::State | DifferenceKind::Value | DifferenceKind::Export => writeln!(
                output,
                "~ {}: {} -> {}",
                entry.key,
                describe(entry.left.as_ref(), mask),
                describe(entry.right.as_ref(), mask)
            ),
            _ => writeln!(output, "~ {}: <unknown difference>", entry.key),
        };
        result.expect("writing to String cannot fail");
    }
}

fn describe(value: Option<&ComparedValue>, mask: bool) -> String {
    let Some(value) = value else {
        return "<unset>".to_string();
    };
    let rendered = match (value.state, value.value.as_deref()) {
        (AssignmentState::Bare, None) => "<bare>".to_string(),
        (AssignmentState::Empty, _) => "\"\"".to_string(),
        (_, Some(_)) if mask => MASK.to_string(),
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

/// Maximum bytes of the original file name kept in a temporary name.
///
/// The suffix includes the PID and counter; 160 keeps the whole component
/// under the common 255-byte NAME_MAX limit.
const TEMP_STEM_BYTES: usize = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileIdentity {
    len: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}

fn file_identity(path: &Path) -> io::Result<FileIdentity> {
    let metadata = fs::metadata(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(FileIdentity {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            dev: metadata.dev(),
            ino: metadata.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        Ok(FileIdentity {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }
}

/// Resolve the write target once, before reading, and capture its identity.
///
/// Reading and replacing the same resolved path keeps a symbolic link swapped
/// after this point from redirecting the write. On Unix the device and inode
/// are compared again just before the replace.
fn resolve_write(path: &Path) -> io::Result<(PathBuf, FileIdentity)> {
    let target = fs::canonicalize(path)?;
    let metadata = fs::metadata(&target)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path is not a regular file",
        ));
    }
    let identity = file_identity(&target)?;
    Ok((target, identity))
}

fn write_atomic_checked(path: &Path, contents: &str, identity: FileIdentity) -> io::Result<()> {
    write_atomic_inner(path, contents, Some(identity))
}

/// Fail unless `path` still names the resolved regular file whose identity was
/// captured before the read. The identity includes size and modification time
/// as well as device and inode on Unix, so an in-place rewrite is detected
/// when the file system advances the timestamp.
fn check_target(path: &Path, expected: FileIdentity) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || file_identity(path)? != expected {
        return Err(io::Error::other("file changed after it was read"));
    }
    Ok(())
}

/// Replace `path` with `contents` through a temporary file in the same directory.
///
/// When `path` is a symbolic link, the file it finally points to is replaced
/// in that file's directory and the link is kept. The temporary file is created
/// private on Unix (mode `0600`), receives the existing file's owner and group
/// where possible, and receives its permissions only after the contents are
/// written. A file that does not exist yet stays private.
fn write_atomic_inner(
    path: &Path,
    contents: &str,
    expected_identity: Option<FileIdentity>,
) -> io::Result<()> {
    // A checked write already resolved its target, so resolve it again only
    // for the unchecked legacy helper. In particular, do not follow a symbolic
    // link that appeared at the resolved target after the read.
    let target = match expected_identity {
        Some(_) => path.to_path_buf(),
        None => replacement_target(path)?,
    };
    let path = target.as_path();
    if let Some(expected) = expected_identity {
        check_target(path, expected)?;
    }
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
    if let Some(expected) = expected_identity {
        if let Err(error) = check_target(path, expected) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
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
    let name = name.to_string_lossy();
    let mut stem_end = name.len().min(TEMP_STEM_BYTES);
    while !name.is_char_boundary(stem_end) {
        stem_end -= 1;
    }
    let stem = &name[..stem_end];
    for _ in 0..16 {
        let candidate = parent.join(format!(
            ".adotenv-tmp-{}-{}-{stem}",
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
            ".adotenv-backup-{}-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed),
            name.to_string_lossy()
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

fn reject_unknown_option(argument: &str) -> Result<(), String> {
    if argument.starts_with('-') && argument != "-" && !is_stdin(argument) {
        Err(format!("unknown option: {argument}"))
    } else {
        Ok(())
    }
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
        let metadata =
            fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!("{}: path is not a regular file", path.display()));
        }
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
    use super::{
        create_temporary, github_escape_data, github_escape_property, json_string,
        write_atomic_inner, NEXT_TEMP,
    };
    #[cfg(unix)]
    use super::{read_text, resolve_write, write_atomic_checked};
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
    fn github_escaping_vectors_match() {
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/vectors/github-escaping.json")).unwrap();
        assert_eq!(
            vectors["version"].as_u64(),
            Some(1),
            "GitHub vectors version"
        );
        for case in vectors["cases"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            let expected = case["expected"].as_str().unwrap();
            let actual = match case["kind"].as_str().unwrap() {
                "data" => github_escape_data(input),
                "property" => github_escape_property(input),
                other => panic!("unknown GitHub escaping kind {other}"),
            };
            assert_eq!(actual, expected, "{input:?}");
        }
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
        write_atomic_inner(&target, "A=1\n", None).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "A=1\n");
        let leftovers: Vec<_> = std::fs::read_dir(&directory).unwrap().collect();
        assert_eq!(leftovers.len(), 1);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn checked_write_rejects_a_replaced_target() {
        let directory = std::env::temp_dir().join(format!(
            "adotenv-write-identity-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let target = directory.join("app.env");
        std::fs::write(&target, "A=1\n").unwrap();
        let (resolved, identity) = resolve_write(&target).unwrap();

        let replacement = directory.join("replacement.env");
        std::fs::write(&replacement, "A=2\n").unwrap();
        std::fs::rename(&replacement, &resolved).unwrap();

        let error = write_atomic_checked(&resolved, "A=3\n", identity).unwrap_err();
        assert!(error.to_string().contains("changed after it was read"));
        assert_eq!(std::fs::read_to_string(&resolved).unwrap(), "A=2\n");
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn checked_write_does_not_follow_a_target_replaced_by_a_symlink() {
        use std::os::unix::fs::symlink;

        let directory = std::env::temp_dir().join(format!(
            "adotenv-write-symlink-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let target = directory.join("app.env");
        std::fs::write(&target, "A=1\n").unwrap();
        let (resolved, identity) = resolve_write(&target).unwrap();

        let elsewhere = directory.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let victim = elsewhere.join("victim.env");
        std::fs::write(&victim, "A=safe\n").unwrap();
        std::fs::remove_file(&resolved).unwrap();
        symlink(&victim, &resolved).unwrap();

        let error = write_atomic_checked(&resolved, "A=3\n", identity).unwrap_err();
        assert!(error.to_string().contains("changed after it was read"));
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "A=safe\n");
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 1);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn temporary_names_fit_common_name_limits() {
        let directory = std::env::temp_dir().join(format!(
            "adotenv-write-name-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let long = "a".repeat(254);
        let (temporary, file) = create_temporary(&directory, std::ffi::OsStr::new(&long)).unwrap();
        let name = temporary.file_name().unwrap().to_string_lossy();
        assert!(name.len() <= 255, "temporary name is {} bytes", name.len());
        drop(file);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn write_atomic_rejects_a_directory_target() {
        let directory = std::env::temp_dir().join(format!(
            "adotenv-write-directory-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let target = directory.join("target.env");
        std::fs::create_dir(&target).unwrap();
        let error = write_atomic_inner(&target, "A=1\n", None).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn checked_write_rejects_an_in_place_change() {
        let directory = std::env::temp_dir().join(format!(
            "adotenv-write-identity-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let target = directory.join("app.env");
        std::fs::write(&target, "A=1\n").unwrap();
        let (target, identity) = resolve_write(&target).unwrap();
        std::fs::write(&target, "A=1\nB=2\n").unwrap();
        assert!(write_atomic_checked(&target, "A=3\n", identity).is_err());
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn non_regular_inputs_are_rejected_without_reading() {
        assert!(read_text("/dev/null").is_err());
        assert!(resolve_write(std::path::Path::new("/dev/null")).is_err());
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
        assert!(temporary.exists());
        assert!(!temporary
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".env."));
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
    fn child_statuses_map_to_exit_codes() {
        use super::exit_code_for;

        for code in [0, 1, 7, 128, 255] {
            assert_eq!(i32::from(exit_code_for(Some(code), None)), code);
        }
        // Windows reports crashes as NTSTATUS values, which are negative as i32.
        let access_violation = i32::from_ne_bytes(0xC000_0005_u32.to_ne_bytes());
        let stack_overflow = i32::from_ne_bytes(0xC000_00FD_u32.to_ne_bytes());
        for code in [
            256,
            1 << 16,
            i32::MAX,
            -1,
            i32::MIN,
            access_violation,
            stack_overflow,
        ] {
            assert_eq!(exit_code_for(Some(code), None), 255, "{code}");
        }
        assert_eq!(exit_code_for(None, Some(1)), 129);
        assert_eq!(exit_code_for(None, Some(9)), 137);
        assert_eq!(exit_code_for(None, Some(15)), 143);
        assert_eq!(exit_code_for(None, Some(127)), 255);
        for signal in [0, -1, 128, i32::MAX, i32::MIN] {
            assert_eq!(exit_code_for(None, Some(signal)), 255, "{signal}");
        }
        assert_eq!(exit_code_for(None, None), 255);
    }

    #[test]
    fn json_strings_escape_controls_quotes_and_backslashes() {
        assert_eq!(
            json_string("quote\" slash\\ line\n tab\t control\u{1}"),
            "\"quote\\\" slash\\\\ line\\n tab\\t control\\u0001\""
        );
    }
}
