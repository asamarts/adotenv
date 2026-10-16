use std::error::Error;
use std::io::{self, Read, Write};

use adotenv::{parse_with, Dialect, Severity};
use serde_json::{json, Map, Value};

fn main() -> Result<(), Box<dyn Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let input: Value = serde_json::from_str(&input)?;
    let cases = input["cases"].as_array().ok_or("cases must be an array")?;
    let presets = [
        ("portable", Dialect::PORTABLE),
        ("node_dotenv", Dialect::NODE_DOTENV),
        ("python_dotenv", Dialect::PYTHON_DOTENV),
        ("compose", Dialect::COMPOSE),
        ("docker_raw", Dialect::DOCKER_RAW),
    ];
    let mut output = Map::new();
    for (name, dialect) in presets {
        let mut results = Map::new();
        for case in cases {
            let id = case["id"].as_str().ok_or("case id must be a string")?;
            let source = case["source"]
                .as_str()
                .ok_or("case source must be a string")?;
            let (document, diagnostics) = parse_with(source, dialect);
            let error = diagnostics
                .iter()
                .any(|item| item.severity() == Severity::Error)
                || document.items().any(|item| {
                    item.diagnostic()
                        .is_some_and(|item| item.severity() == Severity::Error)
                });
            let mut values = Map::new();
            let mut lines = Vec::new();
            for entry in document.entries() {
                if !entry.participates_in_lookup() {
                    continue;
                }
                // Lookup, rather than insertion order, applies the selected duplicate
                // and bare-key policies. Bare Python bindings remain JSON null.
                values.insert(entry.key().to_owned(), json!(document.get(entry.key())));
                if name == "docker_raw" {
                    if let Some(value) = entry.value() {
                        lines.push(format!("{}={value}", entry.key()));
                    }
                }
            }
            if results
                .insert(
                    id.to_owned(),
                    json!({
                        "values": values, "lines": lines, "error": error,
                    }),
                )
                .is_some()
            {
                return Err(format!("duplicate case id: {id}").into());
            }
        }
        output.insert(name.to_owned(), Value::Object(results));
    }
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, &Value::Object(output))?;
    stdout.write_all(b"\n")?;
    Ok(())
}
