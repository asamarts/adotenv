fn main() {
    let path = std::env::args().nth(1).expect("fixture path");
    let text = std::fs::read_to_string(path).unwrap();
    let map = dotenv_ng_core::EnvLoader::with_reader(std::io::Cursor::new(text.as_bytes()))
        .sequence(dotenv_ng_core::EnvSequence::InputOnly)
        .load()
        .unwrap();
    println!("{}", map.len());
}
