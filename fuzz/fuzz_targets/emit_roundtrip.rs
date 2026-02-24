#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};

// Fuzzes exact text preservation through erase-reproject-normalize-emit.
//
// Uses a structured generator that produces complex TOML sequences designed
// to trigger edge cases while avoiding known non-preservable formatting
// (dotted key implicit segments, header path keys without real key spans).
//
// The generated TOML exercises: varied whitespace around `=`, nasty comments,
// multi-line inline arrays/tables, diverse scalar formats (literal strings,
// hex/octal/binary integers, floats, datetimes), trailing commas, quoted keys,
// headers, AOTs, dotted keys, and deep nesting.
fuzz_target!(|data: &[u8]| -> Corpus {
    let mut buffer = String::new();
    fuzz::gen_toml::random_roundtrip_toml(&mut buffer, data);
    let text = &buffer;

    let arena = toml_spanner::Arena::new();
    let Ok(root) = toml_spanner::parse(text, &arena) else {
        return Corpus::Keep;
    };

    if root.table().try_as_normalized().is_none() {
        return Corpus::Keep;
    }

    let mut dest = root.table().clone_in(&arena);
    if dest.is_empty() {
        return Corpus::Keep;
    }
    fuzz::gen_tree::erase_kinds_table(&mut dest);

    let mut items = Vec::new();
    toml_spanner::reproject(&root, &mut dest, &mut items);

    let norm = dest.normalize();
    let config = toml_spanner::EmitConfig {
        projected_source_text: text,
        projected_source_items: &items,
        reprojected_order: false,
    };
    let mut out_buf = Vec::new();
    toml_spanner::emit_with_config(norm, &config, &mut out_buf);

    let input = text.as_bytes().trim_ascii();
    // Exact text match — input must be preserved byte-for-byte.
    assert!(
        input == out_buf.trim_ascii(),
        "roundtrip did not preserve input text!\ninput:\n{text}\noutput:\n{}",
        String::from_utf8_lossy(&out_buf),
    );

    Corpus::Keep
});
