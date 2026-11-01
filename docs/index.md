<div class="hero">
<span class="hero-eyebrow">Rust library and command-line tool</span>
<h1>Read and edit <span class="hero-accent">.env</span> files exactly as written</h1>
<p class="hero-lead">adotenv is a literal, lossless <code>.env</code> parser and editing library. It keeps every byte of the source, reports precise locations and checks every edit before it returns the result.</p>
<div class="hero-actions">
<a class="button primary" href="guide/getting-started.html">Get started</a>
<a class="button" href="https://docs.rs/adotenv">API reference</a>
<a class="button" href="https://github.com/asamarts/adotenv">GitHub</a>
</div>
</div>

<div class="feature-grid">
<div class="feature"><h3>Literal values</h3><p><code>${HOME}</code>, <code>$USER</code> and <code>$(command)</code> stay text. Nothing is expanded or run unless you ask for it.</p></div>
<div class="feature"><h3>Lossless source</h3><p>Any UTF-8 input prints back byte for byte, including comments, invalid lines, a BOM and mixed line endings.</p></div>
<div class="feature"><h3>Precise locations</h3><p>Every token has a byte span. Positions convert to UTF-8, Unicode scalar or UTF-16 columns.</p></div>
<div class="feature"><h3>Verified edits</h3><p>Set, rename, insert, remove, format and sort. Each result is parsed again and checked before you get it.</p></div>
<div class="feature"><h3>Explicit dialects</h3><p>Portable, Node dotenv, python-dotenv, Docker Compose and Docker presets, plus custom reading rules.</p></div>
<div class="feature"><h3>Small core</h3><p>No runtime dependencies, no I/O, no unsafe code and a <code>no_std</code> build with <code>alloc</code>.</p></div>
</div>

## Install

```toml
[dependencies]
adotenv = "1"
```

The core needs Rust 1.81. File loading, Serde support and command execution
are optional features; see [Getting started](guide/getting-started.md).

## Quick start

```rust
let text = "# app\nexport PORT=8080\nURL=\"${HOST}/api\"\n";
let doc = adotenv::parse_strict(text).unwrap();

// Values are literal, and the source prints back unchanged.
assert_eq!(doc.get("PORT"), Some("8080"));
assert_eq!(doc.get("URL"), Some("${HOST}/api"));
assert_eq!(doc.to_string(), text);

// Edits return the verified output and the text edits that produce it.
let edited = doc.set("PORT", "9090").unwrap();
assert_eq!(edited.as_str(), "# app\nexport PORT=9090\nURL=\"${HOST}/api\"\n");
```

When only the final values are needed, the values-only parser skips the
document model:

```rust
let values = adotenv::parse_values("A=1\nA=2\nB=3\n").unwrap();
assert_eq!(values.get("A").map(String::as_str), Some("2"));
```

## Crates

| Crate | Use it to | Reference |
|---|---|---|
| [`adotenv`](https://crates.io/crates/adotenv) | Parse, inspect, edit, format, sort, compare and resolve `.env` files. Optional features load files, deserialize with Serde and run commands. | [API](https://docs.rs/adotenv) |
| [`adotenv-cli`](https://crates.io/crates/adotenv-cli) | Check, format, compare and load files from a shell or a CI job. | [Commands](design/cli.md) |
| [`adotenv-process`](https://crates.io/crates/adotenv-process) | Apply a loaded environment to the current process. | [API](https://docs.rs/adotenv-process) |
| [`adotenv-config`](https://crates.io/crates/adotenv-config) | Use values as a `config` source. | [API](https://docs.rs/adotenv-config) |
| [`adotenv-figment`](https://crates.io/crates/adotenv-figment) | Use values as a Figment provider. | [API](https://docs.rs/adotenv-figment) |

## Explore

<div class="card-grid">
<a class="card" href="guide/parsing.html"><span class="card-title">Parsing and lookup</span><span class="card-text">Documents, diagnostics, duplicate keys, spans and positions.</span></a>
<a class="card" href="design/dialects.html"><span class="card-title">Dialects</span><span class="card-text">The five presets and how to build custom reading rules.</span></a>
<a class="card" href="guide/editing.html"><span class="card-title">Editing</span><span class="card-text">Verified edits, batches and encoding values for a dialect.</span></a>
<a class="card" href="design/expansion.html"><span class="card-title">References and resolution</span><span class="card-text">Find <code>$VAR</code> references and resolve them on request.</span></a>
<a class="card" href="design/loading.html"><span class="card-title">Loading files</span><span class="card-text">Cascades, origins, reports and values-only loading.</span></a>
<a class="card" href="design/cli.html"><span class="card-title">Command-line tool</span><span class="card-text">Commands, JSON schemas, GitHub annotations and exit codes.</span></a>
</div>
