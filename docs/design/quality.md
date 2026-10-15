# Quality, testing and compatibility

Status: quality targets for 0.2 and later. The 0.1 library has unit and
property tests, compiled README examples, and CI checks for formatting,
Clippy, documentation, MSRV, packaging, and tests on Linux, macOS and Windows.
Fuzzing, public conformance and benchmarks below are planned.

## Testing

- **Unit tests** for every syntax rule and diagnostic, with the expected items
  and diagnostic codes, severity and spans, not incidental message wording.
- **Property tests** (proptest):
  - ordered raw items and the BOM partition any UTF-8 source without gaps
    or overlaps, including malformed syntax;
  - spans are in bounds, contained in the appropriate item and aligned to
    valid UTF-8 boundaries;
  - `encode_value` output reads back exactly, for every dialect that can
    represent the value;
  - every edit changes only its targets: unrelated entries, invalid items
    and diagnostic identities remain unchanged, allowing expected position
    shifts; all bytes outside the edits are identical.
  - position conversions round-trip exactly where representable and reject
    invalid coordinates, including Unicode boundaries, split surrogate
    pairs, EOF, final-newline, lone-CR and CRLF edge cases.
- **Fuzzing** (cargo-fuzz): the parser, the edit operations and resolution,
  checking that nothing panics and that the round trip and edit properties
  hold on arbitrary UTF-8 text. Byte-oriented fuzz inputs are validated as
  UTF-8 before entering the parser. The corpus is kept in the repository,
  and fuzzing runs on a schedule in CI.
- **Documentation examples** are compiled and run as tests, including the
  README's.

## Conformance corpus

A public set of `.env` inputs, each with the expected parse under every
preset: items, decoded values, and diagnostics. The expected results for a
preset come from running the implementation it imitates, with the exact
version, API/options, controlled lookup and file role recorded next to the
results. Literal parsing, expansion and environment loading are separate
observations; a helper API is not a substitute for a different file context.
The corpus starts with the cases where common implementations disagree (see
[dialects.md](dialects.md)), and grows with every reported difference.

## Performance

- Linear parsing per selected dialect; no regular expressions; no allocation
  for values that need no unescaping. Portability checking parses the same
  retained source once per requested dialect without copying borrowed input.
- Criterion benchmarks for parsing, edits and resolution on small, typical
  and large files (including multi-megabyte inputs), run in CI to catch
  regressions.
- adotenv's own numbers (throughput and scaling) are published with each
  release in the benchmarks documentation.

## Safety

- `#![forbid(unsafe_code)]` in `adotenv`, unconditionally across features,
  and in other crates that do not apply values to the process environment.
- No I/O, environment access or process spawning in the core. Loading lives
  behind the `load` feature, and command execution behind `exec`, which only
  runs when the caller passes a command callback.
- Changing the process environment is confined to the opt-in std-only
  `adotenv-process` adapter. Its application function is `unsafe`, with the
  caller's conditions documented; file loading and map generation stay in
  the unsafe-free `adotenv` crate.
- Configurable value-size and expansion-depth limits bound supported
  resource use. Linear scanning and adversarial fuzz cases are also required;
  limits alone do not establish parser safety.

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
