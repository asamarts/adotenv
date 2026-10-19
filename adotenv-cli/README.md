# adotenv-cli

Command-line tools for literal `.env` documents, built on
[`adotenv`](https://crates.io/crates/adotenv).

```console
cargo install adotenv-cli
```

The binary is `adotenv-cli`:

```text
adotenv-cli [--dialect <portable|node|python|compose|docker>] <command> [options]

get <file> <key>             print an effective literal value
check <file>...              report parse diagnostics
fmt [--write|--check] [--sort] <file>...
                             format, check or write documents
diff <left> <right>          compare effective keys
example <file>               print a value-free example
load <directory> [mode]      load the standard dotenv cascade
set <file> <key> <value>     set and write one declaration
run <directory> [mode] -- <program> [args...]
                             run a child with loaded values
```

`run` starts a child with the loaded values and never mutates the parent
process environment. `fmt --write` and `set` use an atomic same-directory
rename on Unix. Values remain literal: no expansion, environment inheritance or
command substitution is performed.

This crate is part of the [`adotenv` workspace](https://github.com/asamarts/adotenv).
It has no runtime dependencies outside the workspace and supports Rust 1.81.
