# 1.7.3 benchmark results

Status: publication-grade post-release 1.7.3 compact-values, compact-entries
and map evidence. Both the memory and speed matrices were recorded from the
clean `v1.7.3` release commit `c5c199f`; neither replaces the historical
[1.6.0 results](RESULTS-1.6.0.md), [1.5.0 results](RESULTS-1.5.0.md) or
[1.4.1 results](RESULTS-1.4.1.md).

**Compact** (`adotenv-compact`) is the opt-in `CompactValues` API with one
value buffer and one fixed-size entry per key. **CompactEntries**
(`adotenv-compact-entries`) uses the same single-buffer layout while preserving
winning bare declarations as `None` under `BareKeys::Preserve`. **Map**
(`adotenv`) is the standard `parse_values`/`load_values` API returning a
`BTreeMap<String, String>`. The charts default to all available pinned
libraries and can also isolate compact vs map or show the best competitor.

<div class="bench-highlights">
<div class="bench-highlight"><strong>20/20</strong><span>non-empty compact speed wins</span></div>
<div class="bench-highlight"><strong>10/10</strong><span>CompactEntries speed wins</span></div>
<div class="bench-highlight"><strong>20/20</strong><span>non-empty retained-heap wins</span></div>
<div class="bench-highlight"><strong>30/30</strong><span>peak-heap and allocation wins</span></div>
</div>

<p class="bench-callouts">
<a class="bench-callout" href="compact-memory-1.7.3.html"><strong>1.7.3 memory chart</strong><span>Explore retained heap, peak heap and allocations for CompactValues, CompactEntries, Map and pinned competitors.</span></a>
<a class="bench-callout" href="speed-1.7.3.html"><strong>1.7.3 speed chart</strong><span>Explore values and load timings across all sizes, shapes and libraries.</span></a>
</p>

Both matrices were measured on the dedicated Linux reference host from clean
`v1.7.3` release SHA `c5c199fd102a09c58cf7e28d8695335c190f3570`, using the same
content-addressed fixtures and comparable values/load modes as the
[1.6.0 results](RESULTS-1.6.0.md), [1.5.0 results](RESULTS-1.5.0.md) and
[1.4.1 results](RESULTS-1.4.1.md). This is the first versioned publication
snapshot recorded after `CountingAllocator::realloc` stopped double-counting
growing reallocations. Raw evidence:

- [1.7.3 compact memory JSONL](compact-memory-1.7.3.jsonl) -- 180 cases
- [1.7.3 compact memory table](compact-memory-1.7.3.md)
- [1.7.3 compact memory fingerprint](compact-memory-1.7.3.fingerprint.json)
- [1.7.3 speed bencher output](speed-1.7.3.txt) -- 150 rows across 25 cells
- [1.7.3 speed table](speed-1.7.3.md)
- [1.7.3 speed fingerprint](speed-1.7.3.fingerprint.json)

## Provenance

| Metric set | Source SHA | Fingerprint |
|---|---|---|
| Memory | `c5c199fd102a09c58cf7e28d8695335c190f3570` | `load1=0.18` |
| Speed | `c5c199fd102a09c58cf7e28d8695335c190f3570` | `load1=0.13` |

Both runs used the dedicated quiet Linux x86_64 reference host (Intel Core
i7-6700HQ, 8 logical CPUs, Linux 7.1.3-arch1-2, Rust 1.97.1) and a single-slot
serialized queue (`bench` group, one slot). Both runs are from the exact tagged
`v1.7.3` commit.

## Claim

Against the best pinned competitor in each comparable cell:

### `CompactValues` (`adotenv-compact`, values and load)

| Metric | Result |
|---|---:|
| Speed (all 25 cells) | **20/25 strict wins** (20/20 non-empty wins, 5/5 empty `load` losses) |
| Speed (non-empty, 20 cells) | **20/20 strict wins** (`0.16-0.89x`, median `0.22x`) |
| Speed (empty `load`, 5 cells) | **5/5 losses** (`4.4-4.6 us` vs `2.5 us`, `1.79-1.87x`, median `1.84x`) |
| Retained heap | **20/20 non-empty strict wins; 10/10 empty ties** (`0.57-0.87x`, median `0.76x`) |
| Peak heap | **30/30 strict wins** (non-empty `0.085-0.82x`, median `0.49x`; `0 B` on empty) |
| Allocation count | **30/30 strict wins** (non-empty `0.000049-0.12x`, median `0.0043x`; `0` on empty) |

`adotenv-compact` wins all 10 `values` speed cells (`0.16-0.30x`, median
`0.21x`, within `-0.8%` to `+2.4%` of 1.6.0) and all 10 non-empty `load` speed
cells (`0.16-0.89x`, median `0.24x`). In the 5 empty-file `load` cells
(`empty/{8,128,1k,10k,100k}`), `adotenv` takes `4.4-4.7 us` (`4.42-4.66 us`)
and `adotenv-compact` takes `4.4-4.6 us` (`4.43-4.65 us`) versus `2.5 us` for
the fastest competitor (`dotenv`/`dotenvy`/`dotenv-flow`).
That ~2 us fixed per-file difference comes from the intentional 1.7.0 file-read
hardening (`#211`, `#220`): `read_load_path` performs a pre-open regular-file
check (preventing Unix FIFO hangs), opens the file, verifies `is_file()` on the
open descriptor and reads it instead of returning early on `metadata.len() == 0`
(which previously bypassed empty-file permissions and `/proc` pseudo-files). The
5 empty cells remain in the matrix and are recorded as losses per
[METHODOLOGY.md](METHODOLOGY.md).

`adotenv-compact` passes the committed post-realloc-fix CI compact memory gate
(`30/30` cases) with zero deterministic diffs across all 180 rows versus
[compact-memory.md](compact-memory.md). The standard map API (`adotenv`)
records **20/25 strict speed wins** (**20/20 non-empty strict wins**,
`0.27-0.91x`, median `0.36x`; 5/5 empty `load` losses `1.76-1.89x`), **30/30
peak-heap wins** (non-empty `0.087-0.90x`, median `0.62x`), **30/30
allocation-count wins** (non-empty `0.43-0.96x`, median `0.51x`) and **30/30
retained-heap ties**.

### `CompactEntries` (`adotenv-compact-entries`, values)

| Metric | Result |
|---|---:|
| Speed | **10/10 strict wins** (`0.16-0.30x`, median `0.21x`) |
| Retained heap | **10/10 non-empty strict wins; 5/5 empty ties** (`0.57-0.87x`, median `0.76x`) |
| Peak heap | **15/15 strict wins** (non-empty `0.12-0.59x`, median `0.40x`; `0 B` on empty) |
| Allocation count | **15/15 strict wins** (non-empty `0.000088-0.11x`, median `0.0055x`; `0` on empty) |

Across all 15 comparable `values` memory cells, `adotenv-compact-entries` has
byte-for-byte identical retained heap, peak heap and allocation counts to
`adotenv-compact` (passing the `15/15` CI gate with zero diffs), and runs at
`0.97-0.99x` (median `0.99x`) of `adotenv-compact` wall time. No `values` cell
loses.

## Scope

This evaluation covers `values` and `load` memory and speed for `adotenv`,
`adotenv-compact`, `adotenv-compact-entries` (`values` mode) and the five
pinned competitors. The published [1.6.0 results](RESULTS-1.6.0.md),
[1.5.0 results](RESULTS-1.5.0.md) and [1.4.1 results](RESULTS-1.4.1.md) remain
historical evidence and are unchanged.

## Reproduce

```console
cargo run --release --locked -p adotenv-xtask -- queue \
  --host "$ADOTENV_BENCH_HOST" \
  --remote-root "$ADOTENV_BENCH_ROOT" \
  --out ~/.tmp/adotenv-bench-1.7.3/compact-memory-1.7.3.jsonl \
  --remote-out bench-compact-memory-1.7.3.jsonl \
  --sizes 8,128,1k,10k,100k --shapes plain,long-line,empty \
  --modes values,load --compact --sync

cargo run --release --locked -p adotenv-xtask -- queue-speed \
  --host "$ADOTENV_BENCH_HOST" \
  --remote-root "$ADOTENV_BENCH_ROOT" \
  --out ~/.tmp/adotenv-bench-1.7.3/speed-1.7.3.txt \
  --remote-out bench-speed-1.7.3.txt --bench competitors --sync

cargo run --release --locked -p adotenv-xtask -- report \
  --input docs/benchmarks/compact-memory-1.7.3.jsonl \
  --out docs/benchmarks/compact-memory-1.7.3.md
```

Write intermediate remote outputs to a scratch directory outside the repository
(such as `~/.tmp/adotenv-bench-1.7.3`) so sequential `--sync` runs keep a clean
working tree, then copy the raw artifacts and fingerprints into
`docs/benchmarks/` before rendering. Use the same single-slot serialized queue
as every other publication run.
