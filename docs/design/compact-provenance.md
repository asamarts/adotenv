# Unified provenance model for 0.10/1.0

Status: decisions recorded 2026-10-25. The unified breaking model is planned for
release **0.10.0**; 0.9.19 remains the current published release. No unified
implementation has shipped yet.

## Decisions

- There will be one provenance type, `LoadedEnvironment`. The separate
  `CompactLoadedEnvironment`, `CompactLoadOrigin` and compact loader functions
  will be removed rather than carried into 1.0.
- Values, origins and raw winning tokens are retained by default.
- Reports are opt-in.
- `LoadOptions::new(dialect)` selects the default lean profile; a `full`
  constructor selects reports too.
- Keys and values use `Arc<str>` so value-table and report records can share
  allocations.
- `LoadedEnvironment` exposes borrowed iterators/views rather than owned
  `BTreeMap<String, String>` or `&[LoadedEntry]` snapshots:
  - `get(&str)` and borrowed value iteration;
  - borrowed `LoadedEntry` views;
  - compact `LoadOrigin<'_>` views with accessors;
  - borrowed override/repeated/skipped report views when reports are enabled.
- Public view fields are private; callers use accessors.
- The 0.9.19 compact API is removed in 0.10.0. Assumed no external users.
- `load_values`/`LoadedValues` remain the values-only path.
- The unified model ships as 0.10.0 first; 1.0 follows only after the existing
  sustained-fuzz and external-adopter evidence bars.

## Target memory profile

For the 65,536-entry fixture, the intended default profile is values + shared
origins + raw tokens with reports off. The 0.9.19 measurements provide the
bounds:

| Loader/profile | Peak RSS | Retained RSS |
|---|---:|---:|
| Current full default | 63.6 MB | 37.1 MB |
| Compact values + origins | 41.3 MB | 16.2 MB |
| Values + origins + raw (target) | about 54 MB | about 27 MB |
| Values-only | 13.6 MB | 13.6 MB |

The exact unified numbers must be measured after implementation; the target is
below 30 MB retained with the default profile.

## Public shape

```text
LoadedEnvironment
  get(key)                 -> Option<&str>
  values()                 -> Iterator<Item = (&str, &str)>
  origin(key)              -> Option<LoadOrigin<'_>>
  entries()                -> Iterator<Item = LoadedEntry<'_>>   // reports opt-in
  overrides()              -> Iterator<Item = LoadedOverride<'_>>
  repeated()               -> Iterator<Item = LoadRepeated<'_>>
  skipped()                -> Iterator<Item = LoadSkipped<'_>>
  retains_raw_tokens()
  resolve_contextual(...)
  resolve_decoded(...)
  into_values()            -> LoadedValues

LoadOrigin<'_>
  path() -> &Path
  span() -> &Range<usize>
  line() -> usize
```

`LoadedEntry<'_>`, `LoadedOverride<'_>`, `LoadRepeated<'_>` and
`LoadSkipped<'_>` expose private borrowed data through accessors. Report
iterators are empty unless enabled through `LoadOptions`.

## Internal shape

- `values: BTreeMap<Arc<str>, Arc<str>>`
- one shared `files: Vec<PathBuf>` table
- compact final origins keyed by interned key
- contiguous raw-token source buffer with ranges
- opt-in report records sharing `Arc<str>` keys/values and compact origins
- no owned report snapshots or cached values map

## Implementation sequence

1. Introduce the unified internal storage and borrowed public view types.
2. Change `LoadOptions` to lean-by-default with an explicit `full` constructor.
3. Adapt full loading and resolution; remove compact types/loaders.
4. Update CLI, process, config and Figment consumers to borrowed iteration.
5. Rewrite loader tests for borrowed views, report opt-in and default profiles.
6. Measure full default, values-only, and report-enabled memory profiles.
7. Run the full workspace, MSRV, alloc-only, conformance, semver, docs, Windows
   and release gates.
8. Publish 0.10.0 with a migration section explaining the breaking view/API
   changes and the removal of the 0.9.19 compact API.

## Compatibility and evidence

This model intentionally breaks the 0.8–0.9 public field/return types under
pre-1.0 semver. The assumption of no existing users makes a direct 0.10.0
transition preferable to compatibility aliases. The 1.0 evidence bars remain
unchanged.
