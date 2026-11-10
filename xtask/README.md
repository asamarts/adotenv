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

`queue` accepts an absolute, benchq-readable working directory at
`--remote-root`. With `--sync` it builds the local memory and xtask binaries,
copies them there, and queues a serialized task through the shared benchq
daemon. The daemon runs as `benchq`, so do not use a `0700` home directory as
the remote root:

```console
adotenv-xtask queue --remote-root /var/tmp/adotenv-bench \
  --out docs/benchmarks/memory.jsonl --sync
```

The daemon configuration is `/etc/benchq/pueue.yml` and the group is `bench`
with one parallel slot. `--sync` requires a clean working tree so the queued
result can be tied to the local git SHA. Use `--dry-run` to inspect the queued
command without contacting the host. The task writes `bench-memory.jsonl` and
`bench-memory.fingerprint.json` in the remote root and copies both back. Do not
change host-wide settings or other projects' queues.
