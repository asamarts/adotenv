# Unified provenance model for 0.10/1.0

Status: **shipped in 0.10.0** on 2026-10-25. The 0.9.19 compact API was a
short-lived pre-1.0 bridge and is removed.

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
| 0.9.19 full default | 63.6 MB | 37.1 MB |
| 0.9.19 compact values + origins | 41.3 MB | 16.2 MB |
| 0.10.0 default (values + origins + raw) | 53.8 MB | 29.7 MB |
| 0.10.0 full reports | 63.9 MB | 35.6 MB |
| 0.10.0 values-only | 13.9 MB | 12.6 MB |

The measured 0.10.0 default is below the 30 MB retained target. The extra
residency over the 0.9.19 compact API is the retained raw-token context and
final origin map.

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

LoadOriginOwned              // resolution errors
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

## Implementation status

Completed:
1. Unified internal storage and borrowed public view types.
2. Lean `LoadOptions::new` default with an explicit `LoadOptions::full`.
3. Unified full loading and resolution; compact types/loaders removed.
4. CLI/process/config/Figment consumer updates.
5. Loader tests for borrowed views, report opt-in and default profiles.
6. Memory measurement: 53.8/29.7 MB default, 63.9/35.6 MB full, 13.9/12.6 MB
   values-only.

Publication:
7. Full workspace, MSRV, alloc-only, conformance, semver, docs, Windows and
   release gates passed.
8. 0.10.0 published on 2026-10-25; the migration section documents the breaking
   view/API changes and removal of the 0.9.19 compact API.

## Compatibility and evidence

This model intentionally breaks the 0.8–0.9 public field/return types under
pre-1.0 semver. The assumption of no existing users makes a direct 0.10.0
transition preferable to compatibility aliases. The 1.0 evidence bars remain
unchanged.
