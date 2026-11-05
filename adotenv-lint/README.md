# adotenv-lint

Optional lint rules and verified fixes for [`adotenv`](https://crates.io/crates/adotenv)
documents.

The linter is a companion crate: lint policy does not belong in the parser
core. Findings are structured and never contain values. A finding may carry a
fix that is applied through `adotenv`'s verified edit or format APIs.

```rust
use adotenv_lint::{lint, LintOptions};

let doc = adotenv::parse_strict("B=2\nA=1\n")?;
let findings = lint(&doc, LintOptions::default().with_key_order(true));
assert_eq!(findings[0].rule(), adotenv_lint::Rule::KeyOrder);

let fixed = adotenv_lint::apply_fix(&doc, findings[0].fix().unwrap())?;
assert_eq!(fixed.as_str(), "A=1\nB=2\n");
# Ok::<(), Box<dyn std::error::Error>>(())
```

## CLI

```console
adotenv-lint [options] <file-or-directory>...
```

Examples:

```console
adotenv-lint --key-order --key-case upper .env
adotenv-lint --format=json --recursive .
adotenv-lint --format=github --key-order .
```

Without `--fix` the CLI is report-only. With `--fix` it applies verified
library fixes and rewrites changed files through a temporary file in the same
directory. It discovers `.env` and `.env.*` files with `--recursive`, skips
`.git`, `target` and `node_modules`, and exits with `0` for no findings (or all
findings fixed), `1` for remaining findings and `2` for usage, read, parse or
write errors. The Windows replace fallback is best-effort and not
crash-atomic, matching the CLI write contract.

## Rules

The default policy reports duplicate keys only. Enable the other rules
explicitly:

| Rule | Default | Rationale |
|---|---|---|
| `duplicate-key` | on | Repeated declarations are usually accidental and make lookup depend on dialect policy. |
| `key-order` | off | Sorting is a style choice; some files group related keys deliberately. |
| `key-case` | off | Upper- and lowercase keys are both common in the wild. |
| `quote-style` | preserve | Quote policy depends on the project and on values that contain references. |
| `whitespace` | preserve | Whitespace policy is project-specific; formatting is opt-in. |

Findings include the rule, warning severity, one-based line, source span, key
and an optional verified fix. Parse diagnostics remain available from
`adotenv::parse`.

This crate requires Rust 1.81 and does not change the core crate's `no_std` or
dependency-free guarantees.
