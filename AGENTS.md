# Agent notes

## Heavy validation

Do not run full workspace tests, mutation tests, fuzzing, benchmarks,
deterministic performance runs or conformance setup on the shared development
host. Use a dedicated remote validation host through its single-slot serialized
queue or GitHub Actions. Check queue status first and do not queue while
another project's task is running or queued; submit one shard at a time. On a
15 GiB host without swap, use one mutation worker and an 8 GiB per-process cap;
two workers caused a 14.5 GiB OOM. The policy, incident analysis and bounded
validation procedure are in
[docs/design/heavy-validation.md](docs/design/heavy-validation.md).
