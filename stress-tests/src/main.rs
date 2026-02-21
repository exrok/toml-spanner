use toml_spanner::{Arena, Item, Table};

#[derive(Clone, Debug)]
enum Expected {
    Integer(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Array(Vec<Expected>),
    Table(Vec<(String, Expected)>),
}

fn verify_table(actual: &Table<'_>, expected: &[(String, Expected)], path: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{path}: table length mismatch: actual={}, expected={}",
        actual.len(),
        expected.len(),
    );
    for (key, exp_val) in expected {
        let child = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        let item = actual
            .get(key)
            .unwrap_or_else(|| panic!("{child}: missing key"));
        verify_item(item, exp_val, &child);
    }
}

fn verify_item(actual: &Item<'_>, expected: &Expected, path: &str) {
    match expected {
        Expected::Integer(exp) => {
            let v = actual
                .as_i64()
                .unwrap_or_else(|| panic!("{path}: expected integer, got {}", actual.type_str()));
            assert_eq!(v, *exp, "{path}: integer mismatch");
        }
        Expected::Float(exp) => {
            let v = actual
                .as_f64()
                .unwrap_or_else(|| panic!("{path}: expected float, got {}", actual.type_str()));
            if exp.is_nan() {
                assert!(v.is_nan(), "{path}: expected NaN, got {v}");
            } else {
                assert_eq!(v, *exp, "{path}: float mismatch");
            }
        }
        Expected::Bool(exp) => {
            let v = actual
                .as_bool()
                .unwrap_or_else(|| panic!("{path}: expected bool, got {}", actual.type_str()));
            assert_eq!(v, *exp, "{path}: bool mismatch");
        }
        Expected::Str(exp) => {
            let v = actual
                .as_str()
                .unwrap_or_else(|| panic!("{path}: expected string, got {}", actual.type_str()));
            assert_eq!(v, exp.as_str(), "{path}: string mismatch");
        }
        Expected::Array(exp_items) => {
            let arr = actual
                .as_array()
                .unwrap_or_else(|| panic!("{path}: expected array, got {}", actual.type_str()));
            assert_eq!(
                arr.len(),
                exp_items.len(),
                "{path}: array length mismatch: actual={}, expected={}",
                arr.len(),
                exp_items.len(),
            );
            for (i, exp_item) in exp_items.iter().enumerate() {
                let child = format!("{path}[{i}]");
                verify_item(arr.get(i).unwrap(), exp_item, &child);
            }
        }
        Expected::Table(exp_entries) => {
            let tbl = actual
                .as_table()
                .unwrap_or_else(|| panic!("{path}: expected table, got {}", actual.type_str()));
            verify_table(tbl, exp_entries, path);
        }
    }
}

fn escape_basic(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            '\u{1B}' => out.push_str("\\e"),
            c if c.is_control() => {
                let n = c as u32;
                if n <= 0xFF {
                    write_fmt(&mut out, format_args!("\\x{n:02X}"));
                } else if n <= 0xFFFF {
                    write_fmt(&mut out, format_args!("\\u{n:04X}"));
                } else {
                    write_fmt(&mut out, format_args!("\\U{n:08X}"));
                }
            }
            c => out.push(c),
        }
    }
    out
}

fn write_fmt(s: &mut String, args: std::fmt::Arguments<'_>) {
    use std::fmt::Write;
    s.write_fmt(args).unwrap();
}

fn gen_simple_string(rng: &mut fastrand::Rng) -> String {
    let len = rng.usize(1..=25);
    let pool = b"abcdefghijklmnopqrstuvwxyz0123456789";
    (0..len)
        .map(|_| pool[rng.usize(0..pool.len())] as char)
        .collect()
}

// 20 distinct base prefixes — each index maps to a unique base, guaranteeing
// no collisions within a single table scope.
const KEY_BASES: &[&str] = &[
    "name", "cfg", "path", "src", "opt", "val", "item", "data", "meta", "spec", "port", "host",
    "mode", "tag", "kind", "level", "scope", "fmt", "key", "addr",
];

/// Generate a key with random style: bare, quoted, escaped, or unicode.
/// The `idx` parameter guarantees uniqueness within a table scope (each idx
/// produces a distinct base string regardless of the random style chosen).
fn gen_key(rng: &mut fastrand::Rng, idx: usize) -> (String, String) {
    let prefix = KEY_BASES[idx % KEY_BASES.len()];
    let base = if idx < KEY_BASES.len() {
        prefix.to_string()
    } else {
        format!("{prefix}{}", idx / KEY_BASES.len())
    };

    match rng.u8(0..10) {
        0 => {
            // Bare key
            (base.clone(), base)
        }
        1 => {
            // Bare key with underscore
            let k = format!("{base}_z");
            (k.clone(), k)
        }
        2 => {
            // Bare key with dash
            let k = format!("{base}-q");
            (k.clone(), k)
        }
        3 => {
            // Quoted but bare-safe
            (format!("\"{base}\""), base)
        }
        4 => {
            // Quoted with \t
            let k = format!("{base}\tx");
            let t = format!("\"{base}\\tx\"");
            (t, k)
        }
        5 => {
            // Quoted with \n
            let k = format!("{base}\nw");
            let t = format!("\"{base}\\nw\"");
            (t, k)
        }
        6 => {
            // Quoted with \e
            let k = format!("{base}\u{1B}e");
            let t = format!("\"{base}\\ee\"");
            (t, k)
        }
        7 => {
            // Quoted with literal unicode
            let chars = ['\u{00E9}', '\u{00FC}', '\u{03B1}', '\u{03B2}', '\u{4E16}'];
            let ch = chars[rng.usize(0..chars.len())];
            let k = format!("{base}{ch}");
            (format!("\"{k}\""), k)
        }
        8 => {
            // Quoted with \u escape
            let pairs = [
                ('\u{00E9}', "\\u00E9"),
                ('\u{00FC}', "\\u00FC"),
                ('\u{03B1}', "\\u03B1"),
                ('\u{03B2}', "\\u03B2"),
                ('\u{4E16}', "\\u4E16"),
            ];
            let (ch, esc) = pairs[rng.usize(0..pairs.len())];
            let k = format!("{base}{ch}");
            let t = format!("\"{base}{esc}\"");
            (t, k)
        }
        _ => {
            // Quoted with \x hex escape
            let k = format!("{base}\u{07}b");
            let t = format!("\"{base}\\x07b\"");
            (t, k)
        }
    }
}

/// Re-encode an existing key string into TOML representation.
/// Keys with special characters are always quoted; bare-safe keys are
/// randomly bare or quoted for variety.
fn re_quote_key(rng: &mut fastrand::Rng, key_str: &str) -> String {
    let needs_quotes = key_str
        .chars()
        .any(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_');
    if needs_quotes || rng.u8(0..3) == 0 {
        format!("\"{}\"", escape_basic(key_str))
    } else {
        key_str.to_string()
    }
}

fn gen_scalar(rng: &mut fastrand::Rng) -> (String, Expected) {
    match rng.u8(0..10) {
        0 => {
            let v = rng.i64(i64::MIN / 2..=i64::MAX / 2);
            (v.to_string(), Expected::Integer(v))
        }
        1 => {
            let v = gen_float(rng);
            (format_float(v), Expected::Float(v))
        }
        2 => {
            let v = rng.bool();
            (v.to_string(), Expected::Bool(v))
        }
        3 => {
            // Plain basic string
            let s = gen_simple_string(rng);
            (format!("\"{s}\""), Expected::Str(s))
        }
        4 => {
            // Basic string with escape sequence
            let base = gen_simple_string(rng);
            let escapes = [
                ("\\n", "\n"),
                ("\\t", "\t"),
                ("\\\\", "\\"),
                ("\\\"", "\""),
                ("\\b", "\u{08}"),
                ("\\f", "\u{0C}"),
                ("\\e", "\u{1B}"),
                ("\\x07", "\u{07}"),
            ];
            let (esc_t, esc_s) = escapes[rng.usize(0..escapes.len())];
            let val_str = format!("{base}{esc_s}");
            let val_toml = format!("\"{}{esc_t}\"", escape_basic(&base));
            (val_toml, Expected::Str(val_str))
        }
        5 => {
            // Literal string
            let s = gen_simple_string(rng);
            (format!("'{s}'"), Expected::Str(s))
        }
        6 => {
            // Multiline basic string
            let s = gen_simple_string(rng);
            (format!("\"\"\"\n{s}\"\"\""), Expected::Str(s))
        }
        7 => {
            // Multiline literal string
            let s = gen_simple_string(rng);
            (format!("'''\n{s}'''"), Expected::Str(s))
        }
        8 => {
            // String with literal unicode
            let base = gen_simple_string(rng);
            let chars = ['\u{00E9}', '\u{00FC}', '\u{03B1}', '\u{4E16}', '\u{754C}'];
            let ch = chars[rng.usize(0..chars.len())];
            let s = format!("{base}{ch}");
            (format!("\"{s}\""), Expected::Str(s))
        }
        _ => {
            // String with \u unicode escape
            let base = gen_simple_string(rng);
            let pairs = [
                ('\u{00E9}', "\\u00E9"),
                ('\u{00FC}', "\\u00FC"),
                ('\u{03B1}', "\\u03B1"),
                ('\u{4E16}', "\\u4E16"),
            ];
            let (ch, esc) = pairs[rng.usize(0..pairs.len())];
            let s = format!("{base}{ch}");
            (format!("\"{base}{esc}\""), Expected::Str(s))
        }
    }
}

fn gen_float(rng: &mut fastrand::Rng) -> f64 {
    let int_part = rng.i32(-999..=999);
    let frac = rng.u32(0..=99999);
    let s = format!("{int_part}.{frac:05}");
    s.parse::<f64>().unwrap()
}

fn format_float(v: f64) -> String {
    let s = format!("{v}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
    }
}

/// Generate a value that may be a scalar, inline table, or inline array
/// depending on remaining `depth`. At depth 0, always returns a scalar.
fn gen_value(rng: &mut fastrand::Rng, depth: usize) -> (String, Expected) {
    if depth == 0 {
        return gen_scalar(rng);
    }
    match rng.u8(0..8) {
        0 => gen_inline_table_val(rng, depth - 1),
        1 => gen_inline_array_val(rng, depth - 1),
        _ => gen_scalar(rng),
    }
}

fn gen_inline_table_val(rng: &mut fastrand::Rng, depth: usize) -> (String, Expected) {
    let n = rng.usize(1..=6);
    let mut parts_toml = Vec::new();
    let mut parts_exp = Vec::new();
    for i in 0..n {
        let (k_toml, k_str) = gen_key(rng, i);
        let (v_toml, v_exp) = gen_value(rng, depth);
        parts_toml.push(format!("{k_toml} = {v_toml}"));
        parts_exp.push((k_str, v_exp));
    }
    (
        format!("{{{}}}", parts_toml.join(", ")),
        Expected::Table(parts_exp),
    )
}

fn gen_inline_array_val(rng: &mut fastrand::Rng, depth: usize) -> (String, Expected) {
    let n = rng.usize(0..=8);
    let mut parts_toml = Vec::new();
    let mut parts_exp = Vec::new();
    for _ in 0..n {
        let (v_toml, v_exp) = gen_value(rng, depth);
        parts_toml.push(v_toml);
        parts_exp.push(v_exp);
    }
    (
        format!("[{}]", parts_toml.join(", ")),
        Expected::Array(parts_exp),
    )
}

/// Generate a complete TOML document mixing root key-value pairs (scalars,
/// dotted keys, inline tables, inline arrays), table header sections
/// (simple and nested), array-of-tables (simple and nested with child
/// subtables), and the super-table-afterwards pattern. All keys use the
/// full variety of gen_key styles.
fn gen_document(rng: &mut fastrand::Rng) -> (String, Vec<(String, Expected)>) {
    let mut toml = String::new();
    let mut root_exp: Vec<(String, Expected)> = Vec::new();
    let mut root_kidx = 0usize;

    // Mix of plain scalars, dotted keys, inline tables, inline arrays
    let n_root = rng.usize(2..=10);
    for _ in 0..n_root {
        let (key_toml, key_str) = gen_key(rng, root_kidx);
        root_kidx += 1;

        match rng.u8(0..8) {
            0..=1 => {
                // Dotted key: key.sub = value
                let (sub_toml, sub_str) = gen_key(rng, 0);
                let (val_toml, val_exp) = gen_value(rng, 1);
                write_fmt(
                    &mut toml,
                    format_args!("{key_toml}.{sub_toml} = {val_toml}\n"),
                );
                root_exp.push((key_str, Expected::Table(vec![(sub_str, val_exp)])));
            }
            2 => {
                // Inline table
                let (val_toml, val_exp) = gen_inline_table_val(rng, 2);
                write_fmt(&mut toml, format_args!("{key_toml} = {val_toml}\n"));
                root_exp.push((key_str, val_exp));
            }
            3 => {
                // Inline array
                let (val_toml, val_exp) = gen_inline_array_val(rng, 1);
                write_fmt(&mut toml, format_args!("{key_toml} = {val_toml}\n"));
                root_exp.push((key_str, val_exp));
            }
            _ => {
                // Scalar
                let (val_toml, val_exp) = gen_scalar(rng);
                write_fmt(&mut toml, format_args!("{key_toml} = {val_toml}\n"));
                root_exp.push((key_str, val_exp));
            }
        }
    }

    let n_sections = rng.usize(0..=4);
    for _ in 0..n_sections {
        let depth = rng.usize(1..=3);
        // First path segment is unique at root level
        let mut path_keys: Vec<(String, String)> = Vec::new();
        for d in 0..depth {
            if d == 0 {
                path_keys.push(gen_key(rng, root_kidx));
                root_kidx += 1;
            } else {
                path_keys.push(gen_key(rng, d));
            }
        }
        let header: Vec<&str> = path_keys.iter().map(|(t, _)| t.as_str()).collect();
        write_fmt(&mut toml, format_args!("[{}]\n", header.join(".")));

        let mut inner_exp = Vec::new();
        let mut inner_kidx = 0;
        gen_section_kvs(rng, &mut toml, &mut inner_exp, &mut inner_kidx);

        // Build expected from leaf to root
        let mut current = Expected::Table(inner_exp);
        for d in (1..depth).rev() {
            current = Expected::Table(vec![(path_keys[d].1.clone(), current)]);
        }
        root_exp.push((path_keys[0].1.clone(), current));
    }

    let n_aots = rng.usize(0..=2);
    for _ in 0..n_aots {
        let depth = rng.usize(1..=3);
        let mut path_keys: Vec<(String, String)> = Vec::new();
        for d in 0..depth {
            if d == 0 {
                path_keys.push(gen_key(rng, root_kidx));
                root_kidx += 1;
            } else {
                path_keys.push(gen_key(rng, d));
            }
        }

        let n_entries = rng.usize(1..=8);
        let mut aot_entries = Vec::new();

        for _ in 0..n_entries {
            // Re-quote each segment randomly for variety on each [[header]]
            let header: Vec<String> = path_keys
                .iter()
                .map(|(_, s)| re_quote_key(rng, s))
                .collect();
            write_fmt(&mut toml, format_args!("[[{}]]\n", header.join(".")));

            let mut inner_exp = Vec::new();
            let mut inner_kidx = 0;
            gen_section_kvs(rng, &mut toml, &mut inner_exp, &mut inner_kidx);

            // Optionally add a child subtable via [path.child]
            if rng.u8(0..4) == 0 {
                let (child_toml, child_str) = gen_key(rng, inner_kidx);
                let mut child_header: Vec<String> = path_keys
                    .iter()
                    .map(|(_, s)| re_quote_key(rng, s))
                    .collect();
                child_header.push(child_toml);
                write_fmt(&mut toml, format_args!("[{}]\n", child_header.join(".")));
                let mut ch_exp = Vec::new();
                let mut ch_kidx = 0;
                gen_section_kvs(rng, &mut toml, &mut ch_exp, &mut ch_kidx);
                inner_exp.push((child_str, Expected::Table(ch_exp)));
            }

            aot_entries.push(Expected::Table(inner_exp));
        }

        // Build expected from leaf AOT upward
        let mut result = Expected::Array(aot_entries);
        for d in (1..depth).rev() {
            result = Expected::Table(vec![(path_keys[d].1.clone(), result)]);
        }
        root_exp.push((path_keys[0].1.clone(), result));
    }

    // [sec.sub1], [sec.sub2], ..., then [sec] adding direct keys
    if rng.u8(0..3) == 0 {
        let (_, sec_str) = gen_key(rng, root_kidx);
        root_kidx += 1;
        let mut sec_exp = Vec::new();
        let mut sec_kidx = 0;

        let n_sub = rng.usize(2..=5);
        for _ in 0..n_sub {
            let (sub_toml, sub_str) = gen_key(rng, sec_kidx);
            sec_kidx += 1;
            write_fmt(
                &mut toml,
                format_args!("[{}.{}]\n", re_quote_key(rng, &sec_str), sub_toml,),
            );
            let mut sub_exp = Vec::new();
            let mut sub_kidx = 0;
            gen_section_kvs(rng, &mut toml, &mut sub_exp, &mut sub_kidx);
            sec_exp.push((sub_str, Expected::Table(sub_exp)));
        }

        // Open [sec] and add direct keys
        write_fmt(
            &mut toml,
            format_args!("[{}]\n", re_quote_key(rng, &sec_str)),
        );
        gen_section_kvs(rng, &mut toml, &mut sec_exp, &mut sec_kidx);
        root_exp.push((sec_str, Expected::Table(sec_exp)));
    }

    let _ = root_kidx;
    (toml, root_exp)
}

/// Generate key-value pairs for a table section.
/// Mixes plain scalars, dotted keys, inline tables, and inline arrays.
fn gen_section_kvs(
    rng: &mut fastrand::Rng,
    toml: &mut String,
    expected: &mut Vec<(String, Expected)>,
    kidx: &mut usize,
) {
    let n = rng.usize(1..=10);
    for _ in 0..n {
        let (key_toml, key_str) = gen_key(rng, *kidx);
        *kidx += 1;

        match rng.u8(0..10) {
            0 => {
                let (val_toml, val_exp) = gen_inline_table_val(rng, 2);
                write_fmt(toml, format_args!("{key_toml} = {val_toml}\n"));
                expected.push((key_str, val_exp));
            }
            1 => {
                let (val_toml, val_exp) = gen_inline_array_val(rng, 1);
                write_fmt(toml, format_args!("{key_toml} = {val_toml}\n"));
                expected.push((key_str, val_exp));
            }
            2 => {
                let (sub_toml, sub_str) = gen_key(rng, *kidx);
                *kidx += 1;
                let (val_toml, val_exp) = gen_value(rng, 1);
                write_fmt(toml, format_args!("{key_toml}.{sub_toml} = {val_toml}\n"));
                expected.push((key_str, Expected::Table(vec![(sub_str, val_exp)])));
            }
            _ => {
                let (val_toml, val_exp) = gen_scalar(rng);
                write_fmt(toml, format_args!("{key_toml} = {val_toml}\n"));
                expected.push((key_str, val_exp));
            }
        }
    }
}

fn main() {
    let total_iterations: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(500_000);

    let mut rng = fastrand::Rng::new();
    let mut total_tests = 0u64;

    for seed in 0..total_iterations {
        rng.seed(seed);
        let (toml_text, expected) = gen_document(&mut rng);

        let arena = Arena::new();
        let root = match toml_spanner::parse(&toml_text, &arena) {
            Ok(r) => r,
            Err(e) => {
                panic!(
                    "\n============= PARSE ERROR ===============\n\
                     Seed:     {seed}\n\
                     Input ({} bytes):\n{toml_text}\n\
                     Error: {e:?}\n\
                     ============================================",
                    toml_text.len(),
                );
            }
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            verify_table(root.table(), &expected, "");
        }));

        if let Err(panic_info) = result {
            let msg = if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "unknown panic".to_string()
            };
            panic!(
                "\n============== MISMATCH =============\n\
                 Seed:     {seed}\n\
                 Input ({} bytes):\n{toml_text}\n\
                 Error: {msg}\n\
                 ========================================",
                toml_text.len(),
            );
        }

        total_tests += 1;

        if seed % 50_000 == 0 && seed > 0 {
            eprintln!("Progress: {seed}/{total_iterations} seeds ({total_tests} tests)");
        }
    }
    eprintln!("All {total_iterations} seeds passed ({total_tests} tests).");
}
