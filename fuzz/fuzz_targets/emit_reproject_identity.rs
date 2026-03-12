#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};
use toml_spanner::{Item, Table, Value};

fn clear_flags_table(table: &mut Table<'_>) {
    match table.style() {
        toml_spanner::TableStyle::Dotted | toml_spanner::TableStyle::Inline => {}
        _ => table.set_style(toml_spanner::TableStyle::Implicit),
    }
    for (_, item) in table {
        clear_flags(item);
    }
}

fn clear_flags(item: &mut Item<'_>) {
    match item.value_mut() {
        toml_spanner::ValueMut::Array(array) => {
            array.set_style(toml_spanner::ArrayStyle::Inline);
            for item in array {
                clear_flags(item);
            }
        }
        toml_spanner::ValueMut::Table(table) => clear_flags_table(table),
        _ => (),
    }
}

// Fuzzes the `emit_with_config` + reprojection identity round-trip.
//
// For any valid TOML input, parsing as both source and dest, reprojecting,
// normalizing, and emitting with the reprojection config must:
//   1. Produce valid UTF-8
//   2. Parse as valid TOML
//   3. Be semantically equal to the input (same values and flags)
//   4. Be idempotent (re-emitting produces identical bytes)
fuzz_target!(|data: &[u8]| -> Corpus {
    let Ok(text) = std::str::from_utf8(data) else {
        return Corpus::Reject;
    };

    // Parse as source (holds the table index for reprojection).
    let arena = toml_spanner::Arena::new();
    let Ok(src_root) = toml_spanner::parse(text, &arena) else {
        return Corpus::Keep;
    };

    let mut dest = src_root.table().clone_in(&arena);
    clear_flags_table(&mut dest);
    // Reproject source structure onto dest.
    let mut items = Vec::new();
    toml_spanner::reproject(&src_root, &mut dest, &mut items);

    // Normalize and emit with reprojection config.
    let norm = dest.normalize();
    let config = toml_spanner::EmitConfig {
        projected_source_text: text,
        projected_source_items: &items,
        reprojected_order: false,
    };
    let mut buf = Vec::new();
    toml_spanner::emit_with_config(norm, &config, &mut buf);

    // Invariant 1: valid UTF-8.
    let output = std::str::from_utf8(&buf).expect("emit_with_config must produce valid UTF-8");

    // Invariant 2: parses as valid TOML.
    let out_root = toml_spanner::parse(output, &arena).unwrap_or_else(|e| {
        panic!("emit output must be valid TOML!\ninput:\n{text}\noutput:\n{output}\nerror: {e:?}")
    });

    // Invariant 3: semantically equal with matching flags.
    assert_items_equal_with_flags(
        src_root.table().as_item(),
        out_root.table().as_item(),
        text,
        output,
    );

    // Invariant 4: idempotent — re-emit through the same pipeline.
    {
        let src2 = toml_spanner::parse(output, &arena).unwrap();
        let mut dest2 = toml_spanner::parse(output, &arena).unwrap().into_table();
        let mut items2 = Vec::new();
        toml_spanner::reproject(&src2, &mut dest2, &mut items2);
        let norm2 = dest2.normalize();
        let cfg2 = toml_spanner::EmitConfig {
            projected_source_text: output,
            projected_source_items: &items2,
            reprojected_order: false,
        };
        let mut buf2 = Vec::new();
        toml_spanner::emit_with_config(norm2, &cfg2, &mut buf2);
        assert!(
            buf == buf2,
            "emit_with_config is not idempotent!\ninput:\n{text}\nfirst:\n{output}\nsecond:\n{}",
            String::from_utf8_lossy(&buf2),
        );
    }

    Corpus::Keep
});

fn assert_items_equal_with_flags(a: &Item<'_>, b: &Item<'_>, input: &str, emitted: &str) {
    items_eq(a, b, &mut Vec::new(), input, emitted);
}

fn items_eq(a: &Item<'_>, b: &Item<'_>, path: &mut Vec<String>, input: &str, emitted: &str) {
    let p = || {
        if path.is_empty() {
            "<root>".to_string()
        } else {
            path.join(".")
        }
    };

    assert!(
        a.kind() as u8 == b.kind() as u8,
        "kind mismatch at {}\ninput:\n{input}\nemitted:\n{emitted}",
        p(),
    );

    assert!(
        a.flag() == b.flag(),
        "flag mismatch at {}: {} vs {}\ninput:\n{input}\nemitted:\n{emitted}",
        p(),
        a.flag(),
        b.flag(),
    );

    match a.value() {
        Value::String(s) => assert!(
            b.as_str() == Some(*s),
            "string mismatch at {}\ninput:\n{input}\nemitted:\n{emitted}",
            p(),
        ),
        Value::Integer(i) => assert!(
            b.as_i64() == Some(*i),
            "integer mismatch at {}\ninput:\n{input}\nemitted:\n{emitted}",
            p(),
        ),
        Value::Float(f) => {
            let bf = b.as_f64().unwrap();
            assert!(
                f.to_bits() == bf.to_bits(),
                "float mismatch at {}\ninput:\n{input}\nemitted:\n{emitted}",
                p(),
            );
        }
        Value::Boolean(v) => assert!(
            b.as_bool() == Some(*v),
            "boolean mismatch at {}\ninput:\n{input}\nemitted:\n{emitted}",
            p(),
        ),
        Value::DateTime(dt_a) => {
            let dt_b = b.as_datetime().unwrap();
            assert!(
                dt_a == dt_b,
                "datetime mismatch at {}\ninput:\n{input}\nemitted:\n{emitted}",
                p(),
            );
        }
        Value::Array(arr_a) => {
            let arr_b = b.as_array().unwrap();
            assert!(
                arr_a.len() == arr_b.len(),
                "array length mismatch at {}\ninput:\n{input}\nemitted:\n{emitted}",
                p(),
            );
            for i in 0..arr_a.len() {
                path.push(format!("[{i}]"));
                items_eq(
                    &arr_a.as_slice()[i],
                    &arr_b.as_slice()[i],
                    path,
                    input,
                    emitted,
                );
                path.pop();
            }
        }
        Value::Table(tab_a) => {
            let tab_b = b.as_table().unwrap();
            assert!(
                tab_a.len() == tab_b.len(),
                "table length mismatch at {}\ninput:\n{input}\nemitted:\n{emitted}",
                p(),
            );
            for (key, val_a) in tab_a {
                path.push(key.name.to_string());
                let val_b = tab_b.get(key.name).unwrap_or_else(|| {
                    panic!(
                        "key {} missing in emitted output\ninput:\n{input}\nemitted:\n{emitted}",
                        path.join("."),
                    );
                });
                items_eq(val_a, val_b, path, input, emitted);
                path.pop();
            }
        }
    }
}
