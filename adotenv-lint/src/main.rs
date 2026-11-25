//! Command-line interface for `adotenv-lint`.
//!
//! Without `--fix` the command is report-only. With `--fix` it applies verified
//! library fixes and replaces changed files through a temporary file in the
//! same directory, preserving the target's permissions where possible.

use std::{
    collections::BTreeSet,
    env,
    fmt::Write as _,
    fs,
    io::{self, Read as _, Write as IoWrite},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use adotenv::{parse_strict_with_options, Dialect, ParseOptions, Severity};
use adotenv_lint::{
    apply_batch, lint, load_effective_config, parse_dialect, parse_equals_spacing as parse_spacing,
    parse_key_case, parse_output_format as parse_format, parse_quote_style, ConfigExplicit,
    Finding, Fix, IgnoredPaths, LintOptions, LintPolicy, OutputFormat,
};

const SUCCESS: u8 = 0;
const FINDINGS: u8 = 1;
const FAILURE: u8 = 2;

const USAGE: &str = r"adotenv-lint [options] <file-or-directory>...

Lint .env files using adotenv's literal document model.

Options:
  --dialect <portable|node|python|compose|docker>
  --format <human|json|github>
  -r, --recursive             Search directories for .env and .env.* files
  --key-order                 Require sorted declarations
  --key-case <lower|upper>    Require key case
  --quote-style <double|single|unquoted|preserve>
  --equals-spacing <none|spaces|preserve>
  --trim-trailing-whitespace  Require trimmed trailing whitespace
  --config <path>             Read policy from a TOML file
  --exclude <pattern>         Ignore paths matching a glob
  --max-input-bytes <n>       Reject files larger than <n> bytes
  --max-lines <n>             Reject files with more than <n> physical lines
  --fix                       Apply verified fixes and write changed files
  --                          End option parsing
  -h, --help                  Print this usage
  --version                   Print the version

Exit codes: 0 no findings (or all findings fixed), 1 remaining findings,
2 usage/read/parse/write failure.

With no path, searches the current directory recursively. The default rule
reports duplicate keys. Fixes are verified through the library before they
are written.";

type Format = OutputFormat;

// These independent flags record which command-line values override config.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default)]
struct Explicit {
    dialect: bool,
    format: bool,
    recursive: bool,
    key_order: bool,
    key_case: bool,
    quote_style: bool,
    equals_spacing: bool,
    trim_trailing_whitespace: bool,
    exclude: bool,
}

#[derive(Debug)]
struct Options {
    dialect: Dialect,
    parse_limits: ParseOptions,
    format: Format,
    recursive: bool,
    fix: bool,
    lint: LintOptions,
    paths: Vec<String>,
    config: Option<String>,
    exclude: Vec<String>,
    explicit: Explicit,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(FAILURE)
        }
    }
}

static STDOUT_FAILED: AtomicBool = AtomicBool::new(false);

fn report_stdout_error(error: &io::Error) {
    if error.kind() != io::ErrorKind::BrokenPipe && !STDOUT_FAILED.swap(true, Ordering::Relaxed) {
        let _ = writeln!(io::stderr().lock(), "adotenv-lint: stdout error: {error}");
    }
}

fn write_stdout(text: &str) {
    let mut stdout = io::stdout().lock();
    if let Err(error) = stdout.write_all(text.as_bytes()) {
        report_stdout_error(&error);
    }
}

fn write_stdout_line(text: &str) {
    let mut stdout = io::stdout().lock();
    if let Err(error) = writeln!(stdout, "{text}") {
        report_stdout_error(&error);
    }
}

fn flush_stdout() {
    if let Err(error) = io::stdout().lock().flush() {
        report_stdout_error(&error);
    }
}

fn finish_output(code: u8) -> u8 {
    flush_stdout();
    if STDOUT_FAILED.load(Ordering::Relaxed) {
        FAILURE
    } else {
        code
    }
}

fn read_file_bounded(path: &Path, max_input_bytes: Option<usize>) -> Result<String, String> {
    if let Ok(metadata) = fs::metadata(path) {
        if !metadata.is_file() {
            return Err("path is not a regular file".to_owned());
        }
    }
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("path is not a regular file".to_owned());
    }
    if let Some(limit) = max_input_bytes {
        let observed = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        if observed > limit {
            return Err(format!(
                "input exceeds max_input_bytes limit ({limit} bytes)"
            ));
        }
    }
    let mut bytes = Vec::new();
    let read_result = match max_input_bytes {
        Some(limit) => file
            .take(limit.saturating_add(1) as u64)
            .read_to_end(&mut bytes),
        None => file.read_to_end(&mut bytes),
    };
    read_result.map_err(|error| error.to_string())?;
    if let Some(limit) = max_input_bytes {
        if bytes.len() > limit {
            return Err(format!(
                "input exceeds max_input_bytes limit ({limit} bytes)"
            ));
        }
    }
    String::from_utf8(bytes).map_err(|_| "invalid UTF-8".to_owned())
}

fn run() -> Result<u8, String> {
    let args = env::args_os()
        .skip(1)
        .map(|argument| {
            argument
                .into_string()
                .map_err(|_| "arguments must be valid UTF-8".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut options = match parse_cli_args(&args)? {
        CliAction::Help => {
            write_stdout_line(USAGE);
            return Ok(finish_output(SUCCESS));
        }
        CliAction::Version => {
            write_stdout_line(&format!("adotenv-lint {}", env!("CARGO_PKG_VERSION")));
            return Ok(finish_output(SUCCESS));
        }
        CliAction::Run(options) => *options,
    };
    apply_config(&mut options)?;
    if options.paths.is_empty() {
        options.paths.push(".".to_owned());
        options.recursive = true;
    }
    let paths = collect_paths(&options.paths, options.recursive, &options.exclude)?;
    let mut total = 0usize;
    let mut had_error = false;

    if options.format == Format::Json {
        write_stdout("[");
    }
    let mut printed = 0usize;
    for path in paths {
        let source = match read_file_bounded(&path, options.parse_limits.max_input_bytes) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("{}: {error}", path.display());
                had_error = true;
                continue;
            }
        };
        let document =
            match parse_strict_with_options(&source, options.dialect, options.parse_limits) {
                Ok(document) => document,
                Err(error) => {
                    eprintln!("{}: {error}", path.display());
                    had_error = true;
                    continue;
                }
            };
        let mut findings = lint(&document, options.lint);
        if options.fix && !findings.is_empty() {
            match fix_file(&path, source, &options) {
                Ok(remaining) => findings = remaining,
                Err(error) => {
                    eprintln!("{}: {error}", path.display());
                    had_error = true;
                    findings = Vec::new();
                }
            }
        }
        total += findings.len();
        match options.format {
            Format::Json => {
                for finding in &findings {
                    if printed > 0 {
                        write_stdout(",");
                    }
                    print_json(&path, finding);
                    printed += 1;
                }
            }
            Format::Github => print_github(&path, &findings),
            _ => print_human(&path, &findings),
        }
    }
    if options.format == Format::Json {
        write_stdout_line("]");
    }

    if had_error {
        return Ok(finish_output(FAILURE));
    }
    Ok(finish_output(if total == 0 { SUCCESS } else { FINDINGS }))
}

const MAX_FIX_ROUNDS: usize = 64;
const TEMP_NAME_BYTES: usize = 160;
static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileIdentity {
    len: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}

fn file_identity_from_metadata(metadata: &fs::Metadata) -> FileIdentity {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        FileIdentity {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            dev: metadata.dev(),
            ino: metadata.ino(),
        }
    }
    #[cfg(not(unix))]
    {
        FileIdentity {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}

fn file_identity(path: &Path) -> io::Result<FileIdentity> {
    Ok(file_identity_from_metadata(&fs::metadata(path)?))
}

fn fix_file(path: &Path, mut source: String, options: &Options) -> Result<Vec<Finding>, String> {
    let target = fs::canonicalize(path).map_err(|error| error.to_string())?;
    let identity = file_identity(&target).map_err(|error| error.to_string())?;
    let current = read_file_bounded(&target, options.parse_limits.max_input_bytes)?;
    if current != source {
        return Err("file changed after it was read".to_owned());
    }
    let original = source.clone();
    for _ in 0..MAX_FIX_ROUNDS {
        let document = parse_strict_with_options(&source, options.dialect, options.parse_limits)
            .map_err(|error| format!("cannot parse after fix: {error}"))?;
        let findings = lint(&document, options.lint);
        let fixes: Vec<Fix> = findings
            .iter()
            .filter_map(|finding| finding.fix().cloned())
            .collect();
        if fixes.is_empty() {
            if source != original {
                write_fixed(&target, &source, identity)?;
            }
            return Ok(findings);
        }
        let batch = apply_batch(&document, &fixes).map_err(|error| error.to_string())?;
        if batch.output() == source {
            if source != original {
                write_fixed(&target, &source, identity)?;
            }
            return Ok(findings);
        }
        source.clear();
        source.push_str(batch.output());
    }
    Err("fixes did not converge".to_owned())
}

fn write_fixed(path: &Path, source: &str, expected: FileIdentity) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let parent = parent.unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| "target has no file name".to_owned())?
        .to_string_lossy();
    let mut stem_end = name.len().min(TEMP_NAME_BYTES);
    while !name.is_char_boundary(stem_end) {
        stem_end -= 1;
    }
    let name = &name[..stem_end];
    let mut temporary = None;
    for _ in 0..16 {
        let candidate = parent.join(format!(
            ".adotenv-lint-{}-{}-{name}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut open = fs::OpenOptions::new();
        open.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open.mode(0o600);
        }
        match open.open(&candidate) {
            Ok(mut file) => {
                if let Err(error) = file
                    .write_all(source.as_bytes())
                    .and_then(|()| file.sync_all())
                {
                    let _ = fs::remove_file(&candidate);
                    return Err(error.to_string());
                }
                temporary = Some(candidate);
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    let temporary = temporary.ok_or_else(|| "could not create a temporary file".to_owned())?;
    let result = (|| {
        let symlink_meta = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        if symlink_meta.file_type().is_symlink() || !symlink_meta.is_file() {
            return Err("file changed after it was read".to_owned());
        }
        if file_identity_from_metadata(&symlink_meta) != expected {
            return Err("file changed after it was read".to_owned());
        }
        fs::set_permissions(&temporary, symlink_meta.permissions())
            .map_err(|error| error.to_string())?;
        replace_file(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(unix)]
fn replace_file(temporary: &Path, target: &Path) -> Result<(), String> {
    fs::rename(temporary, target).map_err(|error| error.to_string())?;
    if let Some(parent) = target.parent() {
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn replace_file(temporary: &Path, target: &Path) -> Result<(), String> {
    match fs::rename(temporary, target) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            // Windows cannot replace an existing target with rename. Move the
            // old file aside and restore it if installing the new one fails.
            // This fallback is best-effort and not crash-atomic.
            let backup = target.with_file_name(format!(
                ".adotenv-backup-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            fs::rename(target, &backup).map_err(|error| error.to_string())?;
            match fs::rename(temporary, target) {
                Ok(()) => {
                    let _ = fs::remove_file(&backup);
                    Ok(())
                }
                Err(replace_error) => {
                    let restore = fs::rename(&backup, target);
                    match restore {
                        Ok(()) => Err(replace_error.to_string()),
                        Err(restore_error) => Err(format!(
                            "replacement failed: {replace_error}; restoring the original failed: {restore_error}; backup left at {}",
                            backup.display()
                        )),
                    }
                }
            }
        }
        Err(error) => Err(error.to_string()),
    }
}

fn apply_config(options: &mut Options) -> Result<(), String> {
    let start = env::current_dir().map_err(|error| error.to_string())?;
    let config = load_effective_config(options.config.as_deref().map(Path::new), &start)
        .map_err(|error| error.to_string())?;
    let Some(config) = config else {
        return Ok(());
    };
    let explicit = ConfigExplicit {
        dialect: options.explicit.dialect,
        format: options.explicit.format,
        recursive: options.explicit.recursive,
        key_order: options.explicit.key_order,
        key_case: options.explicit.key_case,
        quote_style: options.explicit.quote_style,
        equals_spacing: options.explicit.equals_spacing,
        trim_trailing_whitespace: options.explicit.trim_trailing_whitespace,
        ignore: options.explicit.exclude,
    };
    let mut policy = LintPolicy {
        dialect: options.dialect,
        format: options.format,
        recursive: options.recursive,
        lint: options.lint,
        ignore: options.exclude.clone(),
    };
    config.apply(&mut policy, explicit);
    options.dialect = policy.dialect;
    options.format = policy.format;
    options.recursive = policy.recursive;
    options.lint = policy.lint;
    options.exclude = policy.ignore;
    Ok(())
}

fn parse_limit(flag: &str, value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("{flag} expects a non-negative integer: {value}"))
}

#[derive(Debug)]
enum CliAction {
    Help,
    Version,
    Run(Box<Options>),
}

#[cfg(test)]
fn parse_args(args: &mut [String]) -> Result<Options, String> {
    match parse_cli_args(args)? {
        CliAction::Run(options) => Ok(*options),
        CliAction::Help => Err("help requested".to_owned()),
        CliAction::Version => Err("version requested".to_owned()),
    }
}

#[allow(clippy::too_many_lines)]
fn parse_cli_args(args: &[String]) -> Result<CliAction, String> {
    let mut options = Options {
        dialect: Dialect::PORTABLE,
        parse_limits: ParseOptions::new(),
        format: Format::Human,
        recursive: false,
        fix: false,
        lint: LintOptions::default(),
        paths: Vec::new(),
        config: None,
        exclude: Vec::new(),
        explicit: Explicit::default(),
    };
    let mut index = 0;
    let mut options_ended = false;
    while index < args.len() {
        let argument = &args[index];
        if options_ended {
            options.paths.push(argument.clone());
            index += 1;
            continue;
        }
        match argument.as_str() {
            "--" => options_ended = true,
            "-h" | "--help" => return Ok(CliAction::Help),
            "--version" => return Ok(CliAction::Version),
            "--recursive" | "-r" => {
                options.recursive = true;
                options.explicit.recursive = true;
            }
            "--fix" => options.fix = true,
            "--key-order" => {
                options.lint = options.lint.with_key_order(true);
                options.explicit.key_order = true;
            }
            "--trim-trailing-whitespace" => {
                options.lint = options.lint.with_trim_trailing_whitespace(true);
                options.explicit.trim_trailing_whitespace = true;
            }
            "--config" => {
                index += 1;
                options.config = Some(value(args, index, "--config")?.to_owned());
            }
            "--exclude" => {
                index += 1;
                options
                    .exclude
                    .push(value(args, index, "--exclude")?.to_owned());
                options.explicit.exclude = true;
            }
            "--max-input-bytes" => {
                index += 1;
                let limit = parse_limit(
                    "--max-input-bytes",
                    value(args, index, "--max-input-bytes")?,
                )?;
                options.parse_limits = options.parse_limits.with_max_input_bytes(limit);
            }
            "--max-lines" => {
                index += 1;
                let limit = parse_limit("--max-lines", value(args, index, "--max-lines")?)?;
                options.parse_limits = options.parse_limits.with_max_lines(limit);
            }
            "--dialect" => {
                index += 1;
                options.dialect = parse_dialect(value(args, index, "--dialect")?)?;
                options.explicit.dialect = true;
            }
            "--format" => {
                index += 1;
                options.format = parse_format(value(args, index, "--format")?)?;
                options.explicit.format = true;
            }
            "--key-case" => {
                index += 1;
                options.lint =
                    options
                        .lint
                        .with_key_case(parse_key_case(value(args, index, "--key-case")?)?);
                options.explicit.key_case = true;
            }
            "--quote-style" => {
                index += 1;
                options.lint = options.lint.with_quote_style(parse_quote_style(value(
                    args,
                    index,
                    "--quote-style",
                )?)?);
                options.explicit.quote_style = true;
            }
            "--equals-spacing" => {
                index += 1;
                options.lint = options.lint.with_equals_spacing(parse_spacing(value(
                    args,
                    index,
                    "--equals-spacing",
                )?)?);
                options.explicit.equals_spacing = true;
            }
            argument if argument.starts_with("--dialect=") => {
                options.dialect = parse_dialect(&argument["--dialect=".len()..])?;
                options.explicit.dialect = true;
            }
            argument if argument.starts_with("--config=") => {
                options.config = Some(argument["--config=".len()..].to_owned());
            }
            argument if argument.starts_with("--exclude=") => {
                options
                    .exclude
                    .push(argument["--exclude=".len()..].to_owned());
                options.explicit.exclude = true;
            }
            argument if argument.starts_with("--max-input-bytes=") => {
                let limit =
                    parse_limit("--max-input-bytes", &argument["--max-input-bytes=".len()..])?;
                options.parse_limits = options.parse_limits.with_max_input_bytes(limit);
            }
            argument if argument.starts_with("--max-lines=") => {
                let limit = parse_limit("--max-lines", &argument["--max-lines=".len()..])?;
                options.parse_limits = options.parse_limits.with_max_lines(limit);
            }
            argument if argument.starts_with("--format=") => {
                options.format = parse_format(&argument["--format=".len()..])?;
                options.explicit.format = true;
            }
            argument if argument.starts_with("--key-case=") => {
                options.lint = options
                    .lint
                    .with_key_case(parse_key_case(&argument["--key-case=".len()..])?);
                options.explicit.key_case = true;
            }
            argument if argument.starts_with("--quote-style=") => {
                options.lint = options
                    .lint
                    .with_quote_style(parse_quote_style(&argument["--quote-style=".len()..])?);
                options.explicit.quote_style = true;
            }
            argument if argument.starts_with("--equals-spacing=") => {
                options.lint = options
                    .lint
                    .with_equals_spacing(parse_spacing(&argument["--equals-spacing=".len()..])?);
                options.explicit.equals_spacing = true;
            }
            argument if argument.starts_with('-') => {
                return Err(format!("unknown option: {argument}"));
            }
            path => options.paths.push(path.to_owned()),
        }
        index += 1;
    }
    Ok(CliAction::Run(Box::new(options)))
}

fn value<'a>(args: &'a [String], index: usize, option: &str) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{option} requires a value"))
}

fn trim_trailing_separators(raw: &str) -> PathBuf {
    let trimmed = raw.trim_end_matches(|c| c == '/' || (cfg!(windows) && c == '\\'));
    if trimmed.is_empty() || (cfg!(windows) && trimmed.len() == 2 && trimmed.as_bytes()[1] == b':')
    {
        PathBuf::from(raw)
    } else {
        PathBuf::from(trimmed)
    }
}

fn collect_paths(
    paths: &[String],
    recursive: bool,
    exclude_patterns: &[String],
) -> Result<Vec<PathBuf>, String> {
    let excludes = IgnoredPaths::new(exclude_patterns)?;
    let mut found = BTreeSet::new();
    for raw_path in paths {
        let probe = trim_trailing_separators(raw_path);
        let path = PathBuf::from(raw_path);
        let symlink_meta =
            fs::symlink_metadata(&probe).map_err(|error| format!("{}: {error}", path.display()))?;
        let metadata =
            fs::metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if excludes.is_ignored_with_root(&path, &path) {
            continue;
        }
        if metadata.is_file() {
            found.insert(path);
        } else if metadata.is_dir() {
            if symlink_meta.file_type().is_symlink() {
                return Err(format!(
                    "{}: symbolic link directory is not allowed",
                    path.display()
                ));
            }
            if !recursive {
                return Err(format!(
                    "{}: directory requires --recursive",
                    path.display()
                ));
            }
            walk(&path, &path, &excludes, &mut found)?;
        } else {
            return Err(format!(
                "{}: not a regular file or directory",
                path.display()
            ));
        }
    }
    let mut seen_canonical = BTreeSet::new();
    let mut unique = Vec::with_capacity(found.len());
    for path in found {
        let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen_canonical.insert(key) {
            unique.push(path);
        }
    }
    Ok(unique)
}

fn walk(
    directory: &Path,
    root: &Path,
    excludes: &IgnoredPaths,
    found: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    let entries =
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if excludes.is_ignored_with_root(&path, root) {
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
                walk(&path, root, excludes, found)?;
            }
        } else if file_type.is_file() && is_env_file(&entry.file_name().to_string_lossy()) {
            found.insert(path);
        }
    }
    Ok(())
}

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
    [".git", "target", "node_modules"]
        .iter()
        .any(|ignored| name.eq_ignore_ascii_case(ignored))
}

#[cfg(not(windows))]
fn is_ignored_dir(name: &str) -> bool {
    matches!(name, ".git" | "target" | "node_modules")
}

fn print_human(path: &Path, findings: &[Finding]) {
    for finding in findings {
        let key = finding
            .key()
            .map_or_else(String::new, |key| format!(" [{key}]"));
        write_stdout_line(&format!(
            "{}:{}: {}[{}]: {}{key}",
            path.display(),
            finding.line(),
            severity_name(finding.severity()),
            finding.rule().as_str(),
            finding.message()
        ));
    }
}

fn print_github(path: &Path, findings: &[Finding]) {
    for finding in findings {
        write_stdout_line(&format!(
            "::{} file={},line={},title={}::{}",
            github_level(finding.severity()),
            github_property(&path.display().to_string()),
            finding.line(),
            github_property(finding.rule().as_str()),
            github_message(finding.message())
        ));
    }
}

fn github_level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        _ => "notice",
    }
}

fn print_json(path: &Path, finding: &Finding) {
    write_stdout(&format!(
        "{{\"path\":{},\"line\":{},\"rule\":{},\"severity\":{},\"message\":{},\"key\":{},\"span\":{{\"start\":{},\"end\":{}}}}}",
        json_string(&path.display().to_string()),
        finding.line(),
        json_string(finding.rule().as_str()),
        json_string(severity_name(finding.severity())),
        json_string(finding.message()),
        finding
            .key()
            .map_or_else(|| "null".to_owned(), json_string),
        finding.span().start,
        finding.span().end,
    ));
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        _ => "info",
    }
}

fn github_property(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(':', "%3A")
        .replace(',', "%2C")
}

fn github_message(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
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
            character if character.is_control() => {
                let _ = write!(output, "\\u{:04x}", u32::from(character));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use adotenv::{EqualsSpacing, QuoteStyle};
    use adotenv_lint::{KeyCase, Rule};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn parses_defaults_and_paths() {
        let options = parse_args(&mut args(&["a.env", "b.env"])).unwrap();
        assert_eq!(options.dialect, Dialect::PORTABLE);
        assert_eq!(options.format, Format::Human);
        assert!(!options.recursive);
        assert_eq!(options.paths, ["a.env", "b.env"]);
    }

    #[test]
    fn parses_rule_options_and_end_of_options() {
        let options = parse_args(&mut args(&[
            "--dialect=node",
            "--format",
            "json",
            "--key-order",
            "--key-case",
            "upper",
            "--quote-style=single",
            "--equals-spacing=none",
            "--trim-trailing-whitespace",
            "--recursive",
            "--",
            "--literal-path",
        ]))
        .unwrap();
        assert_eq!(options.dialect, Dialect::NODE_DOTENV);
        assert_eq!(options.format, Format::Json);
        assert!(options.recursive);
        assert!(options.lint.key_order);
        assert_eq!(options.lint.key_case, KeyCase::Upper);
        assert_eq!(options.lint.quote_style, QuoteStyle::Single);
        assert_eq!(options.lint.equals_spacing, EqualsSpacing::None);
        assert!(options.lint.trim_trailing_whitespace);
        assert_eq!(options.paths, ["--literal-path"]);
    }

    #[test]
    fn rejects_unknown_options() {
        assert!(parse_args(&mut args(&["--unknown"])).is_err());
        assert!(parse_args(&mut args(&["--format", "nope"])).is_err());
        assert!(parse_args(&mut args(&["--dialect"])).is_err());
    }

    #[test]
    fn github_output_escapes_command_data() {
        assert_eq!(github_property("a,b:c\n%"), "a%2Cb%3Ac%0A%25");
        assert_eq!(github_message("a\r\n%"), "a%0D%0A%25");
    }

    #[test]
    fn json_escaping_is_value_safe() {
        assert_eq!(
            json_string("a\"b\\c\n\r\t\x01"),
            "\"a\\\"b\\\\c\\n\\r\\t\\u0001\""
        );
    }

    #[test]
    fn parses_space_and_equals_option_forms() {
        let options = parse_args(&mut args(&[
            "--recursive",
            "--fix",
            "--max-lines",
            "100",
            "--dialect",
            "python",
            "--config=custom.toml",
            "--exclude",
            "vendor/**",
            "--format=github",
            "--key-case=lower",
            "--quote-style",
            "double",
            "--equals-spacing",
            "spaces",
            "app.env",
        ]))
        .unwrap();
        assert!(options.recursive);
        assert!(options.fix);
        assert_eq!(options.dialect, Dialect::PYTHON_DOTENV);
        assert_eq!(options.config.as_deref(), Some("custom.toml"));
        assert_eq!(options.exclude, ["vendor/**"]);
        assert_eq!(options.format, Format::Github);
        assert_eq!(options.lint.key_case, KeyCase::Lower);
        assert_eq!(options.lint.quote_style, QuoteStyle::Double);
        assert_eq!(options.lint.equals_spacing, EqualsSpacing::Spaces);
        assert_eq!(options.paths, ["app.env"]);

        assert!(is_ignored_dir(".git"));
        assert!(is_ignored_dir("target"));
        assert!(is_ignored_dir("node_modules"));
        assert!(!is_ignored_dir("nested"));
        assert_eq!(github_level(Severity::Warning), "warning");
        assert_eq!(github_level(Severity::Info), "notice");
        assert_eq!(severity_name(Severity::Warning), "warning");
        assert_eq!(severity_name(Severity::Info), "info");
    }

    #[test]
    fn env_name_matching_covers_cascades() {
        assert!(is_env_file(".env"));
        assert!(is_env_file(".env.production"));
        assert!(!is_env_file("env"));
        assert!(!is_env_file(".environment"));
    }

    #[test]
    fn rule_and_error_messages_are_stable() {
        assert_eq!(Rule::DuplicateKey.as_str(), "duplicate-key");
        assert_eq!(Rule::KeyOrder.as_str(), "key-order");
    }

    #[test]
    fn output_helpers_do_not_panic() {
        let path = Path::new("app.env");
        let doc = adotenv::parse_strict("A=1\nA=2\n").unwrap();
        let findings = lint(&doc, LintOptions::default());
        print_human(path, &findings);
        print_github(path, &findings);
        print_json(path, &findings[0]);
    }

    #[test]
    fn io_error_is_usage_failure() {
        assert!(collect_paths(&["definitely-missing.env".to_owned()], false, &[]).is_err());
    }

    #[test]
    fn read_file_bounded_and_write_fixed_enforce_boundaries() {
        let dir = env::temp_dir().join(format!(
            "adotenv-lint-unit-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join(".env");
        fs::write(&file, "A=1\n").unwrap();

        assert_eq!(read_file_bounded(&file, Some(4)).unwrap(), "A=1\n");
        assert_eq!(read_file_bounded(&file, Some(10)).unwrap(), "A=1\n");
        assert!(read_file_bounded(&file, Some(2))
            .unwrap_err()
            .contains("max_input_bytes"));

        let expected = file_identity(&file).unwrap();
        let sub = dir.join("subdir");
        fs::create_dir(&sub).unwrap();
        assert!(write_fixed(&sub, "A=2\n", expected)
            .unwrap_err()
            .contains("changed after it was read"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn trim_trailing_separators_preserves_root() {
        assert_eq!(trim_trailing_separators("/"), PathBuf::from("/"));
        assert_eq!(
            trim_trailing_separators("/tmp/a///"),
            PathBuf::from("/tmp/a")
        );
    }
}
