# Process environment

`adotenv-process` applies a loaded environment to the current process. It is a
separate crate because changing the process environment is unsafe: the
environment is shared by every thread, and on some platforms reading it while
another thread writes it is undefined behavior. The core `adotenv` crate
forbids unsafe code and never changes the environment.

```toml
[dependencies]
adotenv = { version = "1", features = ["load"] }
adotenv-process = "1"
```

The crate has no features of its own and needs Rust 1.81.

## Prefer passing values explicitly

Most programs do not need to change their own environment. Read values from
`LoadedEnvironment::get` or `values()`, deserialize them with the `serde`
feature, or pass them to a child process:

```rust,no_run
use std::process::Command;

let loaded = adotenv::load::load_cascade(".", "production")?;
let status = Command::new("my-server").envs(loaded.values()).status()?;
# let _ = status;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`adotenv-cli run` does the same from a shell. Use `adotenv-process` only when
code you do not control reads the process environment.

## Apply

```rust,ignore
use adotenv::load::load_cascade;
use adotenv_process::apply;

let loaded = load_cascade("/srv/my-app", "production")?;

// SAFETY: called at startup, before any other thread exists.
unsafe { apply(&loaded)? };
```

`apply(&LoadedEnvironment)` sets every loaded key to its loaded value:

- It validates every key and value before the first change, so a rejected
  environment changes nothing.
- It sets only the loaded keys. Existing variables with other names are kept,
  and nothing is removed.
- Values are set as loaded. References such as `${HOME}` are not expanded.
- Changes cannot be rolled back.

The caller must guarantee that no other thread reads or writes the process
environment while `apply` runs. In practice, call it at the start of `main`,
before starting threads or an async runtime.

`apply` takes a `LoadedEnvironment`. To apply resolved values, set them
yourself after resolving.

## Validate

`validate(&LoadedEnvironment)` runs the same checks without changing anything.
It is safe to call from any thread.

`ApplyError` reports the first problem:

| Variant | Meaning |
|---|---|
| `EmptyKey` | A key is empty. |
| `InvalidKey { key }` | A key contains NUL or `=`. |
| `InvalidValue { key }` | A value contains NUL. |

The error never includes a value.
