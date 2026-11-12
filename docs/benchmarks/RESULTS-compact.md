# Compact values benchmark evaluation

Status: publication-grade compact-values evaluation for a future minor release.
It does not replace the published 1.4.1 map-based release claim.

The compact representation was measured on `kbench` at source SHA `ed90ae8`
using the same fixtures, competitors and comparable values/load modes as the
[1.4.1 results](RESULTS-1.4.1.md). Raw evidence:

- [Compact memory JSONL](compact-memory.jsonl) -- 165 cases
- [Compact memory table](compact-memory.md)
- [Compact memory fingerprint](compact-memory.fingerprint.json)

## Provenance

| Item | Value |
|---|---|
| Source SHA | `ed90ae8` |
| Reference host | `kbench` |
| CPU | Intel Core i7-6700HQ, 8 logical CPUs |
| OS / kernel | Linux / 7.1.3-arch1-2 |
| Rust | 1.97.1 (8bab26f4f 2026-07-14) |
| Fingerprint | `load1=0.17` |
| Queue | benchq/pueue, group `bench`, one slot |

## Claim

Against the best pinned competitor in each values/load cell:

| Metric | Non-empty cells (20) | Empty cells (10) |
|---|---:|---:|
| Retained heap | **20/20 strict wins** | **10/10 ties** |
| Peak heap | **20/20 strict wins** | **10/10 strict wins** |
| Allocation count | **20/20 strict wins** | **10/10 strict wins** |

No cell loses. Empty retained heaps are zero bytes for both compact values and
the map-based reference, so a strict retained win is impossible there. The
empty peak/allocation wins come from the competitors' parsers allocating a
container while the compact and map-based APIs allocate nothing.

For the non-empty cells, compact retained heap is 0.57-0.87 times the best
competitor (median 0.76), peak heap is 0.70-0.98 times (median 0.85) and
allocation count is 0.00016-0.33 times (median 0.0097).

## Scope

This evaluation covers only `values` and `load` memory. It does not add a
speed claim. The published 1.4.1 comparison remains the map-based result:

- 25/25 speed wins
- 30/30 peak-heap wins
- 30/30 allocation wins
- 30/30 retained-heap ties, 0 losses

The compact API is intended to turn the non-empty retained cells into strict
wins in a later minor release. It is additive: `parse_values` and
`load_values` keep their existing map-based APIs.

## Reproduce

```console
cargo run --release -p adotenv-xtask -- queue \
  --remote-root /var/lib/benchq/adotenv-bench \
  --out docs/benchmarks/compact-memory.jsonl \
  --remote-out bench-compact-memory.jsonl \
  --sizes 8,128,1k,10k,100k --shapes plain,long-line,empty \
  --modes values,load --compact --sync

cargo run --release -p adotenv-xtask -- report \
  --input docs/benchmarks/compact-memory.jsonl \
  --out docs/benchmarks/compact-memory.md
```

Use the same `PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml` group and one-slot queue
as every other publication run. Do not bypass the shared queue.
