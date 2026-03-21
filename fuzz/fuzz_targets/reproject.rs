#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};
use toml_spanner::{ArrayStyle, Item, Table, TableStyle, Arena};

fuzz_target!(|data: &[u8]| -> Corpus {
    let Ok(text) = std::str::from_utf8(data) else {
        return Corpus::Reject;
    };

    // Parse once as source (owns the table index).
    let arena_src = toml_spanner::Arena::new();
    let Ok(src_root) = toml_spanner::parse(text, &arena_src) else {
        return Corpus::Keep;
    };

    // Emit the original as the reference output.
    let ref_buf = {
        let arena = Arena::new();
        let Ok(r) = toml_spanner::parse(text, &arena) else {
            return Corpus::Keep;
        };
        let t = r.into_table();
        toml_spanner::Formatting::default().format_table_to_bytes(t, &arena)
    };

    // Parse again as dest and erase all structural kinds.
    let arena_dest = Arena::new();
    let Ok(dest_root) = toml_spanner::parse(text, &arena_dest) else {
        return Corpus::Keep;
    };
    let mut dest_table = dest_root.into_table();
    erase_kinds_table(&mut dest_table);

    // Reproject from src onto the erased dest, normalize, and emit.
    let buf = toml_spanner::Formatting::of(&src_root)
        .format_table_to_bytes(dest_table, &arena_dest);

    // Core invariant: reprojected output must match the reference.
    assert!(
        ref_buf == buf,
        "reproject did not recover original structure!\ninput:\n{text}\nreference:\n{}\nreprojected:\n{}",
        String::from_utf8_lossy(&ref_buf),
        String::from_utf8_lossy(&buf),
    );

    Corpus::Keep
});

fn erase_kinds_table(table: &mut Table<'_>) {
    for (_, item) in table {
        erase_kinds_item(item);
    }
}

fn erase_kinds_item(item: &mut Item<'_>) {
    if let Some(t) = item.as_table_mut() {
        match t.style() {
            TableStyle::Dotted | TableStyle::Inline => {}
            _ => t.set_style(TableStyle::Implicit),
        }
        erase_kinds_table(t);
    } else if let Some(a) = item.as_array_mut() {
        a.set_style(ArrayStyle::Inline);
        for elem in a.as_mut_slice() {
            erase_kinds_item(elem);
        }
    }
}
