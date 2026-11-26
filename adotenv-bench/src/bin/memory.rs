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
    let mut mode = None;
    let mut implementation = None;
    let mut size = None;
    let mut shape = None;
    let mut format = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => {
                println!(
                    "usage: memory --mode <values|document|load|resolve> \
                     --implementation <name> --size <8|128|1k|10k|100k> \
                     --shape <shape> [--format <json|markdown>]"
                );
                return Ok(());
            }
            flag @ ("--mode" | "--implementation" | "--size" | "--shape" | "--format") => {
                let value = args
                    .get(index + 1)
                    .map(String::as_str)
                    .ok_or_else(|| format!("missing value for {flag}"))?;
                if matches!(
                    value,
                    "--mode"
                        | "--implementation"
                        | "--size"
                        | "--shape"
                        | "--format"
                        | "--help"
                        | "-h"
                ) {
                    return Err(format!("missing value for {flag} (got flag {value})"));
                }
                let slot = match flag {
                    "--mode" => &mut mode,
                    "--implementation" => &mut implementation,
                    "--size" => &mut size,
                    "--shape" => &mut shape,
                    "--format" => &mut format,
                    _ => unreachable!(),
                };
                if slot.replace(value).is_some() {
                    return Err(format!("duplicate argument: {flag}"));
                }
                index += 2;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let mode = mode.ok_or("missing --mode")?;
    let implementation = implementation.ok_or("missing --implementation")?;
    let size = size.ok_or("missing --size")?;
    let shape = shape.ok_or("missing --shape")?;
    let format = format.unwrap_or("json");
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
