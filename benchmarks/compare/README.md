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

Measure clean builds, feature sizes and startup with:

```console
python3 scripts/engineering.py
```

Use `--check-budgets` to fail when a measured profile exceeds the budgets in
`scripts/compare.py`. Use `--check-baseline` to compare the current kbench run
against `benchmarks/compare/baseline.json`. Use `--smoke` for a quick local check that skips
instruction counts and only uses the small fixture. Do not run timing-sensitive comparisons on a busy host.
