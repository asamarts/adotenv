# adotenv-process

Explicit process-environment mutation for [`adotenv`](https://crates.io/crates/adotenv)
loaded environments.

```rust,ignore
use adotenv::load::load_cascade;
use adotenv_process::apply;

let loaded = load_cascade("/srv/my-app", "production")?;
// The caller must guarantee no concurrent process-environment access.
unsafe { apply(&loaded)? };
```

`apply` validates every key and value before the first mutation, sets only the
loaded keys and cannot roll changes back. The process environment is global and
not thread-safe on all platforms, so the caller owns those guarantees. Reading
`LoadedEnvironment::values()` into an application map remains the safe default.

This is the only crate in the workspace with an unsafe API; the core `adotenv`
crate remains `#![forbid(unsafe_code)]`. This crate is part of the
[`adotenv` workspace](https://github.com/asamarts/adotenv) and supports Rust
1.81.
