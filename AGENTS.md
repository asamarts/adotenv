# Agent notes

## Heavy validation

Do not run full workspace tests, mutation tests, fuzzing, benchmarks,
deterministic performance runs or conformance setup on this shared
development host. Use `kbench` through the serialized `benchq` pueue queue or
GitHub Actions. On `kbench`, use one mutation worker and an 8 GiB per-process
cap; two workers caused a 14.5 GiB OOM on the 15 GiB host. The policy,
incident analysis and exact kbench commands are in
[docs/design/heavy-validation.md](docs/design/heavy-validation.md).
