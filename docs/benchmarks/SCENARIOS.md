# Benchmark scenarios

## Sizes

| Label | Declarations |
|---|---:|
| `8` | 8 |
| `128` | 128 |
| `1k` | 1,000 |
| `10k` | 10,000 |
| `100k` | 100,000 |

## Shapes

| Shape | Purpose | Comparable |
|---|---|---|
| `plain` | `KEY=value` assignments | values, load |
| `long-line` | Long literal values | values, load |
| `empty` | Empty source | load (values has no cell) |
| `quoted-escaped` | Quotes, escapes, Unicode | not compared |
| `comments-export-bare` | Comments, `export`, bare keys | not compared |
| `duplicate` | Repeated keys | not compared |
| `malformed` | Invalid lines and recovery | not compared |
| `multiline` | Physical multiline values | not compared |
| `crlf-bom` | CRLF and byte-order mark | not compared |
| `references` | Literal reference operators | not compared |
| `realistic` | Conformance-derived mixed input | not compared |

The non-comparable shapes remain deterministic fixture targets for parser,
edit, format, resolve and internal tests, but are not used for the competitive
claim because the pinned libraries define different semantics for them.
Competitor output validation is still performed in the benchmark setup for
every comparable cell.

## Modes

| Mode | Implementation boundary |
|---|---|
| `values` | `adotenv::parse_values` vs in-memory competitor value parsers |
| `load` | `adotenv::load::load_values` vs competitor value loaders |
| `document` | Internal lossless adotenv document; no competitor |
| `resolve` | Internal literal resolution; no competitor |

## Comparable fixture hashes

These are the exact generated sources for the competitive matrix. The same
hashes are recorded in every memory JSONL row.

| Fixture | Size | Shape | Entries | Bytes | SHA-256 |
|---|---|---|---|---:|---:|---|
| `plain-8` | 8 | plain | 8 | 104 | `6ee242e0e29423ccf125f7433fe4f3da4bcb73bd9a338e01c07ae55c2391a0cf` |
| `long-line-8` | 8 | long-line | 8 | 608 | `341501be378e0933ca3d84c727ccb9193efb4bb0b8aa9cd0b5c7c00fad082e7b` |
| `empty-8` | 8 | empty | 0 | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `plain-128` | 128 | plain | 128 | 1810 | `8ebfee82a7efc8b185c463434b0920a11938426b9bcd633375b80fa8027b1511` |
| `long-line-128` | 128 | long-line | 128 | 9728 | `8aadefb43d65be0a64b6b6d3abdcc8792ae78e021ad76ddbc24f57d94730dcb9` |
| `empty-128` | 128 | empty | 0 | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `plain-1k` | 1k | plain | 1000 | 14890 | `afc72317ea349ba5328a7ae698d868e883d1b39f1bd228d3f2d6938764fb3384` |
| `long-line-1k` | 1k | long-line | 1000 | 76000 | `8c34de49d40298ad4a97dae0c1f718d7c142d5243ba19c60a8378e484b164f83` |
| `empty-1k` | 1k | empty | 0 | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `plain-10k` | 10k | plain | 10000 | 158890 | `8552dd3d21caccc41235b614ee8d6bf6acbaed18abf909c8f7076aecd4c863b2` |
| `long-line-10k` | 10k | long-line | 10000 | 760000 | `47f37841300d9f95cbde55ebca678de2bb60aa6eeac91dbb4df94acaed8355d7` |
| `empty-10k` | 10k | empty | 0 | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `plain-100k` | 100k | plain | 100000 | 1688890 | `e968e50fc1e028900f85b98023c67fafd6a5ff0f3df653c8331cebe5a4d9ef5d` |
| `long-line-100k` | 100k | long-line | 100000 | 7600000 | `8b971bc0fb16dcf52b7917fae7a0c6cce691a91bc67b888f4a6937009fb4bd8b` |
| `empty-100k` | 100k | empty | 0 | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

The memory output contains 165 rows: 60 internal adotenv cells across
`document`/`resolve` and the supported values/load cells, plus competitor
values/load cells. The speed output contains 115 cells: 40 values and 75 load
lines. Empty values are skipped by the Criterion values target because there is
no non-empty parse workload.

## Fixture generation

```console
cargo run -p adotenv-xtask -- fixtures > ~/.tmp/adotenv-fixtures.tsv
```

The command prints fixture name, SHA-256, entry count and byte length. Fixture
generation is deterministic and does not read the environment or filesystem
state.
