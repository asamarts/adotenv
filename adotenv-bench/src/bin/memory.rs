//! Runs one isolated memory benchmark case.

#![allow(unsafe_code)]

use std::process::ExitCode;

use adotenv_bench::memory::{run, CountingAllocator, Implementation, Mode};
use adotenv_bench::{Shape, Size};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn main() -> ExitCode {
    match run_cli() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run_cli() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!(
            "usage: memory --mode <values|document|load|resolve> \
             --implementation <name> --size <8|128|1k|10k|100k> \
             --shape <shape> [--format <json|markdown>]"
        );
        return Ok(());
    }
    let mode = argument(&args, "--mode")?;
    let implementation = argument(&args, "--implementation")?;
    let size = argument(&args, "--size")?;
    let shape = argument(&args, "--shape")?;
    let format = match optional_argument(&args, "--format") {
        Some(value) => value,
        None if args.iter().any(|argument| argument == "--format") => {
            return Err("missing value for --format".to_owned())
        }
        None => "json",
    };
    let mode = Mode::from_label(mode).ok_or_else(|| format!("unknown mode: {mode}"))?;
    let implementation = Implementation::from_label(implementation)
        .ok_or_else(|| format!("unknown implementation: {implementation}"))?;
    let size = Size::from_label(size).ok_or_else(|| format!("unknown size: {size}"))?;
    let shape = Shape::from_label(shape).ok_or_else(|| format!("unknown shape: {shape}"))?;
    let report = run(size, shape, mode, implementation)?;
    match format {
        "json" => println!("{}", report.json()),
        "markdown" => println!("{}", report.markdown()),
        other => return Err(format!("unknown format: {other}")),
    }
    Ok(())
}

fn argument<'a>(args: &'a [String], name: &str) -> Result<&'a str, String> {
    optional_argument(args, name).ok_or_else(|| format!("missing {name}"))
}

fn optional_argument<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let index = args.iter().position(|argument| argument == name)?;
    args.get(index + 1).map(String::as_str)
}
