# Benchmark dashboard

Reference host: kbench, Intel Core i7-6700HQ, 4 cores and 8 threads, 2.6 GHz,
15 GiB RAM, rustc 1.97.0. Tests were pinned to one CPU.

Method and budgets are in [performance.md](performance.md) and
[benchmark-plan.md](benchmark-plan.md). Full comparison tables are in
[competitive-analysis.md](competitive-analysis.md).

Raw data:

- [Comparison results](https://github.com/asamarts/adotenv/blob/main/benchmarks/compare/results.json)
- [Build and startup results](https://github.com/asamarts/adotenv/blob/main/benchmarks/compare/engineering.json)
- [Baseline](https://github.com/asamarts/adotenv/blob/main/benchmarks/compare/baseline.json)

## Simple fixture

65,536 declarations:

| Implementation | Wall | Instructions | Retained |
|---|---:|---:|---:|
| adotenv parse | 33.3 ms | 207M | 26.5 MB |
| adotenv parse + map | 40.9 ms | 277M | 14.2 MB |
| dotenv-compat | 50.3 ms | 271M | 14.5 MB |
| dotenvy + map | 69.5 ms | 361M | 11.4 MB |
| dotenv-ng | 94.5 ms | 384M | 20.3 MB |
| dotenv-parser | 163.9 ms | 1,243M | 10.6 MB |

## Mixed fixture

| Implementation | Wall | Instructions | Retained |
|---|---:|---:|---:|
| adotenv parse | 20.6 ms | 106M | 11.0 MB |
| adotenv parse + map | 27.6 ms | 153M | 5.8 MB |
| dotenv-compat | 17.7 ms | 108M | 4.8 MB |

## Real-world corpus

Pinned public configurations, repeated with unique key prefixes.

| Implementation | Wall | Instructions | Retained |
|---|---:|---:|---:|
| adotenv parse | 9.4 ms | 63M | 4.2 MB |
| adotenv parse + map | 11.2 ms | 78M | 2.4 MB |
| dotenv-compat | 7.8 ms | 56M | 2.0 MB |

## Loader profiles

| Profile | Wall | Instructions | Retained |
|---|---:|---:|---:|
| Values only | 54.9 ms | 366M | 9.9 MB |
| Default | 73.9 ms | 465M | 27.9 MB |
| Full reports | 120.1 ms | 687M | 37.7 MB |

## Dialects

Adotenv only, 65,536 declarations:

| Dialect | Wall | Instructions | Retained |
|---|---:|---:|---:|
| Portable | 33.3 ms | 207M | 26.5 MB |
| Node | 60.6 ms | 423M | 14.3 MB |
| Python | 50.6 ms | 329M | 14.3 MB |
| Compose | 51.7 ms | 305M | 14.2 MB |
| Docker | 42.7 ms | 269M | 14.3 MB |

## Adotenv-only operations

65,536-entry workloads:

| Operation | Time |
|---|---:|
| native set | 61.7 ms |
| native batch | 63.6 ms |
| format normalize | 76.7 ms |
| sort | 161.1 ms |
| compare large versus typical | 61.0 ms |

## Build and startup

Clean release builds on kbench:

| Binary | Build | Stripped size | Startup |
|---|---:|---:|---:|
| adotenv | 46.6 s | 454 KB | 651 us |
| dotenv-compat | 45.0 s | 367 KB | 664 us |
| dotenvy | 45.2 s | 386 KB | 660 us |
| dotenv-ng | 45.1 s | 404 KB | 661 us |
| dotenv-parser | 45.7 s | 450 KB | 698 us |
| adotenv-cli | 16.8 s | 1,199 KB | 653 us |

Feature-set library sizes:

| Features | Build | Library |
|---|---:|---:|
| alloc | 11.9 s | 2,616 KB |
| std | 11.9 s | 2,616 KB |
| load | 13.2 s | 3,111 KB |
| serde | 14.8 s | 2,789 KB |
| load + serde | 16.2 s | 3,324 KB |
| exec | 12.1 s | 2,654 KB |

## Current conclusions

- adotenv is fastest on simple parse and map building.
- dotenv-compat is faster on mixed and real-world map building because it
  returns an unordered map and does not retain the lossless document model.
- adotenv uses the lowest instructions for simple parsing and keeps default
  loading below 30 MB retained.
- The parse-only binary is about 24 percent larger than the smallest measured
  parser.
