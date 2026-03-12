#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};
use toml_spanner::{Item, Value};

fuzz_target!(|data: &[u8]| -> Corpus {
    let Ok(text) = std::str::from_utf8(data) else {
        return Corpus::Reject;
    };

    // Parse the raw input.
    let arena = toml_spanner::Arena::new();
    let Ok(root) = toml_spanner::parse(text, &arena) else {
        return Corpus::Keep;
    };

    // Emit.
    let Some(normalized) = root.table().try_as_normalized() else {
        return Corpus::Keep;
    };
    let mut buf = Vec::new();
    toml_spanner::emit(normalized, &mut buf);
    let emitted = std::str::from_utf8(&buf).expect("emit must produce valid UTF-8");

    // Parse the emitted output.
    let arena2 = toml_spanner::Arena::new();
    let root2 = toml_spanner::parse(emitted, &arena2)
        .expect("emitted output must parse as valid TOML");

    // Items must be semantically equal with matching flags.
    assert_items_equal_with_flags(
        root.table().as_item(),
        root2.table().as_item(),
        text,
        emitted,
    );

    // Idempotency: emitting the re-parsed output must produce identical bytes.
    let normalized2 = root2
        .table()
        .try_as_normalized()
        .expect("round-tripped table should be valid");
    let mut buf2 = Vec::new();
    toml_spanner::emit(normalized2, &mut buf2);
    assert!(
        buf == buf2,
        "emit is not idempotent!\ninput:\n{text}\nfirst emit:\n{emitted}\nsecond emit:\n{}",
        String::from_utf8_lossy(&buf2),
    );

    Corpus::Keep
});

fn assert_items_equal_with_flags(a: &Item<'_>, b: &Item<'_>, input: &str, emitted: &str) {
    items_eq(a, b, &mut Vec::new(), input, emitted);
}

fn items_eq(
    a: &Item<'_>,
    b: &Item<'_>,
    path: &mut Vec<String>,
    input: &str,
    emitted: &str,
) {
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
                items_eq(&arr_a.as_slice()[i], &arr_b.as_slice()[i], path, input, emitted);
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
