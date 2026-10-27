//! Synthetic parsing, exact encoding, portability, structure, formatting and
//! complete verified-edit workloads.

use std::{fmt::Write, hint::black_box};

use adotenv::{encode_value_with, parse, parse_strict, Dialect, Edit, FormatOptions, QuoteStyle};
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

fn encoding_fixture(bytes: usize) -> String {
    const FRAGMENT: &str = "literal # \"'\\\\${UNCHANGED}\\t;";
    let mut value = String::with_capacity(bytes);
    while value.len() < bytes {
        value.push_str(FRAGMENT);
    }
    value.truncate(bytes);
    value
}

fn batch_fixture(entries: usize) -> String {
    let mut source = String::with_capacity(entries * 16);
    for index in 0..entries {
        writeln!(source, "K{index:08}=x").expect("writing to String cannot fail");
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
    lookup(c, &fixtures);

    let values: Vec<_> = [("small", 32), ("typical", 4_096), ("large", 32_768)]
        .into_iter()
        .map(|(name, bytes)| (name, encoding_fixture(bytes)))
        .collect();
    let mut encoding = c.benchmark_group("encode_value_with");
    for (dialect_name, dialect) in [
        ("portable", Dialect::PORTABLE),
        ("node", Dialect::NODE_DOTENV),
        ("python", Dialect::PYTHON_DOTENV),
        ("compose", Dialect::COMPOSE),
        ("docker", Dialect::DOCKER_RAW),
    ] {
        for (size, value) in &values {
            encoding.throughput(Throughput::Bytes(value.len() as u64));
            encoding.bench_function(BenchmarkId::new(dialect_name, size), |b| {
                b.iter(|| {
                    black_box(
                        encode_value_with(black_box(value), dialect)
                            .expect("benchmark value is exactly representable"),
                    )
                });
            });
        }
    }
    encoding.finish();

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

    portability(c, &fixtures);
    references(c, &fixtures);
    resolution(c, &fixtures);
    structure(c, &fixtures);
    formatting(c, &fixtures);
    examples(c, &fixtures);
    sorting(c, &fixtures);
    comparison(c, &fixtures);

    scaling(c);
}

fn portability(c: &mut Criterion, fixtures: &[(&str, usize, String)]) {
    let dialects = [
        Dialect::NODE_DOTENV,
        Dialect::PYTHON_DOTENV,
        Dialect::COMPOSE,
        Dialect::DOCKER_RAW,
    ];
    let mut portability = c.benchmark_group("portability");
    for (name, _, source) in fixtures {
        let document = parse_strict(source).expect("benchmark fixture is valid Portable syntax");
        portability.throughput(Throughput::Bytes(source.len() as u64));
        portability.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| black_box(document.portability(black_box(&dialects))));
        });
    }
    portability.finish();
}

fn sorting(c: &mut Criterion, fixtures: &[(&str, usize, String)]) {
    let mut sorting = c.benchmark_group("sort");
    for (name, _, source) in fixtures {
        let document = parse_strict(source).expect("benchmark fixture is valid Portable syntax");
        sorting.throughput(Throughput::Bytes(source.len() as u64));
        sorting.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| black_box(document.sort()));
        });
    }
    sorting.finish();
}

fn examples(c: &mut Criterion, fixtures: &[(&str, usize, String)]) {
    let mut examples = c.benchmark_group("example");
    for (name, _, source) in fixtures {
        let document = parse_strict(source).expect("benchmark fixture is valid Portable syntax");
        examples.throughput(Throughput::Bytes(source.len() as u64));
        examples.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| black_box(document.example()));
        });
    }
    examples.finish();
}

fn structure(c: &mut Criterion, fixtures: &[(&str, usize, String)]) {
    let mut grouping = c.benchmark_group("structure");
    for (name, _, source) in fixtures {
        let document = parse_strict(source).expect("benchmark fixture is valid Portable syntax");
        grouping.throughput(Throughput::Bytes(source.len() as u64));
        grouping.bench_function(BenchmarkId::new("groups", name), |b| {
            b.iter(|| {
                let value = document
                    .groups()
                    .map(|group| group.items().len())
                    .sum::<usize>();
                black_box(value)
            });
        });
        grouping.bench_function(BenchmarkId::new("leading_comments", name), |b| {
            b.iter(|| {
                let value = document
                    .entries()
                    .map(|entry| entry.leading_comments().count())
                    .sum::<usize>();
                black_box(value)
            });
        });
    }
    grouping.finish();
}

fn lookup(c: &mut Criterion, fixtures: &[(&str, usize, String)]) {
    let mut lookup = c.benchmark_group("lookup_index");
    for (name, _, source) in fixtures {
        let document = parse_strict(source).expect("benchmark fixture is valid Portable syntax");
        let index = document.lookup_index();
        let keys: Vec<String> = document
            .entries()
            .take(64)
            .map(|entry| entry.key().to_string())
            .collect();
        lookup.bench_function(BenchmarkId::new("build", name), |b| {
            b.iter(|| black_box(document.lookup_index()));
        });
        lookup.bench_function(BenchmarkId::new("get", name), |b| {
            b.iter(|| {
                let mut found = 0usize;
                for key in &keys {
                    found += usize::from(index.get(black_box(key)).is_some());
                }
                black_box(found)
            });
        });
    }
    lookup.finish();
}

fn formatting(c: &mut Criterion, fixtures: &[(&str, usize, String)]) {
    let mut formatting = c.benchmark_group("format");
    let options = FormatOptions::normalize();
    let groups = FormatOptions::normalize_groups();
    let quotes = FormatOptions::normalize().with_quote_style(QuoteStyle::Double);
    for (name, _, source) in fixtures {
        let document = parse_strict(source).expect("benchmark fixture is valid Portable syntax");
        formatting.throughput(Throughput::Bytes(source.len() as u64));
        formatting.bench_function(BenchmarkId::new("normalize", name), |b| {
            b.iter(|| black_box(document.format(black_box(&options))));
        });
        formatting.bench_function(BenchmarkId::new("collapse", name), |b| {
            b.iter(|| black_box(document.format(black_box(&groups))));
        });
        formatting.bench_function(BenchmarkId::new("double_quotes", name), |b| {
            b.iter(|| black_box(document.format(black_box(&quotes))));
        });
    }
    formatting.finish();
}

fn comparison(c: &mut Criterion, fixtures: &[(&str, usize, String)]) {
    let left = parse_strict(&fixtures[2].2).expect("benchmark fixture is valid Portable syntax");
    let right = parse_strict(&fixtures[1].2).expect("benchmark fixture is valid Portable syntax");
    let mut group = c.benchmark_group("compare");
    group.throughput(Throughput::Bytes(fixtures[2].2.len() as u64));
    group.bench_function("large-vs-typical", |b| {
        b.iter(|| black_box(left.compare(black_box(&right))));
    });
    group.finish();
}

fn resolution(c: &mut Criterion, fixtures: &[(&str, usize, String)]) {
    let mut resolving = c.benchmark_group("resolve");
    let options = adotenv::ExpandOptions::compose();
    for (name, _, source) in fixtures {
        let document = parse_strict(source).expect("benchmark fixture is valid Portable syntax");
        resolving.throughput(Throughput::Bytes(source.len() as u64));
        resolving.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| {
                black_box(document.resolve(|key| Some(format!("bench-{key}")), black_box(&options)))
            });
        });
    }
    resolving.finish();
}

fn references(c: &mut Criterion, fixtures: &[(&str, usize, String)]) {
    let mut references = c.benchmark_group("references");
    for (name, _, source) in fixtures {
        let document = parse_strict(source).expect("benchmark fixture is valid Portable syntax");
        references.throughput(Throughput::Bytes(source.len() as u64));
        references.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| {
                let mut count = 0usize;
                for entry in document.entries() {
                    count += entry.references().len();
                }
                black_box(count)
            });
        });
    }
    references.finish();
}

fn scaling(c: &mut Criterion) {
    let mut scaling = c.benchmark_group("native_edits_scaling");
    for count in [1_024, 4_096, 16_384] {
        let source = batch_fixture(count);
        let document = parse_strict(&source).expect("batch fixture is valid Portable syntax");
        let sets: Vec<_> = (0..count)
            .map(|entry| Edit::Set { entry, value: "y" })
            .collect();
        let keys: Vec<_> = (0..count).map(|index| format!("R{index:08}")).collect();
        let mut combined = Vec::with_capacity(count * 2);
        for (entry, key) in keys.iter().enumerate() {
            combined.push(Edit::Set { entry, value: "y" });
            combined.push(Edit::Rename { entry, key });
        }
        scaling.throughput(Throughput::Bytes(source.len() as u64));
        scaling.bench_function(BenchmarkId::new("set_all", count), |b| {
            b.iter(|| {
                black_box(
                    document
                        .edit(black_box(&sets))
                        .expect("all benchmark sets are valid"),
                )
            });
        });
        scaling.bench_function(BenchmarkId::new("set_rename_all", count), |b| {
            b.iter(|| {
                black_box(
                    document
                        .edit(black_box(&combined))
                        .expect("all benchmark sets and renames are valid"),
                )
            });
        });
    }
    scaling.finish();
}

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    core(&mut criterion);
    criterion.final_summary();
}
