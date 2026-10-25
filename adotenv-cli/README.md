# adotenv-cli

Command-line tools for literal `.env` files, built on
[`adotenv`](https://crates.io/crates/adotenv).

```console
cargo install adotenv-cli
```

```text
adotenv-cli [--dialect portable|node|python|compose|docker] <command> [options]

get <file> <key>
check [--format human|json|github] [--source] [--recursive] [--exclude <path>] <path>...
fmt [--write|--check] [--sort] [--recursive] [--exclude <path>] <path>...
diff [--format human|matrix|json] <baseline> <file>...
example <file>
load <directory> [mode]
set <file> <key> <value>
run <directory> [mode] -- <program> [args...]
```

`run` starts a child with loaded values and does not change the parent
environment. `fmt --write` and `set` use a same-directory replace on Unix. On
Windows the fallback backup swap is not crash-atomic.

Values stay literal. The CLI does not expand references, inherit environment
values or run command substitutions from files.

The crate has no runtime dependencies outside the workspace and supports Rust
1.81.
