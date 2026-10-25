# References and resolution

Finding a reference and resolving it are separate operations.

## Reference inspection

`Entry::references()` returns raw candidates in source order:

- `$VAR` and `${VAR}`;
- `${VAR:-default}`, `${VAR-default}`, `${VAR:?message}`, `${VAR?message}`,
  `${VAR:+alternative}` and `${VAR+alternative}`;
- `$(command)`, including nested payloads.

Each `Reference` records its kind, full span, quote and escape context, nesting,
closing delimiter state and contained forms. Variable forms expose name and
operator spans plus arguments. Command forms expose the raw payload span.

Single-quoted, escaped-dollar and malformed candidates remain visible. Visibility
does not make a reference eligible.

`Reference::is_eligible(ReferenceRules)` is a pure policy check. Rules select
operators, single-quote behavior, escaped dollars, backticks and commands.
Nested references require every containing form to be eligible.

Inspection performs no lookup, expansion, I/O or command execution.

## Resolution

`Document::resolve(lookup, options)` substitutes eligible references using file
bindings and a caller closure. The core never reads the environment or files.

```rust,ignore
let resolved = doc.resolve(
    |key| lookup.get(key).cloned(),
    &ExpandOptions::portable(),
)?;
```

Caller values are literal and are not expanded again.

`ExpandOptions` selects:

| Option | Choices |
|---|---|
| Operators | none, braced only, basic, Compose or dotenv-expand style |
| Lookup order | caller first or file first |
| Scope | sequential or whole-file |
| Unresolved variables | empty, kept literal or error |
| Quote and escape rules | single quotes, escaped dollars, backticks |
| Limits | nesting depth and expanded bytes per value |

Sequential scope uses the latest earlier binding. Whole-file scope uses the final
binding. With no caller seed, `A=base` followed by `A=${A}x` gives `basex` in
sequential scope and a cycle in whole-file scope.

Every resolved value keeps its origin: file, span and key, or caller lookup.
Nested file provenance is retained.

Resolution reports required variables, cycles, depth and size limits as
structured errors. It never loops on cycles.

## Commands

Command forms stay literal unless the caller supplies a callback and the rules
enable commands.

`Document::resolve_with_commands` passes each command to the callback with:

- the command text as written;
- the full reference and payload spans;
- an optional caller source identity.

Output is literal and is not expanded again. Every call is recorded, including
failed calls. Earlier calls are not rolled back when a later resolution error
occurs.

The optional `exec` feature provides `exec::Shell`. It needs an explicit working
directory, runs through the platform shell, trims one trailing newline and never
uses the host working directory as a fallback. Only use it with trusted input.
