fn main() {
    let path = std::env::args().nth(1).expect("fixture path");
    let text = std::fs::read_to_string(path).unwrap();
    let doc = adotenv::parse_strict(&text).unwrap();
    let map: std::collections::BTreeMap<_, _> = doc
        .effective_entries()
        .map(|entry| (entry.key().to_string(), entry.value().to_string()))
        .collect();
    println!("{}", map.len());
}
