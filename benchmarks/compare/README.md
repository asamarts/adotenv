# Comparison harness

This crate is not published and is not part of the adotenv workspace. It builds
small programs against adotenv and several other `.env` parsers so their
performance can be compared on the same fixture.

Requirements:

- Rust toolchain
- `hyperfine`
- `valgrind`
- Python 3

Run the full comparison from the repository root:

```console
python3 scripts/compare.py
```

The command writes `target/compare/report.md` and JSON results. Use
`--update-doc` to replace the generated benchmark section in
`docs/design/competitive-analysis.md`.

Use `--smoke` for a quick local check that skips instruction counts and only
uses the small fixture. Do not run timing-sensitive comparisons on a busy host.
