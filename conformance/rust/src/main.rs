use std::error::Error;
use std::io::{self, Read, Write};

use adotenv::{parse_with, Dialect, ExpandOptions, Severity};
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
    let mut resolved = Map::new();
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
    for case in cases {
        let id = case["id"].as_str().ok_or("case id must be a string")?;
        let source = case["source"]
            .as_str()
            .ok_or("case source must be a string")?;
        let environment = case["environment"]
            .as_object()
            .ok_or("case environment must be an object")?;
        let (document, _) = parse_with(source, Dialect::COMPOSE);
        let lookup = |key: &str| {
            environment
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let (values, error) = match document.resolve(lookup, &ExpandOptions::compose()) {
            Ok(result) => {
                let mut values = Map::new();
                for entry in result.entries() {
                    values.insert(entry.key.clone(), json!(entry.value));
                }
                (Value::Object(values), false)
            }
            Err(_) => (Value::Object(Map::new()), true),
        };
        resolved.insert(id.to_owned(), json!({"values": values, "error": error}));
    }
    output.insert("compose_resolve".to_owned(), Value::Object(resolved));
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, &Value::Object(output))?;
    stdout.write_all(b"\n")?;
    Ok(())
}
