# Quality

## Test layers

The workspace uses:

- unit tests for parser, model, edits, encoding, formatting, sorting, comparison
  and positions;
- property tests for arbitrary UTF-8 parsing, edits, encoding and comparison;
- integration tests for loading, serde, CLI schemas and adapter behavior;
- compile tests for `no_std`, MSRV and feature combinations;
- doctests for public examples.

Run the main gate:

```console
cargo fmt --all --check
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked
cargo +1.81 build -p adotenv-cli --locked
python3 conformance/check_adotenv.py --setup
cargo package -p adotenv --locked
```

Feature checks cover default, `load`, `load,serde`, no-default `alloc` and the
`thumbv7em-none-eabi` target.

## Diagnostics

Recovering parsers retain items and diagnostics. Strict parsing and native edit
verification avoid building diagnostic lists that they do not use.

Diagnostic codes and spans are stable. Message text may change.

## Fuzzing

The workspace has five scheduled fuzz targets:

- parser;
- edits;
- encoding;
- resolution;
- formatting.

Each target has a public seed corpus. Scheduled runs use bounded time and memory.
Manual campaigns are useful during development but do not replace the scheduled
coverage used for release evidence.

The fuzz crate also has a local differential target for the simple Portable
subset against dotenv-compat:

```console
cd fuzz
cargo +nightly fuzz run --target x86_64-unknown-linux-gnu differential
```

This target is for local investigation and is not part of the five-target
scheduled evidence.

Run one target with:

```console
cargo fuzz run parser
```

## Conformance

`conformance/manifest.json` records pinned observations from real Node, Python,
Compose and Docker env-file APIs. The current corpus has 80 cases and 20 profiles.
The runner checks package and source identities, hashes inputs and compares only
observations that map to the literal adotenv model.

```console
python3 conformance/check_adotenv.py --setup
```

Compose's default parser performs interpolation, so only dollar-free inputs with
an empty lookup are literal comparison oracles. Portable is not asserted equal to
any upstream implementation.

## Performance

Performance tests are local and use fixed inputs. CI runs smoke benchmarks only.
The comparison rules and current results are in [performance.md](performance.md).

## MSRV and semver

The MSRV is Rust 1.81. It rises only in a minor release.

The public API follows semver. New variants and fields use non-exhaustive types
and builders. A breaking release is announced with its compatibility details in the
release notes.

## Limits

A successful bounded test or fuzz run is not a proof for all inputs. Callers
handling untrusted source should set byte, line and declaration budgets before
parsing. Retained documents, decoded maps and requested diagnostics consume
memory.
