# Quality

This page describes how adotenv is tested and what each check covers. The
current coverage figures and known gaps are in the [test audit](test-audit.md).

## Test layers

- Unit tests for the parser, model, dialects, edits, encoding, formatting,
  sorting, comparison, portability, structure, positions, references and
  resolution.
- Property tests on arbitrary UTF-8 input for parsing, dialects, edits,
  encoding, formatting, sorting, comparison, portability, structure, examples,
  references, resolution and `parse_values`. CI runs them with 2,000 cases
  each.
- Integration tests for loading, serde, the CLI and its JSON schemas, the
  adapters, and the memory use of reference scanning.
- Shared format-contract vectors for positions, text edit order, GitHub
  escaping and the check report schema, with a hash manifest.
- Doctests for the rustdoc examples, the README and every page of this site.
- Builds for `no_std`, the minimum supported Rust versions and each feature
  set.

## Continuous integration

The gate is the [CI workflow](https://github.com/asamarts/adotenv/blob/main/.github/workflows/ci.yml),
which runs on every pull request, every push to `main` and every release tag.
Its main steps:

```console
cargo fmt --all --check
python3 scripts/check_docs.py
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked
cargo test --workspace --locked
cargo test -p adotenv --features load,serde --locked
cargo audit --deny warnings
cargo deny check
cargo semver-checks check-release --package adotenv --package adotenv-process \
  --package adotenv-config --package adotenv-figment
cargo package -p adotenv --locked
python3 conformance/check_adotenv.py --setup
```

The workflow also:

- builds and tests every feature set: default, `exec`, `load`, `serde`,
  `load,serde`, no default features, and no default features with `serde`,
  plus the adapters with `load`;
- runs the tests on Linux, macOS and Windows;
- builds the libraries, `adotenv-process`, `adotenv-cli` and `adotenv-figment`
  with Rust 1.81, and `adotenv-config` with Rust 1.88;
- builds the core for the `thumbv7em-none-eabi` target without default
  features;
- resolves every workspace dependency to its lowest compatible version on
  nightly and runs the full test suite;
- checks that every package includes its license files;
- runs the benchmarks once as a smoke test.

All warnings are errors in CI.

## Diagnostics

Diagnostic codes and spans are stable; message text may change. Recovering
parsers retain items and diagnostics. Strict parsing and edit verification do
not build diagnostic lists they do not use.

## Fuzzing

The repository has five bounded fuzz targets with public seed corpora:
`parser`, `edits`, `encoding`, `resolve` and `format`. The
[fuzz workflow](https://github.com/asamarts/adotenv/blob/main/.github/workflows/fuzz.yml)
runs each target for five minutes, weekly and on demand, with bounded time per
input, memory and input length.

```console
cargo +nightly fuzz run parser
```

`scripts/dispatch_fuzz.sh` starts one workflow run and waits for its result.
Release evidence uses scheduled runs; the rule is in
[1.0 readiness](1.0-readiness.md#fuzz-rule). Ad hoc local campaigns are
development checks only.

## Conformance

`conformance/manifest.json` records pinned observations from real Node,
Python, Compose and Docker env-file implementations. The corpus has 89 cases
and 21 profiles. The runner checks package and source identities, hashes the
inputs and compares literal observations plus a curated Compose resolution
profile.

```console
python3 conformance/check_adotenv.py --setup
```

Compose's default parser performs interpolation. Literal comparison uses
dollar-free inputs with an empty lookup; the separate `compose_resolve` profile
compares curated Compose interpolation cases. Portable is not asserted equal
to any upstream implementation.

## Coverage and mutation testing

CI produces a coverage report with `cargo llvm-cov` and uploads it as an
artifact. There is no coverage threshold, and coverage alone does not block a
release.

Mutation testing runs weekly and on demand in four shards. It targets parser
diagnostics, key validation, edit verification and formatting equivalence,
with a 90-second timeout and two jobs per shard. Survivors are input for
targeted regression tests, not an automatic release blocker.

```console
cargo mutants --file src/parser.rs --file src/edit.rs --file src/format.rs \
  --re 'in (merge_lone_cr|valid_key_in|verify_entry_region|same_meaning)' \
  --shard 0/4 --timeout 90 --jobs 2
```

## Supported Rust versions and semver

The minimum supported Rust version is 1.81. `adotenv-config` follows the
minimum of `config`, currently 1.88. A minimum version rises only in a minor
release.

The public API follows semantic versioning. Enums and structs that may grow
are non-exhaustive, and options are set through builders. CI runs
`cargo semver-checks` for the four library crates against their latest
published versions. Breaking releases list their changes and a migration guide
in the changelog; see [Upgrading to 1.0](../reference/upgrading.md).

## Supply chain

`deny.toml` allows only Apache-2.0, MIT and Unicode-3.0 dependencies, denies
unknown registries and Git sources, and reports duplicate versions as warnings.
Add a reviewed exception only when the dependency or duplicate is intentional.
GitHub Actions are pinned by commit.

## Limits

A successful bounded test or fuzz run is not a proof for all inputs. Callers
handling untrusted source should limit its size before parsing; see
[Security and limits](../reference/security.md).
