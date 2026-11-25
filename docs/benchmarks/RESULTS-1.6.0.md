# 1.6.0 benchmark results

Status: publication-grade 1.6.0 compact-values and compact-entries evidence.
Both the memory and speed matrices were recorded from the same clean
release-candidate commit `79860ac`; neither replaces the historical
[1.5.0 results](RESULTS-1.5.0.md) or [1.4.1 results](RESULTS-1.4.1.md).

**Compact** (`adotenv-compact`) is the opt-in `CompactValues` API with one
value buffer and one fixed-size entry per key. **CompactEntries**
(`adotenv-compact-entries`, added in 1.6.0) uses the same single-buffer layout
while preserving winning bare declarations as `None` under `BareKeys::Preserve`.
**Map** (`adotenv`) is the standard `parse_values`/`load_values` API returning a
`BTreeMap<String, String>`. The charts default to all available pinned libraries
and can also isolate compact vs map or show the best competitor.

<div class="bench-highlights">
<div class="bench-highlight"><strong>25/25</strong><span>compact speed wins</span></div>
<div class="bench-highlight"><strong>10/10</strong><span>CompactEntries speed wins</span></div>
<div class="bench-highlight"><strong>20/20</strong><span>non-empty retained-heap wins</span></div>
<div class="bench-highlight"><strong>30/30</strong><span>peak-heap and allocation wins</span></div>
</div>

<p class="bench-callouts">
<a class="bench-callout" href="compact-memory-1.6.0.html"><strong>1.6.0 memory chart</strong><span>Explore retained heap, peak heap and allocations for CompactValues, CompactEntries, Map and pinned competitors.</span></a>
<a class="bench-callout" href="speed-1.6.0.html"><strong>1.6.0 speed chart</strong><span>Explore values and load timings across all sizes, shapes and libraries.</span></a>
</p>

Both matrices were measured on `kbench` from clean release-candidate SHA
`79860ac9b2d6c8b7a37772d692c6f82a9d8a1ac9`, using the same content-addressed
fixtures and comparable values/load modes as the
[1.5.0 results](RESULTS-1.5.0.md) and [1.4.1 results](RESULTS-1.4.1.md). Raw
evidence:

- [1.6.0 compact memory JSONL](compact-memory-1.6.0.jsonl) -- 180 cases
- [1.6.0 compact memory table](compact-memory-1.6.0.md)
- [1.6.0 compact memory fingerprint](compact-memory-1.6.0.fingerprint.json)
- [1.6.0 speed bencher output](speed-1.6.0.txt) -- 150 rows across 25 cells
- [1.6.0 speed table](speed-1.6.0.md)
- [1.6.0 speed fingerprint](speed-1.6.0.fingerprint.json)

## Provenance

| Metric set | Source SHA | Fingerprint |
|---|---|---|
| Memory | `79860ac9b2d6c8b7a37772d692c6f82a9d8a1ac9` | `load1=0.74` |
| Speed | `79860ac9b2d6c8b7a37772d692c6f82a9d8a1ac9` | `load1=0.91` |

Both runs used `kbench`, Intel Core i7-6700HQ (8 logical CPUs), Linux
7.1.3-arch1-2, Rust 1.97.1, and the serialized `benchq` `bench` group with one
slot. Both runs are from the same clean commit with zero Rust code changes
between the measured commit and the release commit. This historical 1.6.0
memory snapshot was recorded before `CountingAllocator::realloc` stopped
double-counting growing reallocations; heap byte metrics and speed timings are
unaffected, and the current post-fix CI compact baseline is in
[compact-memory.md](compact-memory.md).

## Claim

Against the best pinned competitor in each comparable cell:

### `CompactValues` (`adotenv-compact`, values and load)

| Metric | Result |
|---|---:|
| Speed | **25/25 strict wins** (`0.16-0.78x`, median `0.23x`) |
| Retained heap | **20/20 non-empty strict wins; 10/10 empty ties** (`0.57-0.87x`, median `0.76x`) |
| Peak heap | **30/30 strict wins** (`0.08-0.82x`, median `0.49x`) |
| Allocation count | **30/30 strict wins** (`0.000086-0.14x`, median `0.0071x`) |

`adotenv-compact` also passes the committed 1.5.0 memory baseline gate
(`30/30` cases, zero allocation or heap regressions) and stays within `-3.9%`
to `+3.0%` of the 1.5.0 speed record across all 25 cells. The standard map API
(`adotenv`) records **25/25 strict speed wins** (`0.27-0.81x`, median `0.33x`).

### `CompactEntries` (`adotenv-compact-entries`, values)

| Metric | Result |
|---|---:|
| Speed | **10/10 strict wins** (`0.15-0.29x`, median `0.20x`) |
| Retained heap | **10/10 non-empty strict wins; 5/5 empty ties** (`0.57-0.87x`, median `0.76x`) |
| Peak heap | **15/15 strict wins** (`0.12-0.59x`, median `0.40x`) |
| Allocation count | **15/15 strict wins** (`0.00016-0.12x`, median `0.0090x`) |

Across all 15 comparable `values` memory cells, `adotenv-compact-entries` has
byte-for-byte identical retained heap, peak heap and allocation counts to
`adotenv-compact`, and runs at `0.95-0.99x` (median `0.99x`) of
`adotenv-compact` wall time. No cell loses.

## Scope

This evaluation covers `values` and `load` memory and speed for `adotenv`,
`adotenv-compact`, `adotenv-compact-entries` (`values` mode) and the five pinned
competitors. The published [1.5.0 results](RESULTS-1.5.0.md) and
[1.4.1 results](RESULTS-1.4.1.md) remain historical evidence and are unchanged.

## Reproduce

```console
cargo run --release -p adotenv-xtask -- queue \
  --remote-root /var/lib/benchq/adotenv-bench \
  --out docs/benchmarks/compact-memory-1.6.0.jsonl \
  --remote-out bench-compact-memory-1.6.0.jsonl \
  --sizes 8,128,1k,10k,100k --shapes plain,long-line,empty \
  --modes values,load --compact --sync

cargo run --release -p adotenv-xtask -- report \
  --input docs/benchmarks/compact-memory-1.6.0.jsonl \
  --out docs/benchmarks/compact-memory-1.6.0.md

cargo run --release -p adotenv-xtask -- queue-speed \
  --remote-root /var/lib/benchq/adotenv-bench \
  --out docs/benchmarks/speed-1.6.0.txt \
  --remote-out bench-speed-1.6.0.txt --bench competitors --sync
```

Use the same serialized `benchq` `bench` group with one slot as every other
publication run. Do not bypass the shared queue.
