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
