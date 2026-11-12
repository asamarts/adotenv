//! Comparable values and load benchmarks against pinned `.env` libraries.

use std::collections::BTreeMap;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use adotenv::{parse_values, parse_values_compact, CompactValues, Dialect};
use adotenv_bench::competitors::{
    adotenv_load_values, load_values, parse_values as parse_competitor_values, Competitor,
    COMPARABLE_LOAD_SHAPES, COMPARABLE_VALUES_SHAPES,
};
use adotenv_bench::{fixture, Size};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

fn values(c: &mut Criterion) {
    let mut group = c.benchmark_group("competitor_values");
    for size in Size::ALL {
        for shape in COMPARABLE_VALUES_SHAPES {
            let fixture = fixture(size, shape);
            if fixture.bytes() == 0 {
                continue;
            }
            let parameter = format!("{}/{}", shape.label(), size.label());
            let expected = parse_values(fixture.source()).expect("fixture is valid Portable");
            let compact_expected = parse_values_compact(fixture.source())
                .expect("fixture is valid Portable")
                .into_btreemap();
            assert_eq!(
                compact_expected,
                expected,
                "adotenv-compact {}",
                fixture.name()
            );
            group.throughput(Throughput::Bytes(fixture.bytes() as u64));
            group.bench_function(BenchmarkId::new("adotenv", &parameter), |b| {
                b.iter(|| black_box(parse_values(black_box(fixture.source()))));
            });
            group.bench_function(BenchmarkId::new("adotenv-compact", &parameter), |b| {
                b.iter(|| black_box(parse_values_compact(black_box(fixture.source()))));
            });
            for competitor in Competitor::ALL
                .into_iter()
                .filter(|competitor| competitor.supports_values())
            {
                let actual =
                    parse_competitor_values(competitor, fixture.source()).unwrap_or_else(|error| {
                        panic!("{} {}: {error}", competitor.label(), fixture.name())
                    });
                assert_eq!(
                    actual,
                    expected,
                    "{} {}",
                    competitor.label(),
                    fixture.name()
                );
                group.bench_function(BenchmarkId::new(competitor.label(), &parameter), |b| {
                    b.iter(|| {
                        black_box(parse_competitor_values(
                            competitor,
                            black_box(fixture.source()),
                        ))
                    });
                });
            }
        }
    }
    group.finish();
}

fn loads(c: &mut Criterion) {
    let mut group = c.benchmark_group("competitor_load");
    for size in Size::ALL {
        for shape in COMPARABLE_LOAD_SHAPES {
            let fixture = fixture(size, shape);
            let parameter = format!("{}/{}", shape.label(), size.label());
            let path = temporary_path();
            fs::write(&path, fixture.source()).expect("write benchmark fixture");
            let expected = adotenv_load_values(fixture.source()).expect("fixture loads");
            let compact_expected = load_adotenv_compact(&path)
                .expect("fixture loads")
                .into_btreemap();
            assert_eq!(
                compact_expected,
                expected,
                "adotenv-compact {}",
                fixture.name()
            );
            group.throughput(Throughput::Bytes(fixture.bytes() as u64));
            group.bench_function(BenchmarkId::new("adotenv", &parameter), |b| {
                b.iter(|| black_box(load_adotenv(black_box(&path))));
            });
            group.bench_function(BenchmarkId::new("adotenv-compact", &parameter), |b| {
                b.iter(|| black_box(load_adotenv_compact(black_box(&path))));
            });
            for competitor in Competitor::ALL
                .into_iter()
                .filter(|competitor| competitor.supports_load())
            {
                let actual = load_values(competitor, &path).unwrap_or_else(|error| {
                    panic!("{} {}: {error}", competitor.label(), fixture.name())
                });
                assert_eq!(
                    actual,
                    expected,
                    "{} {}",
                    competitor.label(),
                    fixture.name()
                );
                group.bench_function(BenchmarkId::new(competitor.label(), &parameter), |b| {
                    b.iter(|| black_box(load_values(competitor, black_box(&path))));
                });
            }
            let _ = fs::remove_file(path);
        }
    }
    group.finish();
}

fn load_adotenv(path: &Path) -> Result<BTreeMap<String, String>, String> {
    adotenv::load::load_values([path], Dialect::PORTABLE)
        .map(adotenv::load::LoadedValues::into_values)
        .map_err(|error| error.to_string())
}

fn load_adotenv_compact(path: &Path) -> Result<CompactValues, String> {
    adotenv::load::load_values_compact([path], Dialect::PORTABLE).map_err(|error| error.to_string())
}

fn temporary_path() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "adotenv-bench-load-{}-{}.env",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

criterion_group!(benches, values, loads);
criterion_main!(benches);
