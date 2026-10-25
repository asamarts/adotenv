# adotenv-process

Explicit process-environment mutation for
[`adotenv`](https://crates.io/crates/adotenv) loaded environments.

```rust,ignore
use adotenv::load::load_cascade;
use adotenv_process::apply;

let loaded = load_cascade("/srv/my-app", "production")?;
// The caller must guarantee no concurrent process-environment access.
unsafe { apply(&loaded)? };
```

`apply` validates every key and value before the first mutation. It sets only
keys from the loaded environment and cannot roll changes back. The process
environment is global, so the caller owns the concurrency guarantee.

Prefer iterating or collecting `LoadedEnvironment::values()` when mutation is
not required.

This is the only workspace crate with an unsafe API. The core `adotenv` crate
forbids unsafe code. The crate supports Rust 1.81.
