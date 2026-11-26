# adotenv-xtask

Internal benchmark automation for adotenv. The package is unpublished and is
not a runtime dependency of the library or CLI.

## Commands

```console
adotenv-xtask fixtures
adotenv-xtask memory --out docs/benchmarks/memory.jsonl --sizes 8,128 --shapes plain
adotenv-xtask micro --bench fixtures --quick
adotenv-xtask speed --out docs/benchmarks/speed.txt --bench competitors
adotenv-xtask cli --out docs/benchmarks/cli.json --size 128 --shape plain
adotenv-xtask report --input docs/benchmarks/memory.jsonl --out docs/benchmarks/memory.md
adotenv-xtask compare --baseline old.jsonl --current docs/benchmarks/memory.jsonl
adotenv-xtask gate --baseline docs/benchmarks/memory.jsonl --current target/memory-current.jsonl
adotenv-xtask fingerprint
```

`memory` builds and invokes the release-mode isolated memory binary (accepting
`--compact` to include `adotenv-compact` and `adotenv-compact-entries`), writes
JSONL results, and writes a sibling `<name>.fingerprint.json`. `cli` builds `adotenv-cli` and
runs hyperfine over a generated fixture. `report` renders memory JSONL rows as
a Markdown table. `compare` reports numeric deltas against a previous memory
JSONL file, and `gate` enforces the deterministic memory baseline thresholds.

## Remote queue

`queue` and `queue-speed` run benchmarks on a dedicated remote Linux host over
SSH using a serialized `pueue` queue. Pass the host and queue settings with CLI
flags or environment variables:

- `--host <host>` or `ADOTENV_BENCH_HOST` (required): SSH target host.
- `--remote-root <path>` or `ADOTENV_BENCH_ROOT` (required): absolute, non-root,
  non-home working directory writable by the queue service.
- `--pueue-config <path>` or `PUEUE_CONFIG_PATH` (optional): remote `pueue`
  configuration file.
- `--group <name>` or `ADOTENV_BENCH_GROUP` (optional, default `bench`):
  single-slot `pueue` group.
- `--queue-user <user>` or `ADOTENV_BENCH_USER` (optional): remote service user
  used with non-interactive `sudo` when syncing binaries into a restricted
  state directory.

With `--sync`, `queue` and `queue-speed` require a clean working tree so the
queued result can be tied to the local git SHA, build the release binaries
locally, install them into `--remote-root`, queue one serialized task, wait for
completion, and copy back the outputs and provenance fingerprint:

```console
export ADOTENV_BENCH_HOST=<bench-host>
export ADOTENV_BENCH_ROOT=/var/lib/bench-runner/adotenv-bench

adotenv-xtask queue --host "$ADOTENV_BENCH_HOST" \
  --remote-root "$ADOTENV_BENCH_ROOT" \
  --out docs/benchmarks/memory.jsonl --sync
adotenv-xtask queue-speed --host "$ADOTENV_BENCH_HOST" \
  --remote-root "$ADOTENV_BENCH_ROOT" \
  --out docs/benchmarks/speed.txt --sync
```

Use `--dry-run` to inspect the queued SSH command without contacting the remote
host. Memory tasks write `bench-memory.jsonl` and its fingerprint remotely and
copy both to `--out`; speed tasks write `bench-speed.txt` and its fingerprint
remotely, copy both to `--out`, and render the local Markdown sibling (derived
from `--out`). Neither path changes host-wide settings or other projects' queues.
