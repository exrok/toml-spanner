#![no_main]

use libfuzzer_sys::fuzz_target;
use toml_spanner::Value;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    let arena = toml_spanner::Arena::new();
    let spanner_result = toml_spanner::parse(text, &arena);
    let toml_result = text.parse::<toml::Table>();

    match (spanner_result, toml_result) {
        (Ok(spanner_val), Ok(toml_tbl)) => {
            // Both parsed successfully — values must match exactly.
            // Datetimes cannot appear here since toml-spanner would have
            // rejected the input if it contained any.

            assert!(
                tables_match(&spanner_val, &toml_tbl),
                "values differ for input:\n{text}\nspanner: {spanner_val:?}\ntoml: {toml_tbl:?}"
            );
        }
        (Ok(spanner_val), Err(toml_err)) => {
            // toml-spanner must never accept input that the toml crate rejects.
            panic!(
                "toml-spanner accepted but toml rejected!\n\
                 input:\n{text}\n\
                 spanner: {spanner_val:?}\n\
                 toml error: {toml_err}"
            );
        }
        (Err(spanner_err), Ok(toml_tbl)) => {
            // toml accepted but toml-spanner rejected. Only acceptable if the
            // parsed value contains datetimes (unsupported by toml-spanner).
            //
            // Note: the other known difference (integer range) cannot manifest
            // here because toml::Value::Integer is i64, so if the toml crate
            // parsed successfully the integer already fits in i64.
            let toml_val = toml::Value::Table(toml_tbl);
            assert!(
                contains_datetime(&toml_val),
                "toml accepted but toml-spanner rejected unexpectedly!\n\
                 input:\n{text}\n\
                 toml: {toml_val:?}\n\
                 spanner error: {spanner_err:?}"
            );
        }
        (Err(_), Err(_)) => {}
    }
});

/// Returns true if a `toml::Value` tree contains any `Datetime` variant.
fn contains_datetime(val: &toml::Value) -> bool {
    match val {
        toml::Value::Datetime(_) => true,
        toml::Value::Array(arr) => arr.iter().any(contains_datetime),
        toml::Value::Table(tbl) => tbl.values().any(contains_datetime),
        _ => false,
    }
}

/// Strict recursive comparison between a `toml_spanner::Value` and a
/// `toml::Value`. Since both parsers succeeded, no datetimes can be present
/// (toml-spanner would have rejected the input). Any mismatch is a real bug.
fn tables_match(spanner: &toml_spanner::Table<'_>, toml_val: &toml::Table) -> bool {
    spanner.len() == toml_val.len()
        && spanner
            .into_iter()
            .zip(toml_val.iter())
            .all(|((sk, sv), (tk, tv))| &*sk.name == tk && values_match(sv, tv))
}

/// Strict recursive comparison between a `toml_spanner::Value` and a
/// `toml::Value`. Since both parsers succeeded, no datetimes can be present
/// (toml-spanner would have rejected the input). Any mismatch is a real bug.
fn values_match(spanner: &toml_spanner::Item<'_>, toml_val: &toml::Value) -> bool {
    match (spanner.value(), toml_val) {
        (Value::String(s), toml::Value::String(t)) => s.as_str() == t,
        (Value::Integer(a), toml::Value::Integer(b)) => a == b,
        (Value::Float(a), toml::Value::Float(b)) => (a.is_nan() && b.is_nan()) || a == b,
        (Value::Boolean(a), toml::Value::Boolean(b)) => a == b,
        (Value::Array(sa), toml::Value::Array(ta)) => {
            sa.len() == ta.len()
                && sa
                    .as_slice()
                    .iter()
                    .zip(ta.iter())
                    .all(|(s, t)| values_match(s, t))
        }
        (Value::Table(st), toml::Value::Table(tt)) => {
            st.len() == tt.len()
                && st
                    .into_iter()
                    .zip(tt.iter())
                    .all(|((sk, sv), (tk, tv))| &*sk.name == tk && values_match(sv, tv))
        }
        _ => false,
    }
}
