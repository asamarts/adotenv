//! Initial criterion targets over the deterministic fixture corpus.

use std::hint::black_box;

use adotenv::{parse, parse_values, parse_values_compact};
use adotenv_bench::fixtures;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

fn fixture_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("fixture_parse");
    for fixture in fixtures() {
        if fixture.bytes() == 0 {
            continue;
        }
        let parameter = format!("{}/{}", fixture.shape().label(), fixture.size().label());
        group.throughput(Throughput::Bytes(fixture.bytes() as u64));
        group.bench_function(BenchmarkId::new("parse", &parameter), |b| {
            b.iter(|| black_box(parse(black_box(fixture.source()))));
        });
        group.bench_function(BenchmarkId::new("parse_values", &parameter), |b| {
            b.iter(|| black_box(parse_values(black_box(fixture.source()))));
        });
        group.bench_function(BenchmarkId::new("parse_values_compact", &parameter), |b| {
            b.iter(|| black_box(parse_values_compact(black_box(fixture.source()))));
        });
    }
    group.finish();
}

criterion_group!(benches, fixture_throughput);
criterion_main!(benches);
