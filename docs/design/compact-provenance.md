# Compact provenance for 1.0

Status: design note recorded 2026-10-24 after the 0.9.18 residency work. This is
a proposed 1.0 direction, not shipped API.

## Current profile

For the 65,536-entry fixture at 0.9.18:

| Configuration | Peak RSS | Retained RSS |
|---|---:|---:|
| Full default | 63.6 MB | 37.1 MB |
| Raw tokens disabled | 53.4 MB | 26.3 MB |
| Values-only | 13.6 MB | 13.6 MB |

The remaining default residency is dominated by the public value map, compact
entry keys/origins and raw winning tokens. Existing non-breaking improvements
already removed the duplicate final-origin map, shared file paths, materialized
entries lazily, stored overridden values sparsely and compacted raw tokens into
one buffer.

## Tested limit

A separate compact provenance result was prototyped: values plus a shared-file
origin map, with no entries, reports or raw tokens. It retained about 36.6 MB
on the same fixture versus 37.1 MB for the current full loader. The gain was not
enough to justify another public result type, so the prototype was not shipped.

The limiting factor is that `LoadedEnvironment::values()` must return an owned
`BTreeMap<String, String>`, while entry reports also require owned `String` keys
and values. Reducing the default further therefore requires an explicit API
data-model decision.

## Proposed 1.0 model

The recommended direction is a compact provenance representation with explicit
accessors rather than public owned fields:

- Intern keys and values with `Arc<str>` so the value map and report records can
  share allocations.
- Store origins by shared file index plus span/line everywhere, not only in the
  lazy entry records.
- Keep `LoadedEntry` materialization lazy and optional.
- Keep raw tokens in one contiguous source buffer.
- Preserve the existing `LoadedEnvironment` semantics and builder-style API
  during a migration window, even if underlying field types change.

A possible compact API surface:

```text
LoadedEnvironment::values()          // borrowed or materialized map
LoadedEnvironment::entries()         // lazy, cached report
LoadedEnvironment::origin(key)       // compact origin accessor
LoadedEnvironment::origin_path(...)  // resolved file path
```

The exact public types should be decided in a 1.0 migration proposal; changing
public `LoadOrigin.path`, `LoadedEntry.key` or `LoadedEntry.value` types is a
breaking change and should not be attempted in a patch release.

## Acceptance criteria

- Full default retained RSS below 30 MB on the 65,536-entry fixture, or a
  documented reason why the public compatibility target prevents it.
- No regression in `origin()` lookup performance after materialization.
- One shared file table for all origins; one allocation per interned key/value.
- Lazy entry reports remain byte-for-byte equivalent in values, ordering and
  origins.
- Migration guide and API examples updated before the 1.0 tag.

## Sequencing

1. Decide whether 1.0 changes public provenance/entry field types or introduces
   a separate compact result.
2. Implement interned values and origins behind the chosen API.
3. Re-run the full memory, loader, conformance, MSRV, Windows and fuzz gates.
4. Keep the 0.9 retention options (`with_reports`, `with_origins`,
   `with_raw_tokens`) as the supported lower-memory paths until 1.0 lands.
