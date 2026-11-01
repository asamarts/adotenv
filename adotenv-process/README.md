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

`apply` takes a `LoadedEnvironment` from the `adotenv` `load` feature. It
validates every key and value before the first mutation and returns an
`ApplyError` for an empty key, a key that contains NUL or `=`, or a value that
contains NUL. It sets only keys from the loaded environment, removes nothing and
cannot roll changes back. The process environment is global, so the caller owns
the concurrency guarantee.

`validate` runs the same checks without changing anything.

Prefer passing `LoadedEnvironment::values()` to a child process, or reading them
directly, when mutation is not required. See the
[process environment guide](https://asamarts.github.io/adotenv/guide/process.html).

This is the only workspace crate with an unsafe API. The core `adotenv` crate
forbids unsafe code. The crate supports Rust 1.81.
