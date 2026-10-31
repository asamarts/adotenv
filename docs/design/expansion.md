# References and resolution

Finding a reference and resolving it are separate operations.

## Reference inspection

`Entry::references()` returns raw candidates in source order:

- `$VAR` and `${VAR}`;
- `${VAR:-default}`, `${VAR-default}`, `${VAR:?message}`, `${VAR?message}`,
  `${VAR:+alternative}` and `${VAR+alternative}`;
- `$(command)`, including nested payloads.

Each `Reference` records its kind, full span, quote and escape context, nesting
depth and closing delimiter state. Variable forms expose name and operator spans
plus arguments. Command forms expose the raw payload span.

Candidates are sorted by start. A nested candidate's `parent` is the index of
its innermost containing form, which comes earlier in the list, and the
candidates nested in a form directly follow it. The list holds at most one
candidate per `$`, so its size is linear in the value length.

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

`ExpandOptions::compose()` selects Compose-style reference rules: which forms
and operators are eligible, including nested arguments. It does not emulate
Docker Compose interpolation as a whole. Under the Compose dialect an escaped
`$$B` is not eligible, so resolution leaves `$$B` as written, while Compose
prints `$B`.

`ExpandOptions` selects:

| Option | Choices |
|---|---|
| Operators | none, braced only, basic, Compose or dotenv-expand style |
| Lookup order | caller first or file first |
| Scope | sequential or whole-file |
| Unresolved variables | empty, kept literal or error |
| Quote and escape rules | single quotes, escaped dollars, backticks |
| Limits | nesting depth, expanded bytes per value and total output bytes |

Sequential scope uses the latest earlier binding. Whole-file scope uses the final
binding. With no caller seed, `A=base` followed by `A=${A}x` gives `basex` in
sequential scope and a cycle in whole-file scope.

Every resolved value keeps its origin: file, span and key, or caller lookup.
Nested file provenance is retained.

Resolution reports required variables, cycles, depth and size limits as
structured errors. It never loops on cycles.

Nested provenance copies the referenced value's segments, so references that
repeat each other can make the output grow much faster than the file.
`max_output_bytes` bounds the whole output of one resolution: every resolved
value and every segment counts, nested parts included, with a fixed overhead
per segment. The budget is checked before each copy is made. Presets allow
64 MiB, several times what a 65,536-entry file needs. Exceeding it returns
`ResolveError::OutputTooLarge` with the entry being resolved and the limit.

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
