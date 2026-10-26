fn main() {
    let path = std::env::args().nth(1).expect("fixture path");
    let text = std::fs::read_to_string(path).unwrap();
    let map = dotenv_compat::parse(text.as_bytes());
    println!("{}", map.len());
}
