#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};
use toml_spanner::Value;

fuzz_target!(|data: &[u8]| -> Corpus {
    let Ok(text) = std::str::from_utf8(data) else {
        return Corpus::Keep;
    };

    let arena = toml_spanner::Arena::new();
    let spanner_result = toml_spanner::parse(text, &arena);

    if text.contains("$__toml_") {
        // toml uses magic keys starting with $__toml_ for internal purposes.
        return Corpus::Reject;
    }
    let toml_result = text.parse::<toml::Table>();

    match (spanner_result, toml_result) {
        (Ok(spanner_val), Ok(toml_tbl)) => {
            assert!(
                tables_match(&spanner_val.table(), &toml_tbl),
                "values differ for input:\n{text}\nspanner: {spanner_val:?}\ntoml: {toml_tbl:?}"
            );
        }
        (Ok(spanner_val), Err(toml_err)) => {
            if toml_err.message().contains("recursion limit") {
                return Corpus::Reject;
            }
            panic!(
                "toml-spanner accepted but toml rejected!\n\
                 input:\n{text}\n\
                 spanner: {spanner_val:?}\n\
                 toml error: {toml_err}"
            );
        }
        (Err(spanner_err), Ok(toml_tbl)) => {
            let toml_val = toml::Value::Table(toml_tbl);
            panic!(
                "toml accepted but toml-spanner rejected!\n\
                 input:\n{text}\n\
                 toml: {toml_val:?}\n\
                 spanner error: {spanner_err:?}"
            );
        }
        (Err(_), Err(_)) => {}
    }
    Corpus::Keep
});

fn tables_match(spanner: &toml_spanner::Table<'_>, toml_val: &toml::Table) -> bool {
    if spanner.len() != toml_val.len() {
        return false;
    }
    if spanner
        .into_iter()
        .zip(toml_val.iter())
        .all(|((sk, sv), (tk, tv))| &*sk.name == tk && values_match(sv, tv))
    {
        return true;
    }
    // Even though toml are supposed preserve order the two libraries differ in there
    // definition of order. In edge cases like:
    // [[U.U]]
    // [US]
    // [[U.U]]
    // [U]
    // toml-spanner will use order is `U, US` but toml will say `US, U`.
    for (key, value) in spanner {
        match toml_val.get(key.name) {
            Some(toml_value) => {
                if !values_match(value, toml_value) {
                    return false;
                }
            }
            None => return false,
        }
    }
    true
}

fn values_match(spanner: &toml_spanner::Item<'_>, toml_val: &toml::Value) -> bool {
    match (spanner.value(), toml_val) {
        (Value::String(s), toml::Value::String(t)) => s == t,
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
        (Value::Table(st), toml::Value::Table(tt)) => tables_match(st, tt),
        (Value::DateTime(sd), toml::Value::Datetime(td)) => datetimes_match(sd, td),
        _ => false,
    }
}

fn datetimes_match(spanner: &toml_spanner::DateTime, toml_dt: &toml::value::Datetime) -> bool {
    let dates_match = match (spanner.date(), &toml_dt.date) {
        (Some(sd), Some(td)) => sd.year == td.year && sd.month == td.month && sd.day == td.day,
        (None, None) => true,
        _ => false,
    };
    if !dates_match {
        return false;
    }

    let times_match = match (spanner.time(), &toml_dt.time) {
        (Some(st), Some(tt)) => {
            if st.hour != tt.hour || st.minute != tt.minute {
                return false;
            }
            match tt.second {
                Some(s) => {
                    if !st.has_seconds() || st.second != s {
                        return false;
                    }
                }
                None => {
                    if st.has_seconds() {
                        return false;
                    }
                }
            }
            match tt.nanosecond {
                Some(n) => {
                    if st.subsecond_precision() == 0 || st.nanosecond != n {
                        return false;
                    }
                }
                None => {
                    if st.subsecond_precision() != 0 {
                        return false;
                    }
                }
            }
            true
        }
        (None, None) => true,
        _ => false,
    };
    if !times_match {
        return false;
    }

    match (spanner.offset(), &toml_dt.offset) {
        (Some(toml_spanner::TimeOffset::Z), Some(toml::value::Offset::Z)) => true,
        (Some(toml_spanner::TimeOffset::Custom { minutes }), Some(toml::value::Offset::Z)) => {
            minutes == 0
        }
        (Some(toml_spanner::TimeOffset::Z), Some(toml::value::Offset::Custom { minutes })) => {
            *minutes == 0
        }
        (
            Some(toml_spanner::TimeOffset::Custom { minutes: sm }),
            Some(toml::value::Offset::Custom { minutes: tm }),
        ) => sm == *tm,
        (None, None) => true,
        _ => false,
    }
}
