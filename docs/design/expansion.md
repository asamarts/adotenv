# References and expansion

Status: planned for 0.4.

Variable expansion is where `.env` handling most often goes wrong silently: a
`$` inside a password or hash gets replaced, a variable name is cut short, a
self-reference loops, or a command in a file runs on load. adotenv therefore
separates finding references from resolving them, and resolves nothing unless
asked.

## References (always available, nothing resolved)

`entry.references()` returns recognized reference candidates in the raw value,
without deciding that they are eligible for expansion:

- `$VAR` and `${VAR}`
- `${VAR:-default}`, `${VAR-default}`, `${VAR:?message}`, `${VAR?message}`,
  `${VAR:+alternative}`, `${VAR+alternative}`, including nested defaults
- `$(command)`

Each candidate has its kind, whole-form span and quote/escape context.
Variable forms also carry a name span and any operator/default/message
spans; command forms carry their raw payload span. Single-quoted and
escaped-dollar candidates remain observable; their presence does not imply
that they would expand or execute.

A pure eligibility helper applies explicit reference/expansion rules,
including operator support, quote activity and escaping. Resolution uses
the same rules. Portable rules suppress single-quoted and escaped-dollar
candidates, while other selected rules may permit single-quoted expansion.
Inspection performs no lookup, I/O or command execution.

This supports linter analysis and editor navigation/renaming without
executing expansion. Callers must still select eligibility and binding rules.
Without caller values or conditional-branch state, undefined-variable,
cycle and unused-key findings may be potential rather than certain.

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
| Scope | sequential (latest earlier definition) or whole-file (final definition, including for self-reference) |
| Unresolved variable | empty string, kept as written, or an error |
| Single quotes | not expanded (default) or expanded, for dialects that do |

Bindings follow the selected scope consistently, with no special fallback
to an earlier definition for self-reference. With no caller-supplied seed,
`A=base` followed by `A=${A}x` resolves to `basex` in sequential scope but
is a cycle in whole-file scope. Caller/file lookup precedence remains an
explicit, separate option.

Guarantees:

- `${VAR:?message}` produces a structured error naming the key, the
  reference and its span.
- Reachable cycles under the selected bindings (`A=${B}`, `B=${A}`, or
  whole-file `A=${A}x`) are detected and reported, never looped on.
- Nesting depth and the size of an expanded value are limited, with
  configurable limits.
- Every resolved value records where each part came from: which key, which
  file and span, or the caller's lookup.

## Command substitution

adotenv never runs commands on its own. `$(command)` is reported as a raw
candidate with its quote/escape context. A diagnostic says it would run under
another implementation only when eligible under that implementation's rules;
candidates ineligible under those rules are not labeled executable.

To run commands, the caller passes a callback receiving structured context:

- The command text exactly as written.
- The reference's source span.
- An optional source identity supplied by the caller.

Source identity is opaque in the no-std core; it does not imply a filesystem
path. The caller supplies execution context and per-source policy.

The callback returns command output or an error. Every call is recorded with
its source identity and reference span in the resolution's origin data.
Failures retain collected call provenance too: an error does not imply that
earlier callbacks had no side effects, and adotenv cannot roll them back.

For developers working with their own, trusted files, the `exec` feature
provides a ready-made `exec::shell` callback.

The helper is constructed with an explicit working directory or a
source-identity-to-origin mapping. Missing directory context is an error;
it never silently falls back to the host process's working directory.

It runs each command through the system shell exactly as written, in the
selected directory, with output trimmed of one trailing newline. Its
documentation is explicit that it runs arbitrary commands and must only be
used with trusted files. It is never enabled by default, and the core crate
contains no process-spawning code without the `exec` feature. The CLI offers
the same through `--allow-commands`.
