# adotenv-xtask

Internal benchmark automation for adotenv. The package is unpublished and is
not a runtime dependency of the library or CLI.

## Commands

```console
adotenv-xtask fixtures
adotenv-xtask memory --out docs/benchmarks/memory.jsonl --sizes 8,128 --shapes plain
adotenv-xtask micro --bench fixtures --quick
adotenv-xtask cli --out docs/benchmarks/cli.json --size 128 --shape plain
adotenv-xtask report --input docs/benchmarks/memory.jsonl --out docs/benchmarks/memory.md
adotenv-xtask compare --baseline old.jsonl --current docs/benchmarks/memory.jsonl
adotenv-xtask fingerprint
```

`memory` builds and invokes the isolated memory binary, writes JSONL results,
and writes a sibling `<name>.fingerprint.json`. `cli` builds `adotenv-cli` and
runs hyperfine over a generated fixture. `report` renders memory JSONL rows as
a Markdown table. `compare` reports numeric deltas against a previous memory
JSONL file; it is advisory until the deterministic gates in #77 land.

## kbench

`queue` accepts an absolute workspace path at `--remote-root` on `kbench` and
uses `--sync` to mirror the current clean commit there before queueing. It
queues a serialized task through the shared benchq daemon:

```console
adotenv-xtask queue --remote-root /home/kaminsod/projects/adotenv \
  --out docs/benchmarks/memory.jsonl --sync
```

The daemon configuration is `/etc/benchq/pueue.yml` and the group is `bench`
with one parallel slot. Use `--dry-run` to inspect the queued command without
contacting the host. The task copies `target/bench-memory.jsonl` and its
fingerprint back after it finishes. Do not change host-wide settings or other
projects' queues.
