//! Small zero-dependency CLI for adotenv documents.

use std::{
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};
#[cfg(unix)]
use std::{
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

use adotenv::{
    load::{load_cascade_with, LoadedEnvironment},
    parse_strict_with, parse_with, AssignmentState, ComparedValue, Comparison, Diagnostic, Dialect,
    DifferenceKind, FormatOptions, Severity,
};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    ExitCode::from(run(&args))
}

fn run(args: &[String]) -> u8 {
    let mut args = args.to_vec();
    let dialect = match extract_dialect(&mut args) {
        Ok(dialect) => dialect,
        Err(()) => return 2,
    };
    let Some(command) = args.first().map(String::as_str) else {
        usage();
        return 2;
    };
    match command {
        "-h" | "--help" | "help" => {
            usage();
            0
        }
        "--version" | "version" => {
            println!("adotenv-cli {}", env!("CARGO_PKG_VERSION"));
            0
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
            eprintln!("unknown command: {command}");
            usage();
            2
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
        eprintln!("--dialect requires a value");
        return Err(());
    }
    let dialect = match args[position + 1].as_str() {
        "portable" => Dialect::PORTABLE,
        "node" => Dialect::NODE_DOTENV,
        "python" => Dialect::PYTHON_DOTENV,
        "compose" => Dialect::COMPOSE,
        "docker" | "docker-raw" => Dialect::DOCKER_RAW,
        other => {
            eprintln!("unknown dialect: {other}");
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
  diff <left> <right>          Compare effective keys
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
    print!("{USAGE}");
}

fn get(args: &[String], dialect: Dialect) -> u8 {
    if args.len() != 2 {
        eprintln!("usage: adotenv-cli get <file> <key>");
        return 2;
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
            println!("{value}");
            0
        }
        None => {
            eprintln!("missing key: {}", args[1]);
            1
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
            relative == exclude || relative.starts_with(exclude)
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
) -> Result<Vec<String>, String> {
    if !recursive {
        return Ok(paths.to_vec());
    }
    let roots = if paths.is_empty() {
        vec![".".to_string()]
    } else {
        paths.to_vec()
    };
    let mut found = Vec::new();
    for root in roots {
        if is_stdin(&root) {
            found.push(PathBuf::from(root));
            continue;
        }
        let root_path = Path::new(&root);
        let metadata =
            fs::symlink_metadata(root_path).map_err(|error| format!("{root}: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!("{root}: refusing to follow symbolic links"));
        }
        if metadata.is_file() {
            let base = root_path.parent().unwrap_or_else(|| Path::new("."));
            if !is_excluded(base, root_path, excludes) {
                found.push(root_path.to_path_buf());
            }
        } else if metadata.is_dir() {
            walk_env_files(root_path, root_path, excludes, &mut found)?;
        } else {
            return Err(format!("{root}: not a regular file or directory"));
        }
    }
    let mut found: Vec<String> = found
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
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
            eprintln!("{error}");
            return 2;
        }
    };
    if raw_paths.is_empty() && !recursive {
        eprintln!(
            "usage: adotenv-cli check [--format human|json|github] [--json] [--source] [--recursive] [--exclude <path>] <file-or-dir>..."
        );
        return 2;
    }
    let paths = match expand_paths(&raw_paths, recursive, &excludes) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    if paths.is_empty() {
        eprintln!("no .env files found");
        if format == CheckFormat::Json {
            println!("{{\"version\":1,\"files\":[]}}");
        }
        return 0;
    }
    if paths.iter().filter(|path| is_stdin(path)).count() > 1 {
        eprintln!("standard input may be used at most once");
        return 2;
    }
    let mut failed = false;
    let mut files = Vec::new();
    let mut errors = 0usize;
    let mut warnings = 0usize;
    let mut infos = 0usize;
    let mut affected_files = 0usize;
    for path in &paths {
        let source = match read_text(path) {
            Ok(source) => source,
            Err(error) => {
                failed = true;
                errors += 1;
                affected_files += 1;
                match format {
                    CheckFormat::Json => files.push(format!(
                        "{{\"path\":{},\"ok\":false,\"error\":{},\"diagnostics\":[]}}",
                        json_string(path),
                        json_string(&error)
                    )),
                    CheckFormat::Human => eprintln!("{error}"),
                    CheckFormat::Github => println!("{}", github_read_error(path, &error)),
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
                    println!("{}", render_human(path, &source, diagnostic, show_source));
                }
                CheckFormat::Github => {
                    println!("{}", github_annotation(path, &source, diagnostic));
                }
            }
        }
        if file_has_error {
            failed = true;
        }
        if !diagnostics.is_empty() {
            affected_files += 1;
        }
        if format == CheckFormat::Json {
            files.push(format!(
                "{{\"path\":{},\"ok\":{},\"diagnostics\":[{}]}}",
                json_string(path),
                !file_has_error,
                rendered.join(",")
            ));
        }
    }
    match format {
        CheckFormat::Json => println!("{{\"version\":1,\"files\":[{}]}}", files.join(",")),
        CheckFormat::Human if affected_files > 0 => println!(
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
    u8::from(failed)
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
            eprintln!("{error}");
            return 2;
        }
    };
    if raw_paths.is_empty() && !recursive {
        eprintln!(
            "usage: adotenv-cli fmt [--write|--check] [--sort] [--recursive] [--exclude <path>] <file-or-dir>..."
        );
        return 2;
    }
    let paths = match expand_paths(&raw_paths, recursive, &excludes) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    if paths.is_empty() {
        eprintln!("no .env files found");
        return 0;
    }
    if write && check {
        eprintln!("--write and --check are mutually exclusive");
        return 2;
    }
    if paths.iter().filter(|path| is_stdin(path)).count() > 1 {
        eprintln!("standard input may be used at most once");
        return 2;
    }
    if write && paths.iter().any(|path| is_stdin(path)) {
        eprintln!("--write cannot write standard input");
        return 2;
    }
    let mut failed = false;
    for path in &paths {
        let source = match read_text(path) {
            Ok(source) => source,
            Err(error) => {
                fail(error);
                failed = true;
                continue;
            }
        };
        let (doc, diagnostics) = parse_with(&source, dialect);
        let has_errors = diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == Severity::Error);
        for diagnostic in diagnostics {
            eprintln!("{path}: {diagnostic}");
        }
        let formatting_source;
        let formatting_doc;
        let doc_ref = if sort {
            let sorted = match doc.sort() {
                Ok(sorted) => sorted,
                Err(error) => {
                    fail(format!("{path}: {error}"));
                    failed = true;
                    continue;
                }
            };
            formatting_source = sorted.into_string();
            formatting_doc = match parse_strict_with(&formatting_source, dialect) {
                Ok(doc) => doc,
                Err(error) => {
                    fail(format!("{path}: {error}"));
                    failed = true;
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
                fail(format!("{path}: {error}"));
                failed = true;
                continue;
            }
        };
        if check {
            if formatted.as_str() != source {
                println!("{path}: would reformat");
                failed = true;
            }
        } else if write {
            if let Err(error) = write_atomic(Path::new(path), formatted.as_str()) {
                fail(format!("{path}: {error}"));
                failed = true;
            }
        } else if paths.len() > 1 {
            println!("==> {path} <==");
            print!("{}", formatted.as_str());
        } else {
            print!("{}", formatted.as_str());
        }
        if has_errors {
            failed = true;
        }
    }
    u8::from(failed)
}

fn diff(args: &[String], dialect: Dialect) -> u8 {
    if args.len() != 2 {
        eprintln!("usage: adotenv-cli diff <left> <right>");
        return 2;
    }
    if is_stdin(&args[0]) && is_stdin(&args[1]) {
        eprintln!("standard input may be used at most once");
        return 2;
    }
    let left_source = match read_text(&args[0]) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let right_source = match read_text(&args[1]) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let left = match parse_strict_with(&left_source, dialect) {
        Ok(doc) => doc,
        Err(error) => return fail(format!("{}: {error}", args[0])),
    };
    let right = match parse_strict_with(&right_source, dialect) {
        Ok(doc) => doc,
        Err(error) => return fail(format!("{}: {error}", args[1])),
    };
    let comparison = left.compare(&right);
    print_comparison(&comparison);
    u8::from(!comparison.is_empty())
}

fn example(args: &[String], dialect: Dialect) -> u8 {
    if args.len() != 1 {
        eprintln!("usage: adotenv-cli example <file>");
        return 2;
    }
    let source = match read_text(&args[0]) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let (doc, diagnostics) = parse_with(&source, dialect);
    for diagnostic in diagnostics {
        eprintln!("{}: {diagnostic}", args[0]);
    }
    print!("{}", doc.example());
    0
}

fn load(args: &[String], dialect: Dialect) -> u8 {
    if args.is_empty() || args.len() > 2 {
        eprintln!("usage: adotenv-cli load <directory> [mode]");
        return 2;
    }
    let mode = args.get(1).map_or("", String::as_str);
    let loaded = match load_cascade_with(&args[0], mode, dialect) {
        Ok(loaded) => loaded,
        Err(error) => return fail(error.to_string()),
    };
    for (key, value) in loaded.values() {
        println!("{key}={value}");
    }
    0
}

fn set(args: &[String], dialect: Dialect) -> u8 {
    if args.len() != 3 {
        eprintln!("usage: adotenv-cli set <file> <key> <value>");
        return 2;
    }
    if is_stdin(&args[0]) {
        eprintln!("set cannot write standard input");
        return 2;
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
    0
}

fn run_command(args: &[String], dialect: Dialect) -> u8 {
    let Some(separator) = args.iter().position(|arg| arg == "--") else {
        eprintln!("usage: adotenv-cli run <directory> [mode] -- <program> [args...]");
        return 2;
    };
    if separator == 0 || separator + 1 >= args.len() || separator > 2 {
        eprintln!("usage: adotenv-cli run <directory> [mode] -- <program> [args...]");
        return 2;
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

fn print_comparison(comparison: &Comparison) {
    for entry in comparison.differences() {
        match entry.kind {
            DifferenceKind::OnlyLeft => {
                println!("- {}={}", entry.key, describe(entry.left.as_ref()));
            }
            DifferenceKind::OnlyRight => {
                println!("+ {}={}", entry.key, describe(entry.right.as_ref()));
            }
            DifferenceKind::State | DifferenceKind::Value | DifferenceKind::Export => {
                println!(
                    "~ {}: {} -> {}",
                    entry.key,
                    describe(entry.left.as_ref()),
                    describe(entry.right.as_ref())
                );
            }
            _ => {
                println!("~ {}: <unknown difference>", entry.key);
            }
        }
    }
}

fn describe(value: Option<&ComparedValue>) -> String {
    let Some(value) = value else {
        return "<unset>".to_string();
    };
    match (value.state, value.value.as_deref()) {
        (AssignmentState::Bare, None) => "<bare>".to_string(),
        (AssignmentState::Empty, _) => "\"\"".to_string(),
        (_, Some(value)) => value.to_string(),
        (_, None) => "<unset>".to_string(),
    }
}

fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    #[cfg(unix)]
    {
        static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
        let mut temporary: Option<PathBuf> = None;
        for _ in 0..16 {
            let candidate = parent.join(format!(
                ".{}.adotenv-{}-{}",
                name.to_string_lossy(),
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(mut file) => {
                    if let Err(error) = file
                        .write_all(contents.as_bytes())
                        .and_then(|()| file.sync_all())
                    {
                        let _ = fs::remove_file(&candidate);
                        return Err(error);
                    }
                    temporary = Some(candidate);
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        let Some(temporary) = temporary else {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not create a temporary file",
            ));
        };
        match fs::metadata(path) {
            Ok(metadata) => {
                if let Err(error) = fs::set_permissions(&temporary, metadata.permissions()) {
                    let _ = fs::remove_file(&temporary);
                    return Err(error);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                return Err(error);
            }
        }
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::write(path, contents)
    }
}

fn is_stdin(path: &str) -> bool {
    path == "-" || path == "--stdin"
}

fn read_text(path: &str) -> Result<String, String> {
    let bytes = if is_stdin(path) {
        let mut bytes = Vec::new();
        io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|error| format!("-: {error}"))?;
        bytes
    } else {
        fs::read(Path::new(path)).map_err(|error| format!("{path}: {error}"))?
    };
    String::from_utf8(bytes).map_err(|_| format!("{path}: invalid UTF-8"))
}

fn fail(message: String) -> u8 {
    eprintln!("{message}");
    2
}

#[cfg(test)]
mod tests {
    use super::{github_escape_data, github_escape_property, json_string};

    #[test]
    fn github_workflow_values_are_escaped() {
        assert_eq!(github_escape_property("a,b:c%\n"), "a%2Cb%3Ac%25%0A");
        assert_eq!(github_escape_data("a,b:c%\r\n"), "a,b:c%25%0D%0A");
    }

    #[test]
    fn json_strings_escape_controls_quotes_and_backslashes() {
        assert_eq!(
            json_string("quote\" slash\\ line\n tab\t control\u{1}"),
            "\"quote\\\" slash\\\\ line\\n tab\\t control\\u0001\""
        );
    }
}
