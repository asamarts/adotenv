fn main() {
    let path = std::env::args().nth(1).expect("fixture path");
    let text = std::fs::read_to_string(path).unwrap();
    let map: std::collections::BTreeMap<_, _> =
        dotenvy::from_read_iter(std::io::Cursor::new(text.as_bytes()))
            .map(|item| item.unwrap())
            .collect();
    println!("{}", map.len());
}
