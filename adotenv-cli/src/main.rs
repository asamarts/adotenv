//! Small zero-dependency CLI for adotenv documents.

use std::{
    env, fs, io,
    path::Path,
    process::{Command, ExitCode},
};
#[cfg(unix)]
use std::{
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use adotenv::{
    load::{load_cascade_with, LoadedEnvironment},
    parse_strict_with, parse_with, AssignmentState, ComparedValue, Comparison, Dialect,
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

fn usage() {
    println!(
        "adotenv-cli [--dialect <portable|node|python|compose|docker>] <command> [options]\n\
         \n\
         Commands:\n\
           get <file> <key>             Print an effective literal value\n\
           check <file>...              Report parse diagnostics\n\
           fmt [--write|--check] [--sort] <file>...\n\
                                        Format documents (optionally sorted)\n\
           diff <left> <right>          Compare effective keys\n\
           example <file>               Print a value-free example\n\
           load <directory> [mode]      Load the standard dotenv cascade\n\
           set <file> <key> <value>     Set and write one declaration\n\
           run <directory> [mode] -- <program> [args...]\n\
                                        Run a child with loaded values\n\
         \n\
         run never mutates the parent environment. Command substitution from\n\
         files is not enabled."
    );
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

fn check(args: &[String], dialect: Dialect) -> u8 {
    if args.is_empty() {
        eprintln!("usage: adotenv-cli check <file>...");
        return 2;
    }
    let mut failed = false;
    for path in args {
        let source = match read_text(path) {
            Ok(source) => source,
            Err(error) => {
                fail(error);
                failed = true;
                continue;
            }
        };
        let (_, diagnostics) = parse_with(&source, dialect);
        for diagnostic in diagnostics {
            println!(
                "{path}:{}: {:?} {}: {}",
                diagnostic.line(),
                diagnostic.severity(),
                diagnostic.code().as_str(),
                diagnostic.message()
            );
            if diagnostic.severity() == Severity::Error {
                failed = true;
            }
        }
    }
    u8::from(failed)
}

fn fmt(args: &[String], dialect: Dialect) -> u8 {
    let mut write = false;
    let mut check = false;
    let mut sort = false;
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--write" => write = true,
            "--check" => check = true,
            "--sort" => sort = true,
            _ => paths.push(arg.as_str()),
        }
    }
    if paths.is_empty() {
        eprintln!("usage: adotenv-cli fmt [--write|--check] [--sort] <file>...");
        return 2;
    }
    if write && check {
        eprintln!("--write and --check are mutually exclusive");
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
                Ok(file) => {
                    temporary = Some(candidate);
                    let mut file = file;
                    file.write_all(contents.as_bytes())?;
                    file.sync_all()?;
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
        if let Ok(metadata) = fs::metadata(path) {
            fs::set_permissions(&temporary, metadata.permissions())?;
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

fn read_text(path: &str) -> Result<String, String> {
    let bytes = fs::read(Path::new(path)).map_err(|error| format!("{path}: {error}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{path}: invalid UTF-8"))
}

fn fail(message: String) -> u8 {
    eprintln!("{message}");
    2
}
