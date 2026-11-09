# adotenv-bench

Unpublished benchmark foundation for `adotenv`.

The crate generates byte-identical `.env` fixtures across sizes and syntax
shapes, exposes their names, entry counts and SHA-256 hashes, and provides the
initial criterion targets. Benchmark-only dependencies stay out of the
published core crate.
