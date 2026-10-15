//! Synthetic Portable parsing and complete verified-edit performance workloads.

use std::{fmt::Write, hint::black_box};

use adotenv::{parse, parse_strict, Edit};
use criterion::{BenchmarkId, Criterion, Throughput};

fn fixture(entries: usize) -> String {
    let mut source = String::from("# Synthetic public benchmark fixture; no real credentials.\n");
    for index in 0..entries {
        match index % 4 {
            0 => writeln!(source, "BENCH_{index}=literal-${{UNCHANGED}} # comment"),
            1 => writeln!(source, "export BENCH_{index}='Unicode: café 🦀'"),
            2 => writeln!(source, "BENCH_{index}=\"escaped\\nline\\tand\\\\slash\""),
            _ => writeln!(source, "BENCH_{index}=\"physical\nmultiline\""),
        }
        .expect("writing to String cannot fail");
    }
    source
}

fn core(c: &mut Criterion) {
    let fixtures: Vec<_> = [("small", 8), ("typical", 128), ("large", 65_536)]
        .into_iter()
        .map(|(name, count)| (name, count, fixture(count)))
        .collect();

    let mut parsing = c.benchmark_group("parse");
    for (name, _, source) in &fixtures {
        parsing.throughput(Throughput::Bytes(source.len() as u64));
        parsing.bench_with_input(BenchmarkId::from_parameter(name), source, |b, source| {
            b.iter(|| black_box(parse(black_box(source.as_str()))));
        });
    }
    parsing.finish();

    let mut editing = c.benchmark_group("native_edits");
    for (name, count, source) in &fixtures {
        let document = parse_strict(source).expect("benchmark fixture is valid Portable syntax");
        let target = count / 2;
        editing.throughput(Throughput::Bytes(source.len() as u64));
        editing.bench_function(BenchmarkId::new("set", name), |b| {
            b.iter(|| {
                black_box(
                    document
                        .edit(black_box(&[Edit::Set {
                            entry: target,
                            value: "replacement with # and \"quotes\"\nnext line",
                        }]))
                        .expect("exact value replacement is representable"),
                )
            });
        });
        let batch = [
            Edit::Set {
                entry: target,
                value: "batch replacement",
            },
            Edit::Rename {
                entry: target + 1,
                key: "RENAMED_BENCHMARK_KEY",
            },
            Edit::Export {
                entry: target + 2,
                enabled: true,
            },
        ];
        editing.bench_function(BenchmarkId::new("batch", name), |b| {
            b.iter(|| {
                black_box(
                    document
                        .edit(black_box(&batch))
                        .expect("independent benchmark operations are valid"),
                )
            });
        });
    }
    editing.finish();
}

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    core(&mut criterion);
    criterion.final_summary();
}
