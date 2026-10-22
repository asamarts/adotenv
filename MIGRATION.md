# Migrating and adopting adotenv

Status: current published release **0.9.3**; target compatibility release **1.0**.
The core API has been frozen since 0.8 and no core breaking changes are planned
for 1.0. This guide is the user-facing path; the evidence checklist for the 1.0
milestone lives in [docs/design/1.0-readiness.md](docs/design/1.0-readiness.md).

## Adopt the core

```toml
[dependencies]
adotenv = "0.9.3"
```

The default feature set is `std`. Disable default features for `alloc`-only
`no_std`; enable optional features explicitly:

| Feature | Effect |
|---|---|
| default (`std`) | Standard-library error integration and normal desktop use |
| `load` | Opt-in filesystem cascade loading |
| `serde` | Opt-in typed deserialization |
| `exec` | Opt-in process command substitution with an explicit working directory |

The core has no runtime dependencies, performs no I/O or environment access by
default, and forbids unsafe code. Loading, process mutation and command
execution are separate opt-ins or workspace crates.

```rust
use adotenv::parse_strict;

let doc = parse_strict("host=localhost\nport=8080\n")?;
assert_eq!(doc.get("host"), Some("localhost"));

let edited = doc.set("port", "9090")?;
assert_eq!(parse_strict(edited.as_str())?.get("port"), Some("9090"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Common next steps:

- Format or sort: [`Document::format`](README.md#format-whitespace-05) and
  [`Document::sort`](README.md#sort-within-groups-05).
- Compare or redact: [`Document::compare`](README.md#compare-files-by-key-05)
  and [`Document::example`](README.md#generate-an-example-05).
- Load a cascade: [`load_cascade`](README.md#load-files-06-opt-in).
- Deserialize: [`Document::deserialize`](README.md#deserialize-with-serde-06-opt-in).
- Apply to a child process: [`run`](README.md#cli-09-workspace-companion) or the
  separate [`adotenv-process`](README.md#apply-to-the-process-environment-07-opt-in)
  adapter.
- Use the CLI: [`adotenv-cli`](docs/design/cli.md).

## Version map

| Version | Main additions |
|---|---|
| 0.1 | Portable literal parsing, verified `set`/`remove` |
| 0.2 | Lossless items, diagnostics, spans, batch edits, `no_std` |
| 0.3 | Dialect presets, portability reporting, `parse_with` |
| 0.4 | Reference inspection, opt-in resolution, command callbacks |
| 0.5 | Formatting, structure views, sorting, comparison, examples |
| 0.6 | Cascade loading, origin-aware loads, opt-in serde |
| 0.7 | CLI, process adapter, `config` and Figment integrations |
| 0.8 | Effective entries, lookup index, resolved loaded values, API freeze |
| 0.9 | Recursive discovery, diagnostic formats, multi-file diff, Windows writes |
| 0.9.1 | Non-UTF-8 discovery and `./` exclude fixes |
| 0.9.2 | Published API examples, frozen CLI/JSON contracts, performance budget |
| 0.9.3 | Migration/adoption guide and 1.0 readiness checklist |
| 1.0 | Compatibility target; no planned core API break |

## Migrating from 0.9.x to 1.0

No source migration is expected. Update the dependency and check:

- wildcard arms for non-exhaustive public enums and report/policy structs;
- feature selection if you use `load`, `serde` or `exec`;
- CLI machine consumers: version-1 JSON and GitHub annotations remain stable;
- `Document::get` still scans entries; use `Document::lookup_index()` for
  repeated lookups;
- MSRV remains Rust 1.81 unless a future release explicitly raises it.

The CLI's `--json` flag remains an alias for `--format json`. Human-readable
output and the matrix table are not machine contracts; see
[Machine-output compatibility](docs/design/cli.md#machine-output-compatibility).

## Migrating from 0.8 or earlier

Read the focused README migration notes:

- [Migrating from 0.7](README.md#migrating-from-07)
- [Migrating from 0.3](README.md#migrating-from-03)
- [Migrating from 0.2](README.md#migrating-from-02)
- [Migrating from 0.1](README.md#migrating-from-01)

The main cross-version themes are lossless parsing replacing permissive loading,
explicit dialect selection instead of guessing, verified native edits,
non-exhaustive API structs with `with_*` builders, and opt-in I/O/process
features.

## Adoption checklist

- [ ] Pin a released version and select only the features you need.
- [ ] Match on diagnostic `code().as_str()` values, not message text.
- [ ] Keep `.env` values literal unless you explicitly call `resolve` or load a
  cascade with a resolver.
- [ ] Use `unique`/`get_all` when duplicate ambiguity matters.
- [ ] Treat JSON version 1 and GitHub annotation fields as machine contracts.
- [ ] Run `cargo test` and `cargo semver-checks` against your own public API if
  you wrap adotenv types.
- [ ] Report migration problems as issues with a minimal input and version.
- [ ] If you adopt adotenv externally, consider sharing the version/features used
  and one integration or migration finding; private confirmation is sufficient.
  The 1.0 external-user criterion is defined in
  [the readiness checklist](docs/design/1.0-readiness.md#external-user-definition).

## Known boundaries

- No implicit file search, environment inheritance, expansion or command
  execution in the core.
- `config` support uses `config` 0.14; 0.15 changes key normalization and the
  current 0.15 line requires newer Rust/edition support.
- Windows writes use a best-effort backup swap and are not crash-atomic.
- There is no language server or semantic-token integration yet.
- The external-user and sustained-fuzz criteria for 1.0 are tracked separately;
  this guide does not claim they are complete.
