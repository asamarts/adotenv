# References and expansion

Status: 0.4 development. Literal reference inspection is implemented; resolution,
command callbacks, the `exec` feature and the CLI remain future. The core still
parses literal values and never expands or executes.

Variable expansion is where `.env` handling most often goes wrong silently: a
`$` inside a password or hash gets replaced, a variable name is cut short, a
self-reference loops, or a command in a file runs on load. adotenv therefore
separates finding references from resolving them, and resolves nothing unless
asked.

## References (implemented in 0.4 development)

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

## Resolution (future opt-in)

```rust,ignore
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
used with trusted files. This proposed helper will never be enabled by default;
0.2 contains no process-spawning code under any feature. The future CLI will
offer the same explicit trust decision through `--allow-commands`.
