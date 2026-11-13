# Benchmark result schema

## Memory JSONL

`memory.jsonl` contains one JSON object per case. Field names are stable;
consumers must ignore unknown fields added in a later release.

| Field | Type | Meaning |
|---|---|---|
| `size` | string | Size label (`8`, `128`, `1k`, `10k`, `100k`) |
| `shape` | string | Fixture shape label |
| `mode` | string | `values`, `document`, `load` or `resolve` |
| `implementation` | string | `adotenv` or a pinned competitor label |
| `entries` | integer | Parsed declaration count |
| `source_bytes` | integer | Generated fixture byte length |
| `fixture_sha256` | string | SHA-256 of the exact fixture source |
| `peak_heap_bytes` | integer | Peak live heap above the pre-operation baseline |
| `retained_heap_bytes` | integer | Live heap while the result is held |
| `heap_after_drop_bytes` | integer | Live heap after dropping the result |
| `allocations` | integer | Allocation calls above the baseline |
| `rss_before_bytes` | integer or null | RSS before the operation |
| `rss_peak_bytes` | integer or null | Peak RSS after the operation |
| `rss_after_bytes` | integer or null | RSS after dropping the result |

Example:

```json
{
  "size": "8",
  "shape": "plain",
  "mode": "values",
  "implementation": "adotenv",
  "entries": 8,
  "source_bytes": 104,
  "fixture_sha256": "6ee242e0e29423ccf125f7433fe4f3da4bcb73bd9a338e01c07ae55c2391a0cf",
  "peak_heap_bytes": 632,
  "retained_heap_bytes": 632,
  "heap_after_drop_bytes": 0,
  "allocations": 17,
  "rss_before_bytes": 2613248,
  "rss_peak_bytes": 2613248,
  "rss_after_bytes": 2613248
}
```

The rendered `memory.md` is a presentation of these rows. The JSONL is the
source for the gates, comparisons and reports.

## Speed bencher output

`speed.txt` preserves Criterion's `bencher` lines:

```text
test <group>/<implementation>/<shape>/<size> ... bench: <estimate> ns/iter (+/- <spread>)
```

Groups are `competitor_values` and `competitor_load`. The estimate is the
comparison number; the spread is reported but not converted into a synthetic
confidence interval. The rendered `speed.md` table preserves the exact string
from the raw file.

`--quick` is not a publication mode because Criterion omits the benchmark name
from some quick bencher lines.

## Fingerprints

Each raw result has a sibling `*.fingerprint.json`:

```json
{
  "arch": "x86_64",
  "os": "linux",
  "cpu": "...",
  "cpu_count": 8,
  "kernel": "...",
  "rustc": "rustc ...",
  "hyperfine": "hyperfine ...",
  "git_sha": "...",
  "hostname": "...",
  "load1": "..."
}
```

`git_sha` is the source commit used to build the benchmark binary. The remote
queue path sets `ADOTENV_GIT_SHA` and `ADOTENV_RUSTC` from the local build, so a
benchmark run is traceable even when the host is reached over SSH.

## Baseline and gate files

- `memory.jsonl` is the committed memory baseline used by
  `adotenv-xtask gate`.
- `memory.fingerprint.json` and `speed.fingerprint.json` are provenance, not
  inputs to the correctness gate.
- The deterministic gungraun gate has no committed baseline; it always compares
  the merge base with the PR in one run.
- A baseline update requires a reviewed code change and a fresh fingerprint.
