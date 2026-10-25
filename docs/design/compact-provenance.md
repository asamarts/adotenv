# Compact provenance (0.9.19)

Status: **shipped in 0.9.19** as an additive API. `LoadedEnvironment` remains
source-compatible.

## Result

`CompactLoadedEnvironment` stores one keyed map from `Box<str>` key to a
`Box<str>` value plus an optional compact origin. Origins point into a shared
file table instead of owning a `PathBuf` per key. The type exposes:

```text
get(key)                 -> Option<&str>
iter()                   -> Iterator<Item = (&str, &str)>
origin(key)              -> Option<&CompactLoadOrigin>
path(origin)             -> Option<&Path>
origin_path(key)         -> Option<&Path>
len() / is_empty()
into_values()            -> BTreeMap<String, String>
```

`CompactLoadOrigin` keeps its file index private and exposes `file_index()`,
`span()` and `line()`. Reports, raw context and override lists are intentionally
not retained.

## Loading APIs

```text
load_cascade_compact(directory, mode)
load_cascade_compact_with(directory, mode, dialect)
load_files_compact(paths, dialect)
LoadedEnvironment::into_compact()
```

The direct loaders use the same strict file reads, UTF-8 decoding, parser,
dialect, duplicate policy and later-file override rules as the full loader.
`into_compact` consumes an existing environment and preserves values plus
whatever origins were retained; if `with_origins(false)` was used, compact
origins remain absent.

## Memory

For the 65,536-entry fixture at 0.9.19:

| Loader | Peak RSS | Retained RSS |
|---|---:|---:|
| Full default | 63.6 MB | 37.1 MB |
| `load_files_compact` | 41.3 MB | 16.2 MB |
| Values-only | 13.6 MB | 13.6 MB |

The earlier two-map prototype retained only about 36.6 MB because it duplicated
map nodes and key/value ownership. The final one-map design is materially
smaller and is why the additive API is justified.

## Rationale and boundaries

- The API is additive; `LoadedEnvironment` values and report field types remain
  unchanged.
- `CompactLoadedEnvironment` deliberately has no `values()` returning an owned
  `BTreeMap`; callers use borrowed iteration or explicitly consume
  `into_values()`.
- `Box<str>` avoids the capacity word and atomic reference-count overhead that a
  single-owner compact map does not need.
- The separate shared file table avoids one path clone per key.
- Full-loader default residency remains dominated by its public report and value
  representations; changing those would require a 1.0 data-model decision.
