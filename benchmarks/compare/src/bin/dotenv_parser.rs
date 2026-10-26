fn main() {
    let path = std::env::args().nth(1).expect("fixture path");
    let text = std::fs::read_to_string(path).unwrap();
    let map = dotenv_parser::parse_dotenv(&text).unwrap();
    println!("{}", map.len());
}
