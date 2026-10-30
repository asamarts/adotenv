# Benchmark dashboard

Reference host: Intel Core i7-6700HQ, 4 cores and 8 threads, 2.6 GHz,
15 GiB RAM, rustc 1.97.0. Tests were pinned to one CPU.

Method and budgets are in [performance.md](performance.md) and
[benchmark-plan.md](benchmark-plan.md).

## Simple fixture

65,536 declarations:

| Workload | Wall | Instructions | Retained |
|---|---:|---:|---:|
| parse | 33.3 ms | 207M | 26.5 MB |
| parse + map | 40.9 ms | 277M | 14.2 MB |

## Mixed fixture

| Workload | Wall | Instructions | Retained |
|---|---:|---:|---:|
| parse | 20.6 ms | 106M | 11.0 MB |
| parse + map | 27.6 ms | 153M | 5.8 MB |

## Real-world corpus

Pinned public configurations, repeated with unique key prefixes.

| Workload | Wall | Instructions | Retained |
|---|---:|---:|---:|
| parse | 9.4 ms | 63M | 4.2 MB |
| parse + map | 11.2 ms | 78M | 2.4 MB |

## Loader profiles

| Profile | Wall | Instructions | Retained |
|---|---:|---:|---:|
| Values only | 54.9 ms | 366M | 9.9 MB |
| Default | 73.9 ms | 465M | 27.9 MB |
| Full reports | 120.1 ms | 687M | 37.7 MB |

## Dialects

65,536 declarations:

| Dialect | Wall | Instructions | Retained |
|---|---:|---:|---:|
| Portable | 33.3 ms | 207M | 26.5 MB |
| Node | 60.6 ms | 423M | 14.3 MB |
| Python | 50.6 ms | 329M | 14.3 MB |
| Compose | 51.7 ms | 305M | 14.2 MB |
| Docker | 42.7 ms | 269M | 14.3 MB |

## Operations

65,536-entry workloads:

| Operation | Time |
|---|---:|
| native set | 61.7 ms |
| native batch | 63.6 ms |
| format normalize | 76.7 ms |
| sort | 161.1 ms |
| compare large versus typical | 61.0 ms |

## Build and startup

Clean release builds on the reference host:

| Binary | Build | Stripped size | Startup |
|---|---:|---:|---:|
| parse-and-print demo | 46.6 s | 454 KB | 651 us |
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

- Default loading stays below 30 MB retained for 65,536 declarations.
- Values-only loading retains about a third of the default profile.
- Mixed and real-world map building pays for the ordered map and the lossless
  document model. Both remain candidates for later optimization.
- The parse-only binary is within its 500 KB budget.
