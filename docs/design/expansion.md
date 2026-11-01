# References and resolution

Parsing keeps `$VAR`, `${VAR:-default}` and `$(command)` as literal text.
Finding those references and resolving them are separate, explicit steps.

## Reference inspection

`Entry::references()` returns the raw candidates in a value, in source order:

- `$VAR` and `${VAR}`;
- `${VAR:-default}`, `${VAR-default}`, `${VAR:?message}`, `${VAR?message}`,
  `${VAR:+alternative}` and `${VAR+alternative}`;
- `$(command)`.

Candidates nested in operator arguments and in commands are returned too, as in
`${TAG:-$(git describe)}`.

```rust
use adotenv::{parse_strict, ReferenceKind, ReferenceOperator, ReferenceRules};

let source = "A=${HOST:-localhost}/$(hostname)\n";
let doc = parse_strict(source).unwrap();
let references = doc.unique("A").unwrap().references();

assert_eq!(&source[references[0].span.clone()], "${HOST:-localhost}");
assert_eq!(references[0].operator, Some(ReferenceOperator::DefaultIfUnsetOrEmpty));
assert_eq!(references[1].kind, ReferenceKind::Command);
assert!(references[0].is_eligible(ReferenceRules::COMPOSE));
assert!(!references[0].is_eligible(ReferenceRules::PORTABLE));
```

Each `Reference` records its `kind`, full `span`, `quote` and `escaped`
context, nesting `depth`, whether it is `braced` and `closed`, and its `parent`.
Variable forms also have `name`, `operator`, `operator_span` and `argument`
spans; command forms have the `command` payload span.

Candidates are sorted by start. A nested candidate's `parent` is the index of
its innermost containing form, which comes earlier in the list, and the
candidates nested in a form directly follow it. The list holds at most one
candidate per `$`, so its size is linear in the value length.

Single-quoted, escaped and malformed candidates remain visible. Being
visible does not make a reference eligible.

Inspection performs no lookup, expansion, I/O or command execution.

## Eligibility rules

`Reference::is_eligible(rules)` is a pure check of one candidate against
`ReferenceRules`. A nested reference is eligible only when every containing
form is eligible too.

| Field | Meaning |
|---|---|
| `operators` | `OperatorSet::None`, `Braced` (`${VAR}` only), `Basic` (`$VAR` and `${VAR}`), `Compose` or `DotenvExpand` (all operators and nesting) |
| `single_quotes` | References inside single quotes are eligible |
| `escaped_dollar` | Escaped candidates are eligible: `\$VAR` in double quotes when the dialect decodes `\$`, `\$` inside an operator argument or command, and the `$B` of `$$B` under the Compose grammar |
| `commands` | `$(command)` forms are eligible |
| `backtick_quotes` | References inside backticks are eligible |

| Preset | Operators | Single quotes | Commands |
|---|---|---|---|
| `ReferenceRules::NONE` | None | no | no |
| `ReferenceRules::PORTABLE` (default) | Basic | no | no |
| `ReferenceRules::COMPOSE` | Compose | no | no |
| `ReferenceRules::DOTENV_EXPAND` | DotenvExpand | yes | yes |

`OperatorSet::Compose` and `DotenvExpand` accept the same forms; the presets
differ in quotes and commands. Each field has a `with_*` builder.

`ReferenceRules::COMPOSE` selects Compose-style eligibility. It does not
emulate Docker Compose interpolation as a whole: under the Compose dialect an
escaped `$$B` is not eligible, so resolution leaves `$$B` as written, while
Compose prints `$B`.

## Resolution

`Document::resolve(lookup, options)` substitutes eligible references using the
file's own declarations and a lookup closure supplied by the caller. The core
never reads the environment or files on its own.

```rust
use adotenv::{parse_strict, ExpandOptions, LookupOrder};

let doc = parse_strict("BASE=base\nURL=${BASE}/api\nHOME_DIR=$HOME\n").unwrap();
let lookup = |key: &str| (key == "HOME").then(|| "/home/app".to_string());

let resolved = doc.resolve(lookup, &ExpandOptions::portable()).unwrap();
assert_eq!(resolved.entries()[1].value.as_deref(), Some("base/api"));
assert_eq!(resolved.entries()[2].value.as_deref(), Some("/home/app"));

let options = ExpandOptions::portable().with_lookup_order(LookupOrder::CallerFirst);
let caller = doc
    .resolve(|key| (key == "BASE").then(|| "external".to_string()), &options)
    .unwrap();
assert_eq!(caller.entries()[1].value.as_deref(), Some("external/api"));
```

Values from the lookup closure are literal and are not expanded again. To use
the process environment, pass `|key| std::env::var(key).ok()`.

### Options

`ExpandOptions` has four presets. All of them use sequential scope, file-first
lookup, keep unresolved references literal, and share the same limits:

| Preset | Rules |
|---|---|
| `ExpandOptions::portable()` (default) | `ReferenceRules::PORTABLE` |
| `ExpandOptions::compose()` | `ReferenceRules::COMPOSE` |
| `ExpandOptions::dotenv_expand()` | `ReferenceRules::DOTENV_EXPAND` |
| `ExpandOptions::none()` | `ReferenceRules::NONE` |

Builders change one option each:

| Builder | Choices | Preset value |
|---|---|---|
| `with_rules` | Any `ReferenceRules` | See above |
| `with_scope` | `ExpandScope::Sequential` or `WholeFile` | `Sequential` |
| `with_lookup_order` | `LookupOrder::FileFirst` or `CallerFirst` | `FileFirst` |
| `with_unresolved` | `Unresolved::Keep`, `Empty` or `Error` | `Keep` |
| `with_max_depth` | Nesting depth | 64 |
| `with_max_value_bytes` | Expanded bytes per value | 1 MiB |
| `with_max_output_bytes` | Total bytes of one resolution | 64 MiB |

In sequential scope a reference sees the declarations before its entry; in
whole-file scope it sees every declaration. Only declarations that take part in
lookup count. When a name is declared more than once, the dialect's
duplicate-key rule picks one of the visible declarations: the latest under
`Duplicates::Last`, which every dialect preset uses, or the first under
`Duplicates::First`. In whole-file scope this is the declaration that
`Document::get` returns. With a dialect preset and no caller value, `A=base`
followed by `A=${A}x` gives `basex` in sequential scope and a cycle in
whole-file scope. Under `Duplicates::First` it gives `basex` in both, because
the reference binds to `A=base`.

With `FileFirst`, a visible declaration wins over the lookup closure, even a
bare key: the closure is not called. With `CallerFirst`, the closure wins when
it returns a value.

### Unset variables and operators

A variable is unset when the lookup order finds no value for it. An empty value
is still set. A visible bare key supplies no value, except under
`BareKeys::Empty`, where it reads as empty.

A plain `$NAME` or `${NAME}` that is unset follows `Unresolved`: `Keep` (every
preset) leaves the reference as written, `Empty` replaces it with nothing, and
`Error` fails with `ResolveError::Unresolved`. Operator forms need rules that
accept them, such as `ExpandOptions::compose()`, and decide for themselves:

| Form | Result |
|---|---|
| `${NAME:-default}` | `default` when `NAME` is unset or empty, else its value |
| `${NAME-default}` | `default` when `NAME` is unset, else its value |
| `${NAME:+alternative}` | `alternative` when `NAME` is set and not empty, else empty |
| `${NAME+alternative}` | `alternative` when `NAME` is set, else empty |
| `${NAME:?message}` | `ResolveError::Required` when `NAME` is unset or empty, else its value |
| `${NAME?message}` | `ResolveError::Required` when `NAME` is unset, else its value |

An argument is resolved only when it is used, and may contain further
references.

### Results

`resolve` returns a `ResolvedDocument` with `entries()`, `commands()`, `len()`
and `is_empty()`. Each `ResolvedEntry` has the `entry` ID, `key`, assignment
`state`, resolved `value`, `effective_value` (the value the entry gives to
lookup, after the dialect's bare-key rule) and `segments`. The segments
concatenate to the value and record where each part came from: literal text, a
variable from the file (entry, key and span) or from the lookup, an unresolved
reference, or a command.

### Errors and limits

Resolution stops at the first error. `ResolveError` reports `Unresolved` (an
unset plain reference under `Unresolved::Error`), `Required` (from a `?` or
`:?` operator), `Cycle` (file values that refer to each other in a loop, in
whole-file scope), `DepthExceeded`, `ValueTooLarge`, `OutputTooLarge` and
`Command` (a `CommandError` from the callback). Resolution never loops on a
cycle.

A reference to a file value copies that value's segments into the result, so
references that repeat each other can make the output grow much faster than the
file. `max_output_bytes` bounds the whole output of one resolution: every
resolved value, segment and recorded command call counts, nested parts
included, with a fixed overhead for each. The budget is checked before each
copy is made. The presets allow 64 MiB, about three times what a file of 65,536
short declarations without references needs. Exceeding it returns
`ResolveError::OutputTooLarge` with the entry being resolved and the limit.

## Commands

Command forms stay literal unless the caller supplies a callback and the rules
enable commands.

```rust
use adotenv::{parse_strict, ExpandOptions};

let doc = parse_strict("A=$(echo hi)\n").unwrap();
let resolved = doc
    .resolve_with_commands(
        |_| None,
        |context| Ok(format!("ran:{}", context.command)),
        Some("app.env"),
        &ExpandOptions::dotenv_expand(),
    )
    .unwrap();
assert_eq!(resolved.entries()[0].value.as_deref(), Some("ran:echo hi"));
assert_eq!(resolved.commands().len(), 1);
```

`Document::resolve_with_commands(lookup, callback, source, options)` passes
each command to the callback as a `CommandContext` with:

- `command`, the command text as written;
- `reference` and `payload`, the full reference and payload spans;
- `source`, the optional caller source identity.

The callback returns the output or a `CommandError`. Output is literal and is
not expanded again. A command inside an operator argument runs only when the
argument is used. Every call is recorded as a `CommandCall`, in
`ResolvedDocument::commands()` on success. On failure, `resolve_with_commands`
returns a `ResolveFailure` with the `error` and the `commands` called so far,
including a failed call. Calls that already ran are not rolled back.

### `exec::Shell`

The `exec` feature provides `exec::Shell`, a ready-made callback that runs each
command through the platform shell:

```rust,no_run
use adotenv::{exec::Shell, parse_strict, ExpandOptions};

let doc = parse_strict("REVISION=$(git rev-parse HEAD)\n").unwrap();
let shell = Shell::new("/srv/my-app");
let resolved = doc
    .resolve_with_commands(|_| None, shell.callback(), None, &ExpandOptions::dotenv_expand())
    .map_err(|failure| failure.error)?;
# let _ = resolved;
# Ok::<(), adotenv::ResolveError>(())
```

- Each command runs as `sh -c <command>`, or `cmd /C <command>` on Windows,
  with the command text as written.
- `Shell::new(directory)` runs every command in that directory. An invalid
  directory is an error when a command runs; the current directory of the
  process is never used instead.
- `Shell::with_origins(directory, origins)` maps source identities to
  directories, so commands from each file run next to it.
- The command inherits the environment of the current process and gets an
  empty standard input. The shell expands variables in the command from that
  environment, not from the file or the lookup closure.
- Standard output is returned with one trailing newline removed. A command
  that cannot start, exits with a nonzero status or prints output that is not
  UTF-8 is an error; for a nonzero status the message includes the command's
  standard error.

Commands run exactly as written. Only use `exec::Shell` with trusted files.
