#![allow(missing_docs)]

//! Deterministic instruction and cycle gate for adotenv's values parsers.
//!
//! These benches cover map-based parsing, compact parsing and compact loading.
//! They run under Valgrind Callgrind through gungraun, so their counts do not
//! depend on host load. The CI gate compares them against a freshly built base
//! and enforces the soft limits declared below.

use std::fs;
use std::hint::black_box;
use std::path::PathBuf;

use adotenv::load::load_values_compact;
use adotenv::{parse_values, parse_values_compact, Dialect};
use adotenv_bench::{fixture, Shape, Size};
use gungraun::{
    library_benchmark, library_benchmark_group, main, Callgrind, EventKind, LibraryBenchmarkConfig,
};

fn plain_8() -> String {
    fixture(Size::Eight, Shape::Plain).source().to_owned()
}

fn plain_128() -> String {
    fixture(Size::Typical, Shape::Plain).source().to_owned()
}

fn plain_1k() -> String {
    fixture(Size::OneK, Shape::Plain).source().to_owned()
}

fn long_line_128() -> String {
    fixture(Size::Typical, Shape::LongLine).source().to_owned()
}

#[library_benchmark(setup = plain_8)]
#[bench::plain_8()]
fn parse_plain_8(source: String) {
    black_box(parse_values(&source).expect("fixture parses"));
}

#[library_benchmark(setup = plain_128)]
#[bench::plain_128()]
fn parse_plain_128(source: String) {
    black_box(parse_values(&source).expect("fixture parses"));
}

#[library_benchmark(setup = plain_1k)]
#[bench::plain_1k()]
fn parse_plain_1k(source: String) {
    black_box(parse_values(&source).expect("fixture parses"));
}

#[library_benchmark(setup = long_line_128)]
#[bench::long_line_128()]
fn parse_long_line_128(source: String) {
    black_box(parse_values(&source).expect("fixture parses"));
}

#[library_benchmark(setup = plain_8)]
#[bench::plain_8()]
fn compact_plain_8(source: String) {
    black_box(parse_values_compact(&source).expect("fixture parses"));
}

#[library_benchmark(setup = plain_128)]
#[bench::plain_128()]
fn compact_plain_128(source: String) {
    black_box(parse_values_compact(&source).expect("fixture parses"));
}

#[library_benchmark(setup = plain_1k)]
#[bench::plain_1k()]
fn compact_plain_1k(source: String) {
    black_box(parse_values_compact(&source).expect("fixture parses"));
}

#[library_benchmark(setup = long_line_128)]
#[bench::long_line_128()]
fn compact_long_line_128(source: String) {
    black_box(parse_values_compact(&source).expect("fixture parses"));
}

struct TempFixture {
    path: PathBuf,
}

impl Drop for TempFixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn temp_fixture(name: &str, shape: Shape, size: Size) -> TempFixture {
    let path = std::env::temp_dir().join(format!(
        "adotenv-det-{}-{}-{}.env",
        std::process::id(),
        name,
        size.label()
    ));
    fs::write(&path, fixture(size, shape).source()).expect("fixture file can be written");
    TempFixture { path }
}

fn load_plain_128() -> TempFixture {
    temp_fixture("compact-load", Shape::Plain, Size::Typical)
}

fn load_long_line_128() -> TempFixture {
    temp_fixture("compact-load", Shape::LongLine, Size::Typical)
}

#[library_benchmark(setup = load_plain_128)]
#[bench::plain_128()]
fn compact_load_plain_128(fixture: TempFixture) {
    black_box(
        load_values_compact([fixture.path.as_path()], Dialect::PORTABLE).expect("fixture loads"),
    );
}

#[library_benchmark(setup = load_long_line_128)]
#[bench::long_line_128()]
fn compact_load_long_line_128(fixture: TempFixture) {
    black_box(
        load_values_compact([fixture.path.as_path()], Dialect::PORTABLE).expect("fixture loads"),
    );
}

library_benchmark_group!(
    name = values,
    benchmarks = [
        parse_plain_8,
        parse_plain_128,
        parse_plain_1k,
        parse_long_line_128
    ]
);

library_benchmark_group!(
    name = compact,
    benchmarks = [
        compact_plain_8,
        compact_plain_128,
        compact_plain_1k,
        compact_long_line_128
    ]
);

library_benchmark_group!(
    name = compact_load,
    benchmarks = [compact_load_plain_128, compact_load_long_line_128]
);

main!(
    config = LibraryBenchmarkConfig::default().tool(
        Callgrind::default()
            .args(["--cache-sim=yes"])
            .soft_limits([(EventKind::Ir, 2.0), (EventKind::EstimatedCycles, 5.0)]),
    ),
    library_benchmark_groups = [values, compact, compact_load]
);
