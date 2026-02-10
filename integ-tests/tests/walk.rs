use std::fs;

#[test]
fn parse_all_data_files() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/data");
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "toml") {
            let src = fs::read_to_string(&path).unwrap();
            // Discard the result; invalid files are expected.
            let _ = toml_spanner::parse(&src);
        }
    }
}
