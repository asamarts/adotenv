# adotenv-bench

Unpublished benchmark foundation for `adotenv`.

The crate generates byte-identical `.env` fixtures across sizes and syntax
shapes, exposes their names, entry counts and SHA-256 hashes, and provides the
initial criterion targets. Benchmark-only dependencies stay out of the
published core crate.

## Competitor matrix

Pinned adapters: `dotenvy` 0.15.7, `dotenv` 0.15.0, `dotenvs` 0.2.2,
`dotenv-parser` 0.1.3 and `dotenv-flow` 0.16.2. `dotenv-ng` is deferred until
its loader/CLI surface can be compared without semantic ambiguity.

Only validated cells are timed. The comparable values and load shapes are
currently `plain`, `long-line` and `empty`; quoted/escaped, duplicate,
malformed, multiline, CRLF/BOM, reference and realistic mixed fixtures are
reported as unsupported because the libraries disagree on their semantics.
Reference resolution is not compared because the competitors interpolate with
different policies; `unsupported_cells()` records every excluded cell and
reason.

## Memory runs

The `memory` binary measures one case in a fresh process using a counting
allocator. It reports peak live heap, live heap while the result is held,
heap after dropping it, allocation count and Linux RSS fields.

```console
cargo run -p adotenv-bench --bin memory -- \
  --mode values --implementation adotenv --size 8 --shape plain
```

Use `--format markdown` for one table row. Supported modes are `values`,
`document`, `load` and `resolve`; competitor document/resolve cells are
intentionally unsupported because their stateful semantics are not comparable.
