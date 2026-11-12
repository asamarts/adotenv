# 1.5.0 compact values benchmark results

Status: publication-grade 1.5.0 compact-values release evidence. It does not
replace the historical 1.4.1 map-based release claim.

<div class="bench-highlights">
<div class="bench-highlight"><strong>25/25</strong><span>speed wins</span></div>
<div class="bench-highlight"><strong>20/20</strong><span>non-empty retained-heap wins</span></div>
<div class="bench-highlight"><strong>30/30</strong><span>peak-heap wins</span></div>
<div class="bench-highlight"><strong>30/30</strong><span>allocation wins</span></div>
</div>

<p class="bench-callouts">
<a class="bench-callout" href="compact-memory.html"><strong>Memory chart</strong><span>Explore retained heap, peak heap and allocations by size, shape and mode.</span></a>
<a class="bench-callout" href="speed-1.5.0.html"><strong>Speed chart</strong><span>Explore values and load timings by size, shape and mode.</span></a>
</p>

The compact representation was measured on `kbench` for memory from SHA
`0a9fc92` and for speed from SHA `79937cc`, using the same fixtures and
comparable values/load modes as the [1.4.1 results](RESULTS-1.4.1.md). Version
1.5.0 was tagged at `70f9762`. The memory source is the release candidate;
the speed source is a later benchmark-only commit. Raw evidence:

- [Compact memory JSONL](compact-memory.jsonl) -- 165 cases
- [Compact memory table](compact-memory.md)
- [Compact memory fingerprint](compact-memory.fingerprint.json)
- [Compact speed bencher output](speed-1.5.0.txt) -- 140 cells
- [Compact speed table](speed-1.5.0.md)
- [Compact speed fingerprint](speed-1.5.0.fingerprint.json)

## Provenance

| Metric set | Source SHA | Fingerprint |
|---|---|---|
| Memory | `0a9fc92` | `load1=0.09` |
| Speed | `79937cc` | `load1=0.21` |

Both runs used `kbench`, Intel Core i7-6700HQ (8 logical CPUs), Linux
7.1.3-arch1-2, Rust 1.97.1, and the benchq/pueue `bench` group with one slot.

## Claim

Against the best pinned competitor in each values/load cell:

| Metric | Result |
|---|---:|
| Speed | **25/25 strict wins** |
| Retained heap | **20/20 non-empty strict wins; 10/10 empty ties** |
| Peak heap | **30/30 strict wins** |
| Allocation count | **30/30 strict wins** |

No cell loses. Empty retained heaps are zero bytes for both compact values and
the map-based reference, so a strict retained win is impossible there. The
empty peak/allocation wins come from the competitors' parsers allocating a
container while the compact and map-based APIs allocate nothing.

For the non-empty memory cells, compact retained heap is 0.57-0.87 times the
best competitor (median 0.76), peak heap is 0.70-0.98 times (median 0.85) and
allocation count is 0.00016-0.33 times (median 0.0097). Speed is 0.16-0.78
times the best competitor (median 0.22).

## Scope

This evaluation covers `values` and `load` memory and speed. The published
1.4.1 map-based comparison remains historical evidence:

- 25/25 speed wins
- 30/30 peak-heap wins
- 30/30 allocation wins
- 30/30 retained-heap ties, 0 losses

The compact API is the 1.5.0 minor-release addition. It is opt-in:
`parse_values` and `load_values` keep their existing map-based APIs.

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

cargo run --release -p adotenv-xtask -- queue-speed \
  --remote-root /var/lib/benchq/adotenv-bench \
  --out docs/benchmarks/speed-1.5.0.txt \
  --remote-out bench-speed-1.5.0.txt --bench competitors --sync
```

Use the same `PUEUE_CONFIG_PATH=/etc/benchq/pueue.yml` group and one-slot queue
as every other publication run. Do not bypass the shared queue.
