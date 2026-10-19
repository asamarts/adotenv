# References and expansion

Status: 0.4, released 2026-10-18. Literal reference inspection, opt-in variable
resolution, explicit command callbacks and an opt-in `exec` helper are
implemented; the CLI remains future. The core never executes commands by
itself; resolution uses only the caller-supplied lookup and callback.

Variable expansion is where `.env` handling most often goes wrong silently: a
`$` inside a password or hash gets replaced, a variable name is cut short, a
self-reference loops, or a command in a file runs on load. adotenv therefore
separates finding references from resolving them, and resolves nothing unless
asked.

## References (0.4)

`entry.references()` returns recognized raw candidates without deciding that
they are eligible for expansion:

- `$VAR` and `${VAR}`
- `${VAR:-default}`, `${VAR-default}`, `${VAR:?message}`, `${VAR?message}`,
  `${VAR:+alternative}`, `${VAR+alternative}`, including nested defaults
- `$(command)`, including nested command payloads

Each `Reference` carries its kind, whole-form source span, quote and escape
context, nesting depth, and whether the closing delimiter was found. Variable
forms carry a name span and any operator/operator-span/argument spans; command
forms carry a raw payload span. `ReferenceContainer` records the containing
operator or command forms, outermost first, so a caller can see why a nested
candidate is or is not active. Single-quoted, escaped-dollar and malformed
candidates remain observable; their presence does not imply eligibility.
`Entry::references()` inspects an appended quoted tail as unquoted context.
Delimiter matching balances mixed `${...}` and `$(...)` forms and honors basic
quote/backslash state, but it is not a full shell parser: exotic heredoc,
arithmetic or nested-quoting constructs remain inspection candidates rather
than a shell-exact interpretation.

`Reference::is_eligible(ReferenceRules)` is a pure helper. `OperatorSet`
selects no variables, braced-only, basic, Compose-style or dotenv-expand-style
forms; `ReferenceRules` additionally selects single-quote, escaped-dollar,
backtick and command eligibility. Nested candidates require every containing
form to be eligible as well. Constants such as `ReferenceRules::PORTABLE`,
`COMPOSE`, `DOTENV_EXPAND` and `NONE` are convenience policies, not conformance
claims. Portable rules suppress single-quoted and escaped-dollar candidates;
other selected rules may permit them. Inspection performs no lookup, I/O or
command execution.

This supports linter analysis and editor navigation/renaming without
executing expansion. Callers must still select eligibility and binding rules.
Without caller values or conditional-branch state, undefined-variable,
cycle and unused-key findings may be potential rather than certain.

## Resolution (0.4)

```rust,ignore
let resolved = doc.resolve(|key| lookup.get(key).cloned(), &ExpandOptions::compose())?;
```

`lookup` is supplied by the caller as a closure: a map, a process-environment
snapshot, or any other source must be wrapped explicitly. Caller values are
literal and are not re-expanded. Options:

| Option | Choices |
|---|---|
| Operators | none, braced only (`${VAR}`), basic (`$VAR`, `${VAR}`), Compose (the full operator set, nested), or dotenv-expand style |
| Lookup order | caller's lookup first, or the file first |
| Scope | sequential (latest earlier definition) or whole-file (final definition, including for self-reference) |
| Unresolved variable | empty string, kept as written, or an error |
| Quotes/escapes | single-quote, escaped-dollar and backtick eligibility |
| Limits | maximum active depth and resolved bytes per value |

Bindings follow the selected scope consistently, with no special fallback
to an earlier definition for self-reference. With no caller-supplied seed,
`A=base` followed by `A=${A}x` resolves to `basex` in sequential scope but
is a cycle in whole-file scope. Caller/file lookup precedence remains an
explicit, separate option. Command substitutions remain literal unless
`resolve_with_commands` receives an explicit callback; ineligible references
remain literal. Nothing is executed implicitly.

Resolution guarantees:

- `${VAR:?message}` produces a structured error naming the key, the
  reference and its span.
- Reachable cycles under the selected bindings (`A=${B}`, `B=${A}`, or
  whole-file `A=${A}x`) are detected and reported, never looped on.
- Nesting depth and the size of an expanded value are limited, with
  configurable limits.
- Every resolved value records where each part came from: which key, which
  file and span, or the caller's lookup. File-origin parts retain nested
  provenance.

## Command substitution (callback and opt-in exec helper implemented)

adotenv never runs commands on its own. `$(command)` is reported as a raw
candidate with its quote/escape context. Command forms remain literal during
normal resolution and are executed only when the caller supplies a callback to
`Document::resolve_with_commands` and the selected rules enable commands.

The callback receives structured context:

- The command text exactly as written between `$(` and `)`.
- The whole reference span and raw payload span.
- An optional source identity supplied by the caller.

Source identity is opaque in the no-std core; it does not imply a filesystem
path. The caller supplies execution context and per-source policy.

The callback returns output or a `CommandError`. Every call is recorded as a
`CommandCall` with its spans, source identity and output/error outcome.
Successful outputs are literal resolved data and are not re-expanded. A
`ResolveFailure` retains calls made before a later error: an error does not
imply that earlier callbacks had no side effects, and adotenv cannot roll them
back.

The opt-in std-only `exec` feature provides `exec::Shell`. It requires an
explicit working directory with optional source-identity-to-directory origins,
never falls back to the host process's working directory, runs the command
through the platform shell exactly as written, trims one trailing newline and
returns nonzero-status, spawn and UTF-8 output errors as `CommandError`. Its
documentation is explicit that it executes arbitrary trusted input. The core
and default features contain no process-spawning code. The workspace CLI does
not enable command substitution; its `run` command starts a child with loaded
values and remains separate from `exec`.
