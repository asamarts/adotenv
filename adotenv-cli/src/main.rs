//! Small zero-dependency CLI for adotenv documents.

use std::{env, fs, path::Path, process::ExitCode};

use adotenv::{
    load::load_cascade, parse, parse_strict, parse_with, AssignmentState, ComparedValue,
    Comparison, DifferenceKind, FormatOptions, Severity,
};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    ExitCode::from(run(&args))
}

fn run(args: &[String]) -> u8 {
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
        "get" => get(&args[1..]),
        "check" => check(&args[1..]),
        "fmt" => fmt(&args[1..]),
        "diff" => diff(&args[1..]),
        "example" => example(&args[1..]),
        "load" => load(&args[1..]),
        "set" => set(&args[1..]),
        "run" => {
            eprintln!("run is not implemented yet");
            2
        }
        _ => {
            eprintln!("unknown command: {command}");
            usage();
            2
        }
    }
}

fn usage() {
    println!(
        "adotenv-cli <command> [options]\n\
         \n\
         Commands:\n\
           get <file> <key>             Print an effective literal value\n\
           check <file>...              Report parse diagnostics\n\
           fmt [--write] [--sort] <file>...\n\
                                        Format documents (optionally sorted)\n\
           diff <left> <right>          Compare effective keys\n\
           example <file>               Print a value-free example\n\
           load <directory> [mode]      Load the standard dotenv cascade\n\
           set <file> <key> <value>     Set and write one declaration\n\
         \n\
         run is reserved for a future trusted-command mode."
    );
}

fn get(args: &[String]) -> u8 {
    if args.len() != 2 {
        eprintln!("usage: adotenv-cli get <file> <key>");
        return 2;
    }
    let path = &args[0];
    let source = match read_text(path) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let doc = match parse_strict(&source) {
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

fn check(args: &[String]) -> u8 {
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
        let (_, diagnostics) = parse(&source);
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

fn fmt(args: &[String]) -> u8 {
    let mut write = false;
    let mut sort = false;
    let mut paths = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--write" => write = true,
            "--sort" => sort = true,
            _ => paths.push(arg.as_str()),
        }
    }
    if paths.is_empty() {
        eprintln!("usage: adotenv-cli fmt [--write] [--sort] <file>...");
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
        let (doc, diagnostics) = parse_with(&source, adotenv::Dialect::PORTABLE);
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
            formatting_doc = match parse_strict(&formatting_source) {
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
        if write {
            if let Err(error) = fs::write(path, formatted.as_str()) {
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

fn diff(args: &[String]) -> u8 {
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
    let left = match parse_strict(&left_source) {
        Ok(doc) => doc,
        Err(error) => return fail(format!("{}: {error}", args[0])),
    };
    let right = match parse_strict(&right_source) {
        Ok(doc) => doc,
        Err(error) => return fail(format!("{}: {error}", args[1])),
    };
    let comparison = left.compare(&right);
    print_comparison(&comparison);
    u8::from(!comparison.is_empty())
}

fn example(args: &[String]) -> u8 {
    if args.len() != 1 {
        eprintln!("usage: adotenv-cli example <file>");
        return 2;
    }
    let source = match read_text(&args[0]) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let (doc, diagnostics) = parse(&source);
    for diagnostic in diagnostics {
        eprintln!("{}: {diagnostic}", args[0]);
    }
    print!("{}", doc.example());
    0
}

fn load(args: &[String]) -> u8 {
    if args.is_empty() || args.len() > 2 {
        eprintln!("usage: adotenv-cli load <directory> [mode]");
        return 2;
    }
    let mode = args.get(1).map_or("", String::as_str);
    let loaded = match load_cascade(&args[0], mode) {
        Ok(loaded) => loaded,
        Err(error) => return fail(error.to_string()),
    };
    for (key, value) in loaded.values() {
        println!("{key}={value}");
    }
    0
}

fn set(args: &[String]) -> u8 {
    if args.len() != 3 {
        eprintln!("usage: adotenv-cli set <file> <key> <value>");
        return 2;
    }
    let source = match read_text(&args[0]) {
        Ok(source) => source,
        Err(error) => return fail(error),
    };
    let doc = match parse_strict(&source) {
        Ok(doc) => doc,
        Err(error) => return fail(format!("{}: {error}", args[0])),
    };
    let edited = match doc.set(&args[1], &args[2]) {
        Ok(edited) => edited,
        Err(error) => return fail(format!("{}: {error}", args[0])),
    };
    if let Err(error) = fs::write(&args[0], edited.as_str()) {
        return fail(format!("{}: {error}", args[0]));
    }
    0
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

fn read_text(path: &str) -> Result<String, String> {
    let bytes = fs::read(Path::new(path)).map_err(|error| format!("{path}: {error}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{path}: invalid UTF-8"))
}

fn fail(message: String) -> u8 {
    eprintln!("{message}");
    2
}
