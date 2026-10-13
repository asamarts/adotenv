# References and expansion

Status: planned for 0.4.

Variable expansion is where `.env` handling most often goes wrong silently: a
`$` inside a password or hash gets replaced, a variable name is cut short, a
self-reference loops, or a command in a file runs on load. adotenv therefore
separates finding references from resolving them, and resolves nothing unless
asked.

## References (always available, nothing resolved)

`entry.references()` returns every reference in a value, as written:

- `$VAR` and `${VAR}`
- `${VAR:-default}`, `${VAR-default}`, `${VAR:?message}`, `${VAR?message}`,
  `${VAR:+alternative}`, `${VAR+alternative}`, including nested defaults
- `$(command)`

Each reference has its kind, the span of the whole reference and of the
name, the operator, and the default or message as written. Text in single
quotes and escaped dollars (`\$`, and `$$` in dialects that use it) are not
references. Which forms count as references depends on the dialect.

This is enough for a linter (undefined variable, reference cycle, unused key)
and an editor (go to definition, find references, rename a key and its uses)
without ever expanding anything.

## Resolution (opt-in)

```rust
let resolved = doc.resolve(&lookup, &ExpandOptions::compose())?;
```

`lookup` is supplied by the caller: a map, a closure, or the process
environment, which must be passed explicitly. Options:

| Option | Choices |
|---|---|
| Operators | none, braced only (`${VAR}`), basic (`$VAR`, `${VAR}`), Compose (the full operator set, nested), or dotenv-expand style |
| Lookup order | caller's lookup first, or the file first |
| Scope | sequential (only keys defined earlier in the file) or the whole file |
| Unresolved variable | empty string, kept as written, or an error |
| Single quotes | not expanded (default) or expanded, for dialects that do |

Guarantees:

- `${VAR:?message}` produces a structured error naming the key, the
  reference and its span.
- Cycles (`A=${B}`, `B=${A}`, or `A=${A}x`) are detected and reported, never
  looped on.
- Nesting depth and the size of an expanded value are limited, with
  configurable limits.
- Every resolved value records where each part came from: which key, which
  file and span, or the caller's lookup.

## Command substitution

adotenv never runs commands on its own. `$(command)` is always reported as a
reference, and under dialects whose original implementation runs commands,
a diagnostic says the command would run there.

To run commands, the caller passes a callback:

```rust
let options = ExpandOptions::compose().commands(|command: &str| -> Result<String, String> {
    // the caller decides what is allowed and how to run it
});
```

The callback receives the command text exactly as written and returns its
output or an error. Every call is recorded in the resolution's origin data.

For developers working with their own, trusted files, the `exec` feature
provides a ready-made callback:

```rust
let options = ExpandOptions::compose().commands(adotenv::exec::shell());
```

`exec::shell()` runs each command through the system shell exactly as
written, in the directory of the file, with output trimmed of one trailing
newline. Its documentation is explicit that it runs arbitrary commands and
must only be used with trusted files. It is never enabled by default, and the
core crate contains no process-spawning code without the `exec` feature. The
CLI offers the same through `--allow-commands`.
