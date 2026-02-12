#![no_main]

use libfuzzer_sys::fuzz_target;
use toml_spanner::value::ValueRef;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    let arena = toml_spanner::Arena::new();
    let spanner_result = toml_spanner::parse(text, &arena);
    // Parse as a Table (full TOML document), not Value, since Value::from_str
    // can interpret top-level constructs like [[header]] as inline arrays.
    // The toml crate has internal panics on some inputs (e.g. assertion
    // failures in its number decoder). We must temporarily replace the
    // libfuzzer panic hook (which calls abort) with a no-op so that
    // catch_unwind can actually catch the panic.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let toml_result = std::panic::catch_unwind(|| text.parse::<toml::Table>());
    std::panic::set_hook(prev_hook);
    let toml_result = match toml_result {
        Ok(r) => r,
        Err(_) => return, // toml crate panicked — skip this input
    };

    match (spanner_result, toml_result) {
        (Ok(spanner_val), Ok(toml_tbl)) => {
            let toml_val = toml::Value::Table(toml_tbl);
            // Both parsed successfully — compare values, skipping datetime differences.
            assert!(
                values_match(&spanner_val, &toml_val),
                "values differ for input:\n{text}\nspanner: {spanner_val:?}\ntoml: {toml_val:?}"
            );
        }
        (Ok(_), Err(_)) => {
            // toml-spanner accepted but toml crate rejected.
            // This is acceptable — toml-spanner may be more permissive on some
            // edge cases, or the toml crate rejects things toml-spanner allows.
        }
        (Err(_), Ok(_)) => {
            // toml crate accepted but toml-spanner rejected. This can happen
            // for datetimes (which toml-spanner deliberately omits) or for
            // edge cases where the parsers disagree on validity.
        }
        (Err(_), Err(_)) => {
            // Both rejected — nothing to check.
        }
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

/// Recursively compare a `toml_spanner::Value` against a `toml::Value`.
///
/// Returns `false` if the values differ in a way that is not explained by
/// datetime support differences. Tables/arrays that contain datetimes are
/// skipped (returns `true`).
fn values_match(spanner: &toml_spanner::Value<'_>, toml_val: &toml::Value) -> bool {
    match (spanner.kind(), toml_val) {
        (ValueRef::String(s), toml::Value::String(t)) => &**s == t,
        (ValueRef::Integer(a), toml::Value::Integer(b)) => a == *b,
        (ValueRef::Float(a), toml::Value::Float(b)) => {
            // Handle NaN: both NaN is considered a match.
            (a.is_nan() && b.is_nan()) || a == *b
        }
        (ValueRef::Boolean(a), toml::Value::Boolean(b)) => a == *b,
        (ValueRef::Array(sa), toml::Value::Array(ta)) => {
            // Filter out datetime entries from the toml side. If the toml array
            // has datetimes that toml-spanner cannot represent, skip those
            // entries during comparison.
            let ta_no_dt: Vec<_> = ta.iter().filter(|v| !contains_datetime(v)).collect();
            if sa.len() != ta_no_dt.len() {
                // Length mismatch when datetimes are involved — skip.
                if ta.len() != ta_no_dt.len() {
                    return true;
                }
                return false;
            }
            sa.as_slice()
                .iter()
                .zip(ta_no_dt.iter())
                .all(|(s, t)| values_match(s, t))
        }
        (ValueRef::Table(st), toml::Value::Table(tt)) => {
            // Filter out datetime entries from the toml side.
            let tt_no_dt: Vec<_> = tt
                .iter()
                .filter(|(_, v)| !contains_datetime(v))
                .collect();
            if st.len() != tt_no_dt.len() {
                if tt.len() != tt_no_dt.len() {
                    return true;
                }
                return false;
            }
            // With preserve_order both should iterate in insertion order.
            st.into_iter()
                .zip(tt_no_dt.iter())
                .all(|((sk, sv), (tk, tv))| &*sk.name == *tk && values_match(sv, tv))
        }
        // toml-spanner has no datetime variant. If the toml side is a datetime,
        // the mismatch is expected — skip.
        (_, toml::Value::Datetime(_)) => true,
        // Any other kind mismatch is a real bug.
        _ => false,
    }
}
