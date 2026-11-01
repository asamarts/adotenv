# References and resolution

Parsing keeps `$VAR`, `${VAR:-default}` and `$(command)` as literal text.
Finding those references and resolving them are separate, explicit steps.

## Reference inspection

`Entry::references()` returns the raw candidates in a value, in source order:

- `$VAR` and `${VAR}`;
- `${VAR:-default}`, `${VAR-default}`, `${VAR:?message}`, `${VAR?message}`,
  `${VAR:+alternative}` and `${VAR+alternative}`;
- `$(command)`, including nested payloads.

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

Single-quoted, escaped-dollar and malformed candidates remain visible. Being
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
| `escaped_dollar` | References after `\$` are eligible |
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
file's own bindings and a caller lookup closure. The core never reads the
environment or files on its own.

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

Sequential scope uses the latest earlier binding. Whole-file scope uses the
final binding. With no caller value, `A=base` followed by `A=${A}x` gives
`basex` in sequential scope and a cycle in whole-file scope.

With `FileFirst`, a declaration in the file wins over the lookup closure, even
a declaration without a value. With `CallerFirst`, the closure wins when it
returns a value.

### Results

`resolve` returns a `ResolvedDocument` with `entries()`, `commands()`, `len()`
and `is_empty()`. Each `ResolvedEntry` has the `entry` ID, `key`, assignment
`state`, resolved `value`, `effective_value` after lookup policy, and
`segments`. The segments concatenate to the value and record where each part
came from: literal text, a variable from the file (entry, key and span) or from
the lookup, an unresolved reference, or a command.

### Errors and limits

`ResolveError` reports `Unresolved` (with `Unresolved::Error`), `Required` (a
`?` or `:?` operator without a value), `Cycle`, `DepthExceeded`,
`ValueTooLarge`, `OutputTooLarge` and `Command`. Resolution never loops on a
cycle.

Nested provenance copies the referenced value's segments, so references that
repeat each other can make the output grow much faster than the file.
`max_output_bytes` bounds the whole output of one resolution: every resolved
value and every segment counts, nested parts included, with a fixed overhead
per segment. The budget is checked before each copy is made. The presets allow
64 MiB, several times what a 65,536-entry file needs. Exceeding it returns
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
not expanded again. Every call is recorded as a `CommandCall` in `commands()`,
including failed calls. On failure, `resolve_with_commands` returns a
`ResolveFailure` with the `error` and the `commands` run so far. Earlier calls
are not rolled back.

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

- `Shell::new(directory)` runs every command in that directory. An invalid
  directory is an error when a command runs; the host working directory is
  never used as a fallback.
- `Shell::with_origins(directory, origins)` maps source identities to
  directories, so commands from each file run next to it.
- Standard output is returned with one trailing newline removed. A nonzero exit
  status or output that is not UTF-8 is an error.
- The command inherits the environment of the current process.

Commands run exactly as written. Only use `exec::Shell` with trusted files.
