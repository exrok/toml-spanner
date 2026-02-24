use crate::emit::{self, EmitConfig, reproject};
use crate::parser::parse;
use crate::value::Item;
use crate::Arena;

pub fn main() {
    let text = "3=[]\r\n\r\n";

    let arena_src = Arena::new();
    let src_root = parse(text, &arena_src).unwrap();

    let arena_dest = Arena::new();
    let dest_root = parse(text, &arena_dest).unwrap();

    let mut items: Vec<&Item<'_>> = Vec::new();
    let mut dest_table = dest_root.into_table();
    reproject(&src_root, &mut dest_table, &mut items);

    let norm = dest_table.normalize();
    let config = EmitConfig {
        projected_source_text: text,
        projected_source_items: &items,
        reprojected_order: false,
    };
    let mut buf = Vec::new();
    emit::emit_with_config(norm, &config, &mut buf);
    let output = String::from_utf8(buf).unwrap();
    eprintln!("output: {:?}", output);
    eprintln!("output bytes: {:?}", output.as_bytes());

    let arena_out = Arena::new();
    match parse(&output, &arena_out) {
        Ok(_) => eprintln!("parse OK"),
        Err(e) => eprintln!("parse FAILED: {:?}", e),
    }
}

pub fn make_boolean(b: bool) -> Item<'static> {
    Item::from(b)
}

pub fn make_float(f: f64) -> Item<'static> {
    Item::from(f)
}
