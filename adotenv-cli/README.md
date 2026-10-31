# adotenv-cli

Command-line tools for literal `.env` files, built on
[`adotenv`](https://crates.io/crates/adotenv).

```console
cargo install adotenv-cli
```

```text
adotenv-cli [--dialect portable|node|python|compose|docker] <command> [options]

get <file> <key>
check [--format human|json|github] [--json] [--source] [--recursive] [--exclude <path>] <path>...
fmt [--write|--check] [--sort] [--recursive] [--exclude <path>] <path>...
diff [--format human|matrix|json] [--json] <baseline> <file>...
example <file>
load <directory> [mode]
set <file> <key> <value>
run <directory> [mode] -- <program> [args...]
```

`run` starts the program with the parent environment plus the loaded values. A
loaded value replaces a parent variable of the same name, and the parent
environment is not changed. Loaded files are trusted like code: a loaded `PATH`
changes which program runs.

`get`, `load` and `diff` print values, including JSON and matrix output of
`diff`. `load` prints Portable `KEY=value` lines that parse back to the same
values. `example` removes values but keeps comments verbatim.

`fmt --write` and `set` replace the file through a private temporary file in the
same directory, keep its mode, owner and group where possible, and write through
symbolic links. On Unix the replace is an atomic rename. On Windows the fallback
backup swap is not crash-atomic.

Values stay literal. The CLI does not expand references or run command
substitutions from files.

Exit codes are `0` for success, `1` for findings such as diagnostics or
differences, and `2` for usage errors, read or write failures and input a
command cannot process. `run` exits with the child's code. See the
[CLI contract](https://github.com/asamarts/adotenv/blob/main/docs/design/cli.md).

The crate has no runtime dependencies outside the workspace and supports Rust
1.81.
