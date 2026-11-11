# 1.4.1 benchmark results

Status: release-candidate baseline, recorded on `kbench` at source SHA
`45540ac`. Re-run this complete matrix from the final release-candidate SHA
before publishing 1.4.1.

This page uses the [methodology](METHODOLOGY.md), [scenario catalogue](SCENARIOS.md)
and [result schema](SCHEMA.md). Raw evidence:

- [Memory JSONL](memory.jsonl) -- 165 cases
- [Memory table](memory.md)
- [Memory fingerprint](memory.fingerprint.json)
- [Speed raw bencher output](speed.txt) -- 115 benchmark results
- [Speed table](speed.md)
- [Speed fingerprint](speed.fingerprint.json)

## Provenance

| Item | Value |
|---|---|
| Source SHA | `45540ac` |
| Reference host | `kbench` |
| CPU | Intel Core i7-6700HQ, 8 logical CPUs |
| OS / kernel | Linux / 7.1.3-arch1-2 |
| Rust | 1.97.1 (8bab26f4f 2026-07-14) |
| Criterion environment | Speed fingerprint `load1=0.15` |
| Memory environment | Memory fingerprint `load1=1.16` |
| Queue | benchq/pueue, group `bench`, one slot |

The two runs share the same source SHA. The memory fingerprint records a
higher one-minute load because the deterministic allocator metrics do not
depend on wall-clock scheduling; the load is retained for audit rather than
used to hide the run.

## Claim

Against the best pinned competitor in each comparable cell:

| Metric | Result |
|---|---:|
| Speed | **25/25 strict wins** |
| Peak heap | **30/30 strict wins** |
| Allocation count | **30/30 strict wins** |
| Retained heap | **30/30 ties, 0 losses** |

The retained result is a documented no-loss result, not a strict win. See
[Methodology § Claim boundary](METHODOLOGY.md#claim-boundary).

Speed cells are 40 values lines (5 sizes × 2 non-empty shapes × 4 supported
implementations) and 75 load lines (5 sizes × 3 shapes × 5 implementations).
For the 25 adotenv-vs-best comparisons, the speed ratio ranges from 0.27 to
0.78; every value is below 1.0. Memory peak-heap ratios range from 0.00 to
0.90; every value is below 1.0. Allocation ratios range from 0.00 to 0.965;
every value is below 1.0.

## Reproduce one cell

Memory:

```console
cargo run --release -p adotenv-xtask -- memory \
  --out ~/.tmp/adotenv-bench/one-memory.jsonl \
  --sizes 128 --shapes plain --modes values
```

Find the matching `size`, `shape`, `mode` and `implementation` rows in
[memory.jsonl](memory.jsonl); the `fixture_sha256` must match the
[`plain-128` hash](SCENARIOS.md#comparable-fixture-hashes).

Speed:

```console
cargo run --release -p adotenv-xtask -- speed \
  --out ~/.tmp/adotenv-bench/one-speed.txt \
  --bench competitors --filter 'plain/128'
```

Find the matching `competitor_values/.../plain/128` or
`competitor_load/.../plain/128` line in [speed.txt](speed.txt).

## Publication procedure

```console
cargo run --release -p adotenv-xtask -- queue \
  --remote-root /var/lib/benchq/adotenv-bench \
  --out docs/benchmarks/memory.jsonl \
  --sizes 8,128,1k,10k,100k --shapes plain,long-line,empty \
  --modes values,load,document,resolve --sync

cargo run --release -p adotenv-xtask -- queue-speed \
  --remote-root /var/lib/benchq/adotenv-bench \
  --out docs/benchmarks/speed.txt --sync
```

Then render the tables, compare the fingerprints and update this page before
tagging. Do not publish a run that mixed source SHAs, hosts or queue sessions.
