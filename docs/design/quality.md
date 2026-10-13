# Quality, testing and compatibility

How adotenv backs its claims of being correct, safe and fast, and how it
treats compatibility.

## Testing

- **Unit tests** for every syntax rule and diagnostic, with the input, the
  expected items and the expected diagnostics.
- **Property tests** (proptest):
  - parsing then printing any input reproduces it byte for byte;
  - every span slices to the text it claims;
  - `encode_value` output reads back exactly, for every dialect that can
    represent the value;
  - every edit changes only its target: all other entries read back
    unchanged, and all bytes outside the edit are identical.
- **Fuzzing** (cargo-fuzz): the parser, the edit operations and resolution,
  checking that nothing panics and that the round trip and edit properties
  hold on arbitrary bytes. The corpus is kept in the repository, and fuzzing
  runs on a schedule in CI.
- **Documentation examples** are compiled and run as tests, including the
  README's.

## Conformance corpus

A public set of `.env` inputs, each with the expected parse under every
preset: items, decoded values, and diagnostics. The expected results for a
preset come from running the implementation it imitates, with that
implementation's version recorded next to the results. The corpus starts with
the cases where common implementations disagree (see
[dialects.md](dialects.md)), and grows with every reported difference.

## Performance

- Single pass over the input; no regular expressions; no allocation for
  values that need no unescaping.
- Criterion benchmarks for parsing, edits and resolution on small, typical
  and large files (including multi-megabyte inputs), run in CI to catch
  regressions.
- adotenv's own numbers (throughput and scaling) are published with each
  release in the benchmarks documentation.

## Safety

- `#![forbid(unsafe_code)]` in every crate.
- No I/O, environment access or process spawning in the core. Loading lives
  behind the `load` feature, and command execution behind `exec`, which only
  runs when the caller passes a command callback.
- Changing the process environment is only available as an `unsafe fn`, with
  the conditions for calling it documented.
- Input limits (value size, expansion depth) make adversarial files safe to
  parse and resolve.

## Supported Rust versions

- The minimum supported Rust version (MSRV) is 1.81 from 0.2 (1.74 for 0.1).
- The MSRV is raised only in minor releases, and stays at least six months
  behind the latest stable Rust.
- CI builds the core on the MSRV.

## Semver

- Before 1.0, breaking changes happen in minor releases (0.2, 0.3, ...) and
  are listed in the changelog with migration notes.
- After 1.0, the public API follows semver. Diagnostic codes are never
  renumbered; new codes and new enum variants (on `#[non_exhaustive]` types)
  can arrive in minor releases.
- Each workspace crate is versioned independently.
