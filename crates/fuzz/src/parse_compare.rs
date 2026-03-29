use toml_spanner::{Item, Value};

fn contains_large_number(item: &Item) -> bool {
    match item.value() {
        Value::Integer(i) => i.as_i64().is_none(),
        Value::Array(arr) => arr.iter().any(contains_large_number),
        Value::Table(tbl) => tbl.iter().map(|(_, v)| v).any(contains_large_number),
        _ => false,
    }
}

pub enum Outcome {
    Skip,
    Ok,
}

pub fn compare(text: &str) -> Outcome {
    let arena = toml_spanner::Arena::new();
    let spanner_result = toml_spanner::parse(text, &arena);

    if text.contains("$__toml_") {
        return Outcome::Skip;
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
                return Outcome::Skip;
            }
            if contains_large_number(spanner_val.table().as_item()) {
                return Outcome::Skip;
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
        (Err(err), Err(_)) => {
            let range = err.span().range();
            assert!(
                range.start <= range.end,
                "Error had start before end: {:#?}",
                err
            );
            if range.start == text.len() {
                assert!(
                    range.end == text.len(),
                    "Error had start at end of text but end index was different: text length: {}, {:#?}",
                    text.len(),
                    err
                );
            }
            assert!(
                range.start <= text.len(),
                "Error had start index out of bounds: text length: {}, {:#?}",
                text.len(),
                err
            );
            assert!(
                range.end <= text.len(),
                "Error had end index out of bounds: text length: {}, {:#?}",
                text.len(),
                err
            )
        }
    }
    Outcome::Ok
}

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
        (Value::Integer(a), toml::Value::Integer(b)) => a.as_i128() == *b as i128,
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

pub fn run_cli(path: &str) {
    let data = std::fs::read(path).expect("failed to read artifact");

    println!("artifact: {path}");
    println!("bytes ({len}): {data:?}", len = data.len());
    println!();

    let text = match std::str::from_utf8(&data) {
        Ok(s) => s,
        Err(e) => {
            println!("artifact is not valid UTF-8: {e}");
            println!("fuzzer would skip (from_utf8 fails)");
            return;
        }
    };

    println!("── input ({} bytes) ──\n{text:?}\n", text.len());

    match compare(text) {
        Outcome::Skip => println!("── skipped (rejected by filter) ──"),
        Outcome::Ok => println!("── OK ──"),
    }
}
